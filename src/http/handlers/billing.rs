use axum::extract::{Form, Query, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Redirect, Response};
use axum_extra::extract::CookieJar;
use serde::Deserialize;
use uuid::Uuid;

use crate::app::AppState;
use crate::application::{billing, commercial};
use crate::domain::account::AccountRole;
use crate::domain::billing::{BillingPlan, billing_now};
use crate::domain::commercial::ENTRY_OFFER;
use crate::domain::entitlements::{Operation, OperationalStatus};
use crate::domain::payments::{PaymentMethod, PaymentProvider};
use crate::error::{AppError, AppResult};
use crate::http::handlers::{dashboard::CsrfForm, onboarding::PlanIntent};
use crate::http::middleware::auth::{TenantContext, verify_csrf};
use crate::http::middleware::locale::RequestLocale;
use crate::http::responses::{take_flash, with_flash_redirect};
use crate::i18n::{Locale, format_price};
use crate::infrastructure::asaas::BILLING_PATH;
use crate::infrastructure::database::queries::{
    billing as queries, billing_inter, billing_payers, entitlements,
};
use crate::rendering::billing::{BillingTemplate, ChargeView, MethodOffer, ModuleCard, PlanOffer};
use crate::rendering::{PageMeta, render_html};

pub async fn page(
    State(state): State<AppState>,
    RequestLocale(locale): RequestLocale,
    tenant: TenantContext,
    jar: CookieJar,
    Query(intent): Query<PlanIntent>,
) -> AppResult<Response> {
    let env = billing_environment(&state)?;
    let est = &tenant.establishment.id;
    let is_owner = Operation::ManageBilling.permits(tenant.role);
    let today = billing_now().date_naive();
    let status = entitlements::operational_status(&state.pool, est).await?;
    let suspended = OperationalStatus::parse(&status) != Some(OperationalStatus::Active);
    let (current, coverage, modules) = tokio::try_join!(
        queries::current_checkout(&state.pool, env, est),
        queries::paid_coverage(&state.pool, env, est, today),
        commercial::module_overview(
            &state.pool,
            state.billing.enforcement(),
            est,
            suspended,
            today,
        ),
    )?;
    let paid_through = coverage.as_ref().map(|row| row.paid_through);
    let selected = intent.selected().unwrap_or_default();
    let (plan, price) = current
        .as_ref()
        .map(|row| (row.plan_months, row.price_centavos))
        .or_else(|| {
            coverage
                .as_ref()
                .map(|row| (row.plan_months, row.price_centavos))
        })
        .and_then(|(months, price)| BillingPlan::from_months(months).map(|plan| (plan, price)))
        .unwrap_or_else(|| (selected, selected.price()));
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
    let payer = billing_payers::find(&state.pool, est).await?;
    let inter_charge = match current.as_ref() {
        Some(row) if row.provider == PaymentProvider::Inter.as_str() => {
            billing_inter::latest_for_establishment(&state.pool, env, est)
                .await?
                .filter(|charge| charge.billing_reference == row.reference)
                .map(|charge| charge_view(&charge))
        }
        _ => None,
    };
    let methods = state.billing.methods();
    let start_checkout = current.is_none() && paid_through.is_none();
    let checkout_url = current
        .as_ref()
        .filter(|row| row.checkout_status == "open")
        .and_then(|row| hosted_checkout_url(&state, row));
    let (jar, flash) = take_flash(jar);
    let tpl = BillingTemplate {
        page: PageMeta::new(locale, "billing.heading", flash),
        brand_name: tenant.establishment.name,
        has_establishment: true,
        has_theme: true,
        is_admin: tenant.auth.account.role == AccountRole::Admin,
        active_nav: "billing",
        csrf_token: tenant.auth.csrf_token,
        plan: PlanOffer::new(plan, price),
        plans: PlanOffer::catalog(),
        paid_through,
        status_key,
        is_owner,
        start_checkout,
        checkout_url,
        can_reconcile: current
            .as_ref()
            .is_some_and(|row| row.checkout_status == "paid" && row.subscription_id.is_none()),
        cancellable_id: current.and_then(|row| row.subscription_id),
        modules: modules.into_iter().map(ModuleCard::from_overview).collect(),
        show_payer_form: is_owner && start_checkout && methods.contains(&PaymentMethod::PixBoleto),
        methods: methods
            .into_iter()
            .map(|method| MethodOffer::new(method, payer.is_some()))
            .collect(),
        inter_charge,
        payer_summary: payer.map(|p| format!("{} ({})", p.legal_name, p.masked_document())),
    };
    Ok((jar, render_html(&tpl, StatusCode::OK)?).into_response())
}

