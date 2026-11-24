use axum::Form;
use axum::extract::{Query, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum_extra::extract::CookieJar;
use serde::Deserialize;

pub use super::admin_affiliate_detail::{
    admin_affiliate_detail_page, disqualify_referral_submit, settle_payout_submit,
    update_affiliate_submit,
};
use super::affiliates::active_billing_env;
use crate::app::AppState;
use crate::application::affiliates::{
    self as service, AdminEnrollAffiliateInput, UpdateProgramSettingsInput,
};
use crate::domain::affiliate::{format_commission_input_value, format_commission_rate};
use crate::error::AppResult;
use crate::http::middleware::auth::{AdminContext, verify_csrf};
use crate::http::middleware::locale::RequestLocale;
use crate::http::responses::{take_flash, with_flash_redirect};
use crate::i18n::format_price;
use crate::infrastructure::database::queries::find_establishment_for_account;
use crate::rendering::affiliates::{
    AdminAffiliatesTemplate, AffiliateSummaryView, affiliate_status_meta, commission_unit_key,
};
use crate::rendering::{PageMeta, render_html};

pub(super) const ADMIN_AFFILIATES_PATH: &str = "/admin/affiliates";

#[derive(Deserialize, Default)]
pub struct EnvQuery {
    #[serde(default)]
    pub env: String,
}

pub(super) fn resolve_env(state: &AppState, requested: &str) -> &'static str {
    match requested.trim() {
        "sandbox" => "sandbox",
        "production" => "production",
        _ => active_billing_env(state),
    }
}

pub async fn admin_affiliates_page(
    State(state): State<AppState>,
    RequestLocale(locale): RequestLocale,
    admin: AdminContext,
    jar: CookieJar,
    Query(query): Query<EnvQuery>,
) -> AppResult<Response> {
    let env = resolve_env(&state, &query.env);
    let has_establishment = find_establishment_for_account(&state.pool, &admin.auth.account.id)
        .await?
        .is_some();
    let (settings, stats, rows) = service::load_admin_overview(&state.pool, env).await?;
    let (jar, flash) = take_flash(jar);

    let affiliates = rows
        .into_iter()
        .map(|row| {
            let (status_key, status_badge) = affiliate_status_meta(row.affiliate.status);
            AffiliateSummaryView {
                id: row.affiliate.id,
                account_email: row.account_email,
                code: row.affiliate.code,
                status_key,
                status_badge,
                quota_display: format_commission_rate(
                    row.affiliate.commission_type,
                    row.affiliate.commission_value,
                    locale,
                ),
                quota_unit_key: commission_unit_key(row.affiliate.commission_type),
                hold_days: row.affiliate.hold_days,
                click_count: row.affiliate.click_count,
                referrals_total: row.referrals_total,
                referrals_active: row.referrals_active,
                pending_formatted: format_price(row.pending_centavos, locale),
                available_formatted: format_price(row.available_centavos, locale),
                paid_formatted: format_price(row.paid_centavos, locale),
            }
        })
        .collect();

    let tpl = AdminAffiliatesTemplate {
        page: PageMeta::new(locale, "admin_affiliates.page_title", flash),
        brand_name: "QR Cardápio".to_owned(),
        has_establishment,
        has_theme: false,
        is_admin: true,
        active_nav: "admin_affiliates",
        csrf_token: admin.auth.csrf_token,
        billing_env: env.to_owned(),
        self_signup_enabled: settings.self_signup_enabled,
        default_commission_type: settings.default_commission_type.as_str().to_owned(),
        default_commission_input: format_commission_input_value(
            settings.default_commission_type,
            settings.default_commission_value,
        ),
        default_hold_days: settings.default_hold_days,
        cookie_ttl_days: settings.cookie_ttl_days,
        total_affiliates: stats.total_affiliates,
        active_affiliates: stats.active_affiliates,
        active_referrals: stats.active_referrals,
        pending_formatted: format_price(stats.pending_centavos, locale),
        available_formatted: format_price(stats.available_centavos, locale),
        paid_formatted: format_price(stats.paid_centavos, locale),
        affiliates,
    };

    Ok((jar, render_html(&tpl, StatusCode::OK)?).into_response())
}

#[derive(Deserialize)]
pub struct ProgramSettingsForm {
    pub _csrf: String,
    #[serde(default)]
    pub self_signup_enabled: Option<String>,
    pub default_commission_type: String,
    pub default_commission_value: String,
    pub default_hold_days: i64,
    pub cookie_ttl_days: i64,
}

pub async fn update_settings_submit(
    State(state): State<AppState>,
    admin: AdminContext,
    jar: CookieJar,
    Form(form): Form<ProgramSettingsForm>,
) -> AppResult<Response> {
    verify_csrf(&admin.auth, &form._csrf)?;
    let res = service::update_program_settings(
        &state.pool,
        UpdateProgramSettingsInput {
            self_signup_enabled: form.self_signup_enabled.as_deref() == Some("1"),
            default_commission_type: &form.default_commission_type,
            default_commission_value: &form.default_commission_value,
            default_hold_days: form.default_hold_days,
            cookie_ttl_days: form.cookie_ttl_days,
        },
    )
    .await;
    match res {
        Ok(_) => Ok(with_flash_redirect(
            jar,
            ADMIN_AFFILIATES_PATH,
            false,
            "flash.affiliate_settings_saved",
            state.config.cookie_secure,
        )),
        Err(err) if err.status_code().is_client_error() => Ok(with_flash_redirect(
            jar,
            ADMIN_AFFILIATES_PATH,
            true,
            err.i18n_key(),
            state.config.cookie_secure,
        )),
        Err(err) => Err(err),
    }
}

#[derive(Deserialize)]
pub struct EnrollAffiliateForm {
    pub _csrf: String,
    pub email: String,
    #[serde(default)]
    pub code: String,
    #[serde(default = "default_active_status")]
    pub status: String,
    pub commission_type: String,
    pub commission_value: String,
    pub hold_days: i64,
    #[serde(default)]
    pub pix_key: String,
    #[serde(default)]
    pub notes: String,
}

fn default_active_status() -> String {
    "active".to_owned()
}

pub async fn enroll_submit(
    State(state): State<AppState>,
    admin: AdminContext,
    jar: CookieJar,
    Form(form): Form<EnrollAffiliateForm>,
) -> AppResult<Response> {
    verify_csrf(&admin.auth, &form._csrf)?;
    let res = service::admin_enroll_affiliate(
        &state.pool,
        AdminEnrollAffiliateInput {
            email: &form.email,
            code: &form.code,
            status: &form.status,
            commission_type: &form.commission_type,
            commission_value: &form.commission_value,
            hold_days: form.hold_days,
            pix_key: &form.pix_key,
            notes: &form.notes,
        },
    )
    .await;
    match res {
        Ok(aff) => Ok(with_flash_redirect(
            jar,
            &format!("{ADMIN_AFFILIATES_PATH}/{}", aff.id),
            false,
            "flash.affiliate_enrolled",
            state.config.cookie_secure,
        )),
        Err(err) if err.status_code().is_client_error() => Ok(with_flash_redirect(
            jar,
            ADMIN_AFFILIATES_PATH,
            true,
            err.i18n_key(),
            state.config.cookie_secure,
        )),
        Err(err) => Err(err),
    }
}
