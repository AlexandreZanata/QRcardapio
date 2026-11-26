use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::response::Response;

use super::with_public_cache;
use crate::app::AppState;
use crate::error::AppResult;
use crate::http::middleware::auth::OptionalAuth;
use crate::i18n::{Catalog, Locale};
use crate::rendering::error_page::error_page;
use crate::rendering::seo::{
    GuidePageTemplate, GuidesHubTemplate, SegmentCardView, SegmentPageTemplate,
    SegmentsHubTemplate, SeoNavContext, ToolCardView, other_guide_cards, related_segment_cards,
};
use crate::rendering::{PageMeta, render_html};
use crate::seo::guides::GuideId;
use crate::seo::routes::RouteCluster;
use crate::seo::schema::{guide_json_ld, hub_json_ld, segment_json_ld};
use crate::seo::segments::SegmentId;
use crate::seo::{SeoMeta, absolute_url};

pub async fn solutions_hub_pt(
    State(state): State<AppState>,
    OptionalAuth(auth): OptionalAuth,
) -> AppResult<Response> {
    render_solutions_hub(&state, Locale::PtBr, auth.is_some())
}

pub async fn solutions_hub_en(
    State(state): State<AppState>,
    OptionalAuth(auth): OptionalAuth,
) -> AppResult<Response> {
    render_solutions_hub(&state, Locale::En, auth.is_some())
}

pub async fn solutions_hub_es(
    State(state): State<AppState>,
    OptionalAuth(auth): OptionalAuth,
) -> AppResult<Response> {
    render_solutions_hub(&state, Locale::Es, auth.is_some())
}

pub async fn segment_page_pt(
    State(state): State<AppState>,
    OptionalAuth(auth): OptionalAuth,
    Path(slug): Path<String>,
) -> AppResult<Response> {
    render_segment_page(&state, Locale::PtBr, &slug, auth.is_some())
}

pub async fn segment_page_en(
    State(state): State<AppState>,
    OptionalAuth(auth): OptionalAuth,
    Path(slug): Path<String>,
) -> AppResult<Response> {
    render_segment_page(&state, Locale::En, &slug, auth.is_some())
}

pub async fn segment_page_es(
    State(state): State<AppState>,
    OptionalAuth(auth): OptionalAuth,
    Path(slug): Path<String>,
) -> AppResult<Response> {
    render_segment_page(&state, Locale::Es, &slug, auth.is_some())
}

pub async fn guides_hub_pt(
    State(state): State<AppState>,
    OptionalAuth(auth): OptionalAuth,
) -> AppResult<Response> {
    render_guides_hub(&state, Locale::PtBr, auth.is_some())
}

pub async fn guides_hub_en(
    State(state): State<AppState>,
    OptionalAuth(auth): OptionalAuth,
) -> AppResult<Response> {
    render_guides_hub(&state, Locale::En, auth.is_some())
}

pub async fn guides_hub_es(
    State(state): State<AppState>,
    OptionalAuth(auth): OptionalAuth,
) -> AppResult<Response> {
    render_guides_hub(&state, Locale::Es, auth.is_some())
}

pub async fn guide_page_pt(
    State(state): State<AppState>,
    OptionalAuth(auth): OptionalAuth,
    Path(slug): Path<String>,
) -> AppResult<Response> {
    render_guide_page(&state, Locale::PtBr, &slug, auth.is_some())
}

pub async fn guide_page_en(
    State(state): State<AppState>,
    OptionalAuth(auth): OptionalAuth,
    Path(slug): Path<String>,
) -> AppResult<Response> {
    render_guide_page(&state, Locale::En, &slug, auth.is_some())
}

pub async fn guide_page_es(
    State(state): State<AppState>,
    OptionalAuth(auth): OptionalAuth,
    Path(slug): Path<String>,
) -> AppResult<Response> {
    render_guide_page(&state, Locale::Es, &slug, auth.is_some())
}

fn render_solutions_hub(
    state: &AppState,
    locale: Locale,
    is_logged_in: bool,
) -> AppResult<Response> {
    let cat = Catalog::global();
    let base = &state.config.public_base_url;
    let cluster = RouteCluster::SolutionsHub;
    let path = cluster.path_for(locale);
    let canonical = absolute_url(base, path);
    let title = cat.t(locale, "seo.solutions_hub_title").to_string();
    let desc = cat.t(locale, "seo.solutions_hub_desc").to_string();
    let home_url = absolute_url(base, RouteCluster::Home.path_for(locale));
    let json_ld = hub_json_ld(
        base,
        locale,
        &canonical,
        &title,
        &desc,
        cat.t(locale, "seo.home"),
        &home_url,
        cat.t(locale, "seo.solutions"),
    );
    let seo = SeoMeta::indexable(
        base,
        locale,
        path,
        cluster.alternates(base),
        Some(title),
        "seo.solutions_hub_desc",
        Some(desc),
        "website",
        cat.t(locale, "landing.og_image_alt").to_string(),
        json_ld,
    );
    let tpl = SegmentsHubTemplate {
        page: PageMeta::with_seo(locale, "", None, seo),
        nav: SeoNavContext::build(locale, cluster, is_logged_in),
    };
    with_public_cache(render_html(&tpl, StatusCode::OK)?, is_logged_in)
}

