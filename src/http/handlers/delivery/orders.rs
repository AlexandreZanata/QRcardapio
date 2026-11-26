use axum::Form;
use axum::extract::{Path, Query, State};
use axum::http::header::{CACHE_CONTROL, CONTENT_DISPOSITION, CONTENT_TYPE, ETAG, VARY};
use axum::http::{HeaderMap, StatusCode};
use axum::response::{IntoResponse, Response};
use axum_extra::extract::CookieJar;
use chrono::{NaiveDate, Utc};
use serde::Deserialize;
use sha2::{Digest, Sha256};

use crate::app::AppState;
use crate::application::delivery::access::staff_access;
use crate::application::delivery::orders as order_ops;
use crate::domain::account::AccountRole;
use crate::domain::commerce::order::Fulfillment;
use crate::domain::commerce::state_machine::{OrderPaymentState, OrderState};
use crate::domain::entitlements::MemberRole;
use crate::domain::product::parse_price_centavos;
use crate::error::{AppError, AppResult};
use crate::http::middleware::auth::{TenantContext, verify_csrf};
use crate::http::middleware::locale::RequestLocale;
use crate::http::responses::{check_etag_not_modified, take_flash, with_flash_redirect};
use crate::i18n::{format_price, pick_localized_text};
use crate::infrastructure::database::queries::{
    delivery_exports, delivery_orders, order_customers, order_payments,
};
use crate::rendering::delivery::{
    DeliveryOrderListTemplate, DeliveryOrdersTemplate, OrderCenterCardView, OrderCenterItemView,
};
use crate::rendering::{PageMeta, render_html};

const ORDERS_PATH: &str = "/dashboard/delivery/orders";

#[derive(Deserialize)]
pub struct OrdersFilterQuery {
    #[serde(default)]
    pub state: Option<String>,
    #[serde(default)]
    pub date: Option<String>,
    #[serde(default)]
    pub fragment: Option<String>,
}

#[derive(Deserialize)]
pub struct TransitionForm {
    pub _csrf: String,
    pub to: String,
    pub expected_version: i64,
    #[serde(default)]
    pub reason_code: String,
    #[serde(default)]
    pub prep_minutes: Option<i64>,
}

#[derive(Deserialize)]
pub struct ConfirmPaymentForm {
    pub _csrf: String,
    pub method: String,
    pub expected_version: i64,
}

#[derive(Deserialize)]
pub struct RefundForm {
    pub _csrf: String,
    pub payment_id: String,
    pub amount: String,
    pub reason_code: String,
}

