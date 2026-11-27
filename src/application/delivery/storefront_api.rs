use aws_lc_rs::hmac;
use chrono::{DateTime, Utc};
use sqlx::{SqliteExecutor, SqlitePool};
use uuid::Uuid;

use crate::application::commerce::checkout;
use crate::application::entitlements::{Enforcement, authorize_with_catalog};
use crate::domain::billing::billing_now;
use crate::domain::commerce::business_hours::ServiceMode;
use crate::domain::commerce::checkout::{
    CheckoutCart, CheckoutOutcome, CheckoutRequest, CheckoutTarget,
};
use crate::domain::commerce::customer::{CustomerProfile, PostalAddress};
use crate::domain::commerce::order::{OrderAddonSelection, OrderItemSelection};
use crate::domain::commerce::order_payment::{OfflineMethod, OfflinePaymentChoice};
use crate::domain::commerce::pricing::MAX_LINE_QUANTITY;
use crate::domain::commercial::CommercialCatalog;
use crate::domain::delivery::storefront::StorefrontStatus;
use crate::domain::delivery::zone::normalize_neighborhood_key;
use crate::domain::entitlements::{ActorRole, Operation};
use crate::error::{AppError, AppResult};
use crate::infrastructure::database::commerce_writes;
use crate::infrastructure::database::queries::{
    business_hours, delivery_coupons, delivery_orders, delivery_storefronts,
};
use crate::infrastructure::events::dispatcher;
use crate::infrastructure::security::sha256_hex;

mod quote;
mod tracking;
mod types;

pub use quote::quote;
use quote::{build_quote_response, evaluate_coupon, resolve_zone, zone_fee};
pub use tracking::{cancel_order, get_tracking};
pub use types::*;

const STOREFRONT_CHANNEL: &str = "storefront";
const MAX_LINES: usize = 50;

fn parse_mode(raw: &str) -> AppResult<ServiceMode> {
    match raw.trim() {
        "delivery" => Ok(ServiceMode::Delivery),
        "pickup" => Ok(ServiceMode::Pickup),
        _ => Err(AppError::Validation("delivery.invalid_checkout")),
    }
}

fn validate_lines(lines: &[StorefrontLineInput]) -> AppResult<Vec<OrderItemSelection>> {
    if !(1..=MAX_LINES).contains(&lines.len()) {
        return Err(AppError::Validation("delivery.invalid_checkout"));
    }
    let mut out = Vec::with_capacity(lines.len());
    for line in lines {
        if !(1..=MAX_LINE_QUANTITY).contains(&line.quantity) {
            return Err(AppError::Validation("delivery.invalid_checkout"));
        }
        if let Some(note) = &line.note
            && (note.chars().count() > 140 || note.chars().any(char::is_control))
        {
            return Err(AppError::Validation("delivery.invalid_checkout"));
        }
        let addons = line
            .options
            .iter()
            .map(|opt| OrderAddonSelection {
                option_id: opt.option_id.clone(),
                quantity: opt.quantity,
            })
            .collect();
        out.push(OrderItemSelection {
            product_id: line.product_id.clone(),
            variation_id: line.variation_id.clone(),
            quantity: line.quantity,
            addons,
        });
    }
    Ok(out)
}

async fn authorize_consumer_place(
    pool: &SqlitePool,
    catalog: &CommercialCatalog,
    enforcement: Enforcement,
    est_id: &str,
) -> AppResult<()> {
    authorize_with_catalog(
        pool,
        catalog,
        enforcement,
        est_id,
        ActorRole::Consumer,
        Operation::DeliveryOrderPlace,
        billing_now().date_naive(),
    )
    .await?
    .map_err(|_| AppError::Forbidden("delivery.ordering_unavailable"))?;
    Ok(())
}

