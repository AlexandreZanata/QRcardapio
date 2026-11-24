use askama::Template;

use super::PageMeta;

#[derive(Template)]
#[template(path = "auth/google_account.html")]
pub struct GoogleAccountTemplate {
    pub page: PageMeta,
    pub email: String,
    pub csrf: String,
    pub linked: bool,
    pub has_password: bool,
}

#[derive(Template)]
#[template(path = "auth/google_continue.html")]
pub struct GoogleContinueTemplate<'a> {
    pub page: PageMeta,
    pub url: &'a str,
}
