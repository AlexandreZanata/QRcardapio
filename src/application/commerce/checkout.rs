use aws_lc_rs::hmac;
use chrono::{DateTime, Duration, Utc};
use sha2::{Digest, Sha256};
use sqlx::{Acquire, Sqlite, SqliteConnection, SqliteExecutor, SqlitePool, Transaction};
use subtle::ConstantTimeEq;
use uuid::Uuid;

use super::{order_creation, payments};
use crate::domain::commerce::business_hours::{Schedule, ServiceMode};
use crate::domain::commerce::catalog_snapshot::CatalogContent;
use crate::domain::commerce::checkout::{
    CheckoutCart, CheckoutOutcome, CheckoutQuote, CheckoutQuoteIssue, CheckoutReceipt,
    CheckoutRequest, CheckoutTarget,
};
use crate::domain::commerce::customer::PostalAddress;
use crate::domain::commerce::order::{
    Fulfillment, LineError, NewOrder, OrderChannel, OrderEventContext, OrderItemSnapshot,
    assess_lines,
};
use crate::domain::commerce::pricing::{
    MAX_LINE_QUANTITY, MAX_ORDER_TOTAL_CENTAVOS, PricingError, price_order,
};
use crate::error::{AppError, AppResult};
use crate::infrastructure::database::commerce_writes;
use crate::infrastructure::database::queries::{
    business_hours, catalog_snapshots, checkout_attempts, customers, order_customers,
    order_delivery_details,
};
use crate::infrastructure::events::dispatcher;

const MAX_CART_LINES: usize = 50;
const INVALID_CART: AppError = AppError::Validation("commerce.invalid_checkout");
const INVALID_PRICE: AppError = AppError::Validation("commerce.invalid_order_price");

pub async fn quote(
    pool: &SqlitePool,
    target: CheckoutTarget<'_>,
    cart: &CheckoutCart,
    now: DateTime<Utc>,
) -> AppResult<CheckoutQuote> {
    let mut tx = pool.begin().await?;
    let quote = quote_in(&mut tx, target, cart, now).await?;
    tx.commit().await?;
    Ok(quote)
}

pub(crate) async fn quote_in(
    connection: &mut SqliteConnection,
    target: CheckoutTarget<'_>,
    cart: &CheckoutCart,
    now: DateTime<Utc>,
) -> AppResult<CheckoutQuote> {
    validate_cart(cart)?;
    let schedule = business_hours::load_in(connection, target.establishment_id).await?;
    CheckoutSnapshot::load_in(connection, target, &schedule)
        .await?
        .quote(target, cart, now)
}

/// Pricing inputs established by reads on one SQLite transaction. The caller uses the
/// same transaction for quoting, repricing and persistence.
pub(crate) struct CheckoutSnapshot<'a> {
    schedule: &'a Schedule,
    version: i64,
    content: CatalogContent,
}

impl<'a> CheckoutSnapshot<'a> {
    pub(crate) async fn load_in(
        connection: &mut SqliteConnection,
        target: CheckoutTarget<'_>,
        schedule: &'a Schedule,
    ) -> AppResult<Self> {
        let (version, content) = current_catalog(connection, target).await?;
        Ok(Self {
            schedule,
            version,
            content,
        })
    }

    pub(crate) fn quote(
        &self,
        target: CheckoutTarget<'_>,
        cart: &CheckoutCart,
        now: DateTime<Utc>,
    ) -> AppResult<CheckoutQuote> {
        validate_cart(cart)?;
        let open_now = self.schedule.service_at(now, target.mode).is_some();
        let (quote, _) = evaluate_cart(
            &self.content,
            self.version,
            target,
            cart,
            self.schedule,
            open_now,
        )?;
        Ok(quote)
    }
}

pub async fn place_order(
    pool: &SqlitePool,
    target: CheckoutTarget<'_>,
    request: CheckoutRequest,
    now: DateTime<Utc>,
) -> AppResult<CheckoutOutcome> {
    let fingerprint = AttemptFingerprint::new(target, &request)?;
    if let Some(receipt) = fingerprint
        .replay(pool, target.establishment_id, now)
        .await?
    {
        return Ok(CheckoutOutcome::Placed(receipt));
    }
    let (_turn, mut tx) = commerce_writes::begin(pool).await?;
    let placement = place_locked(
        &mut tx,
        target,
        request,
        now,
        &fingerprint,
        CheckoutRules::default(),
        None,
    )
    .await?;
    tx.commit().await?;
    if placement.new_order_id.is_some() {
        dispatcher::notify_committed();
    }
    Ok(placement.outcome)
}

