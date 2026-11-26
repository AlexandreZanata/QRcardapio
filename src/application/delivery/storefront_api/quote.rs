use chrono::{DateTime, Utc};
use sqlx::{SqliteConnection, SqlitePool};

use super::types::{
    StorefrontCouponResult, StorefrontQuoteInput, StorefrontQuoteLine, StorefrontQuoteResponse,
};
use super::{STOREFRONT_CHANNEL, authorize_consumer_place, parse_mode, validate_lines};
use crate::application::commerce::checkout;
use crate::application::entitlements::Enforcement;
use crate::domain::commerce::business_hours::ServiceMode;
use crate::domain::commerce::checkout::{CheckoutCart, CheckoutQuote, CheckoutTarget};
use crate::domain::commerce::discount::Discount;
use crate::domain::commercial::CommercialCatalog;
use crate::domain::delivery::coupon::{Coupon, CouponEvaluation, validate_coupon_code};
use crate::domain::delivery::zone::DeliveryZone;
use crate::error::{AppError, AppResult};
use crate::infrastructure::database::queries::{
    business_hours, delivery_coupons, delivery_storefronts, delivery_zones,
};

pub async fn quote(
    pool: &SqlitePool,
    catalog: &CommercialCatalog,
    enforcement: Enforcement,
    slug: &str,
    input: StorefrontQuoteInput,
    now: DateTime<Utc>,
) -> AppResult<StorefrontQuoteResponse> {
    if input.catalog_version < 1 {
        return Err(AppError::Validation("delivery.invalid_checkout"));
    }
    let mode = parse_mode(&input.fulfillment)?;
    let items = validate_lines(&input.lines)?;
    let (est, _) = delivery_storefronts::find_published_by_slug(pool, slug)
        .await?
        .ok_or(AppError::NotFound("delivery.storefront_not_found"))?;
    authorize_consumer_place(pool, catalog, enforcement, &est.id).await?;

    let mut tx = pool.begin().await?;
    let schedule = business_hours::load_in(&mut tx, &est.id).await?;
    if !schedule.settings.serves(mode) {
        return Err(AppError::Validation("delivery.mode_unavailable"));
    }
    let zone = resolve_zone(&mut tx, &est.id, mode, input.zone_id.as_deref()).await?;

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
    let base_quote = checkout::quote_in(&mut tx, base_target, &cart, now).await?;
    let subtotal = base_quote.totals.map(|t| t.items_subtotal_centavos);
    let (coupon, coupon_result) = evaluate_coupon(
        &mut tx,
        &est.id,
        mode,
        subtotal,
        input.coupon_code.as_deref(),
        now,
        &schedule.settings.time_zone,
        None,
    )
    .await?;

    let discount = coupon.map(|(_, discount)| discount);
    let effective_fee = zone_fee(zone.as_ref(), subtotal);
    let final_quote = if discount.is_some() || effective_fee > 0 {
        let final_target = CheckoutTarget {
            establishment_id: &est.id,
            catalog_channel: STOREFRONT_CHANNEL,
            mode,
            delivery_fee_centavos: effective_fee,
            discount,
        };
        checkout::quote_in(&mut tx, final_target, &cart, now).await?
    } else {
        base_quote
    };

    tx.commit().await?;
    Ok(build_quote_response(
        &final_quote,
        zone.as_ref(),
        coupon_result,
    ))
}

pub(super) async fn resolve_zone(
    connection: &mut SqliteConnection,
    est_id: &str,
    mode: ServiceMode,
    zone_id: Option<&str>,
) -> AppResult<Option<DeliveryZone>> {
    match mode {
        ServiceMode::Pickup => Ok(None),
        ServiceMode::Delivery => {
            let Some(id) = zone_id.map(str::trim).filter(|s| !s.is_empty()) else {
                return Err(AppError::Validation("delivery.zone_not_served"));
            };
            let zone = delivery_zones::get_in(connection, est_id, id)
                .await?
                .filter(|z| z.is_active)
                .ok_or(AppError::Validation("delivery.zone_not_served"))?;
            Ok(Some(zone))
        }
    }
}

pub(super) fn zone_fee(zone: Option<&DeliveryZone>, subtotal: Option<i64>) -> i64 {
    zone.map_or(0, |zone| {
        subtotal.map_or(zone.fee_centavos, |subtotal| {
            zone.effective_fee_centavos(subtotal)
        })
    })
}

#[allow(clippy::too_many_arguments)]
pub(super) async fn evaluate_coupon(
    conn: &mut SqliteConnection,
    est_id: &str,
    mode: ServiceMode,
    subtotal: Option<i64>,
    raw_code: Option<&str>,
    now: DateTime<Utc>,
    time_zone: &str,
    phone_e164: Option<&str>,
) -> AppResult<(Option<(Coupon, Discount)>, Option<StorefrontCouponResult>)> {
    let Some(raw) = raw_code.map(str::trim).filter(|s| !s.is_empty()) else {
        return Ok((None, None));
    };
    let rejected = (None, Some(StorefrontCouponResult { status: "invalid" }));
    let Ok(code) = validate_coupon_code(raw) else {
        return Ok(rejected);
    };
    let Some(sub) = subtotal else {
        return Ok(rejected);
    };
    let Some(coupon) = delivery_coupons::get_by_code_in(&mut *conn, est_id, &code).await? else {
        return Ok(rejected);
    };
    let redemptions = match (phone_e164, coupon.max_per_customer) {
        (Some(phone), Some(_)) => {
            delivery_coupons::count_customer_redemptions_in(conn, est_id, &coupon.id, phone).await?
        }
        _ => 0,
    };
    match coupon.evaluate(mode, sub, now, time_zone, redemptions) {
        CouponEvaluation::Applied(discount) => Ok((
            Some((coupon, discount)),
            Some(StorefrontCouponResult { status: "applied" }),
        )),
        CouponEvaluation::Rejected => Ok(rejected),
    }
}

pub(super) fn build_quote_response(
    quote: &CheckoutQuote,
    zone: Option<&DeliveryZone>,
    coupon: Option<StorefrontCouponResult>,
) -> StorefrontQuoteResponse {
    let min_order = zone.map_or(quote.minimum_order_centavos, |z| {
        z.effective_min_order_centavos(quote.minimum_order_centavos)
    });
    let mut issues = Vec::new();
    if let Some(issue) = quote.issue {
        issues.push(issue.i18n_key().to_owned());
    }
    if quote
        .totals
        .is_some_and(|t| t.items_subtotal_centavos < min_order)
    {
        issues.push("delivery.below_minimum".to_owned());
    }
    StorefrontQuoteResponse {
        catalog_version: quote.catalog_version,
        lines: quote
            .lines
            .iter()
            .map(|line| match line {
                Some(p) => StorefrontQuoteLine {
                    unit_price_centavos: Some(p.unit_price_centavos),
                    line_total_centavos: Some(p.line_total_centavos),
                    available: true,
                },
                None => StorefrontQuoteLine {
                    unit_price_centavos: None,
                    line_total_centavos: None,
                    available: false,
                },
            })
            .collect(),
        items_subtotal_centavos: quote.totals.map(|t| t.items_subtotal_centavos),
        discount_centavos: quote.totals.map(|t| t.discount_centavos),
        delivery_fee_centavos: quote.totals.map(|t| t.delivery_fee_centavos),
        total_centavos: quote.totals.map(|t| t.total_centavos),
        minimum_order_centavos: min_order,
        coupon,
        open_now: quote.open_now,
        eta_minutes: zone.map(|z| [z.eta_min_minutes, z.eta_max_minutes]),
        issues,
    }
}
