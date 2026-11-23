use std::sync::Arc;

use axum::Router;
use axum::extract::DefaultBodyLimit;
use axum::middleware;
use axum::routing::{get, post};
use sqlx::SqlitePool;
use tower_http::compression::CompressionLayer;
use tower_http::request_id::{MakeRequestUuid, PropagateRequestIdLayer, SetRequestIdLayer};
use tower_http::services::ServeDir;
use tower_http::trace::TraceLayer;

use crate::application::publishing::PublishLocks;
use crate::config::Config;
use crate::http::handlers::{
    admin, auth, categories, dashboard, health, ocr, onboarding, products, public, qr, settings,
};
use crate::http::middleware::branded_errors::branded_errors;
use crate::http::middleware::security_headers::security_headers_middleware;
use crate::i18n::Catalog;
use crate::infrastructure::assets::AssetProcessor;
use crate::infrastructure::ocr::OcrService;
use crate::infrastructure::security::PasswordService;
use crate::infrastructure::security::rate_limit::AppRateLimiters;

#[derive(Clone)]
pub struct AppState {
    pub config: Arc<Config>,
    pub pool: SqlitePool,
    pub catalog: &'static Catalog,
    pub passwords: PasswordService,
    pub assets: AssetProcessor,
    pub ocr: OcrService,
    pub rate_limiters: AppRateLimiters,
    pub publish_locks: PublishLocks,
}

impl AppState {
    pub fn new(config: Config, pool: SqlitePool) -> Self {
        let assets = AssetProcessor::new(config.media_dir());
        Self {
            config: Arc::new(config),
            pool,
            catalog: Catalog::global(),
            passwords: PasswordService::new(),
            assets,
            ocr: OcrService::new(),
            rate_limiters: AppRateLimiters::new(),
            publish_locks: PublishLocks::default(),
        }
    }
}

pub fn build_router(state: AppState) -> Router {
    let static_dir = state.config.static_dir.clone();

    let upload_routes = Router::new()
        .route("/dashboard/products", post(products::create_product_submit))
        .route(
            "/dashboard/products/{id}",
            post(products::update_product_submit),
        )
        .route(
            "/dashboard/settings/logo",
            post(settings::upload_logo_submit),
        )
        .layer(DefaultBodyLimit::max(6 * 1024 * 1024));

    let ocr_upload_routes = Router::new()
        .route("/dashboard/ocr/extract", post(ocr::ocr_extract_submit))
        .layer(DefaultBodyLimit::max(16 * 1024 * 1024));

    Router::new()
        .route("/", get(public::landing))
        .route("/login", get(auth::login_page).post(auth::login_submit))
        .route(
            "/register",
            get(auth::register_page).post(auth::register_submit),
        )
        .route("/logout", post(auth::logout_submit))
        .route(
            "/reset-password",
            get(auth::reset_password_page).post(auth::reset_password_submit),
        )
        .route(
            "/onboarding",
            get(onboarding::onboarding_page).post(onboarding::onboarding_submit),
        )
        .route("/dashboard", get(dashboard::overview_page))
        .route("/dashboard/theme.css", get(dashboard::dashboard_theme_css))
        .route("/dashboard/preview", get(dashboard::preview_menu_page))
        .route(
            "/dashboard/media/{filename}",
            get(dashboard::serve_tenant_media),
        )
        .route("/dashboard/publish", post(dashboard::publish_submit))
        .route("/dashboard/unpublish", post(dashboard::unpublish_submit))
        .route("/dashboard/menu", get(categories::menu_editor_page))
        .route(
            "/dashboard/categories/new",
            get(categories::new_category_page),
        )
        .route(
            "/dashboard/categories",
            post(categories::create_category_submit),
        )
        .route(
            "/dashboard/categories/{id}/edit",
            get(categories::edit_category_page),
        )
        .route(
            "/dashboard/categories/{id}",
            post(categories::update_category_submit),
        )
        .route(
            "/dashboard/categories/{id}/delete",
            post(categories::delete_category_submit),
        )
        .route(
            "/dashboard/categories/{id}/move",
            post(categories::move_category_submit),
        )
        .route("/dashboard/products/new", get(products::new_product_page))
        .route(
            "/dashboard/products/{id}/edit",
            get(products::edit_product_page),
        )
        .route(
            "/dashboard/products/{id}/toggle",
            post(products::toggle_product_submit),
        )
        .route(
            "/dashboard/products/{id}/delete",
            post(products::delete_product_submit),
        )
        .route(
            "/dashboard/products/{id}/move",
            post(products::move_product_submit),
        )
        .route(
            "/dashboard/settings",
            get(settings::settings_page).post(settings::settings_submit),
        )
        .route(
            "/dashboard/settings/logo/remove",
            post(settings::remove_logo_submit),
        )
        .route("/dashboard/qr", get(qr::qr_page))
        .route("/dashboard/qr/print", get(qr::qr_print_page))
        .route("/dashboard/qr.svg", get(qr::download_qr_svg))
        .route("/dashboard/qr.png", get(qr::download_qr_png))
        .route("/dashboard/ocr", get(ocr::ocr_index_page))
        .route("/dashboard/ocr/text", post(ocr::ocr_text_submit))
        .route("/dashboard/ocr/{id}", get(ocr::ocr_review_page))
        .route("/dashboard/ocr/{id}/apply", post(ocr::ocr_apply_submit))
        .route("/dashboard/ocr/{id}/discard", post(ocr::ocr_discard_submit))
        .route("/admin", get(admin::admin_page))
        .route("/c/{slug}", get(public::serve_published_menu))
        .route("/c/{slug}/", get(public::serve_published_menu))
        .route("/c/{slug}/{*file}", get(public::serve_published_asset))
        .route("/healthz", get(health::healthz))
        .route("/readyz", get(health::readyz))
        .route("/static/i18n/{file}", get(health::serve_i18n_catalog))
        .merge(upload_routes)
        .merge(ocr_upload_routes)
        .nest_service("/static", ServeDir::new(static_dir))
        .with_state(state)
        .layer(middleware::from_fn(branded_errors))
        .layer(middleware::from_fn(security_headers_middleware))
        .layer(CompressionLayer::new())
        .layer(TraceLayer::new_for_http())
        .layer(PropagateRequestIdLayer::x_request_id())
        .layer(SetRequestIdLayer::x_request_id(MakeRequestUuid))
}
