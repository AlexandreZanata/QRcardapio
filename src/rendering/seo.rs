use askama::Template;

use super::PageMeta;
use crate::i18n::{Catalog, Locale};
use crate::seo::guides::{GuideContent, GuideId, all_guides};
use crate::seo::routes::RouteCluster;
use crate::seo::segments::{SegmentContent, SegmentId, all_segments};
use crate::seo::tools::{ToolContent, ToolId, all_tools};

#[derive(Debug, Clone)]
pub struct SegmentCardView {
    pub name: &'static str,
    pub short_desc: &'static str,
    pub href: &'static str,
}

#[derive(Debug, Clone)]
pub struct ToolCardView {
    pub name: &'static str,
    pub short_desc: &'static str,
    pub href: &'static str,
}

#[derive(Debug, Clone)]
pub struct GuideCardView {
    pub title: &'static str,
    pub short_desc: &'static str,
    pub read_minutes: u8,
    pub href: &'static str,
}

#[derive(Debug, Clone)]
pub struct SeoNavContext {
    pub is_logged_in: bool,
    pub home_url: &'static str,
    pub solutions_url: &'static str,
    pub tools_url: &'static str,
    pub guides_url: &'static str,
    pub alt_pt_url: &'static str,
    pub alt_en_url: &'static str,
    pub alt_es_url: &'static str,
    pub home_pt_url: &'static str,
    pub home_en_url: &'static str,
    pub home_es_url: &'static str,
    pub segments: Vec<SegmentCardView>,
    pub tools: Vec<ToolCardView>,
    pub guides: Vec<GuideCardView>,
}

impl SeoNavContext {
    pub fn build(locale: Locale, cluster: RouteCluster, is_logged_in: bool) -> Self {
        let (alt_pt_url, alt_en_url, alt_es_url) = cluster.paths();
        Self {
            is_logged_in,
            home_url: RouteCluster::Home.path_for(locale),
            solutions_url: RouteCluster::SolutionsHub.path_for(locale),
            tools_url: RouteCluster::ToolsHub.path_for(locale),
            guides_url: RouteCluster::GuidesHub.path_for(locale),
            alt_pt_url,
            alt_en_url,
            alt_es_url,
            home_pt_url: RouteCluster::Home.path_for(Locale::PtBr),
            home_en_url: RouteCluster::Home.path_for(Locale::En),
            home_es_url: RouteCluster::Home.path_for(Locale::Es),
            segments: segment_cards(locale),
            tools: tool_cards(locale),
            guides: guide_cards(locale),
        }
    }
}

pub fn segment_cards(locale: Locale) -> Vec<SegmentCardView> {
    all_segments(locale)
        .into_iter()
        .map(|s| SegmentCardView {
            name: s.name,
            short_desc: s.short_desc,
            href: s.id.full_path(locale),
        })
        .collect()
}

pub fn tool_cards(locale: Locale) -> Vec<ToolCardView> {
    all_tools(locale)
        .into_iter()
        .map(|t| ToolCardView {
            name: t.name,
            short_desc: t.short_desc,
            href: t.id.full_path(locale),
        })
        .collect()
}

pub fn guide_cards(locale: Locale) -> Vec<GuideCardView> {
    all_guides(locale)
        .into_iter()
        .map(|g| GuideCardView {
            title: g.title,
            short_desc: g.short_desc,
            read_minutes: g.read_minutes,
            href: g.id.full_path(locale),
        })
        .collect()
}

#[derive(Template)]
#[template(path = "seo/segments_hub.html")]
pub struct SegmentsHubTemplate {
    pub page: PageMeta,
    pub nav: SeoNavContext,
}

#[derive(Template)]
#[template(path = "seo/segment.html")]
pub struct SegmentPageTemplate {
    pub page: PageMeta,
    pub nav: SeoNavContext,
    pub seg: &'static SegmentContent,
    pub related_segments: Vec<SegmentCardView>,
}

#[derive(Template)]
#[template(path = "seo/tools_hub.html")]
pub struct ToolsHubTemplate {
    pub page: PageMeta,
    pub nav: SeoNavContext,
}

#[derive(Template)]
#[template(path = "seo/tool.html")]
pub struct ToolPageTemplate {
    pub page: PageMeta,
    pub nav: SeoNavContext,
    pub tool: &'static ToolContent,
    pub form_action: &'static str,
    pub qr_target_url: String,
    pub qr_preview_src: String,
    pub qr_svg_download: String,
    pub qr_png_download: String,
    pub placard_name: String,
    pub placard_cta: String,
    pub other_tools: Vec<ToolCardView>,
}

#[derive(Template)]
#[template(path = "seo/guides_hub.html")]
pub struct GuidesHubTemplate {
    pub page: PageMeta,
    pub nav: SeoNavContext,
}

#[derive(Template)]
#[template(path = "seo/guide.html")]
pub struct GuidePageTemplate {
    pub page: PageMeta,
    pub nav: SeoNavContext,
    pub guide: &'static GuideContent,
    pub related_segment: SegmentCardView,
    pub related_tool: ToolCardView,
    pub other_guides: Vec<GuideCardView>,
}

pub fn related_segment_cards(locale: Locale, ids: &[SegmentId]) -> Vec<SegmentCardView> {
    ids.iter()
        .map(|id| {
            let s = id.content(locale);
            SegmentCardView {
                name: s.name,
                short_desc: s.short_desc,
                href: s.id.full_path(locale),
            }
        })
        .collect()
}

pub fn other_tool_cards(locale: Locale, current: ToolId) -> Vec<ToolCardView> {
    ToolId::ALL
        .into_iter()
        .filter(|id| *id != current)
        .map(|id| {
            let t = id.content(locale);
            ToolCardView {
                name: t.name,
                short_desc: t.short_desc,
                href: t.id.full_path(locale),
            }
        })
        .collect()
}

pub fn other_guide_cards(locale: Locale, current: GuideId) -> Vec<GuideCardView> {
    GuideId::ALL
        .into_iter()
        .filter(|id| *id != current)
        .map(|id| {
            let g = id.content(locale);
            GuideCardView {
                title: g.title,
                short_desc: g.short_desc,
                read_minutes: g.read_minutes,
                href: g.id.full_path(locale),
            }
        })
        .collect()
}

pub fn tool_default_cta(catalog: &Catalog, locale: Locale) -> &str {
    catalog.t(locale, "print.scan_cta")
}