pub struct CheckoutPlacement {
    pub outcome: CheckoutOutcome,
    /// Present only when the caller should attach side effects to a new order.
    pub new_order_id: Option<String>,
}

#[derive(Default)]
pub(crate) struct CheckoutRules {
    pub minimum_override: Option<i64>,
    pub allow_missing_cep: bool,
}

/// Extend checkout inside a caller's `BEGIN IMMEDIATE`. The caller may consume a
/// coupon for `new_order_id` before committing, then signal the outbox dispatcher.
/// An error rolls back only the checkout savepoint.
pub async fn place_order_in(
    tx: &mut Transaction<'_, Sqlite>,
    target: CheckoutTarget<'_>,
    request: CheckoutRequest,
    now: DateTime<Utc>,
) -> AppResult<CheckoutPlacement> {
    let fingerprint = AttemptFingerprint::new(target, &request)?;
    place_order_with_attempt_in(
        tx,
        target,
        request,
        now,
        &fingerprint,
        CheckoutRules::default(),
        None,
    )
    .await
}

pub(crate) async fn place_order_with_attempt_in(
    tx: &mut Transaction<'_, Sqlite>,
    target: CheckoutTarget<'_>,
    request: CheckoutRequest,
    now: DateTime<Utc>,
    fingerprint: &AttemptFingerprint,
    rules: CheckoutRules,
    // Supplied only by a caller that has already ruled out a replay and loaded
    // the snapshot under this same BEGIN IMMEDIATE.
    checked_snapshot: Option<&CheckoutSnapshot<'_>>,
) -> AppResult<CheckoutPlacement> {
    let mut checkout = tx.begin().await?;
    match place_locked(
        &mut checkout,
        target,
        request,
        now,
        fingerprint,
        rules,
        checked_snapshot,
    )
    .await
    {
        Ok(placement) => {
            checkout.commit().await?;
            Ok(placement)
        }
        Err(error) => {
            checkout.rollback().await?;
            Err(error)
        }
    }
}

