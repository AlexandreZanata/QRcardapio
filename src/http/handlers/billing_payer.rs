//! Payer identification form and the documents of the current Inter charge (Pix QR, boleto PDF).

use axum::extract::{Form, State};
use axum::http::header::{CACHE_CONTROL, CONTENT_DISPOSITION, CONTENT_TYPE};
use axum::http::{HeaderValue, StatusCode};
use axum::response::{IntoResponse, Response};
use axum_extra::extract::CookieJar;
use chrono::Utc;
use serde::Deserialize;

use crate::app::AppState;
use crate::application::billing::provider_error;
use crate::domain::payer::{PayerInput, PayerProfile};
use crate::error::{AppError, AppResult};
use crate::http::handlers::billing::authorize_mutation;
use crate::http::middleware::auth::{TenantContext, verify_csrf};
use crate::http::responses::with_flash_redirect;
use crate::infrastructure::asaas::BILLING_PATH;
use crate::infrastructure::database::queries::{billing_inter, billing_payers};
use crate::qr::generate_svg;

#[derive(Deserialize)]
pub struct PayerForm {
    _csrf: String,
    document: String,
    legal_name: String,
    email: String,
    postal_code: String,
    street: String,
    number: String,
    #[serde(default)]
    complement: String,
    district: String,
    city: String,
    state: String,
}

pub async fn save(
    State(state): State<AppState>,
    tenant: TenantContext,
    jar: CookieJar,
    Form(form): Form<PayerForm>,
) -> AppResult<Response> {
    verify_csrf(&tenant.auth, &form._csrf)?;
    authorize_mutation(&state, &tenant).await?;
    let payer = PayerProfile::parse(PayerInput {
        document: &form.document,
        legal_name: &form.legal_name,
        email: &form.email,
        postal_code: &form.postal_code,
        street: &form.street,
        number: &form.number,
        complement: &form.complement,
        district: &form.district,
        city: &form.city,
        state: &form.state,
    })?;
    billing_payers::upsert(
        &state.pool,
        &tenant.establishment.id,
        &payer,
        &tenant.auth.account.id,
        &Utc::now().to_rfc3339(),
    )
    .await?;
    Ok(with_flash_redirect(
        jar,
        BILLING_PATH,
        false,
        "billing.payer_saved",
        state.config.cookie_secure,
    ))
}

async fn current_charge(
    state: &AppState,
    tenant: &TenantContext,
) -> AppResult<billing_inter::InterCharge> {
    let client = state
        .billing
        .inter
        .as_ref()
        .ok_or(AppError::NotFound("error.not_found"))?;
    billing_inter::latest_for_establishment(
        &state.pool,
        client.config().environment.as_str(),
        &tenant.establishment.id,
    )
    .await?
    .ok_or(AppError::NotFound("error.not_found"))
}

fn private(mut response: Response, content_type: &'static str) -> Response {
    let headers = response.headers_mut();
    headers.insert(CONTENT_TYPE, HeaderValue::from_static(content_type));
    headers.insert(CACHE_CONTROL, HeaderValue::from_static("no-store"));
    response
}

/// QR Code of the current charge's Pix copy-and-paste code (rendered locally).
pub async fn pix_svg(State(state): State<AppState>, tenant: TenantContext) -> AppResult<Response> {
    let charge = current_charge(&state, &tenant).await?;
    let code = charge
        .pix_copy_paste
        .ok_or(AppError::NotFound("error.not_found"))?;
    let svg = generate_svg(&code)?;
    Ok(private(
        (StatusCode::OK, svg).into_response(),
        "image/svg+xml; charset=utf-8",
    ))
}

/// Boleto PDF fetched from Inter on demand (never stored).
pub async fn boleto_pdf(
    State(state): State<AppState>,
    tenant: TenantContext,
) -> AppResult<Response> {
    let charge = current_charge(&state, &tenant).await?;
    let code = charge
        .request_code
        .ok_or(AppError::NotFound("error.not_found"))?;
    let client = state
        .billing
        .inter
        .as_ref()
        .ok_or(AppError::NotFound("error.not_found"))?;
    let pdf = client.charge_pdf(&code).await.map_err(provider_error)?;
    let mut response = private((StatusCode::OK, pdf).into_response(), "application/pdf");
    response.headers_mut().insert(
        CONTENT_DISPOSITION,
        HeaderValue::from_static("attachment; filename=\"boleto-qrcardapio.pdf\""),
    );
    Ok(response)
}