pub async fn orders_page(
    State(state): State<AppState>,
    RequestLocale(locale): RequestLocale,
    tenant: TenantContext,
    Query(query): Query<OrdersFilterQuery>,
    headers: HeaderMap,
    jar: CookieJar,
) -> AppResult<Response> {
    let est = &tenant.establishment;
    let access = staff_access(
        &state.pool,
        &state.commercial_catalog,
        state.billing.enforcement(),
        &est.id,
        tenant.role,
    )
    .await?;
    if !access.orders {
        return Err(AppError::Forbidden("error.module_unavailable"));
    }

    let state_filter = order_state_filter(query.state.as_deref())?;
    let date_filter = date_filter(query.date.as_deref())?;

    let fragment_etag = if query.fragment.as_deref() == Some("1") {
        let revision = delivery_orders::center_etag_summary(
            &state.pool,
            &est.id,
            state_filter.as_deref(),
            date_filter.as_deref(),
        )
        .await?;
        let scope = format!(
            "{revision}|{}|{}|{}|{}|{:?}|{:?}",
            est.id,
            tenant.role.as_str(),
            tenant.auth.csrf_token,
            locale.code(),
            state_filter,
            date_filter
        );
        let etag = format!("\"{}\"", hex::encode(Sha256::digest(scope)));
        if let Some(response) = check_etag_not_modified(&headers, &etag) {
            return Ok(response);
        }
        Some(etag)
    } else {
        None
    };
    let raw_orders = delivery_orders::list_orders_for_center(
        &state.pool,
        &est.id,
        state_filter.as_deref(),
        date_filter.as_deref(),
    )
    .await?;

    let mut cards = Vec::with_capacity(raw_orders.len());
    for order in raw_orders {
        let meta = delivery_orders::get_metadata(&state.pool, &est.id, &order.id).await?;
        let payment = order_payments::latest_for_order(&state.pool, &est.id, &order.id).await?;
        let raw_items = delivery_orders::list_order_items(&state.pool, &est.id, &order.id).await?;
        let raw_opts =
            delivery_orders::list_order_item_options(&state.pool, &est.id, &order.id).await?;
        let addr = if order.fulfillment == Fulfillment::Delivery {
            delivery_orders::get_delivery_address(&state.pool, &est.id, &order.id).await?
        } else {
            None
        };

        let items = raw_items
            .into_iter()
            .map(|item| {
                let options_summary = raw_opts
                    .iter()
                    .filter(|opt| opt.order_item_id == item.id)
                    .map(|opt| {
                        let name =
                            pick_localized_text(locale, &opt.name, &opt.name_en, &opt.name_es);
                        format!("{}x {name}", opt.quantity)
                    })
                    .collect::<Vec<_>>()
                    .join(", ");
                let var_name = item.variation_name.as_deref().map(|v| {
                    pick_localized_text(
                        locale,
                        v,
                        item.variation_name_en.as_deref().unwrap_or(""),
                        item.variation_name_es.as_deref().unwrap_or(""),
                    )
                    .to_owned()
                });
                OrderCenterItemView {
                    name: pick_localized_text(locale, &item.name, &item.name_en, &item.name_es)
                        .to_owned(),
                    variation_name: var_name,
                    quantity: item.quantity,
                    line_total_display: format_price(item.line_total_centavos, locale),
                    options_summary,
                    note: item.note,
                }
            })
            .collect();

        let neighborhood_label = meta
            .as_ref()
            .map(|m| m.neighborhood_label.clone())
            .unwrap_or_default();
        let (address_summary, customer_name, customer_phone) = match addr {
            Some(address) => {
                let address_summary = if address.street.is_empty() {
                    String::new()
                } else {
                    [
                        format!("{}, {}", address.street, address.street_number),
                        address.complement,
                        address.neighborhood_label,
                        format!("{}/{}", address.city, address.uf),
                        address.cep,
                        address.reference,
                    ]
                    .into_iter()
                    .filter(|part| !part.is_empty())
                    .collect::<Vec<_>>()
                    .join(" · ")
                };
                (
                    address_summary,
                    address.recipient_name,
                    address.recipient_phone,
                )
            }
            None => {
                let (name, phone) = order_customers::get_contact(&state.pool, &est.id, &order.id)
                    .await?
                    .unwrap_or_default();
                (String::new(), name, phone)
            }
        };

        let is_manager = matches!(tenant.role, MemberRole::Owner | MemberRole::Manager);
        let can_refund = is_manager
            && payment.as_ref().is_some_and(|p| {
                matches!(
                    p.state,
                    OrderPaymentState::Confirmed | OrderPaymentState::PartiallyRefunded
                ) && (p.amount_centavos - p.refunded_centavos) > 0
            });
        let payment_confirmed = payment.as_ref().map_or(0, |p| {
            if matches!(
                p.state,
                OrderPaymentState::Confirmed
                    | OrderPaymentState::PartiallyRefunded
                    | OrderPaymentState::Refunded
            ) {
                p.amount_centavos
            } else {
                0
            }
        });
        let payment_pending_on_completed = order.state == OrderState::Completed
            && payment
                .as_ref()
                .is_some_and(|p| p.state == OrderPaymentState::Pending);

        cards.push(OrderCenterCardView {
            id: order.id.clone(),
            public_id: order.public_id.clone(),
            order_number: order.order_number,
            business_date: order.business_date.to_string(),
            channel: order.channel.as_str().to_owned(),
            fulfillment: order.fulfillment.as_str().to_owned(),
            state: order.state.as_str().to_owned(),
            version: order.version,
            total_display: format_price(order.total_centavos, locale),
            discount_display: format_price(order.discount_centavos, locale),
            fee_display: if order.delivery_fee_centavos > 0 {
                format_price(order.delivery_fee_centavos, locale)
            } else {
                String::new()
            },
            neighborhood_label,
            address_summary,
            customer_name,
            customer_phone,
            change_for_display: payment
                .as_ref()
                .and_then(|p| p.change_for_centavos)
                .map(|value| format_price(value, locale)),
            coupon_code: meta.as_ref().and_then(|m| m.coupon_code.clone()),
            order_note: meta.map(|m| m.order_note).unwrap_or_default(),
            payment_id: payment.as_ref().map(|p| p.id.clone()),
            payment_version: payment.as_ref().map(|p| p.version),
            payment_method: payment.as_ref().map(|p| p.method.as_str().to_owned()),
            payment_state: payment.as_ref().map(|p| p.state.as_str().to_owned()),
            payment_confirmed_centavos: payment_confirmed,
            payment_refunded_centavos: payment.as_ref().map_or(0, |p| p.refunded_centavos),
            payment_refund_due_centavos: payment.as_ref().map_or(0, |p| p.refund_due_centavos),
            payment_pending_on_completed,
            items,
            can_accept: order.state == OrderState::Placed,
            can_reject: order.state == OrderState::Placed,
            can_prepare: order.state == OrderState::Accepted && is_manager,
            can_ready: matches!(order.state, OrderState::Accepted | OrderState::Preparing)
                && (order.state == OrderState::Accepted || is_manager),
            can_complete: order.state == OrderState::Ready,
            can_cancel: matches!(
                order.state,
                OrderState::Accepted | OrderState::Preparing | OrderState::Ready
            ) && is_manager,
            can_confirm_payment: payment
                .as_ref()
                .is_some_and(|p| p.state == OrderPaymentState::Pending),
            can_refund,
        });
    }

    if let Some(etag) = fragment_etag {
        let tpl = DeliveryOrderListTemplate {
            page: PageMeta::new(locale, "delivery.orders.page_title", None),
            csrf_token: tenant.auth.csrf_token,
            orders: cards,
        };
        let html = askama::Template::render(&tpl)
            .map_err(|err| AppError::Internal(format!("order center render failed: {err}")))?;
        return Ok((
            [(ETAG, etag), (VARY, "Accept-Language, Cookie".into())],
            axum::response::Html(html),
        )
            .into_response());
    }

    let (jar, flash) = take_flash(jar);
    let tpl = DeliveryOrdersTemplate {
        page: PageMeta::new(locale, "delivery.orders.page_title", flash),
        brand_name: est.name.clone(),
        csrf_token: tenant.auth.csrf_token,
        is_admin: tenant.auth.account.role == AccountRole::Admin,
        has_establishment: true,
        has_theme: true,
        active_nav: "delivery_orders",
        state_filter: state_filter.unwrap_or_default(),
        date_filter: date_filter.unwrap_or_default(),
        orders: cards,
        can_settings: access.storefront_management,
        push_key: state
            .staff_push_public_key
            .clone()
            .filter(|_| access.new_orders),
        push_scope: format!("{}-{}", est.id, tenant.auth.account.id),
    };

    Ok((jar, render_html(&tpl, StatusCode::OK)?).into_response())
}

