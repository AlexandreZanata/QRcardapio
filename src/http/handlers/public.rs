use std::fs::File;
use std::io::Read;
use std::net::IpAddr;
use std::os::unix::fs::MetadataExt;
use std::path::PathBuf;

use axum::extract::{Path, Query, State};
use axum::http::header::{CACHE_CONTROL, CONTENT_TYPE, ETAG, IF_NONE_MATCH};
use axum::http::{HeaderMap, HeaderValue, StatusCode};
use axum::response::{IntoResponse, Redirect, Response};
use axum_extra::extract::CookieJar;
use serde::Deserialize;

use crate::app::AppState;
use crate::domain::establishment::{PUBLIC_DEMO_SLUG, Slug};
use crate::error::AppResult;
use crate::http::handlers::affiliates::capture_referral_redirect;
use crate::http::handlers::seo::with_public_cache;
use crate::http::middleware::auth::{ClientIp, OptionalAuth};
use crate::http::middleware::locale::{ExplicitRequestLocale, RequestLocale};
use crate::http::responses::check_etag_not_modified;
use crate::i18n::{Catalog, Locale};
use crate::infrastructure::storage::resolve_safe_subpath;
use crate::rendering::error_page::error_page;
use crate::rendering::landing::LandingTemplate;
use crate::rendering::seo::{guide_cards, segment_cards, tool_cards};
use crate::rendering::{PageMeta, render_html};
use crate::seo::SeoMeta;
use crate::seo::faq::LANDING_FAQS;
use crate::seo::routes::RouteCluster;
use crate::seo::schema::landing_json_ld;

#[derive(Deserialize)]
pub struct LandingQuery {
    #[serde(default, rename = "ref")]
    pub referral: Option<String>,
}

pub async fn landing(
    State(state): State<AppState>,
    ExplicitRequestLocale(explicit_locale): ExplicitRequestLocale,
    OptionalAuth(auth): OptionalAuth,
    ClientIp(ip): ClientIp,
    jar: CookieJar,
    Query(query): Query<LandingQuery>,
) -> AppResult<Response> {
    if let Some(code) = query.referral.as_deref().filter(|s| !s.trim().is_empty()) {
        if auth.is_some() {
            return Ok(no_store_redirect("/"));
        }
        return capture_referral_redirect(&state, ip, jar, code, "/", false).await;
    }
    let locale = explicit_locale.unwrap_or(Locale::PtBr);
    render_landing(&state, locale, auth.is_some())
}

pub async fn landing_en(
    State(state): State<AppState>,
    OptionalAuth(auth): OptionalAuth,
    ClientIp(ip): ClientIp,
    jar: CookieJar,
    Query(query): Query<LandingQuery>,
) -> AppResult<Response> {
    if let Some(code) = query.referral.as_deref().filter(|s| !s.trim().is_empty()) {
        if auth.is_some() {
            return Ok(no_store_redirect("/en"));
        }
        return capture_referral_redirect(&state, ip, jar, code, "/en", false).await;
    }
    render_landing(&state, Locale::En, auth.is_some())
}

pub async fn landing_es(
    State(state): State<AppState>,
    OptionalAuth(auth): OptionalAuth,
    ClientIp(ip): ClientIp,
    jar: CookieJar,
    Query(query): Query<LandingQuery>,
) -> AppResult<Response> {
    if let Some(code) = query.referral.as_deref().filter(|s| !s.trim().is_empty()) {
        if auth.is_some() {
            return Ok(no_store_redirect("/es"));
        }
        return capture_referral_redirect(&state, ip, jar, code, "/es", false).await;
    }
    render_landing(&state, Locale::Es, auth.is_some())
}

fn no_store_redirect(location: &str) -> Response {
    let mut res = Redirect::to(location).into_response();
    res.headers_mut()
        .insert(CACHE_CONTROL, HeaderValue::from_static("no-store"));
    res
}

fn render_landing(state: &AppState, locale: Locale, is_logged_in: bool) -> AppResult<Response> {
    let billing_enabled = state.billing.enabled();
    let show_plan = billing_enabled || local_plan_preview(&state.config.public_base_url);
    let base = &state.config.public_base_url;
    let cat = Catalog::global();
    let cluster = RouteCluster::Home;
    let canonical_path = cluster.path_for(locale);
    let canonical = crate::seo::absolute_url(base, canonical_path);
    let json_ld = landing_json_ld(
        base,
        locale,
        &canonical,
        cat.t(locale, "landing.page_title"),
        cat.t(locale, "landing.meta_description"),
        show_plan,
        cat,
    );
    let seo = SeoMeta::indexable(
        base,
        locale,
        canonical_path,
        cluster.alternates(base),
        None,
        "landing.meta_description",
        None,
        "website",
        cat.t(locale, "landing.og_image_alt").to_string(),
        json_ld,
    );

    let tpl = LandingTemplate {
        page: PageMeta::with_seo(locale, "landing.page_title", None, seo),
        is_logged_in,
        billing_enabled,
        show_plan,
        mcp_enabled: state.config.mcp_enabled,
        mcp_endpoint: if state.config.mcp_enabled {
            format!("{}/mcp", state.config.public_base_url)
        } else {
            "https://qrcardapio.com.br/mcp".to_owned()
        },
        plans: crate::rendering::billing::PlanOffer::catalog(),
        faqs: LANDING_FAQS,
        segments: segment_cards(locale),
        tools: tool_cards(locale),
        guides: guide_cards(locale),
        home_pt_url: cluster.path_for(Locale::PtBr),
        home_en_url: cluster.path_for(Locale::En),
        home_es_url: cluster.path_for(Locale::Es),
        nav_home_url: canonical_path,
        solutions_url: RouteCluster::SolutionsHub.path_for(locale),
        tools_url: RouteCluster::ToolsHub.path_for(locale),
        guides_url: RouteCluster::GuidesHub.path_for(locale),
        demo_menu_path: format!("/c/{PUBLIC_DEMO_SLUG}"),
    };
    with_public_cache(render_html(&tpl, StatusCode::OK)?, is_logged_in)
}

