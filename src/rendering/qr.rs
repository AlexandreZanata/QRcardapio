use askama::Template;

use super::PageMeta;

#[derive(Template)]
#[template(path = "dashboard/qr.html")]
pub struct QrTemplate {
    pub page: PageMeta,
    pub brand_name: String,
    pub has_establishment: bool,
    pub has_theme: bool,
    pub is_admin: bool,
    pub active_nav: &'static str,
    pub csrf_token: String,
    pub is_published: bool,
    pub public_url: String,
    pub qr_svg: String,
}

#[derive(Template)]
#[template(path = "dashboard/qr_print.html")]
pub struct QrPrintTemplate {
    pub page: PageMeta,
    pub est_name: String,
    pub public_url: String,
    pub logo_url: Option<String>,
    pub qr_svg: String,
}
