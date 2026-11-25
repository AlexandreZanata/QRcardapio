use std::sync::Arc;

use axum::Router;
use axum::extract::{DefaultBodyLimit, MatchedPath};
use axum::middleware;
use axum::routing::{get, post};
use sqlx::SqlitePool;
use tower_http::compression::CompressionLayer;
use tower_http::request_id::{MakeRequestUuid, PropagateRequestIdLayer, SetRequestIdLayer};
use tower_http::services::ServeDir;
use tower_http::trace::TraceLayer;

use crate::application::payments::BillingGateways;
use crate::application::publishing::PublishLocks;
use crate::config::Config;
use crate::http::handlers::{
    admin, admin_affiliates, admin_outbox, affiliates, auth, billing, billing_payer,
    billing_webhooks, categories, dashboard, google, health, mcp, menu_editor, ocr, onboarding,
    products, public, qr, seo, settings,
};
use crate::http::middleware::branded_errors::branded_errors;
use crate::http::middleware::security_headers::{no_store, security_headers_middleware};
use crate::i18n::Catalog;
use crate::infrastructure::assets::AssetProcessor;
use crate::infrastructure::google::GoogleOidc;
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
    pub billing: BillingGateways,
    pub google: Option<GoogleOidc>,
}

impl AppState {
    pub fn new(config: Config, pool: SqlitePool) -> Self {
        let assets = AssetProcessor::new(config.media_dir());
        let billing = BillingGateways::from_config(&config)
            .expect("payment configuration was validated when it was loaded");
        let google = config.google.clone().map(GoogleOidc::new);
        Self {
            config: Arc::new(config),
            pool,
            catalog: Catalog::global(),
            passwords: PasswordService::new(),
            assets,
            ocr: OcrService::new(),
            rate_limiters: AppRateLimiters::new(),
            publish_locks: PublishLocks::default(),
            billing,
            google,
        }
    }
}

impl AppState {
    /// Replaces the payment gateways (tests use loopback provider mocks).
    #[doc(hidden)]
    pub fn with_billing(mut self, billing: BillingGateways) -> Self {
        self.billing = billing;
        self
    }
}

