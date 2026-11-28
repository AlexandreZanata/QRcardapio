use chrono::{DateTime, Duration, Utc};
use sqlx::SqlitePool;
use uuid::Uuid;

use crate::application::commerce::{order_transitions, payments};
use crate::application::entitlements::{
    Denial, Enforcement, authorize_existing_with_catalog, authorize_with_catalog,
};
use crate::domain::billing::billing_now;
use crate::domain::commerce::order::{
    CancelReason, Fulfillment, Order, OrderAction, OrderChannel, OrderEventContext, RejectReason,
    TransitionOrder,
};
use crate::domain::commerce::order_payment::{
    OfflineMethod, OrderPayment, PaymentOperation, RefundReason,
};
use crate::domain::commerce::state_machine::{Actor, OrderState};
use crate::domain::commercial::CommercialCatalog;
use crate::domain::delivery::storefront::DEFAULT_PREP_MINUTES;
use crate::domain::entitlements::{MemberRole, Operation};
use crate::error::{AppError, AppResult};
use crate::infrastructure::database::queries::{
    delivery_exports::OrderExportRow, delivery_orders, order_payments, orders,
};

#[allow(clippy::too_many_arguments)]
pub async fn transition_staff_order(
    pool: &SqlitePool,
    catalog: &CommercialCatalog,
    enforcement: Enforcement,
    est_id: &str,
    role: MemberRole,
    actor_account_id: &str,
    order_id: &str,
    to: &str,
    expected_version: i64,
    reason_code: Option<&str>,
    prep_minutes: Option<i64>,
    now: DateTime<Utc>,
) -> AppResult<Order> {
    let order = storefront_order(pool, est_id, order_id).await?;
    authorize_existing_with_catalog(
        pool,
        catalog,
        enforcement,
        est_id,
        role,
        Operation::DeliveryOrderProgress,
        order.business_date,
        billing_now().date_naive(),
    )
    .await?
    .map_err(Denial::into_error)?;

    let target: OrderState = to
        .trim()
        .parse()
        .map_err(|_| AppError::Conflict("commerce.order_state_conflict"))?;

    // Effective actor intersection (DELIVERY.md §10):
    // - preparing/ready from preparing, and staff cancellations require Owner or Manager.
    let action = match (order.state, target) {
        (OrderState::Placed, OrderState::Accepted) => {
            let delivery = order.fulfillment == Fulfillment::Delivery;
            let (default_prep, zone_eta) = if prep_minutes.is_none() || delivery {
                delivery_orders::acceptance_estimates(pool, est_id, &order.id, delivery).await?
            } else {
                (None, 0)
            };
            let prep = prep_minutes
                .or(default_prep)
                .unwrap_or(DEFAULT_PREP_MINUTES)
                .clamp(1, 240);
            let promised_at = now + Duration::minutes(prep + zone_eta);
            OrderAction::Accept { promised_at }
        }
        (OrderState::Placed, OrderState::Rejected) => {
            if !matches!(
                role,
                MemberRole::Owner | MemberRole::Manager | MemberRole::Cashier
            ) {
                return Err(AppError::Forbidden("error.permission_denied"));
            }
            let reason = parse_staff_reject_reason(reason_code.unwrap_or("other"))?;
            OrderAction::Reject { reason }
        }
        (OrderState::Accepted, OrderState::Preparing) => {
            if !matches!(role, MemberRole::Owner | MemberRole::Manager) {
                return Err(AppError::Forbidden("error.permission_denied"));
            }
            OrderAction::Prepare
        }
        (OrderState::Preparing, OrderState::Ready) => {
            if !matches!(role, MemberRole::Owner | MemberRole::Manager) {
                return Err(AppError::Forbidden("error.permission_denied"));
            }
            OrderAction::MarkReady
        }
        (OrderState::Accepted, OrderState::Ready) => OrderAction::MarkReady,
        (OrderState::Ready, OrderState::Completed) => OrderAction::Complete,
        (
            OrderState::Accepted | OrderState::Preparing | OrderState::Ready,
            OrderState::Cancelled,
        ) => {
            if !matches!(role, MemberRole::Owner | MemberRole::Manager) {
                return Err(AppError::Forbidden("error.permission_denied"));
            }
            let reason = parse_staff_cancel_reason(reason_code.unwrap_or("other"))?;
            OrderAction::Cancel { reason }
        }
        _ => return Err(AppError::Conflict("commerce.order_state_conflict")),
    };

    order_transitions::transition_loaded(
        pool,
        &order,
        TransitionOrder {
            expected_state: order.state,
            expected_version,
            action,
            actor: member_to_actor(role),
            actor_account_id: Some(actor_account_id),
            event_context: OrderEventContext {
                correlation_id: Some(Uuid::now_v7()),
                causation_id: None,
            },
        },
        now,
    )
    .await
}

#[allow(clippy::too_many_arguments)]
pub async fn confirm_staff_payment(
    pool: &SqlitePool,
    catalog: &CommercialCatalog,
    enforcement: Enforcement,
    est_id: &str,
    role: MemberRole,
    actor_account_id: &str,
    order_id: &str,
    payment_id: &str,
    method: &str,
    expected_version: i64,
    now: DateTime<Utc>,
) -> AppResult<OrderPayment> {
    let order = storefront_order(pool, est_id, order_id).await?;
    authorize_existing_with_catalog(
        pool,
        catalog,
        enforcement,
        est_id,
        role,
        Operation::DeliveryOrderProgress,
        order.business_date,
        billing_now().date_naive(),
    )
    .await?
    .map_err(Denial::into_error)?;

    let collected = OfflineMethod::parse(method.trim())
        .ok_or(AppError::Validation("delivery.payment_method_unavailable"))?;
    let current = order_payments::get(pool, est_id, order_id, payment_id)
        .await?
        .ok_or(AppError::NotFound("error.not_found"))?;
    payments::confirm(
        pool,
        PaymentOperation {
            establishment_id: est_id,
            order_id,
            payment_id: &current.id,
            expected_version,
            actor: member_to_actor(role),
            actor_account_id,
            event_context: OrderEventContext {
                correlation_id: Some(Uuid::now_v7()),
                causation_id: None,
            },
        },
        collected,
        now,
    )
    .await
}