async fn place_locked(
    tx: &mut Transaction<'_, Sqlite>,
    target: CheckoutTarget<'_>,
    request: CheckoutRequest,
    now: DateTime<Utc>,
    fingerprint: &AttemptFingerprint,
    rules: CheckoutRules,
    checked_snapshot: Option<&CheckoutSnapshot<'_>>,
) -> AppResult<CheckoutPlacement> {
    // A concurrent checkout may have committed while this request waited for the writer.
    if checked_snapshot.is_none()
        && let Some(receipt) = fingerprint
            .replay(&mut **tx, target.establishment_id, now)
            .await?
    {
        return Ok(CheckoutPlacement {
            outcome: CheckoutOutcome::Placed(receipt),
            new_order_id: None,
        });
    }

    validate_cart(&request.cart)?;
    if !(0..=MAX_ORDER_TOTAL_CENTAVOS).contains(&request.expected_total_centavos) {
        return Err(INVALID_CART);
    }
    let customer = request.customer.normalized()?;
    let address = delivery_address(target.mode, request.address, rules.allow_missing_cep)?;
    let loaded_schedule;
    let schedule = if let Some(snapshot) = checked_snapshot {
        snapshot.schedule
    } else {
        loaded_schedule = business_hours::load_in(tx, target.establishment_id).await?;
        &loaded_schedule
    };
    if !schedule.settings.serves(target.mode) {
        return Err(AppError::Validation("delivery.mode_unavailable"));
    }
    let service = schedule
        .service_at(now, target.mode)
        .ok_or(AppError::Validation("delivery.closed"))?;

    // The write lock protects this snapshot through order creation and attempt recording.
    let loaded_snapshot;
    let snapshot = if let Some(snapshot) = checked_snapshot {
        snapshot
    } else {
        loaded_snapshot = CheckoutSnapshot::load_in(tx, target, schedule).await?;
        &loaded_snapshot
    };
    let version = snapshot.version;
    let (mut fresh, items) = evaluate_cart(
        &snapshot.content,
        version,
        target,
        &request.cart,
        schedule,
        true,
    )?;
    if let Some(minimum) = rules.minimum_override {
        fresh.minimum_order_centavos = minimum;
    }
    let Some(totals) = fresh.totals.filter(|totals| {
        version == request.cart.catalog_version
            && totals.total_centavos == request.expected_total_centavos
            && request
                .expected_totals
                .is_none_or(|expected| expected == *totals)
    }) else {
        return Ok(CheckoutPlacement {
            outcome: CheckoutOutcome::Repriced(fresh),
            new_order_id: None,
        });
    };
    if totals.items_subtotal_centavos < fresh.minimum_order_centavos {
        return Err(AppError::Validation("delivery.below_minimum"));
    }

    let (channel, fulfillment) = match target.mode {
        ServiceMode::Delivery => (OrderChannel::Delivery, Fulfillment::Delivery),
        ServiceMode::Pickup => (OrderChannel::Pickup, Fulfillment::Pickup),
    };
    let deadline = now
        .checked_add_signed(Duration::minutes(schedule.settings.acceptance_minutes))
        .ok_or(AppError::Validation("commerce.invalid_acceptance_deadline"))?;
    let command = NewOrder {
        channel,
        fulfillment,
        catalog_channel: target.catalog_channel.to_owned(),
        catalog_version: version,
        items: request.cart.items,
        discount: target.discount,
        delivery_fee_centavos: totals.delivery_fee_centavos,
        acceptance_deadline_at: deadline,
        event_context: OrderEventContext {
            correlation_id: Some(Uuid::now_v7()),
            causation_id: None,
        },
    };
    let customer_id =
        customers::upsert_for_checkout(tx, target.establishment_id, &customer, &now.to_rfc3339())
            .await?;
    let order = order_creation::insert_priced_order(
        tx,
        target.establishment_id,
        &command,
        now,
        service.business_date,
        &items,
        totals,
    )
    .await?;
    order_customers::link(tx, &order, &customer_id).await?;
    if let Some(address) = address {
        order_delivery_details::insert(tx, &order, &customer, &address).await?;
    }
    payments::create_initial_pending(tx, &order, request.payment, now).await?;
    checkout_attempts::remember(
        tx,
        &order,
        &fingerprint.key_hash,
        fingerprint.request_hash.as_ref(),
    )
    .await?;
    Ok(CheckoutPlacement {
        outcome: CheckoutOutcome::Placed(CheckoutReceipt {
            public_id: order.public_id,
        }),
        new_order_id: Some(order.id),
    })
}

fn validate_cart(cart: &CheckoutCart) -> AppResult<()> {
    if cart.catalog_version < 1
        || !(1..=MAX_CART_LINES).contains(&cart.items.len())
        || cart
            .items
            .iter()
            .any(|item| !(1..=MAX_LINE_QUANTITY).contains(&item.quantity))
    {
        return Err(INVALID_CART);
    }
    Ok(())
}

fn delivery_address(
    mode: ServiceMode,
    address: Option<PostalAddress>,
    allow_missing_cep: bool,
) -> AppResult<Option<PostalAddress>> {
    match (mode, address) {
        (ServiceMode::Delivery, Some(address)) if allow_missing_cep => {
            address.normalized_with_optional_cep().map(Some)
        }
        (ServiceMode::Delivery, Some(address)) => Ok(Some(address.normalized()?)),
        (ServiceMode::Pickup, None) => Ok(None),
        _ => Err(AppError::Validation("commerce.customer.invalid_address")),
    }
}

async fn current_catalog(
    conn: &mut SqliteConnection,
    target: CheckoutTarget<'_>,
) -> AppResult<(i64, CatalogContent)> {
    let snapshot =
        catalog_snapshots::current_in(conn, target.establishment_id, target.catalog_channel)
            .await?
            .ok_or(AppError::NotFound("error.not_found"))?;
    Ok((snapshot.version, snapshot.decode()?))
}

