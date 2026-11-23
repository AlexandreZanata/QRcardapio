use axum::Form;
use axum::extract::{Query, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Redirect, Response};
use axum_extra::extract::CookieJar;
use serde::Deserialize;

use crate::app::AppState;
use crate::application::establishments::{CreateEstablishmentInput, create_establishment};
use crate::domain::account::AccountRole;
use crate::domain::billing::BillingPlan;
use crate::domain::establishment::AccentColor;
use crate::error::AppResult;
use crate::http::middleware::auth::{AuthContext, verify_csrf};
use crate::http::middleware::locale::RequestLocale;
use crate::http::responses::{take_flash, with_flash_redirect};
use crate::infrastructure::database::queries::find_establishment_for_account;
use crate::rendering::dashboard::OnboardingTemplate;
use crate::rendering::{PageMeta, render_html};

const BILLING_PAGE: &str = "/dashboard/billing";

#[derive(Deserialize)]
pub struct PlanIntent {
    #[serde(default)]
    plan: String,
}

impl PlanIntent {
    pub(crate) fn selected(&self) -> Option<BillingPlan> {
        BillingPlan::from_key(&self.plan)
    }

    pub(crate) fn suffix(&self, state: &AppState) -> String {
        if state.asaas.is_some()
            && let Some(plan) = self.selected()
        {
            return format!("?plan={}", plan.key());
        }
        String::new()
    }
}

#[derive(Deserialize)]
pub struct OnboardingForm {
    pub _csrf: String,
    pub name: String,
    #[serde(default)]
    pub slug: String,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub accent_color: String,
}

pub async fn onboarding_page(
    State(state): State<AppState>,
    RequestLocale(locale): RequestLocale,
    auth: AuthContext,
    jar: CookieJar,
    Query(plan): Query<PlanIntent>,
) -> AppResult<Response> {
    let plan_suffix = plan.suffix(&state);
    if find_establishment_for_account(&state.pool, &auth.account.id)
        .await?
        .is_some()
    {
        let destination = if plan_suffix.is_empty() {
            "/dashboard".to_owned()
        } else {
            format!("{BILLING_PAGE}{plan_suffix}")
        };
        return Ok(Redirect::to(&destination).into_response());
    }

    let (jar, flash) = take_flash(jar);
    let tpl = OnboardingTemplate {
        page: PageMeta::new(locale, "onboarding.page_title", flash),
        brand_name: "QR Cardápio".to_string(),
        has_establishment: false,
        has_theme: false,
        is_admin: auth.account.role == AccountRole::Admin,
        active_nav: "",
        csrf_token: auth.csrf_token,
        plan_suffix,
        step: 2,
        base_host: state.config.public_base_url.clone(),
        name_value: String::new(),
        slug_value: String::new(),
        description_value: String::new(),
        accent_color: AccentColor::DEFAULT_HEX.to_string(),
        error_key: None,
    };
    Ok((jar, render_html(&tpl, StatusCode::OK)?).into_response())
}

pub async fn onboarding_submit(
    State(state): State<AppState>,
    RequestLocale(locale): RequestLocale,
    auth: AuthContext,
    jar: CookieJar,
    Query(plan): Query<PlanIntent>,
    Form(form): Form<OnboardingForm>,
) -> AppResult<Response> {
    verify_csrf(&auth, &form._csrf)?;
    let plan_suffix = plan.suffix(&state);

    let res = create_establishment(
        &state.pool,
        &auth.account.id,
        CreateEstablishmentInput {
            name: &form.name,
            slug: &form.slug,
            description: &form.description,
            accent_color: &form.accent_color,
        },
    )
    .await;

    match res {
        Ok(_) => Ok(with_flash_redirect(
            jar,
            &if plan_suffix.is_empty() {
                "/dashboard/menu".to_owned()
            } else {
                format!("{BILLING_PAGE}{plan_suffix}")
            },
            false,
            "flash.establishment_created",
            state.config.cookie_secure,
        )),
        Err(err) => {
            let tpl = OnboardingTemplate {
                page: PageMeta::new(locale, "onboarding.page_title", None),
                brand_name: "QR Cardápio".to_string(),
                has_establishment: false,
                has_theme: false,
                is_admin: auth.account.role == AccountRole::Admin,
                active_nav: "",
                csrf_token: auth.csrf_token,
                plan_suffix,
                step: 2,
                base_host: state.config.public_base_url.clone(),
                name_value: form.name,
                slug_value: form.slug,
                description_value: form.description,
                accent_color: if form.accent_color.trim().is_empty() {
                    AccentColor::DEFAULT_HEX.to_string()
                } else {
                    form.accent_color
                },
                error_key: Some(err.i18n_key().to_string()),
            };
            render_html(&tpl, err.status_code())
        }
    }
}