pub async fn orders_csv(
    State(state): State<AppState>,
    tenant: TenantContext,
    Query(query): Query<OrdersFilterQuery>,
) -> AppResult<Response> {
    let est = &tenant.establishment;
    let access = staff_access(
        &state.pool,
        &state.commercial_catalog,
        state.billing.enforcement(),
        &est.id,
        tenant.role,
    )
    .await?;
    if !access.orders {
        return Err(AppError::Forbidden("error.module_unavailable"));
    }

    let state_filter = order_state_filter(query.state.as_deref())?;
    let date_filter = date_filter(query.date.as_deref())?;
    let rows = delivery_exports::load(
        &state.pool,
        &est.id,
        state_filter.as_deref(),
        date_filter.as_deref(),
    )
    .await?;
    let csv = order_ops::render_orders_csv(&rows);
    Ok((
        [
            (CONTENT_TYPE, "text/csv; charset=utf-8"),
            (CONTENT_DISPOSITION, "attachment; filename=\"orders.csv\""),
            (CACHE_CONTROL, "no-store"),
        ],
        csv,
    )
        .into_response())
}

pub async fn transition_submit(
    State(state): State<AppState>,
    tenant: TenantContext,
    Path(id): Path<String>,
    jar: CookieJar,
    Form(form): Form<TransitionForm>,
) -> AppResult<Response> {
    verify_csrf(&tenant.auth, &form._csrf)?;
    let reason = (!form.reason_code.trim().is_empty()).then_some(form.reason_code.as_str());
    order_ops::transition_staff_order(
        &state.pool,
        &state.commercial_catalog,
        state.billing.enforcement(),
        &tenant.establishment.id,
        tenant.role,
        &tenant.auth.account.id,
        &id,
        &form.to,
        form.expected_version,
        reason,
        form.prep_minutes,
        Utc::now(),
    )
    .await?;
    Ok(with_flash_redirect(
        jar,
        ORDERS_PATH,
        false,
        "delivery.flash.order_updated",
        state.config.cookie_secure,
    ))
}

