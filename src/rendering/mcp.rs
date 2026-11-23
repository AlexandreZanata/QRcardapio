use askama::Template;

use super::PageMeta;

#[derive(Template)]
#[template(path = "mcp/consent.html")]
pub struct ConsentTemplate<'a> {
    pub page: PageMeta,
    pub flow: &'a str,
    pub provider: &'a str,
    pub establishment: Option<&'a str>,
    pub read_write: bool,
    pub csrf: &'a str,
    pub email: &'a str,
    pub error: Option<&'a str>,
}

pub struct Connection {
    pub id: String,
    pub provider: &'static str,
    pub read_write: bool,
    pub created_at: String,
}

#[derive(Template)]
#[template(path = "dashboard/integrations.html")]
pub struct ConnectionsTemplate {
    pub page: PageMeta,
    pub brand_name: String,
    pub has_establishment: bool,
    pub has_theme: bool,
    pub is_admin: bool,
    pub active_nav: &'static str,
    pub csrf_token: String,
    pub endpoint: String,
    pub connections: Vec<Connection>,
}

#[derive(Template)]
#[template(path = "mcp/return.html")]
pub struct ReturnTemplate {
    pub page: PageMeta,
    pub provider: &'static str,
    pub callback: String,
}
