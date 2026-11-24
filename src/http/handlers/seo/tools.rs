use axum::Json;
use axum::extract::{Path, Query, State};
use axum::http::header::{CACHE_CONTROL, CONTENT_DISPOSITION, CONTENT_TYPE};
use axum::http::{HeaderValue, StatusCode};
use axum::response::{IntoResponse, Response};
use serde::Deserialize;
use serde_json::json;

use super::with_public_cache;
use crate::app::AppState;
use crate::error::AppResult;
use crate::http::middleware::auth::{ClientIp, OptionalAuth};
use crate::i18n::{Catalog, Locale};
use crate::qr::{generate_png, generate_svg};
use crate::rendering::error_page::error_page;
use crate::rendering::seo::{
    SeoNavContext, ToolPageTemplate, ToolsHubTemplate, other_tool_cards, tool_default_cta,
};
use crate::rendering::{PageMeta, render_html};
use crate::seo::routes::RouteCluster;
use crate::seo::schema::{hub_json_ld, tool_json_ld};
use crate::seo::tools::ToolId;
use crate::seo::{SeoMeta, absolute_url};

const MAX_QR_URL_LEN: usize = 512;

#[derive(Debug, Deserialize)]
pub struct ToolQuery {
    pub url: Option<String>,
    pub name: Option<String>,
    pub cta: Option<String>,
    pub download: Option<u8>,
}

pub async fn tools_hub_pt(
    State(state): State<AppState>,
    OptionalAuth(auth): OptionalAuth,
) -> AppResult<Response> {
    render_tools_hub(&state, Locale::PtBr, auth.is_some())
}

pub async fn tools_hub_en(
    State(state): State<AppState>,
    OptionalAuth(auth): OptionalAuth,
) -> AppResult<Response> {
    render_tools_hub(&state, Locale::En, auth.is_some())
}

pub async fn tools_hub_es(
    State(state): State<AppState>,
    OptionalAuth(auth): OptionalAuth,
) -> AppResult<Response> {
    render_tools_hub(&state, Locale::Es, auth.is_some())
}

pub async fn tool_page_pt(
    State(state): State<AppState>,
    OptionalAuth(auth): OptionalAuth,
    Path(slug): Path<String>,
    Query(query): Query<ToolQuery>,
) -> AppResult<Response> {
    render_tool_page(&state, Locale::PtBr, &slug, &query, auth.is_some())
}

pub async fn tool_page_en(
    State(state): State<AppState>,
    OptionalAuth(auth): OptionalAuth,
    Path(slug): Path<String>,
    Query(query): Query<ToolQuery>,
) -> AppResult<Response> {
    render_tool_page(&state, Locale::En, &slug, &query, auth.is_some())
}

pub async fn tool_page_es(
    State(state): State<AppState>,
    OptionalAuth(auth): OptionalAuth,
    Path(slug): Path<String>,
    Query(query): Query<ToolQuery>,
) -> AppResult<Response> {
    render_tool_page(&state, Locale::Es, &slug, &query, auth.is_some())
}

pub async fn tool_qr_svg(
    State(state): State<AppState>,
    ClientIp(ip): ClientIp,
    Query(query): Query<ToolQuery>,
) -> Response {
    if state.rate_limiters.tool_qr_ip.check_key(&ip).is_err() {
        return (
            StatusCode::TOO_MANY_REQUESTS,
            Json(json!({"error": "rate_limited"})),
        )
            .into_response();
    }
    let Some(target) = query.url.as_deref().and_then(validate_tool_url) else {
        return (
            StatusCode::BAD_REQUEST,
            Json(json!({"error": "invalid_url"})),
        )
            .into_response();
    };
    let Ok(svg) = generate_svg(&target) else {
        return (
            StatusCode::BAD_REQUEST,
            Json(json!({"error": "qr_generation_failed"})),
        )
            .into_response();
    };
    let mut res = (StatusCode::OK, svg).into_response();
    let headers = res.headers_mut();
    headers.insert(
        CONTENT_TYPE,
        HeaderValue::from_static("image/svg+xml; charset=utf-8"),
    );
    headers.insert(
        CACHE_CONTROL,
        HeaderValue::from_static("public, max-age=3600"),
    );
    if query.download == Some(1) {
        headers.insert(
            CONTENT_DISPOSITION,
            HeaderValue::from_static("attachment; filename=\"qrcode-cardapio.svg\""),
        );
    }
    res
}

pub async fn tool_qr_png(
    State(state): State<AppState>,
    ClientIp(ip): ClientIp,
    Query(query): Query<ToolQuery>,
) -> Response {
    if state.rate_limiters.tool_qr_ip.check_key(&ip).is_err() {
        return (
            StatusCode::TOO_MANY_REQUESTS,
            Json(json!({"error": "rate_limited"})),
        )
            .into_response();
    }
    let Some(target) = query.url.as_deref().and_then(validate_tool_url) else {
        return (
            StatusCode::BAD_REQUEST,
            Json(json!({"error": "invalid_url"})),
        )
            .into_response();
    };
    let Ok(png) = generate_png(&target) else {
        return (
            StatusCode::BAD_REQUEST,
            Json(json!({"error": "qr_generation_failed"})),
        )
            .into_response();
    };
    let mut res = (StatusCode::OK, png).into_response();
    let headers = res.headers_mut();
    headers.insert(CONTENT_TYPE, HeaderValue::from_static("image/png"));
    headers.insert(
        CACHE_CONTROL,
        HeaderValue::from_static("public, max-age=3600"),
    );
    if query.download == Some(1) {
        headers.insert(
            CONTENT_DISPOSITION,
            HeaderValue::from_static("attachment; filename=\"qrcode-cardapio.png\""),
        );
    }
    res
}

