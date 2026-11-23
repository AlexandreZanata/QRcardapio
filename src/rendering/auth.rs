use askama::Template;

use super::PageMeta;

#[derive(Template)]
#[template(path = "auth/login.html")]
pub struct LoginTemplate {
    pub page: PageMeta,
    pub email_value: String,
    pub subscribe_plan: bool,
    pub error_key: Option<String>,
}

#[derive(Template)]
#[template(path = "auth/register.html")]
pub struct RegisterTemplate {
    pub page: PageMeta,
    pub email_value: String,
    pub subscribe_plan: bool,
    pub error_key: Option<String>,
}

#[derive(Template)]
#[template(path = "auth/reset_password.html")]
pub struct ResetPasswordTemplate {
    pub page: PageMeta,
    pub error_key: Option<String>,
}