fn local_plan_preview(public_base_url: &str) -> bool {
    let Ok(base) = reqwest::Url::parse(public_base_url) else {
        return false;
    };
    base.scheme() == "http"
        && base.host_str().is_some_and(|host| {
            host == "localhost"
                || match host.trim_matches(['[', ']']).parse::<IpAddr>() {
                    Ok(IpAddr::V4(ip)) => ip.is_loopback() || ip.is_private(),
                    Ok(IpAddr::V6(ip)) => ip.is_loopback() || ip.is_unique_local(),
                    Err(_) => false,
                }
        })
}

/// Serves a published static menu (`/c/{slug}`) directly from disk WITHOUT touching SQLite.
pub async fn serve_published_menu(
    State(state): State<AppState>,
    RequestLocale(locale): RequestLocale,
    Path(slug): Path<String>,
    headers: HeaderMap,
) -> Response {
    serve_published_file_from_disk(
        &state.config.published_dir(),
        &slug,
        "index.html",
        locale,
        &headers,
    )
    .await
}

/// Serves static assets inside a published menu directory (`/c/{slug}/{*file}`) WITHOUT touching SQLite.
pub async fn serve_published_asset(
    State(state): State<AppState>,
    RequestLocale(locale): RequestLocale,
    Path((slug, file)): Path<(String, String)>,
    headers: HeaderMap,
) -> Response {
    serve_published_file_from_disk(
        &state.config.published_dir(),
        &slug,
        &file,
        locale,
        &headers,
    )
    .await
}

/// Pure filesystem delivery handler that takes only the `published_dir` root (no database handle).
pub async fn serve_published_file_from_disk(
    published_dir: &std::path::Path,
    slug: &str,
    relative_file: &str,
    locale: crate::i18n::Locale,
    headers: &HeaderMap,
) -> Response {
    let Ok(clean_slug) = Slug::validate_existing(slug) else {
        return render_public_404(locale);
    };
    let Ok(est_dir) = resolve_safe_subpath(published_dir, clean_slug.as_str()) else {
        return render_public_404(locale);
    };
    let Ok(target) = resolve_safe_subpath(&est_dir, relative_file) else {
        return render_public_404(locale);
    };

    let mut conditional = HeaderMap::new();
    if let Some(value) = headers.get(IF_NONE_MATCH) {
        conditional.insert(IF_NONE_MATCH, value.clone());
    }
    tokio::task::spawn_blocking(move || read_published_file(target, &conditional))
        .await
        .ok()
        .flatten()
        .unwrap_or_else(|| render_public_404(locale))
}

fn read_published_file(target: PathBuf, headers: &HeaderMap) -> Option<Response> {
    // Metadata and body come from the same generation if publication replaces the path.
    let mut file = File::open(&target).ok()?;
    let meta = file.metadata().ok()?;
    if !meta.is_file() {
        return None;
    }

    let mtime_ns = meta
        .modified()
        .ok()
        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    let etag = format!("\"{}-{}-{}\"", meta.ino(), meta.size(), mtime_ns);
    if let Some(not_mod) = check_etag_not_modified(headers, &etag) {
        return Some(not_mod);
    }

    let mut bytes = Vec::new();
    file.read_to_end(&mut bytes).ok()?;

    let (content_type, cache_control) = match target.extension().and_then(|e| e.to_str()) {
        Some("html") => (
            "text/html; charset=utf-8",
            "public, max-age=60, stale-while-revalidate=300",
        ),
        Some("css") => (
            "text/css; charset=utf-8",
            "public, max-age=31536000, immutable",
        ),
        Some("jpg") | Some("jpeg") => ("image/jpeg", "public, max-age=31536000, immutable"),
        Some("png") => ("image/png", "public, max-age=31536000, immutable"),
        _ => ("application/octet-stream", "public, max-age=3600"),
    };

    let mut res = (StatusCode::OK, bytes).into_response();
    let res_headers = res.headers_mut();
    res_headers.insert(CONTENT_TYPE, HeaderValue::from_static(content_type));
    res_headers.insert(CACHE_CONTROL, HeaderValue::from_static(cache_control));
    if let Ok(hv) = HeaderValue::from_str(&etag) {
        res_headers.insert(ETAG, hv);
    }
    Some(res)
}

fn render_public_404(locale: crate::i18n::Locale) -> Response {
    error_page(
        locale,
        StatusCode::NOT_FOUND,
        Some("public_menu.not_found_body"),
    )
}
