use askama::Template;

use super::PageMeta;

#[derive(Template)]
#[template(path = "dashboard/onboarding.html")]
pub struct OnboardingTemplate {
    pub google_enabled: bool,
    pub page: PageMeta,
    pub brand_name: String,
    pub has_establishment: bool,
    pub has_theme: bool,
    pub is_admin: bool,
    pub active_nav: &'static str,
    pub csrf_token: String,
    pub step: u8,
    pub continuation_suffix: String,
    pub base_host: String,
    pub name_value: String,
    pub slug_value: String,
    pub description_value: String,
    pub accent_color: String,
    pub error_key: Option<String>,
}

#[derive(Template)]
#[template(path = "dashboard/overview.html")]
pub struct OverviewTemplate {
    pub page: PageMeta,
    pub brand_name: String,
    pub has_establishment: bool,
    pub has_theme: bool,
    pub is_admin: bool,
    pub active_nav: &'static str,
    pub csrf_token: String,
    pub slug: String,
    pub public_url: String,
    pub is_published: bool,
    pub has_unpublished_changes: bool,
    pub published_version: i64,
    pub last_published_at: Option<String>,
    pub category_count: usize,
    pub product_count: usize,
    pub billing_enabled: bool,
    pub can_publish: bool,
    pub delivery: crate::application::delivery::access::StaffAccess,
}

#[derive(Template)]
#[template(path = "dashboard/settings.html")]
pub struct SettingsTemplate {
    pub google_enabled: bool,
    pub mcp_enabled: bool,
    pub page: PageMeta,
    pub brand_name: String,
    pub has_establishment: bool,
    pub has_theme: bool,
    pub is_admin: bool,
    pub active_nav: &'static str,
    pub csrf_token: String,
    pub base_host: String,
    pub name_value: String,
    pub slug_value: String,
    pub slug_locked: bool,
    pub description_value: String,
    pub description_en: String,
    pub description_es: String,
    pub accent_color: String,
    pub logo_url: Option<String>,
    pub google_maps_url: String,
    pub google_review_url: String,
    pub error_key: Option<String>,
}
