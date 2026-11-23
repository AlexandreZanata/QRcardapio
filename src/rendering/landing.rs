use askama::Template;

use super::{PageMeta, billing::PlanOffer};

#[derive(Template)]
#[template(path = "landing/index.html")]
pub struct LandingTemplate {
    pub page: PageMeta,
    pub is_logged_in: bool,
    pub billing_enabled: bool,
    pub show_plan: bool,
    pub plans: [PlanOffer; 3],
}
