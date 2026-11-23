use axum::extract::State;
use axum::http::StatusCode;
use axum::response::Response;

use crate::app::AppState;
use crate::application::admin::load_operational_overview;
use crate::error::AppResult;
use crate::http::middleware::auth::AdminContext;
use crate::http::middleware::locale::RequestLocale;
use crate::infrastructure::database::queries::find_establishment_for_account;
use crate::rendering::admin::{AdminEstablishmentRow, AdminPublicationRow, AdminTemplate};
use crate::rendering::{PageMeta, render_html};

pub async fn admin_page(
    State(state): State<AppState>,
    RequestLocale(locale): RequestLocale,
    admin: AdminContext,
) -> AppResult<Response> {
    let overview = load_operational_overview(&state.pool).await?;
    let has_establishment = find_establishment_for_account(&state.pool, &admin.auth.account.id)
        .await?
        .is_some();

    let establishments = overview
        .establishments
        .into_iter()
        .map(|e| AdminEstablishmentRow {
            name: e.name,
            slug: e.slug,
            is_published: e.is_published,
            published_version: e.published_version,
            created_at: e.created_at,
        })
        .collect();

    let publications = overview
        .publications
        .into_iter()
        .map(|(p, est_name, est_slug)| AdminPublicationRow {
            est_name,
            est_slug,
            status: p.status,
            version: p.version,
            html_bytes: p.html_bytes,
            duration_ms: p.duration_ms,
        })
        .collect();

    let tpl = AdminTemplate {
        page: PageMeta::new(locale, "admin.page_title", None),
        brand_name: "QR Cardápio".to_string(),
        has_establishment,
        has_theme: false,
        is_admin: true,
        active_nav: "admin",
        csrf_token: admin.auth.csrf_token,
        sqlite_version: overview.sqlite_version,
        account_count: overview.stats.account_count,
        establishment_count: overview.stats.establishment_count,
        published_count: overview.stats.published_count,
        publication_count: overview.stats.publication_count,
        establishments,
        publications,
    };
    render_html(&tpl, StatusCode::OK)
}