fn validate_tool_url(raw: &str) -> Option<String> {
    let trimmed = raw.trim();
    if trimmed.is_empty() || trimmed.len() > MAX_QR_URL_LEN {
        return None;
    }
    let parsed = reqwest::Url::parse(trimmed).ok()?;
    let valid_scheme = parsed.scheme() == "https" || parsed.scheme() == "http";
    if !valid_scheme || parsed.host_str().is_none_or(str::is_empty) {
        return None;
    }
    Some(parsed.to_string())
}

fn render_tools_hub(state: &AppState, locale: Locale, is_logged_in: bool) -> AppResult<Response> {
    let cat = Catalog::global();
    let base = &state.config.public_base_url;
    let cluster = RouteCluster::ToolsHub;
    let path = cluster.path_for(locale);
    let canonical = absolute_url(base, path);
    let title = cat.t(locale, "seo.tools_hub_title").to_string();
    let desc = cat.t(locale, "seo.tools_hub_desc").to_string();
    let home_url = absolute_url(base, RouteCluster::Home.path_for(locale));
    let json_ld = hub_json_ld(
        base,
        locale,
        &canonical,
        &title,
        &desc,
        cat.t(locale, "seo.home"),
        &home_url,
        cat.t(locale, "seo.tools"),
    );
    let seo = SeoMeta::indexable(
        base,
        locale,
        path,
        cluster.alternates(base),
        Some(title),
        "seo.tools_hub_desc",
        Some(desc),
        "website",
        cat.t(locale, "landing.og_image_alt").to_string(),
        json_ld,
    );
    let tpl = ToolsHubTemplate {
        page: PageMeta::with_seo(locale, "", None, seo),
        nav: SeoNavContext::build(locale, cluster, is_logged_in),
    };
    with_public_cache(render_html(&tpl, StatusCode::OK)?, is_logged_in)
}

fn render_tool_page(
    state: &AppState,
    locale: Locale,
    slug: &str,
    query: &ToolQuery,
    is_logged_in: bool,
) -> AppResult<Response> {
    let Some(id) = ToolId::from_slug(locale, slug) else {
        return Ok(error_page(locale, StatusCode::NOT_FOUND, None));
    };
    let cat = Catalog::global();
    let base = &state.config.public_base_url;
    let tool = id.content(locale);
    let cluster = RouteCluster::Tool(id);
    let path = cluster.path_for(locale);
    let canonical = absolute_url(base, path);
    let home_url = absolute_url(base, RouteCluster::Home.path_for(locale));
    let hub_url = absolute_url(base, RouteCluster::ToolsHub.path_for(locale));
    let json_ld = tool_json_ld(
        base,
        locale,
        &canonical,
        tool,
        cat.t(locale, "seo.home"),
        &home_url,
        cat.t(locale, "seo.tools"),
        &hub_url,
    );

    let default_target = absolute_url(base, "/c/bistro-da-praca");
    let qr_target_url = query
        .url
        .as_deref()
        .and_then(validate_tool_url)
        .unwrap_or(default_target);
    let encoded_target: String =
        reqwest::Url::parse_with_params("http://local/", &[("url", &qr_target_url)])
            .map(|u| u.query().unwrap_or_default().to_string())
            .unwrap_or_else(|_| format!("url={qr_target_url}"));

    let placard_name = query
        .name
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty() && s.chars().count() <= 80)
        .unwrap_or("Bistrô da Praça")
        .to_string();
    let placard_cta = query
        .cta
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty() && s.chars().count() <= 120)
        .unwrap_or_else(|| tool_default_cta(cat, locale))
        .to_string();

    let seo = SeoMeta::indexable(
        base,
        locale,
        path,
        cluster.alternates(base),
        Some(tool.meta_title.to_string()),
        "landing.meta_description",
        Some(tool.meta_description.to_string()),
        "website",
        cat.t(locale, "landing.og_image_alt").to_string(),
        json_ld,
    );
    let tpl = ToolPageTemplate {
        page: PageMeta::with_seo(locale, "", None, seo),
        nav: SeoNavContext::build(locale, cluster, is_logged_in),
        tool,
        form_action: path,
        qr_target_url,
        qr_preview_src: format!("/api/tools/qr.svg?{encoded_target}"),
        qr_svg_download: format!("/api/tools/qr.svg?{encoded_target}&download=1"),
        qr_png_download: format!("/api/tools/qr.png?{encoded_target}&download=1"),
        placard_name,
        placard_cta,
        other_tools: other_tool_cards(locale, id),
    };
    with_public_cache(render_html(&tpl, StatusCode::OK)?, is_logged_in)
}