#[allow(clippy::too_many_arguments)]
pub async fn place_order(
    pool: &SqlitePool,
    catalog: &CommercialCatalog,
    enforcement: Enforcement,
    rate_limiters: &crate::infrastructure::security::rate_limit::AppRateLimiters,
    slug: &str,
    idempotency_key: Uuid,
    input: StorefrontOrderInput,
    now: DateTime<Utc>,
) -> AppResult<StorefrontOrderOutcome> {
    if input
        .website
        .as_deref()
        .is_some_and(|s| !s.trim().is_empty())
    {
        return Err(AppError::Validation("delivery.invalid_checkout"));
    }
    if idempotency_key.get_version_num() != 4 || input.catalog_version < 1 {
        return Err(AppError::Validation("delivery.invalid_checkout"));
    }
    if let Some(note) = &input.note
        && (note.chars().count() > 280 || note.chars().any(char::is_control))
    {
        return Err(AppError::Validation("delivery.invalid_checkout"));
    }
    let mode = parse_mode(&input.fulfillment)?;
    let items = validate_lines(&input.lines)?;
    let est = delivery_storefronts::find_establishment_by_slug(pool, slug)
        .await?
        .ok_or(AppError::NotFound("delivery.storefront_not_found"))?;
    let attempt =
        checkout::AttemptFingerprint::for_channel(idempotency_key, STOREFRONT_CHANNEL, &input)?;
    if let Some(receipt) = attempt.replay(pool, &est.id, now).await? {
        return existing_receipt(pool, &est.id, &est.slug, idempotency_key, receipt.public_id)
            .await;
    }
    if delivery_storefronts::get_or_default(pool, &est.id)
        .await?
        .status
        != StorefrontStatus::Published
    {
        return Err(AppError::NotFound("delivery.storefront_not_found"));
    }
    authorize_consumer_place(pool, catalog, enforcement, &est.id).await?;

    let customer = CustomerProfile {
        name: input.customer.name,
        phone_e164: input.customer.phone,
        email: String::new(),
    }
    .normalized()?;

    let (_turn, mut tx) = commerce_writes::begin(pool).await?;
    if let Some(receipt) = attempt.replay(&mut *tx, &est.id, now).await? {
        let result = existing_receipt(
            &mut *tx,
            &est.id,
            &est.slug,
            idempotency_key,
            receipt.public_id,
        )
        .await?;
        tx.commit().await?;
        return Ok(result);
    }
    let storefront = delivery_storefronts::get_or_default(&mut *tx, &est.id).await?;
    if storefront.status != StorefrontStatus::Published {
        return Err(AppError::NotFound("delivery.storefront_not_found"));
    }
    let phone_limit_key = sha256_hex(&format!("{}:{}", est.id, customer.phone_e164));
    if rate_limiters
        .delivery_order_phone
        .check_key(&phone_limit_key)
        .is_err()
    {
        return Err(AppError::RateLimited);
    }
    if delivery_orders::is_phone_blocked(&mut *tx, &est.id, &customer.phone_e164).await? {
        return Err(AppError::Forbidden("delivery.ordering_unavailable"));
    }

    let method = OfflineMethod::parse(input.payment.method.trim())
        .ok_or(AppError::Validation("delivery.payment_method_unavailable"))?;
    if !storefront.accepts_method(method) {
        return Err(AppError::Validation("delivery.payment_method_unavailable"));
    }
    let payment = OfflinePaymentChoice {
        method,
        change_for_centavos: input.payment.change_for_centavos,
    };

    let zone_id_owned = input.zone_id.clone().or_else(|| {
        input
            .address
            .as_ref()
            .and_then(|a| a.neighborhood_zone_id.clone())
    });
    if let (Some(z1), Some(z2)) = (
        input.zone_id.as_deref(),
        input
            .address
            .as_ref()
            .and_then(|a| a.neighborhood_zone_id.as_deref()),
    ) && z1 != z2
    {
        return Err(AppError::Validation("delivery.zone_not_served"));
    }
    let zone = resolve_zone(&mut tx, &est.id, mode, zone_id_owned.as_deref()).await?;

    let address = match (mode, input.address, &zone) {
        (ServiceMode::Delivery, Some(addr), Some(z)) => {
            if !addr.city.trim().is_empty()
                && normalize_neighborhood_key(&z.neighborhood, &addr.city) != z.normalized_key
            {
                return Err(AppError::Validation("delivery.zone_not_served"));
            }
            Some(
                PostalAddress {
                    street: addr.street,
                    street_number: addr.number,
                    complement: addr.complement,
                    neighborhood: z.neighborhood.clone(),
                    city: z.city.clone(),
                    uf: addr.uf,
                    cep: addr.cep.unwrap_or_default(),
                    reference: addr.reference,
                }
                .normalized_with_optional_cep()?,
            )
        }
        (ServiceMode::Pickup, None, None) => None,
        _ => return Err(AppError::Validation("commerce.customer.invalid_address")),
    };

    let schedule = business_hours::load_in(&mut tx, &est.id).await?;
    if !schedule.settings.serves(mode) {
        return Err(AppError::Validation("delivery.mode_unavailable"));
    }
    if schedule.service_at(now, mode).is_none() {
        return Err(AppError::Validation("delivery.closed"));
    }

    let cart = CheckoutCart {
        catalog_version: input.catalog_version,
        items,
    };

    let base_target = CheckoutTarget {
        establishment_id: &est.id,
        catalog_channel: STOREFRONT_CHANNEL,
        mode,
        delivery_fee_centavos: 0,
        discount: None,
    };
    let snapshot = checkout::CheckoutSnapshot::load_in(&mut tx, base_target, &schedule).await?;
    let needs_subtotal = zone
        .as_ref()
        .is_some_and(|zone| zone.free_delivery_from_centavos.is_some())
        || input
            .coupon_code
            .as_deref()
            .is_some_and(|code| !code.trim().is_empty());
    let subtotal = if needs_subtotal {
        snapshot
            .quote(base_target, &cart, now)?
            .totals
            .map(|totals| totals.items_subtotal_centavos)
    } else {
        None
    };

    let (coupon, coupon_status) = evaluate_coupon(
        &mut tx,
        &est.id,
        mode,
        subtotal,
        input.coupon_code.as_deref(),
        now,
        &schedule.settings.time_zone,
        Some(&customer.phone_e164),
    )
    .await?;
    let effective_fee = zone_fee(zone.as_ref(), subtotal);
    let no_coupon_target = CheckoutTarget {
        delivery_fee_centavos: effective_fee,
        ..base_target
    };
    if coupon.is_none() && coupon_status.is_some() {
        let fresh = snapshot.quote(no_coupon_target, &cart, now)?;
        tx.rollback().await?;
        return Ok(StorefrontOrderOutcome::Repriced(build_quote_response(
            &fresh,
            zone.as_ref(),
            coupon_status,
        )));
    }

    let target = CheckoutTarget {
        discount: coupon.as_ref().map(|(_, discount)| *discount),
        ..no_coupon_target
    };
    let request = CheckoutRequest {
        idempotency_key,
        cart: cart.clone(),
        expected_total_centavos: input.expected_total_centavos,
        expected_totals: Some(input.expected_totals.into()),
        customer,
        address,
        payment,
    };

    let minimum_override = zone.as_ref().and_then(|z| z.min_order_centavos);
    let placement = checkout::place_order_with_attempt_in(
        &mut tx,
        target,
        request,
        now,
        &attempt,
        checkout::CheckoutRules {
            minimum_override,
            allow_missing_cep: true,
        },
        Some(&snapshot),
    )
    .await?;
    match placement.outcome {
        CheckoutOutcome::Repriced(fresh) => {
            tx.rollback().await?;
            Ok(StorefrontOrderOutcome::Repriced(build_quote_response(
                &fresh,
                zone.as_ref(),
                coupon_status,
            )))
        }
        CheckoutOutcome::Placed(receipt) => {
            let cancel_token = derive_cancel_token(idempotency_key, &receipt.public_id);
            let pix_key_display = if let Some(order_id) = placement.new_order_id {
                let pix_key_display =
                    (method == OfflineMethod::PixOnDelivery).then_some(storefront.pix_key_display);
                if let Some((coupon, _)) = &coupon {
                    let customer_id =
                        delivery_orders::get_customer_id_for_order(&mut *tx, &est.id, &order_id)
                            .await?
                            .ok_or_else(|| {
                                AppError::Internal("missing customer on Delivery order".into())
                            })?;
                    let consumed = delivery_coupons::consume_in(
                        &mut tx,
                        &est.id,
                        &coupon.id,
                        &order_id,
                        &customer_id,
                        now,
                    )
                    .await?;
                    if !consumed {
                        let fresh = snapshot.quote(no_coupon_target, &cart, now)?;
                        tx.rollback().await?;
                        return Ok(StorefrontOrderOutcome::Repriced(build_quote_response(
                            &fresh,
                            zone.as_ref(),
                            Some(StorefrontCouponResult { status: "invalid" }),
                        )));
                    }
                }
                let meta = delivery_orders::DeliveryOrderMetadata {
                    establishment_id: est.id.clone(),
                    order_id,
                    cancel_token_hash: sha256_hex(&cancel_token),
                    zone_id: zone.as_ref().map(|z| z.id.clone()),
                    neighborhood_label: zone
                        .as_ref()
                        .map(|z| format!("{} — {}", z.neighborhood, z.city))
                        .unwrap_or_default(),
                    coupon_id: coupon.as_ref().map(|(c, _)| c.id.clone()),
                    coupon_code: coupon.as_ref().map(|(c, _)| c.code.clone()),
                    order_note: input.note.unwrap_or_default().trim().to_owned(),
                };
                delivery_orders::insert_metadata_in(
                    &mut tx,
                    &meta,
                    pix_key_display.as_deref(),
                    &now.to_rfc3339(),
                )
                .await?;
                for (position, line) in input.lines.iter().enumerate() {
                    if let Some(note) = line
                        .note
                        .as_deref()
                        .map(str::trim)
                        .filter(|note| !note.is_empty())
                    {
                        delivery_orders::insert_item_note_in(
                            &mut tx,
                            &est.id,
                            &meta.order_id,
                            position,
                            note,
                        )
                        .await?;
                    }
                }
                tx.commit().await?;
                dispatcher::notify_committed();
                pix_key_display
            } else {
                let pix_key_display =
                    delivery_orders::get_pix_receipt_key(&mut *tx, &est.id, &receipt.public_id)
                        .await?;
                tx.commit().await?;
                pix_key_display
            };
            Ok(placed(
                &est.slug,
                idempotency_key,
                receipt.public_id,
                pix_key_display,
            ))
        }
    }
}

fn derive_cancel_token(idempotency_key: Uuid, public_id: &str) -> String {
    let key = hmac::Key::new(hmac::HMAC_SHA256, idempotency_key.as_bytes());
    let tag = hmac::sign(&key, public_id.as_bytes());
    hex::encode(tag.as_ref())
}

async fn existing_receipt<'e>(
    executor: impl SqliteExecutor<'e>,
    est_id: &str,
    slug: &str,
    idempotency_key: Uuid,
    order_id: String,
) -> AppResult<StorefrontOrderOutcome> {
    let pix_key_display = delivery_orders::get_pix_receipt_key(executor, est_id, &order_id).await?;
    Ok(placed(slug, idempotency_key, order_id, pix_key_display))
}

fn placed(
    slug: &str,
    idempotency_key: Uuid,
    order_id: String,
    pix_key_display: Option<String>,
) -> StorefrontOrderOutcome {
    StorefrontOrderOutcome::Placed(StorefrontOrderReceipt {
        tracking_path: format!("/api/storefront/v1/{slug}/orders/{order_id}"),
        cancel_token: derive_cancel_token(idempotency_key, &order_id),
        order_id,
        pix_key_display,
    })
}
