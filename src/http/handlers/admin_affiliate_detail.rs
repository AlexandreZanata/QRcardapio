use axum::Form;
use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum_extra::extract::CookieJar;
use serde::Deserialize;

use super::admin_affiliates::{ADMIN_AFFILIATES_PATH, EnvQuery, resolve_env};
use super::affiliates::referral_link_url;
use crate::app::AppState;
use crate::application::affiliates::{self as service, AdminUpdateAffiliateInput};
use crate::domain::affiliate::{
    ReferralStatus, format_commission_input_value, format_commission_rate,
};
use crate::domain::billing::billing_now;
use crate::error::AppResult;
use crate::http::middleware::auth::{AdminContext, verify_csrf};
use crate::http::middleware::locale::RequestLocale;
use crate::http::responses::{take_flash, with_flash_redirect};
use crate::i18n::format_price;
use crate::infrastructure::database::queries::find_establishment_for_account;
use crate::rendering::affiliates::{
    AdminAffiliateDetailTemplate, ReferralView, affiliate_status_meta, commission_unit_key,
    map_commission_view, map_payout_view, referral_status_meta,
};
use crate::rendering::{PageMeta, render_html};

pub async fn admin_affiliate_detail_page(
    State(state): State<AppState>,
    RequestLocale(locale): RequestLocale,
    admin: AdminContext,
    jar: CookieJar,
    Path(affiliate_id): Path<String>,
    Query(query): Query<EnvQuery>,
) -> AppResult<Response> {
    let env = resolve_env(&state, &query.env);
    let has_establishment = find_establishment_for_account(&state.pool, &admin.auth.account.id)
        .await?
        .is_some();
    let detail = service::load_admin_affiliate_detail(&state.pool, &affiliate_id, env).await?;
    let (jar, flash) = take_flash(jar);

    let (status_key, status_badge) = affiliate_status_meta(detail.affiliate.status);
    let referrals_total = detail.referrals.len() as i64;
    let referrals_active = detail
        .referrals
        .iter()
        .filter(|r| r.status == ReferralStatus::Active)
        .count() as i64;

    let referrals = detail
        .referrals
        .into_iter()
        .map(|r| {
            let (sk, sb) = referral_status_meta(r.status);
            ReferralView {
                id: r.id,
                account_label: r.referred_email,
                has_establishment: r.establishment_name.is_some(),
                establishment_name: r.establishment_name.unwrap_or_default(),
                establishment_slug: r.establishment_slug.unwrap_or_default(),
                status_key: sk,
                status_badge: sb,
                can_disqualify: r.status != ReferralStatus::Disqualified,
                disqualified_reason: r.disqualified_reason.unwrap_or_default(),
                attributed_at: r.attributed_at,
                first_paid_at: r.first_paid_at,
            }
        })
        .collect();

    let commissions = detail
        .commissions
        .into_iter()
        .map(|c| map_commission_view(c, locale))
        .collect();
    let payouts = detail
        .payouts
        .into_iter()
        .map(|p| map_payout_view(p, locale))
        .collect();

    let tpl = AdminAffiliateDetailTemplate {
        page: PageMeta::new(locale, "admin_affiliates.detail_title", flash),
        brand_name: "QR Cardápio".to_owned(),
        has_establishment,
        has_theme: false,
        is_admin: true,
        active_nav: "admin_affiliates",
        csrf_token: admin.auth.csrf_token,
        billing_env: env.to_owned(),
        affiliate_id: detail.affiliate.id,
        account_email: detail.account_email,
        referral_url: referral_link_url(&state.config.public_base_url, &detail.affiliate.code),
        code: detail.affiliate.code,
        status: detail.affiliate.status.as_str().to_owned(),
        status_key,
        status_badge,
        commission_type: detail.affiliate.commission_type.as_str().to_owned(),
        commission_input: format_commission_input_value(
            detail.affiliate.commission_type,
            detail.affiliate.commission_value,
        ),
        quota_display: format_commission_rate(
            detail.affiliate.commission_type,
            detail.affiliate.commission_value,
            locale,
        ),
        quota_unit_key: commission_unit_key(detail.affiliate.commission_type),
        hold_days: detail.affiliate.hold_days,
        pix_key: detail.affiliate.pix_key,
        notes: detail.affiliate.notes,
        click_count: detail.affiliate.click_count,
        referrals_total,
        referrals_active,
        pending_formatted: format_price(detail.balances.pending_centavos, locale),
        available_formatted: format_price(detail.balances.available_centavos, locale),
        paid_formatted: format_price(detail.balances.paid_centavos, locale),
        voided_formatted: format_price(detail.balances.voided_centavos, locale),
        can_settle_payout: detail.balances.available_centavos > 0,
        referrals,
        commissions,
        payouts,
    };

    Ok((jar, render_html(&tpl, StatusCode::OK)?).into_response())
}