#[allow(clippy::too_many_arguments)]
pub async fn record_staff_refund(
    pool: &SqlitePool,
    catalog: &CommercialCatalog,
    enforcement: Enforcement,
    est_id: &str,
    role: MemberRole,
    actor_account_id: &str,
    order_id: &str,
    payment_id: &str,
    amount_centavos: i64,
    reason_code: &str,
    now: DateTime<Utc>,
) -> AppResult<OrderPayment> {
    authorize_with_catalog(
        pool,
        catalog,
        enforcement,
        est_id,
        role,
        Operation::DeliveryOrderRefund,
        billing_now().date_naive(),
    )
    .await?
    .map_err(Denial::into_error)?;

    storefront_order(pool, est_id, order_id).await?;
    let reason = parse_refund_reason(reason_code)?;
    let current = order_payments::get(pool, est_id, order_id, payment_id)
        .await?
        .ok_or(AppError::NotFound("error.not_found"))?;
    payments::refund(
        pool,
        PaymentOperation {
            establishment_id: est_id,
            order_id,
            payment_id: &current.id,
            expected_version: current.version,
            actor: member_to_actor(role),
            actor_account_id,
            event_context: OrderEventContext {
                correlation_id: Some(Uuid::now_v7()),
                causation_id: None,
            },
        },
        amount_centavos,
        reason,
        now,
    )
    .await
}

async fn storefront_order(pool: &SqlitePool, est_id: &str, order_id: &str) -> AppResult<Order> {
    orders::get(pool, est_id, order_id)
        .await?
        .filter(|order| {
            order.catalog_channel == "storefront"
                && matches!(order.channel, OrderChannel::Delivery | OrderChannel::Pickup)
        })
        .ok_or(AppError::NotFound("error.not_found"))
}

pub fn render_orders_csv(rows: &[OrderExportRow]) -> String {
    let mut out = String::from(
        "order_number,business_date,public_id,channel,fulfillment,state,items_subtotal_centavos,discount_centavos,delivery_fee_centavos,total_centavos,payment_method,payment_state,neighborhood_label,placed_at\r\n",
    );
    for order in rows {
        let fields = [
            order.order_number.to_string(),
            order.business_date.clone(),
            order.public_id.clone(),
            order.channel.clone(),
            order.fulfillment.clone(),
            order.state.clone(),
            order.items_subtotal_centavos.to_string(),
            order.discount_centavos.to_string(),
            order.delivery_fee_centavos.to_string(),
            order.total_centavos.to_string(),
            order.payment_method.clone(),
            order.payment_state.clone(),
            order.neighborhood_label.clone(),
            order.placed_at.clone(),
        ];
        for (idx, field) in fields.iter().enumerate() {
            if idx > 0 {
                out.push(',');
            }
            out.push_str(&csv_escape(field));
        }
        out.push_str("\r\n");
    }
    out
}

fn csv_escape(raw: &str) -> String {
    let neutralized = if raw.starts_with(['\t', '\r', '\n'])
        || raw.trim_start().starts_with(['=', '+', '-', '@'])
    {
        format!("'{raw}")
    } else {
        raw.to_owned()
    };
    let escaped = neutralized.replace('"', "\"\"");
    format!("\"{escaped}\"")
}

fn member_to_actor(role: MemberRole) -> Actor {
    match role {
        MemberRole::Owner => Actor::Owner,
        MemberRole::Manager => Actor::Manager,
        MemberRole::Cashier => Actor::Cashier,
        MemberRole::Waiter => Actor::Waiter,
    }
}

fn parse_staff_reject_reason(raw: &str) -> AppResult<RejectReason> {
    match raw.trim() {
        "closed" => Ok(RejectReason::Closed),
        "item_unavailable" => Ok(RejectReason::ItemUnavailable),
        "address_not_served" => Ok(RejectReason::AddressNotServed),
        "too_busy" => Ok(RejectReason::TooBusy),
        "suspected_fraud" => Ok(RejectReason::SuspectedFraud),
        "other" => Ok(RejectReason::Other),
        _ => Err(AppError::Validation("delivery.invalid_reason")),
    }
}

fn parse_staff_cancel_reason(raw: &str) -> AppResult<CancelReason> {
    match raw.trim() {
        "consumer_request" => Ok(CancelReason::ConsumerRequest),
        "item_unavailable" => Ok(CancelReason::ItemUnavailable),
        "delivery_failed" => Ok(CancelReason::DeliveryFailed),
        "payment_issue" => Ok(CancelReason::PaymentIssue),
        "other" => Ok(CancelReason::Other),
        _ => Err(AppError::Validation("delivery.invalid_reason")),
    }
}

fn parse_refund_reason(raw: &str) -> AppResult<RefundReason> {
    match raw.trim() {
        "order_cancelled" => Ok(RefundReason::OrderCancelled),
        "customer_request" => Ok(RefundReason::CustomerRequest),
        "payment_error" => Ok(RefundReason::PaymentError),
        "other" => Ok(RefundReason::Other),
        _ => Err(AppError::Validation("delivery.invalid_reason")),
    }
}
