use aws_lc_rs::hmac;
use chrono::{DateTime, Duration, Utc};
use sha2::{Digest, Sha256};
use sqlx::{Acquire, Sqlite, SqliteConnection, SqlitePool, Transaction};
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
    validate_cart(cart)?;
    let mut tx = pool.begin().await?;
    let schedule = business_hours::load_in(&mut tx, target.establishment_id).await?;
    let (version, content) = current_catalog(&mut tx, target).await?;
    tx.commit().await?;
    let (quote, _) = evaluate_cart(&content, version, target, cart, &schedule, now)?;
    Ok(quote)
}

pub async fn place_order(
    pool: &SqlitePool,
    target: CheckoutTarget<'_>,
    request: CheckoutRequest,
    now: DateTime<Utc>,
) -> AppResult<CheckoutOutcome> {
    let (_turn, mut tx) = commerce_writes::begin(pool).await?;
    let placement = place_locked(&mut tx, target, request, now).await?;
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

/// Extend checkout inside a caller's `BEGIN IMMEDIATE`. The caller may consume a
/// coupon for `new_order_id` before committing, then signal the outbox dispatcher.
/// An error rolls back only the checkout savepoint.
pub async fn place_order_in(
    tx: &mut Transaction<'_, Sqlite>,
    target: CheckoutTarget<'_>,
    request: CheckoutRequest,
    now: DateTime<Utc>,
) -> AppResult<CheckoutPlacement> {
    let mut checkout = tx.begin().await?;
    match place_locked(&mut checkout, target, request, now).await {
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
) -> AppResult<CheckoutPlacement> {
    if request.idempotency_key.get_version_num() != 4 {
        return Err(INVALID_CART);
    }
    let key_hash = Sha256::digest(request.idempotency_key.as_bytes());
    let request_hash = request_fingerprint(target, &request)?;
    if let Some((stored_hash, public_id)) =
        checkout_attempts::find_live(tx, target.establishment_id, &key_hash, now).await?
    {
        if !bool::from(stored_hash.as_slice().ct_eq(request_hash.as_ref())) {
            return Err(AppError::Validation("commerce.idempotency_mismatch"));
        }
        return Ok(CheckoutPlacement {
            outcome: CheckoutOutcome::Placed(CheckoutReceipt { public_id }),
            new_order_id: None,
        });
    }

    validate_cart(&request.cart)?;
    if !(0..=MAX_ORDER_TOTAL_CENTAVOS).contains(&request.expected_total_centavos) {
        return Err(INVALID_CART);
    }
    let customer = request.customer.normalized()?;
    let address = delivery_address(target.mode, request.address)?;
    let schedule = business_hours::load_in(tx, target.establishment_id).await?;
    if !schedule.settings.serves(target.mode) {
        return Err(AppError::Validation("delivery.mode_unavailable"));
    }
    let service = schedule
        .service_at(now, target.mode)
        .ok_or(AppError::Validation("delivery.closed"))?;

    // The write lock protects this snapshot through order creation and attempt recording.
    let (version, content) = current_catalog(tx, target).await?;
    let (fresh, items) = evaluate_cart(&content, version, target, &request.cart, &schedule, now)?;
    let Some(totals) = fresh.totals.filter(|totals| {
        version == request.cart.catalog_version
            && totals.total_centavos == request.expected_total_centavos
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
    payments::create_pending_in(tx, &order, request.payment, now).await?;
    checkout_attempts::remember(tx, &order, &key_hash, request_hash.as_ref()).await?;
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
) -> AppResult<Option<PostalAddress>> {
    match (mode, address) {
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
    now: DateTime<Utc>,
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
            open_now: schedule.service_at(now, target.mode).is_some(),
        },
        items,
    ))
}

fn request_fingerprint(
    target: CheckoutTarget<'_>,
    request: &CheckoutRequest,
) -> AppResult<hmac::Tag> {
    let fulfillment = match target.mode {
        ServiceMode::Delivery => "delivery",
        ServiceMode::Pickup => "pickup",
    };
    let bytes = serde_json::to_vec(&(
        "commerce.checkout.v1",
        target.catalog_channel,
        fulfillment,
        &request.cart,
        request.expected_total_centavos,
        &request.customer,
        &request.address,
        request.payment,
    ))
    .map_err(|error| AppError::Internal(format!("checkout serialization failed: {error}")))?;
    // The client key is secret; only its digest is retained alongside the request HMAC.
    let key = hmac::Key::new(hmac::HMAC_SHA256, request.idempotency_key.as_bytes());
    Ok(hmac::sign(&key, &bytes))
}
