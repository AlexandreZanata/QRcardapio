use axum::extract::{Form, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum_extra::extract::CookieJar;
use chrono::Utc;
use serde::Deserialize;
use uuid::Uuid;

use crate::app::AppState;
use crate::application::mcp::oauth::resource;
use crate::domain::{
    account::AccountRole,
    mcp::{READ_WRITE, provider_name},
};
use crate::error::{AppError, AppResult};
use crate::http::middleware::auth::{TenantContext, verify_csrf};
use crate::http::middleware::locale::RequestLocale;
use crate::http::responses::{take_flash, with_flash_redirect};
use crate::infrastructure::database::queries::{is_establishment_owner, mcp_oauth as queries};
use crate::rendering::mcp::{Connection, ConnectionsTemplate};
use crate::rendering::{PageMeta, render_html};

pub async fn require_owner(state: &AppState, tenant: &TenantContext) -> AppResult<()> {
    if !is_establishment_owner(
        &state.pool,
        &tenant.establishment.id,
        &tenant.auth.account.id,
    )
    .await?
    {
        return Err(AppError::Forbidden("mcp.owner_required"));
    }
    Ok(())
}

pub fn owner(tenant: &TenantContext) -> queries::Owner<'_> {
    queries::Owner {
        account: &tenant.auth.account.id,
        establishment: &tenant.establishment.id,
    }
}

pub async fn page(
    State(state): State<AppState>,
    tenant: TenantContext,
    RequestLocale(locale): RequestLocale,
    jar: CookieJar,
) -> AppResult<Response> {
    require_owner(&state, &tenant).await?;
    let endpoint = resource(&state.config.public_base_url);
    let connections = queries::list_grants(
        &state.pool,
        &owner(&tenant),
        &endpoint,
        Utc::now().timestamp(),
    )
    .await?
    .into_iter()
    .filter_map(|grant| {
        Some(Connection {
            id: grant.id,
            provider: provider_name(&grant.client_id)?,
            read_write: grant.scope == READ_WRITE,
            created_at: grant.created_at,
        })
    })
    .collect();
    let (jar, flash) = take_flash(jar);
    let view = ConnectionsTemplate {
        page: PageMeta::new(locale, "mcp.integrations", flash),
        brand_name: tenant.establishment.name,
        has_establishment: true,
        has_theme: true,
        is_admin: tenant.auth.account.role == AccountRole::Admin,
        active_nav: "integrations",
        csrf_token: tenant.auth.csrf_token,
        endpoint,
        connections,
    };
    Ok((jar, render_html(&view, StatusCode::OK)?).into_response())
}

#[derive(Deserialize)]
pub struct RevokeForm {
    _csrf: String,
    connection: String,
}

pub async fn revoke(
    State(state): State<AppState>,
    tenant: TenantContext,
    jar: CookieJar,
    Form(form): Form<RevokeForm>,
) -> AppResult<Response> {
    verify_csrf(&tenant.auth, &form._csrf)?;
    require_owner(&state, &tenant).await?;
    let id = Uuid::parse_str(&form.connection).map_err(|_| AppError::Validation("mcp.expired"))?;
    queries::revoke_grant(&state.pool, &owner(&tenant), &id.to_string()).await?;
    Ok(with_flash_redirect(
        jar,
        "/dashboard/integrations",
        false,
        "mcp.revoked",
        state.config.cookie_secure,
    ))
}
