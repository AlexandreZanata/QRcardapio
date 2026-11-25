use axum::Json;
use axum::extract::State;
use axum::http::header::CACHE_CONTROL;
use axum::response::{IntoResponse, Response};
use chrono::Utc;

use crate::app::AppState;
use crate::error::AppResult;
use crate::http::middleware::auth::AdminContext;
use crate::infrastructure::database::queries::event_dispatch_status;

pub async fn outbox(State(state): State<AppState>, _admin: AdminContext) -> AppResult<Response> {
    let status = event_dispatch_status::status(&state.pool, Utc::now()).await?;
    Ok((
        [
            (CACHE_CONTROL, "no-store"),
            (
                axum::http::header::HeaderName::from_static("x-robots-tag"),
                "noindex, nofollow",
            ),
        ],
        Json(status),
    )
        .into_response())
}