#[derive(Deserialize)]
pub struct CheckoutForm {
    _csrf: String,
    plan: Option<String>,
    offer: Option<String>,
    method: Option<String>,
}

pub async fn checkout(
    State(state): State<AppState>,
    tenant: TenantContext,
    Form(form): Form<CheckoutForm>,
) -> AppResult<Redirect> {
    verify_csrf(&tenant.auth, &form._csrf)?;
    billing_environment(&state)?;
    authorize_mutation(&state, &tenant).await?;
    let method = PaymentMethod::parse(form.method.as_deref().unwrap_or("card"))
        .ok_or(AppError::Validation("error.payment_method_unavailable"))?;
    let plan = match form.plan.as_deref() {
        None => BillingPlan::Monthly,
        Some(key) => {
            BillingPlan::from_key(key).ok_or(AppError::Validation("error.billing_plan_invalid"))?
        }
    };
    // Amounts are never accepted from the browser: only offer codes and a cycle.
    let offer = commercial::parse_offer(form.offer.as_deref().unwrap_or(ENTRY_OFFER))?;
    let quote = commercial::quote_checkout(&[offer], plan)?;
    billing::start_checkout(
        &state.pool,
        &state.billing,
        &state.config.public_base_url,
        &tenant.establishment.id,
        method,
        &quote,
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
    billing_environment(&state)?;
    authorize_mutation(&state, &tenant).await?;
    let linked =
        billing::reconcile_checkout(&state.pool, &state.billing, &tenant.establishment.id).await?;
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
    billing_environment(&state)?;
    authorize_mutation(&state, &tenant).await?;
    if form.confirm.as_deref() != Some("yes") {
        return Err(AppError::Validation("error.billing_confirm_cancellation"));
    }
    billing::cancel_subscription(
        &state.pool,
        &state.billing,
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

fn billing_environment(state: &AppState) -> AppResult<&'static str> {
    state
        .billing
        .environment()
        .ok_or(AppError::NotFound("error.not_found"))
}

/// Hosted checkout link: stored by Stripe, derived from the id for Asaas.
fn hosted_checkout_url(state: &AppState, row: &queries::Checkout) -> Option<String> {
    if let Some(url) = &row.checkout_url {
        return Some(url.clone());
    }
    if row.provider != PaymentProvider::Asaas.as_str() {
        return None;
    }
    let id = Uuid::parse_str(row.checkout_id.as_deref()?).ok()?;
    Some(state.billing.asaas.as_ref()?.checkout_url(id).to_string())
}

fn charge_view(charge: &billing_inter::InterCharge) -> ChargeView {
    let status_key = match charge.status.as_str() {
        "reserved" | "issued" | "EM_PROCESSAMENTO" => "billing.charge_issuing",
        _ => "billing.charge_pending",
    };
    ChargeView {
        amount: format_price(charge.amount_centavos, Locale::PtBr),
        due_date: charge.due_date,
        pix_copy_paste: charge.pix_copy_paste.clone(),
        digitable_line: charge.digitable_line.clone(),
        status_key,
    }
}

pub(crate) async fn authorize_mutation(state: &AppState, tenant: &TenantContext) -> AppResult<()> {
    if !Operation::ManageBilling.permits(tenant.role) {
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
