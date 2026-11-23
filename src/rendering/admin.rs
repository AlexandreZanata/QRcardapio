use askama::Template;

use super::PageMeta;

pub struct AdminEstablishmentRow {
    pub name: String,
    pub slug: String,
    pub is_published: bool,
    pub published_version: i64,
    pub created_at: String,
}

pub struct AdminPublicationRow {
    pub est_name: String,
    pub est_slug: String,
    pub status: String,
    pub version: i64,
    pub html_bytes: i64,
    pub duration_ms: i64,
}

#[derive(Template)]
#[template(path = "dashboard/admin.html")]
pub struct AdminTemplate {
    pub page: PageMeta,
    pub brand_name: String,
    pub has_establishment: bool,
    pub has_theme: bool,
    pub is_admin: bool,
    pub active_nav: &'static str,
    pub csrf_token: String,
    pub sqlite_version: String,
    pub account_count: i64,
    pub establishment_count: i64,
    pub published_count: i64,
    pub publication_count: i64,
    pub establishments: Vec<AdminEstablishmentRow>,
    pub publications: Vec<AdminPublicationRow>,
}
