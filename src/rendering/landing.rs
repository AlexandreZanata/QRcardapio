use askama::Template;

use super::PageMeta;

#[derive(Template)]
#[template(path = "landing/index.html")]
pub struct LandingTemplate {
    pub page: PageMeta,
    pub is_logged_in: bool,
}
