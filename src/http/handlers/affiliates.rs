use std::net::IpAddr;

use axum::Form;
use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::http::header::CACHE_CONTROL;
use axum::response::{IntoResponse, Redirect, Response};
use axum_extra::extract::CookieJar;
use axum_extra::extract::cookie::{Cookie, SameSite};
use serde::Deserialize;

use crate::app::AppState;
use crate::application::affiliates::{self as service, SelfActivateAffiliateInput};
use crate::domain::account::AccountRole;
use crate::domain::affiliate::{format_commission_rate, mask_email_for_partner};
use crate::domain::billing::BillingPlan;
use crate::error::{AppError, AppResult};
use crate::http::middleware::auth::{AuthContext, ClientIp, OptionalAuth, verify_csrf};
use crate::http::middleware::locale::RequestLocale;
use crate::http::responses::{take_flash, with_flash_redirect};
use crate::i18n::{Locale, format_price};
use crate::infrastructure::database::queries::find_establishment_for_account;
use crate::rendering::affiliates::{
    AffiliatePortalTemplate, ReferralView, affiliate_status_meta, commission_unit_key,
    map_commission_view, map_payout_view, referral_status_meta,
};
use crate::rendering::{PageMeta, render_html};

pub const REFERRAL_COOKIE_NAME: &str = "qr_ref";
const PORTAL_PATH: &str = "/dashboard/affiliate";

pub fn active_billing_env(state: &AppState) -> &'static str {
    state
        .asaas
        .as_ref()
        .map_or("production", |client| client.config().environment.as_str())
}

pub fn referral_cookie_value(jar: &CookieJar) -> Option<&str> {
    jar.get(REFERRAL_COOKIE_NAME)
        .map(Cookie::value)
        .map(str::trim)
        .filter(|v| !v.is_empty() && v.len() <= 32)
}

pub fn build_referral_cookie(code: String, ttl_days: i64, secure: bool) -> Cookie<'static> {
    let max_age = ttl_days.clamp(1, 180) * 86_400;
    let secure_attr = if secure { "; Secure" } else { "" };
    let header = format!(
        "{REFERRAL_COOKIE_NAME}={code}; Path=/; HttpOnly; SameSite=Lax; Max-Age={max_age}{secure_attr}"
    );
    Cookie::parse(header).unwrap_or_else(|_| {
        Cookie::build((REFERRAL_COOKIE_NAME, code))
            .path("/")
            .http_only(true)
            .same_site(SameSite::Lax)
            .secure(secure)
            .build()
    })
}

pub fn build_clear_referral_cookie(secure: bool) -> Cookie<'static> {
    let mut cookie = Cookie::build((REFERRAL_COOKIE_NAME, ""))
        .path("/")
        .http_only(true)
        .same_site(SameSite::Lax)
        .secure(secure)
        .build();
    cookie.make_removal();
    cookie
}

pub fn referral_link_url(base_url: &str, code: &str) -> String {
    format!("{}/r/{code}", base_url.trim_end_matches('/'))
}

/// Captures an incoming referral code (if active), sets the `qr_ref` cookie, and redirects
/// with `Cache-Control: no-store` to a clean canonical path so `?ref=` is never cached or indexed.
pub async fn capture_referral_redirect(
    state: &AppState,
    ip: IpAddr,
    jar: CookieJar,
    raw_code: &str,
    target_path: &str,
    enforce_rate_limit: bool,
) -> AppResult<Response> {
    if state
        .rate_limiters
        .affiliate_click_ip
        .check_key(&ip)
        .is_err()
    {
        if enforce_rate_limit {
            return Err(AppError::RateLimited);
        }
        return Ok(no_store_redirect(jar, target_path));
    }

    let existing = referral_cookie_value(&jar);
    let jar = match service::record_referral_click(&state.pool, raw_code, existing).await? {
        Some((code, ttl_days)) => jar.add(build_referral_cookie(
            code,
            ttl_days,
            state.config.cookie_secure,
        )),
        None => jar,
    };

    Ok(no_store_redirect(jar, target_path))
}

fn no_store_redirect(jar: CookieJar, location: &str) -> Response {
    let mut res = (jar, Redirect::to(location)).into_response();
    res.headers_mut().insert(
        CACHE_CONTROL,
        axum::http::HeaderValue::from_static("no-store"),
    );
    res
}

#[derive(Deserialize)]
pub struct ReferralRedirectQuery {
    #[serde(default)]
    pub plan: String,
    #[serde(default)]
    pub lang: String,
}

pub async fn referral_redirect(
    State(state): State<AppState>,
    OptionalAuth(auth): OptionalAuth,
    ClientIp(ip): ClientIp,
    jar: CookieJar,
    Path(code): Path<String>,
    Query(query): Query<ReferralRedirectQuery>,
) -> AppResult<Response> {
    let base_path = match Locale::parse(&query.lang) {
        Some(Locale::En) => "/en",
        Some(Locale::Es) => "/es",
        _ => "/",
    };
    let target = if state.asaas.is_some()
        && let Some(plan) = BillingPlan::from_key(&query.plan)
    {
        format!("/register?plan={}", plan.key())
    } else {
        base_path.to_owned()
    };
    if auth.is_some() {
        return Ok(no_store_redirect(jar, &target));
    }
    capture_referral_redirect(&state, ip, jar, &code, &target, true).await
}