pub async fn confirm_payment_submit(
    State(state): State<AppState>,
    tenant: TenantContext,
    Path((order_id, payment_id)): Path<(String, String)>,
    jar: CookieJar,
    Form(form): Form<ConfirmPaymentForm>,
) -> AppResult<Response> {
    verify_csrf(&tenant.auth, &form._csrf)?;
    order_ops::confirm_staff_payment(
        &state.pool,
        &state.commercial_catalog,
        state.billing.enforcement(),
        &tenant.establishment.id,
        tenant.role,
        &tenant.auth.account.id,
        &order_id,
        &payment_id,
        &form.method,
        form.expected_version,
        Utc::now(),
    )
    .await?;
    Ok(with_flash_redirect(
        jar,
        ORDERS_PATH,
        false,
        "delivery.flash.payment_confirmed",
        state.config.cookie_secure,
    ))
}

pub async fn refund_submit(
    State(state): State<AppState>,
    tenant: TenantContext,
    Path(order_id): Path<String>,
    jar: CookieJar,
    Form(form): Form<RefundForm>,
) -> AppResult<Response> {
    verify_csrf(&tenant.auth, &form._csrf)?;
    let amount_centavos = parse_price_centavos(&form.amount)?;
    order_ops::record_staff_refund(
        &state.pool,
        &state.commercial_catalog,
        state.billing.enforcement(),
        &tenant.establishment.id,
        tenant.role,
        &tenant.auth.account.id,
        &order_id,
        form.payment_id.trim(),
        amount_centavos,
        &form.reason_code,
        Utc::now(),
    )
    .await?;
    Ok(with_flash_redirect(
        jar,
        ORDERS_PATH,
        false,
        "delivery.flash.refund_recorded",
        state.config.cookie_secure,
    ))
}

fn order_state_filter(raw: Option<&str>) -> AppResult<Option<String>> {
    raw.filter(|value| !value.is_empty())
        .map(|value| {
            value
                .parse::<OrderState>()
                .map(|state| state.as_str().to_owned())
                .map_err(|_| AppError::Validation("delivery.orders.invalid_filter"))
        })
        .transpose()
}

fn date_filter(raw: Option<&str>) -> AppResult<Option<String>> {
    raw.filter(|value| !value.is_empty())
        .map(|value| {
            NaiveDate::parse_from_str(value, "%Y-%m-%d")
                .map(|date| date.to_string())
                .map_err(|_| AppError::Validation("delivery.orders.invalid_filter"))
        })
        .transpose()
}
