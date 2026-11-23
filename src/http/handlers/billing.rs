use axum::Json;
use axum::extract::{Form, FromRequest, Request, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Redirect, Response};
use axum_extra::extract::CookieJar;
use serde::Deserialize;
use uuid::Uuid;

use crate::app::AppState;
use crate::application::billing;
use crate::domain::account::AccountRole;
use crate::domain::billing::billing_now;
use crate::error::{AppError, AppResult};
use crate::http::handlers::dashboard::CsrfForm;
use crate::http::middleware::auth::{TenantContext, verify_csrf};
use crate::http::middleware::locale::RequestLocale;
use crate::http::responses::{take_flash, with_flash_redirect};
use crate::infrastructure::asaas::{AsaasClient, BILLING_PATH, Webhook};
use crate::infrastructure::database::queries::{billing as queries, is_establishment_owner};
use crate::infrastructure::security::constant_time_eq;
use crate::rendering::billing::{BillingTemplate, plan_price};
use crate::rendering::{PageMeta, render_html};

pub async fn page(
    State(state): State<AppState>,
    RequestLocale(locale): RequestLocale,
    tenant: TenantContext,
    jar: CookieJar,
) -> AppResult<Response> {
    let client = configured_client(&state)?;
    let env = client.config().environment.as_str();
    let est = &tenant.establishment.id;
    let is_owner = is_establishment_owner(&state.pool, est, &tenant.auth.account.id).await?;
    let (current, paid_through) = tokio::try_join!(
        queries::current_checkout(&state.pool, env, est),
        queries::paid_through(&state.pool, env, est, billing_now().date_naive()),
    )?;
    let status_key = match (current.as_ref(), paid_through) {
        (None, Some(_)) => "billing.status_ends",
        (Some(row), _) if row.subscription_status == "inactive" => "billing.status_inactive",
        (Some(_), Some(_)) => "billing.status_active",
        (Some(row), None) if row.checkout_status == "open" => "billing.status_checkout",
        (Some(row), None) if row.checkout_status == "creating" => "billing.status_creating",
        (Some(row), None) if row.subscription_id.is_some() => "billing.status_awaiting_payment",
        (Some(_), None) => "billing.status_verifying",
        (None, None) => "billing.status_none",
    };
    let (jar, flash) = take_flash(jar);
    let tpl = BillingTemplate {
        page: PageMeta::new(locale, "billing.heading", flash),
        brand_name: tenant.establishment.name,
        has_establishment: true,
        has_theme: true,
        is_admin: tenant.auth.account.role == AccountRole::Admin,
        active_nav: "billing",
        csrf_token: tenant.auth.csrf_token,
        plan_price: plan_price(),
        paid_through,
        status_key,
        is_owner,
        start_checkout: current.is_none() && paid_through.is_none(),
        checkout_url: current
            .as_ref()
            .filter(|row| row.checkout_status == "open")
            .and_then(|row| row.checkout_id.as_deref())
            .and_then(|id| Uuid::parse_str(id).ok())
            .map(|id| client.checkout_url(id).to_string()),
        can_reconcile: current
            .as_ref()
            .is_some_and(|row| row.checkout_status == "paid" && row.subscription_id.is_none()),
        cancellable_id: current.and_then(|row| row.subscription_id),
    };
    Ok((jar, render_html(&tpl, StatusCode::OK)?).into_response())
}

pub async fn checkout(
    State(state): State<AppState>,
    tenant: TenantContext,
    Form(form): Form<CsrfForm>,
) -> AppResult<Redirect> {
    verify_csrf(&tenant.auth, &form._csrf)?;
    let client = configured_client(&state)?;
    authorize_mutation(&state, &tenant).await?;
    billing::start_checkout(
        &state.pool,
        client,
        &state.config.public_base_url,
        &tenant.establishment.id,
    )
    .await?;
    Ok(Redirect::to(BILLING_PATH))
}

pub async fn reconcile(
    State(state): State<AppState>,
    tenant: TenantContext,
    jar: CookieJar,
    Form(form): Form<CsrfForm>,
) -> AppResult<Response> {
    verify_csrf(&tenant.auth, &form._csrf)?;
    let client = configured_client(&state)?;
    authorize_mutation(&state, &tenant).await?;
    let linked = billing::reconcile_checkout(&state.pool, client, &tenant.establishment.id).await?;
    Ok(with_flash_redirect(
        jar,
        BILLING_PATH,
        !linked,
        if linked {
            "billing.reconciled"
        } else {
            "billing.status_verifying"
        },
        state.config.cookie_secure,
    ))
}

#[derive(Deserialize)]
pub struct CancelForm {
    _csrf: String,
    subscription: String,
    confirm: Option<String>,
}

pub async fn cancel(
    State(state): State<AppState>,
    tenant: TenantContext,
    jar: CookieJar,
    Form(form): Form<CancelForm>,
) -> AppResult<Response> {
    verify_csrf(&tenant.auth, &form._csrf)?;
    let client = configured_client(&state)?;
    authorize_mutation(&state, &tenant).await?;
    if form.confirm.as_deref() != Some("yes") {
        return Err(AppError::Validation("error.billing_confirm_cancellation"));
    }
    billing::cancel_subscription(
        &state.pool,
        client,
        &tenant.establishment.id,
        &form.subscription,
    )
    .await?;
    Ok(with_flash_redirect(
        jar,
        BILLING_PATH,
        false,
        "billing.cancellation_saved",
        state.config.cookie_secure,
    ))
}

pub async fn webhook(State(state): State<AppState>, request: Request) -> Response {
    let client = match configured_client(&state) {
        Ok(client) => client,
        Err(error) => return error.into_response(),
    };
    let authorized = request
        .headers()
        .get("asaas-access-token")
        .and_then(|value| value.to_str().ok())
        .is_some_and(|value| constant_time_eq(client.config().webhook_token(), value));
    if !authorized {
        return StatusCode::UNAUTHORIZED.into_response();
    }
    let Json(event) = match Json::<Webhook>::from_request(request, &state).await {
        Ok(event) => event,
        Err(rejection) => return rejection.status().into_response(),
    };
    match billing::process_webhook(&state.pool, client.config().environment.as_str(), event).await {
        Ok(()) => StatusCode::OK.into_response(),
        Err(error) => error.into_response(),
    }
}

fn configured_client(state: &AppState) -> AppResult<&AsaasClient> {
    state
        .asaas
        .as_ref()
        .ok_or(AppError::NotFound("error.not_found"))
}

async fn authorize_mutation(state: &AppState, tenant: &TenantContext) -> AppResult<()> {
    if !is_establishment_owner(
        &state.pool,
        &tenant.establishment.id,
        &tenant.auth.account.id,
    )
    .await?
    {
        return Err(AppError::Forbidden("error.billing_owner_required"));
    }
    if state
        .rate_limiters
        .billing_establishment
        .check_key(&tenant.establishment.id)
        .is_err()
    {
        return Err(AppError::RateLimited);
    }
    Ok(())
}
