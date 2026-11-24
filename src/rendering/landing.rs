use askama::Template;

use super::seo::{GuideCardView, SegmentCardView, ToolCardView};
use super::{PageMeta, billing::PlanOffer};
use crate::seo::faq::FaqItem;

#[derive(Template)]
#[template(path = "landing/index.html")]
pub struct LandingTemplate {
    pub page: PageMeta,
    pub is_logged_in: bool,
    pub billing_enabled: bool,
    pub show_plan: bool,
    pub mcp_enabled: bool,
    pub mcp_endpoint: String,
    pub plans: [PlanOffer; 3],
    pub faqs: &'static [FaqItem],
    pub segments: Vec<SegmentCardView>,
    pub tools: Vec<ToolCardView>,
    pub guides: Vec<GuideCardView>,
    pub home_pt_url: &'static str,
    pub home_en_url: &'static str,
    pub home_es_url: &'static str,
    pub nav_home_url: &'static str,
    pub solutions_url: &'static str,
    pub tools_url: &'static str,
    pub guides_url: &'static str,
    pub demo_menu_path: String,
}
