//! Payment provider webhooks. Each provider authenticates differently; all of them end in the
//! same idempotent `application::billing::apply_events`.

use axum::Json;
use axum::body::Bytes;
use axum::extract::{FromRequest, Path, Request, State};
use axum::http::{HeaderMap, StatusCode};
use axum::response::{IntoResponse, Response};
use serde_json::Value;

use crate::app::AppState;
use crate::application::{billing, billing_inter};
use crate::infrastructure::asaas::Webhook;
use crate::infrastructure::inter::callback_request_codes;
use crate::infrastructure::security::constant_time_eq;
use crate::infrastructure::stripe::signature;
use crate::infrastructure::stripe::webhook::Event as StripeEvent;

/// Asaas: dedicated `asaas-access-token` header compared in constant time.
pub async fn asaas(State(state): State<AppState>, request: Request) -> Response {
    let Some(client) = state.billing.asaas.as_ref() else {
        return StatusCode::NOT_FOUND.into_response();
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
    let env = client.config().environment.as_str();
    match billing::process_webhook(&state.pool, env, event).await {
        Ok(()) => StatusCode::OK.into_response(),
        Err(error) => error.into_response(),
    }
}

/// Stripe: `Stripe-Signature` HMAC over the raw body with a 300 s tolerance.
pub async fn stripe(State(state): State<AppState>, headers: HeaderMap, body: Bytes) -> Response {
    let Some(client) = state.billing.stripe.as_ref() else {
        return StatusCode::NOT_FOUND.into_response();
    };
    let header = headers
        .get("stripe-signature")
        .and_then(|value| value.to_str().ok())
        .unwrap_or_default();
    let now = chrono::Utc::now().timestamp();
    if signature::verify(header, &body, client.config().webhook_secret(), now).is_err() {
        return StatusCode::BAD_REQUEST.into_response();
    }
    let Ok(event) = serde_json::from_slice::<StripeEvent>(&body) else {
        return StatusCode::BAD_REQUEST.into_response();
    };
    match billing::process_stripe_event(&state.pool, client, event).await {
        Ok(()) => StatusCode::OK.into_response(),
        Err(error) => error.into_response(),
    }
}

/// Inter: secret path token (Inter also presents a client certificate that the TLS edge may
/// verify). The payload is only a hint: every announced charge is re-fetched over mTLS.
pub async fn inter(
    State(state): State<AppState>,
    Path(token): Path<String>,
    Json(body): Json<Value>,
) -> Response {
    let Some(client) = state.billing.inter.as_ref() else {
        return StatusCode::NOT_FOUND.into_response();
    };
    if !constant_time_eq(client.config().webhook_token(), &token) {
        return StatusCode::NOT_FOUND.into_response();
    }
    let codes = callback_request_codes(&body);
    match billing_inter::handle_callback(&state.pool, client, &codes).await {
        Ok(()) => StatusCode::OK.into_response(),
        // Non-2xx makes Inter retry (20/30/60/120 minutes); the scheduler also reconciles.
        Err(error) => error.into_response(),
    }
}