pub async fn portal_page(
    State(state): State<AppState>,
    RequestLocale(locale): RequestLocale,
    auth: AuthContext,
    jar: CookieJar,
) -> AppResult<Response> {
    let env = active_billing_env(&state);
    let est = find_establishment_for_account(&state.pool, &auth.account.id).await?;
    let data = service::load_affiliate_portal(&state.pool, &auth.account.id, env).await?;
    let (jar, flash) = take_flash(jar);

    let default_quota_display = format_commission_rate(
        data.settings.default_commission_type,
        data.settings.default_commission_value,
        locale,
    );
    let default_quota_unit_key = commission_unit_key(data.settings.default_commission_type);

    let (
        is_enrolled,
        referral_url,
        code,
        status_key,
        status_badge,
        quota_display,
        quota_unit_key,
        hold_days,
        pix_key,
        click_count,
    ) = match &data.affiliate {
        Some(aff) => {
            let (sk, sb) = affiliate_status_meta(aff.status);
            (
                true,
                referral_link_url(&state.config.public_base_url, &aff.code),
                aff.code.clone(),
                sk,
                sb,
                format_commission_rate(aff.commission_type, aff.commission_value, locale),
                commission_unit_key(aff.commission_type),
                aff.hold_days,
                aff.pix_key.clone(),
                aff.click_count,
            )
        }
        None => (
            false,
            String::new(),
            String::new(),
            "affiliate.status_not_enrolled",
            "badge-neutral",
            default_quota_display.clone(),
            default_quota_unit_key,
            data.settings.default_hold_days,
            String::new(),
            0,
        ),
    };

    let referrals_total = data.referrals.len() as i64;
    let referrals_active = data
        .referrals
        .iter()
        .filter(|r| r.status == crate::domain::affiliate::ReferralStatus::Active)
        .count() as i64;

    let referrals = data
        .referrals
        .into_iter()
        .map(|r| {
            let (sk, sb) = referral_status_meta(r.status);
            ReferralView {
                id: r.id,
                account_label: mask_email_for_partner(&r.referred_email),
                has_establishment: r.establishment_name.is_some(),
                establishment_name: r.establishment_name.unwrap_or_default(),
                establishment_slug: r.establishment_slug.unwrap_or_default(),
                status_key: sk,
                status_badge: sb,
                can_disqualify: false,
                disqualified_reason: r.disqualified_reason.unwrap_or_default(),
                attributed_at: r.attributed_at,
                first_paid_at: r.first_paid_at,
            }
        })
        .collect();

    let commissions = data
        .commissions
        .into_iter()
        .map(|c| map_commission_view(c, locale))
        .collect();
    let payouts = data
        .payouts
        .into_iter()
        .map(|p| map_payout_view(p, locale))
        .collect();

    let tpl = AffiliatePortalTemplate {
        page: PageMeta::new(locale, "affiliate.page_title", flash),
        brand_name: est
            .as_ref()
            .map_or_else(|| "QR Cardápio".to_owned(), |e| e.name.clone()),
        has_establishment: est.is_some(),
        has_theme: est.is_some(),
        is_admin: auth.account.role == AccountRole::Admin,
        active_nav: "affiliate",
        csrf_token: auth.csrf_token,
        billing_env: env.to_owned(),
        self_signup_enabled: data.settings.self_signup_enabled,
        default_quota_display,
        default_quota_unit_key,
        default_hold_days: data.settings.default_hold_days,
        cookie_ttl_days: data.settings.cookie_ttl_days,
        is_enrolled,
        referral_url,
        code,
        status_key,
        status_badge,
        quota_display,
        quota_unit_key,
        hold_days,
        pix_key,
        click_count,
        referrals_total,
        referrals_active,
        pending_formatted: format_price(data.balances.pending_centavos, locale),
        available_formatted: format_price(data.balances.available_centavos, locale),
        paid_formatted: format_price(data.balances.paid_centavos, locale),
        referrals,
        commissions,
        payouts,
    };

    Ok((jar, render_html(&tpl, StatusCode::OK)?).into_response())
}

#[derive(Deserialize)]
pub struct ActivateAffiliateForm {
    pub _csrf: String,
    #[serde(default)]
    pub code: String,
    #[serde(default)]
    pub pix_key: String,
}

pub async fn activate_submit(
    State(state): State<AppState>,
    auth: AuthContext,
    jar: CookieJar,
    Form(form): Form<ActivateAffiliateForm>,
) -> AppResult<Response> {
    verify_csrf(&auth, &form._csrf)?;
    match service::self_activate_affiliate(
        &state.pool,
        &auth.account.id,
        &auth.account.email,
        SelfActivateAffiliateInput {
            desired_code: &form.code,
            pix_key: &form.pix_key,
        },
    )
    .await
    {
        Ok(_) => Ok(with_flash_redirect(
            jar,
            PORTAL_PATH,
            false,
            "flash.affiliate_activated",
            state.config.cookie_secure,
        )),
        Err(err) if err.status_code().is_client_error() => Ok(with_flash_redirect(
            jar,
            PORTAL_PATH,
            true,
            err.i18n_key(),
            state.config.cookie_secure,
        )),
        Err(err) => Err(err),
    }
}

#[derive(Deserialize)]
pub struct UpdatePixForm {
    pub _csrf: String,
    #[serde(default)]
    pub pix_key: String,
}

pub async fn update_pix_submit(
    State(state): State<AppState>,
    auth: AuthContext,
    jar: CookieJar,
    Form(form): Form<UpdatePixForm>,
) -> AppResult<Response> {
    verify_csrf(&auth, &form._csrf)?;
    match service::update_own_pix_key(&state.pool, &auth.account.id, &form.pix_key).await {
        Ok(()) => Ok(with_flash_redirect(
            jar,
            PORTAL_PATH,
            false,
            "flash.affiliate_pix_saved",
            state.config.cookie_secure,
        )),
        Err(err) if err.status_code().is_client_error() => Ok(with_flash_redirect(
            jar,
            PORTAL_PATH,
            true,
            err.i18n_key(),
            state.config.cookie_secure,
        )),
        Err(err) => Err(err),
    }
}