fn evaluate_cart<'a>(
    catalog: &'a CatalogContent,
    version: i64,
    target: CheckoutTarget<'_>,
    cart: &CheckoutCart,
    schedule: &Schedule,
    open_now: bool,
) -> AppResult<(CheckoutQuote, Vec<OrderItemSnapshot<'a>>)> {
    let mut lines = Vec::with_capacity(cart.items.len());
    let mut items = Vec::with_capacity(cart.items.len());
    for line in assess_lines(catalog, &cart.items) {
        match line {
            Ok(item) => {
                lines.push(Some(item.price));
                items.push(item);
            }
            Err(LineError::InvalidSelection) => lines.push(None),
            Err(LineError::Price) => return Err(INVALID_PRICE),
        }
    }
    let (totals, issue) = if items.len() == lines.len() {
        let prices: Vec<_> = lines.iter().copied().flatten().collect();
        let fee = match target.mode {
            ServiceMode::Delivery => target.delivery_fee_centavos,
            ServiceMode::Pickup => 0,
        };
        match price_order(&prices, target.discount, fee) {
            Ok(totals) => (Some(totals), None),
            Err(PricingError::OrderLimit) => (None, Some(CheckoutQuoteIssue::OrderLimit)),
            Err(_) => return Err(INVALID_PRICE),
        }
    } else {
        (None, None)
    };
    Ok((
        CheckoutQuote {
            catalog_version: version,
            lines,
            totals,
            issue,
            minimum_order_centavos: schedule.settings.minimum_centavos,
            open_now,
        },
        items,
    ))
}

pub(crate) struct AttemptFingerprint {
    key_hash: [u8; 32],
    request_hash: hmac::Tag,
}

impl AttemptFingerprint {
    fn new(target: CheckoutTarget<'_>, request: &CheckoutRequest) -> AppResult<Self> {
        if request.idempotency_key.get_version_num() != 4 {
            return Err(INVALID_CART);
        }
        Ok(Self {
            key_hash: Sha256::digest(request.idempotency_key.as_bytes()).into(),
            request_hash: request_fingerprint(target, request)?,
        })
    }

    /// A channel may fingerprint its original request before resolving rules that can change
    /// between the initial checkout and an idempotent retry.
    pub(crate) fn for_channel(
        idempotency_key: Uuid,
        channel: &str,
        request: &impl serde::Serialize,
    ) -> AppResult<Self> {
        if idempotency_key.get_version_num() != 4 {
            return Err(INVALID_CART);
        }
        let bytes = serde_json::to_vec(&("commerce.checkout.channel.v1", channel, request))
            .map_err(|error| {
                AppError::Internal(format!("checkout serialization failed: {error}"))
            })?;
        let key = hmac::Key::new(hmac::HMAC_SHA256, idempotency_key.as_bytes());
        Ok(Self {
            key_hash: Sha256::digest(idempotency_key.as_bytes()).into(),
            request_hash: hmac::sign(&key, &bytes),
        })
    }

    pub(crate) async fn replay<'e>(
        &self,
        executor: impl SqliteExecutor<'e>,
        establishment_id: &str,
        now: DateTime<Utc>,
    ) -> AppResult<Option<CheckoutReceipt>> {
        let attempt =
            checkout_attempts::find_live(executor, establishment_id, &self.key_hash, now).await?;
        let Some((stored_hash, public_id)) = attempt else {
            return Ok(None);
        };
        if !bool::from(stored_hash.as_slice().ct_eq(self.request_hash.as_ref())) {
            return Err(AppError::Validation("commerce.idempotency_mismatch"));
        }
        Ok(Some(CheckoutReceipt { public_id }))
    }
}

fn request_fingerprint(
    target: CheckoutTarget<'_>,
    request: &CheckoutRequest,
) -> AppResult<hmac::Tag> {
    let fulfillment = match target.mode {
        ServiceMode::Delivery => "delivery",
        ServiceMode::Pickup => "pickup",
    };
    let legacy = (
        "commerce.checkout.v1",
        target.catalog_channel,
        fulfillment,
        &request.cart,
        request.expected_total_centavos,
        &request.customer,
        &request.address,
        request.payment,
    );
    // Keep the exact original bytes for attempts already stored by total-only callers.
    let bytes = match request.expected_totals {
        Some(totals) => serde_json::to_vec(&("commerce.checkout.quote.v1", legacy, totals)),
        None => serde_json::to_vec(&legacy),
    }
    .map_err(|error| AppError::Internal(format!("checkout serialization failed: {error}")))?;
    // The client key is secret; only its digest is retained alongside the request HMAC.
    let key = hmac::Key::new(hmac::HMAC_SHA256, request.idempotency_key.as_bytes());
    Ok(hmac::sign(&key, &bytes))
}
