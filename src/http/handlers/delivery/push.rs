use axum::Json;
use axum::extract::{Path, State};
use axum::http::{StatusCode, header};
use axum::response::{IntoResponse, Response};
use chrono::Utc;
use serde::Deserialize;
use serde_json::{Value, json};

use crate::app::AppState;
use crate::application::commerce::notifications;
use crate::application::delivery::access::staff_access;
use crate::error::{AppError, AppResult};
use crate::http::middleware::auth::{TenantContext, verify_csrf};
use crate::infrastructure::web_push::PushSubscription;

#[derive(Deserialize)]
pub struct SubscriptionInput {
    _csrf: String,
    endpoint: String,
    p256dh: String,
    auth: String,
}

#[derive(Deserialize)]
pub struct RevokeInput {
    _csrf: String,
}

pub async fn subscribe(
    State(state): State<AppState>,
    tenant: TenantContext,
    Json(input): Json<SubscriptionInput>,
) -> AppResult<Json<Value>> {
    verify_csrf(&tenant.auth, &input._csrf)?;
    let access = staff_access(
        &state.pool,
        &state.commercial_catalog,
        state.billing.enforcement(),
        &tenant.establishment.id,
        tenant.role,
    )
    .await?;
    if state.staff_push_public_key.is_none() || !access.new_orders {
        return Err(AppError::Forbidden("error.module_unavailable"));
    }
    let subscription = PushSubscription::parse(&input.endpoint, &input.p256dh, &input.auth)?;
    let id = notifications::register(
        &state.pool,
        &tenant.establishment.id,
        &tenant.auth.account.id,
        &subscription,
        Utc::now(),
    )
    .await?;
    Ok(Json(json!({ "id": id })))
}

pub async fn revoke(
    State(state): State<AppState>,
    tenant: TenantContext,
    Path(id): Path<String>,
    Json(input): Json<RevokeInput>,
) -> AppResult<StatusCode> {
    verify_csrf(&tenant.auth, &input._csrf)?;
    notifications::revoke(
        &state.pool,
        &tenant.establishment.id,
        &tenant.auth.account.id,
        &id,
    )
    .await?;
    Ok(StatusCode::NO_CONTENT)
}

pub async fn worker(State(state): State<AppState>, _tenant: TenantContext) -> AppResult<Response> {
    if state.staff_push_public_key.is_none() {
        return Err(AppError::NotFound("error.not_found"));
    }
    Ok((
        [(
            header::CONTENT_TYPE,
            "application/javascript; charset=utf-8",
        )],
        include_str!("../../../../static/js/delivery-staff-worker.js"),
    )
        .into_response())
}