pub fn build_router(state: AppState) -> Router {
    let static_dir = state.config.static_dir.clone();
    let mcp_routes = mcp::routes(&state);
    let google_routes = google::routes(state.google.is_some());

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

    let billing_webhook = Router::new()
        .route("/webhooks/asaas", post(billing_webhooks::asaas))
        .route("/webhooks/stripe", post(billing_webhooks::stripe))
        .route("/webhooks/inter/{token}", post(billing_webhooks::inter))
        .layer(DefaultBodyLimit::max(64 * 1024));

    let admin_outbox_routes = Router::new()
        .route("/admin/outbox", get(admin_outbox::outbox))
        .route(
            admin_outbox::RECOVERY_PATH,
            get(admin_outbox::recovery_page),
        )
        .route("/admin/outbox/replay", post(admin_outbox::replay_submit))
        .layer(DefaultBodyLimit::max(16 * 1024))
        .layer(middleware::from_fn(admin_outbox::private_headers));

    let affiliate_routes = Router::new()
        .route("/r/{code}", get(affiliates::referral_redirect))
        .route("/dashboard/affiliate", get(affiliates::portal_page))
        .route(
            "/dashboard/affiliate/activate",
            post(affiliates::activate_submit),
        )
        .route(
            "/dashboard/affiliate/pix",
            post(affiliates::update_pix_submit),
        )
        .route(
            "/admin/affiliates",
            get(admin_affiliates::admin_affiliates_page),
        )
        .route(
            "/admin/affiliates/settings",
            post(admin_affiliates::update_settings_submit),
        )
        .route(
            "/admin/affiliates/enroll",
            post(admin_affiliates::enroll_submit),
        )
        .route(
            "/admin/affiliates/{id}",
            get(admin_affiliates::admin_affiliate_detail_page),
        )
        .route(
            "/admin/affiliates/{id}/update",
            post(admin_affiliates::update_affiliate_submit),
        )
        .route(
            "/admin/affiliates/{id}/payout",
            post(admin_affiliates::settle_payout_submit),
        )
        .route(
            "/admin/affiliates/{id}/referrals/{referral_id}/disqualify",
            post(admin_affiliates::disqualify_referral_submit),
        )
        .layer(DefaultBodyLimit::max(16 * 1024))
        .layer(middleware::from_fn(no_store));

    Router::new()
        .route("/", get(public::landing))
        .route("/en", get(public::landing_en))
        .route("/en/", get(seo::redirect_en_slash))
        .route("/es", get(public::landing_es))
        .route("/es/", get(seo::redirect_es_slash))
        .route("/robots.txt", get(seo::robots_txt))
        .route("/sitemap.xml", get(seo::sitemap_xml))
        .route("/llms.txt", get(seo::llms_txt))
        .route("/manifest.webmanifest", get(seo::manifest_webmanifest))
        .route("/favicon.ico", get(seo::favicon_ico))
        .route("/apple-touch-icon.png", get(seo::apple_touch_icon))
        .route("/solucoes", get(seo::solutions_hub_pt))
        .route("/en/solutions", get(seo::solutions_hub_en))
        .route("/es/soluciones", get(seo::solutions_hub_es))
        .route("/cardapio-digital/{slug}", get(seo::segment_page_pt))
        .route("/en/digital-menu/{slug}", get(seo::segment_page_en))
        .route("/es/menu-digital/{slug}", get(seo::segment_page_es))
        .route("/ferramentas", get(seo::tools_hub_pt))
        .route("/en/tools", get(seo::tools_hub_en))
        .route("/es/herramientas", get(seo::tools_hub_es))
        .route("/ferramentas/{slug}", get(seo::tool_page_pt))
        .route("/en/tools/{slug}", get(seo::tool_page_en))
        .route("/es/herramientas/{slug}", get(seo::tool_page_es))
        .route("/api/tools/qr.svg", get(seo::tool_qr_svg))
        .route("/api/tools/qr.png", get(seo::tool_qr_png))
        .route("/guias", get(seo::guides_hub_pt))
        .route("/en/guides", get(seo::guides_hub_en))
        .route("/es/guias", get(seo::guides_hub_es))
        .route("/guias/{slug}", get(seo::guide_page_pt))
        .route("/en/guides/{slug}", get(seo::guide_page_en))
        .route("/es/guias/{slug}", get(seo::guide_page_es))
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
        .route("/dashboard/billing", get(billing::page))
        .route("/dashboard/billing/checkout", post(billing::checkout))
        .route("/dashboard/billing/cancel", post(billing::cancel))
        .route("/dashboard/billing/reconcile", post(billing::reconcile))
        .route("/dashboard/billing/payer", post(billing_payer::save))
        .route("/dashboard/billing/pix.svg", get(billing_payer::pix_svg))
        .route(
            "/dashboard/billing/boleto.pdf",
            get(billing_payer::boleto_pdf),
        )
        .route("/dashboard/theme.css", get(dashboard::dashboard_theme_css))
        .route("/dashboard/preview", get(dashboard::preview_menu_page))
        .route(
            "/dashboard/media/{filename}",
            get(dashboard::serve_tenant_media),
        )
        .route("/dashboard/publish", post(dashboard::publish_submit))
        .route("/dashboard/unpublish", post(dashboard::unpublish_submit))
        .route("/dashboard/menu", get(menu_editor::menu_editor_page))
        .merge(crate::http::handlers::commerce::router())
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
        .merge(billing_webhook)
        .merge(admin_outbox_routes)
        .merge(affiliate_routes)
        .merge(mcp_routes)
        .merge(google_routes)
        .nest_service("/static", ServeDir::new(static_dir))
        .with_state(state)
        .layer(middleware::from_fn(branded_errors))
        .layer(middleware::from_fn(security_headers_middleware))
        .layer(CompressionLayer::new())
        .layer(
            TraceLayer::new_for_http().make_span_with(|request: &axum::extract::Request| {
                let path = request
                    .extensions()
                    .get::<MatchedPath>()
                    .map_or("<unmatched>", MatchedPath::as_str);
                tracing::debug_span!("http.request", method = %request.method(), path)
            }),
        )
        .layer(PropagateRequestIdLayer::x_request_id())
        .layer(SetRequestIdLayer::x_request_id(MakeRequestUuid))
}