#[derive(Deserialize)]
pub struct UpdateAffiliateForm {
    pub _csrf: String,
    pub code: String,
    pub status: String,
    pub commission_type: String,
    pub commission_value: String,
    pub hold_days: i64,
    #[serde(default)]
    pub pix_key: String,
    #[serde(default)]
    pub notes: String,
}

pub async fn update_affiliate_submit(
    State(state): State<AppState>,
    admin: AdminContext,
    jar: CookieJar,
    Path(affiliate_id): Path<String>,
    Form(form): Form<UpdateAffiliateForm>,
) -> AppResult<Response> {
    verify_csrf(&admin.auth, &form._csrf)?;
    let detail_path = format!("{ADMIN_AFFILIATES_PATH}/{affiliate_id}");
    let res = service::admin_update_affiliate(
        &state.pool,
        &affiliate_id,
        AdminUpdateAffiliateInput {
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
        Ok(()) => Ok(with_flash_redirect(
            jar,
            &detail_path,
            false,
            "flash.affiliate_updated",
            state.config.cookie_secure,
        )),
        Err(err) if err.status_code().is_client_error() => Ok(with_flash_redirect(
            jar,
            &detail_path,
            true,
            err.i18n_key(),
            state.config.cookie_secure,
        )),
        Err(err) => Err(err),
    }
}

#[derive(Deserialize)]
pub struct SettlePayoutForm {
    pub _csrf: String,
    #[serde(default)]
    pub env: String,
    #[serde(default)]
    pub reference: String,
    #[serde(default)]
    pub notes: String,
    pub confirm: Option<String>,
}

pub async fn settle_payout_submit(
    State(state): State<AppState>,
    admin: AdminContext,
    jar: CookieJar,
    Path(affiliate_id): Path<String>,
    Form(form): Form<SettlePayoutForm>,
) -> AppResult<Response> {
    verify_csrf(&admin.auth, &form._csrf)?;
    let env = resolve_env(&state, &form.env);
    let detail_path = format!("{ADMIN_AFFILIATES_PATH}/{affiliate_id}?env={env}");
    if form.confirm.as_deref() != Some("yes") {
        return Ok(with_flash_redirect(
            jar,
            &detail_path,
            true,
            "error.affiliate_payout_confirm",
            state.config.cookie_secure,
        ));
    }
    let res = service::settle_payout(
        &state.pool,
        &affiliate_id,
        env,
        &admin.auth.account.id,
        &form.reference,
        &form.notes,
        billing_now().date_naive(),
    )
    .await;
    match res {
        Ok(_) => Ok(with_flash_redirect(
            jar,
            &detail_path,
            false,
            "flash.affiliate_payout_recorded",
            state.config.cookie_secure,
        )),
        Err(err) if err.status_code().is_client_error() => Ok(with_flash_redirect(
            jar,
            &detail_path,
            true,
            err.i18n_key(),
            state.config.cookie_secure,
        )),
        Err(err) => Err(err),
    }
}

#[derive(Deserialize)]
pub struct DisqualifyReferralForm {
    pub _csrf: String,
    #[serde(default)]
    pub reason: String,
}

pub async fn disqualify_referral_submit(
    State(state): State<AppState>,
    admin: AdminContext,
    jar: CookieJar,
    Path((affiliate_id, referral_id)): Path<(String, String)>,
    Form(form): Form<DisqualifyReferralForm>,
) -> AppResult<Response> {
    verify_csrf(&admin.auth, &form._csrf)?;
    let detail_path = format!("{ADMIN_AFFILIATES_PATH}/{affiliate_id}");
    match service::admin_disqualify_referral(&state.pool, &affiliate_id, &referral_id, &form.reason)
        .await
    {
        Ok(()) => Ok(with_flash_redirect(
            jar,
            &detail_path,
            false,
            "flash.affiliate_referral_disqualified",
            state.config.cookie_secure,
        )),
        Err(err) if err.status_code().is_client_error() => Ok(with_flash_redirect(
            jar,
            &detail_path,
            true,
            err.i18n_key(),
            state.config.cookie_secure,
        )),
        Err(err) => Err(err),
    }
}