fn render_segment_page(
    state: &AppState,
    locale: Locale,
    slug: &str,
    is_logged_in: bool,
) -> AppResult<Response> {
    let Some(id) = SegmentId::from_slug(locale, slug) else {
        return Ok(error_page(locale, StatusCode::NOT_FOUND, None));
    };
    let cat = Catalog::global();
    let base = &state.config.public_base_url;
    let seg = id.content(locale);
    let cluster = RouteCluster::Segment(id);
    let path = cluster.path_for(locale);
    let canonical = absolute_url(base, path);
    let home_url = absolute_url(base, RouteCluster::Home.path_for(locale));
    let hub_url = absolute_url(base, RouteCluster::SolutionsHub.path_for(locale));
    let json_ld = segment_json_ld(
        base,
        locale,
        &canonical,
        seg,
        cat.t(locale, "seo.home"),
        &home_url,
        cat.t(locale, "seo.solutions"),
        &hub_url,
    );
    let seo = SeoMeta::indexable(
        base,
        locale,
        path,
        cluster.alternates(base),
        Some(seg.meta_title.to_string()),
        "landing.meta_description",
        Some(seg.meta_description.to_string()),
        "website",
        cat.t(locale, "landing.og_image_alt").to_string(),
        json_ld,
    );
    let tpl = SegmentPageTemplate {
        page: PageMeta::with_seo(locale, "", None, seo),
        nav: SeoNavContext::build(locale, cluster, is_logged_in),
        seg,
        related_segments: related_segment_cards(locale, &seg.related),
    };
    with_public_cache(render_html(&tpl, StatusCode::OK)?, is_logged_in)
}

fn render_guides_hub(state: &AppState, locale: Locale, is_logged_in: bool) -> AppResult<Response> {
    let cat = Catalog::global();
    let base = &state.config.public_base_url;
    let cluster = RouteCluster::GuidesHub;
    let path = cluster.path_for(locale);
    let canonical = absolute_url(base, path);
    let title = cat.t(locale, "seo.guides_hub_title").to_string();
    let desc = cat.t(locale, "seo.guides_hub_desc").to_string();
    let home_url = absolute_url(base, RouteCluster::Home.path_for(locale));
    let json_ld = hub_json_ld(
        base,
        locale,
        &canonical,
        &title,
        &desc,
        cat.t(locale, "seo.home"),
        &home_url,
        cat.t(locale, "seo.guides"),
    );
    let seo = SeoMeta::indexable(
        base,
        locale,
        path,
        cluster.alternates(base),
        Some(title),
        "seo.guides_hub_desc",
        Some(desc),
        "website",
        cat.t(locale, "landing.og_image_alt").to_string(),
        json_ld,
    );
    let tpl = GuidesHubTemplate {
        page: PageMeta::with_seo(locale, "", None, seo),
        nav: SeoNavContext::build(locale, cluster, is_logged_in),
    };
    with_public_cache(render_html(&tpl, StatusCode::OK)?, is_logged_in)
}

fn render_guide_page(
    state: &AppState,
    locale: Locale,
    slug: &str,
    is_logged_in: bool,
) -> AppResult<Response> {
    let Some(id) = GuideId::from_slug(locale, slug) else {
        return Ok(error_page(locale, StatusCode::NOT_FOUND, None));
    };
    let cat = Catalog::global();
    let base = &state.config.public_base_url;
    let guide = id.content(locale);
    let cluster = RouteCluster::Guide(id);
    let path = cluster.path_for(locale);
    let canonical = absolute_url(base, path);
    let home_url = absolute_url(base, RouteCluster::Home.path_for(locale));
    let hub_url = absolute_url(base, RouteCluster::GuidesHub.path_for(locale));
    let json_ld = guide_json_ld(
        base,
        locale,
        &canonical,
        guide,
        cat.t(locale, "seo.home"),
        &home_url,
        cat.t(locale, "seo.guides"),
        &hub_url,
    );
    let seo = SeoMeta::indexable(
        base,
        locale,
        path,
        cluster.alternates(base),
        Some(guide.meta_title.to_string()),
        "landing.meta_description",
        Some(guide.meta_description.to_string()),
        "article",
        cat.t(locale, "landing.og_image_alt").to_string(),
        json_ld,
    );
    let seg = guide.related_segment.content(locale);
    let tool = guide.related_tool.content(locale);
    let tpl = GuidePageTemplate {
        page: PageMeta::with_seo(locale, "", None, seo),
        nav: SeoNavContext::build(locale, cluster, is_logged_in),
        guide,
        related_segment: SegmentCardView::new(seg, locale),
        related_tool: ToolCardView::new(tool, locale),
        other_guides: other_guide_cards(locale, id),
    };
    with_public_cache(render_html(&tpl, StatusCode::OK)?, is_logged_in)
}
