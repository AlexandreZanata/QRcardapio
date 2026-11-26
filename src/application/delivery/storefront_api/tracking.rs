use chrono::{DateTime, Utc};
use sqlx::SqlitePool;
use uuid::Uuid;

use super::types::{
    StorefrontTrackingView, TrackingItem, TrackingItemOption, TrackingPayment, TrackingTotals,
};
use crate::application::commerce::order_transitions;
use crate::application::entitlements::{Enforcement, authorize_with_catalog};
use crate::domain::billing::billing_now;
use crate::domain::commerce::order::{
    CancelReason, Order, OrderAction, OrderEventContext, TransitionOrder,
};
use crate::domain::commerce::state_machine::{Actor, OrderState};
use crate::domain::commercial::CommercialCatalog;
use crate::domain::entitlements::{ActorRole, Operation};
use crate::error::{AppError, AppResult};
use crate::infrastructure::database::queries::{
    delivery_orders, delivery_storefronts, order_payments,
};
use crate::infrastructure::security::{constant_time_eq, sha256_hex};

pub async fn get_tracking(
    pool: &SqlitePool,
    slug: &str,
    public_id: &str,
) -> AppResult<(StorefrontTrackingView, String)> {
    validate_public_id(public_id)?;
    let est = delivery_storefronts::find_establishment_by_slug(pool, slug)
        .await?
        .ok_or(AppError::NotFound("delivery.storefront_not_found"))?;
    let order = delivery_orders::find_order_by_public_id(pool, &est.id, public_id)
        .await?
        .ok_or(AppError::NotFound("error.not_found"))?;
    let storefront = delivery_storefronts::get_or_default(pool, &est.id).await?;
    let meta = delivery_orders::get_metadata(pool, &est.id, &order.id).await?;
    let payment = order_payments::latest_for_order(pool, &est.id, &order.id).await?;
    let raw_items = delivery_orders::list_order_items(pool, &est.id, &order.id).await?;
    let raw_options = delivery_orders::list_order_item_options(pool, &est.id, &order.id).await?;

    let payment_marker = payment.as_ref().map_or_else(String::new, |p| {
        sha256_hex(&format!("{}-{}", p.id, p.version))
    });
    let contact_hash = sha256_hex(&storefront.contact_whatsapp);
    let etag = format!(
        "\"trk-{}-{}-{payment_marker}-{contact_hash}\"",
        order.public_id, order.version
    );

    let items = raw_items
        .into_iter()
        .map(|item| {
            let options = raw_options
                .iter()
                .filter(|opt| opt.order_item_id == item.id)
                .map(|opt| TrackingItemOption {
                    group_name: opt.group_name.clone(),
                    group_name_en: opt.group_name_en.clone(),
                    group_name_es: opt.group_name_es.clone(),
                    name: opt.name.clone(),
                    name_en: opt.name_en.clone(),
                    name_es: opt.name_es.clone(),
                    unit_price_centavos: opt.unit_price_centavos,
                    quantity: opt.quantity,
                })
                .collect();
            TrackingItem {
                name: item.name,
                name_en: item.name_en,
                name_es: item.name_es,
                variation_name: item.variation_name,
                variation_name_en: item.variation_name_en,
                variation_name_es: item.variation_name_es,
                unit_price_centavos: item.unit_price_centavos,
                quantity: item.quantity,
                line_total_centavos: item.line_total_centavos,
                options,
            }
        })
        .collect();

    let ts = |dt: Option<DateTime<Utc>>| dt.map(|d| d.to_rfc3339());
    let view = StorefrontTrackingView {
        order_id: order.public_id,
        order_number: order.order_number,
        channel: order.channel.as_str().to_owned(),
        fulfillment: order.fulfillment.as_str().to_owned(),
        state: order.state.as_str().to_owned(),
        version: order.version,
        can_cancel: order.state == OrderState::Placed,
        placed_at: order.placed_at.to_rfc3339(),
        accepted_at: ts(order.accepted_at),
        promised_at: ts(order.promised_at),
        preparing_at: ts(order.preparing_at),
        ready_at: ts(order.ready_at),
        completed_at: ts(order.completed_at),
        rejected_at: ts(order.rejected_at),
        cancelled_at: ts(order.cancelled_at),
        reason_code: order.reason_code,
        neighborhood_label: meta.map(|m| m.neighborhood_label).unwrap_or_default(),
        contact_whatsapp: storefront.contact_whatsapp,
        totals: TrackingTotals {
            items_subtotal_centavos: order.items_subtotal_centavos,
            discount_centavos: order.discount_centavos,
            delivery_fee_centavos: order.delivery_fee_centavos,
            total_centavos: order.total_centavos,
        },
        payment: payment.map(|p| TrackingPayment {
            method: p.method.as_str().to_owned(),
            state: p.state.as_str().to_owned(),
            amount_centavos: p.amount_centavos,
            change_for_centavos: p.change_for_centavos,
        }),
        items,
    };
    Ok((view, etag))
}

pub async fn cancel_order(
    pool: &SqlitePool,
    catalog: &CommercialCatalog,
    enforcement: Enforcement,
    slug: &str,
    public_id: &str,
    cancel_token: &str,
    now: DateTime<Utc>,
) -> AppResult<Order> {
    validate_public_id(public_id)?;
    let est = delivery_storefronts::find_establishment_by_slug(pool, slug)
        .await?
        .ok_or(AppError::NotFound("delivery.storefront_not_found"))?;
    authorize_with_catalog(
        pool,
        catalog,
        enforcement,
        &est.id,
        ActorRole::Consumer,
        Operation::DeliveryOrderCancel,
        billing_now().date_naive(),
    )
    .await?
    .map_err(|_| AppError::Forbidden("delivery.ordering_unavailable"))?;

    let order = delivery_orders::find_order_by_public_id(pool, &est.id, public_id)
        .await?
        .ok_or(AppError::NotFound("error.not_found"))?;
    let meta = delivery_orders::get_metadata(pool, &est.id, &order.id)
        .await?
        .ok_or(AppError::Forbidden("error.forbidden"))?;
    let candidate_hash = sha256_hex(cancel_token.trim());
    if !constant_time_eq(&meta.cancel_token_hash, &candidate_hash) {
        return Err(AppError::Forbidden("error.forbidden"));
    }

    order_transitions::transition(
        pool,
        &est.id,
        &order.id,
        TransitionOrder {
            expected_state: OrderState::Placed,
            expected_version: order.version,
            action: OrderAction::Cancel {
                reason: CancelReason::ConsumerRequest,
            },
            actor: Actor::Consumer,
            actor_account_id: None,
            event_context: OrderEventContext {
                correlation_id: Some(Uuid::now_v7()),
                causation_id: None,
            },
        },
        now,
    )
    .await
}

fn validate_public_id(public_id: &str) -> AppResult<()> {
    if public_id.len() == 32 && public_id.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        Ok(())
    } else {
        Err(AppError::NotFound("error.not_found"))
    }
}
