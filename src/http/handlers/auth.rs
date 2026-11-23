use axum::Form;
use axum::extract::{Query, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Redirect, Response};
use axum_extra::extract::CookieJar;
use serde::Deserialize;

use super::onboarding::{PLAN_ONBOARDING, PlanIntent};
use crate::app::AppState;
use crate::application::auth::{login, logout, redeem_password_reset, register_owner};
use crate::error::AppResult;
use crate::http::middleware::auth::{
    AuthContext, ClientIp, OptionalAuth, build_clear_session_cookie, build_session_cookie,
    verify_csrf,
};
use crate::http::middleware::locale::RequestLocale;
use crate::http::responses::{take_flash, with_flash_redirect};
use crate::rendering::auth::{LoginTemplate, RegisterTemplate, ResetPasswordTemplate};
use crate::rendering::{PageMeta, render_html};

#[derive(Deserialize)]
pub struct LoginForm {
    pub email: String,
    pub password: String,
}

#[derive(Deserialize)]
pub struct RegisterForm {
    pub email: String,
    pub password: String,
    pub password_confirm: String,
    #[serde(default)]
    pub website: String,
}

#[derive(Deserialize)]
pub struct ResetForm {
    pub token: String,
    pub new_password: String,
    pub confirm_password: String,
}

#[derive(Deserialize)]
pub struct CsrfOnlyForm {
    pub _csrf: String,
}

pub async fn login_page(
    State(state): State<AppState>,
    RequestLocale(locale): RequestLocale,
    OptionalAuth(auth): OptionalAuth,
    jar: CookieJar,
    Query(plan): Query<PlanIntent>,
) -> AppResult<Response> {
    let subscribe_plan = plan.enabled(&state);
    if auth.is_some() {
        let destination = if subscribe_plan {
            PLAN_ONBOARDING
        } else {
            "/dashboard"
        };
        return Ok(Redirect::to(destination).into_response());
    }
    let (jar, flash) = take_flash(jar);
    let tpl = LoginTemplate {
        page: PageMeta::new(locale, "auth.login_title", flash),
        subscribe_plan,
        email_value: String::new(),
        error_key: None,
    };
    Ok((jar, render_html(&tpl, StatusCode::OK)?).into_response())
}

pub async fn login_submit(
    State(state): State<AppState>,
    RequestLocale(locale): RequestLocale,
    ClientIp(ip): ClientIp,
    jar: CookieJar,
    Query(plan): Query<PlanIntent>,
    Form(form): Form<LoginForm>,
) -> AppResult<Response> {
    let subscribe_plan = plan.enabled(&state);
    if state.rate_limiters.login_ip.check_key(&ip).is_err() {
        let tpl = LoginTemplate {
            page: PageMeta::new(locale, "auth.login_title", None),
            subscribe_plan,
            email_value: form.email,
            error_key: Some("error.rate_limited".to_string()),
        };
        return render_html(&tpl, StatusCode::TOO_MANY_REQUESTS);
    }

    match login(&state.pool, &state.passwords, &form.email, &form.password).await {
        Ok(success) => {
            let cookie =
                build_session_cookie(success.raw_session_token, state.config.cookie_secure);
            Ok(with_flash_redirect(
                jar.add(cookie),
                if subscribe_plan {
                    PLAN_ONBOARDING
                } else {
                    "/dashboard"
                },
                false,
                "flash.logged_in",
                state.config.cookie_secure,
            ))
        }
        Err(err) => {
            let tpl = LoginTemplate {
                page: PageMeta::new(locale, "auth.login_title", None),
                subscribe_plan,
                email_value: form.email,
                error_key: Some(err.i18n_key().to_string()),
            };
            render_html(&tpl, StatusCode::UNAUTHORIZED)
        }
    }
}

pub async fn register_page(
    State(state): State<AppState>,
    RequestLocale(locale): RequestLocale,
    OptionalAuth(auth): OptionalAuth,
    jar: CookieJar,
    Query(plan): Query<PlanIntent>,
) -> AppResult<Response> {
    let subscribe_plan = plan.enabled(&state);
    if auth.is_some() {
        let destination = if subscribe_plan {
            PLAN_ONBOARDING
        } else {
            "/dashboard"
        };
        return Ok(Redirect::to(destination).into_response());
    }
    let (jar, flash) = take_flash(jar);
    let tpl = RegisterTemplate {
        page: PageMeta::new(locale, "auth.register_title", flash),
        subscribe_plan,
        email_value: String::new(),
        error_key: None,
    };
    Ok((jar, render_html(&tpl, StatusCode::OK)?).into_response())
}

pub async fn register_submit(
    State(state): State<AppState>,
    RequestLocale(locale): RequestLocale,
    ClientIp(ip): ClientIp,
    jar: CookieJar,
    Query(plan): Query<PlanIntent>,
    Form(form): Form<RegisterForm>,
) -> AppResult<Response> {
    let subscribe_plan = plan.enabled(&state);
    // Honeypot field traps naive automated spam bots silently.
    if !form.website.trim().is_empty() {
        return Ok(Redirect::to("/login").into_response());
    }

    if state.rate_limiters.register_ip.check_key(&ip).is_err() {
        let tpl = RegisterTemplate {
            page: PageMeta::new(locale, "auth.register_title", None),
            subscribe_plan,
            email_value: form.email,
            error_key: Some("error.rate_limited".to_string()),
        };
        return render_html(&tpl, StatusCode::TOO_MANY_REQUESTS);
    }

    match register_owner(
        &state.pool,
        &state.passwords,
        &form.email,
        &form.password,
        &form.password_confirm,
        locale,
    )
    .await
    {
        Ok(success) => {
            let cookie =
                build_session_cookie(success.raw_session_token, state.config.cookie_secure);
            Ok(with_flash_redirect(
                jar.add(cookie),
                if subscribe_plan {
                    PLAN_ONBOARDING
                } else {
                    "/onboarding"
                },
                false,
                "flash.registered",
                state.config.cookie_secure,
            ))
        }
        Err(err) => {
            let status = err.status_code();
            let tpl = RegisterTemplate {
                page: PageMeta::new(locale, "auth.register_title", None),
                subscribe_plan,
                email_value: form.email,
                error_key: Some(err.i18n_key().to_string()),
            };
            render_html(&tpl, status)
        }
    }
}

pub async fn logout_submit(
    State(state): State<AppState>,
    auth: AuthContext,
    jar: CookieJar,
    Form(form): Form<CsrfOnlyForm>,
) -> AppResult<Response> {
    verify_csrf(&auth, &form._csrf)?;
    if let Some(cookie) = jar.get(crate::http::middleware::auth::SESSION_COOKIE_NAME) {
        logout(&state.pool, cookie.value()).await?;
    }
    let cleared = build_clear_session_cookie(state.config.cookie_secure);
    Ok(with_flash_redirect(
        jar.add(cleared),
        "/login",
        false,
        "flash.logged_out",
        state.config.cookie_secure,
    ))
}

pub async fn reset_password_page(
    RequestLocale(locale): RequestLocale,
    jar: CookieJar,
) -> AppResult<Response> {
    let (jar, flash) = take_flash(jar);
    let tpl = ResetPasswordTemplate {
        page: PageMeta::new(locale, "auth.reset_title", flash),
        error_key: None,
    };
    Ok((jar, render_html(&tpl, StatusCode::OK)?).into_response())
}

pub async fn reset_password_submit(
    State(state): State<AppState>,
    RequestLocale(locale): RequestLocale,
    ClientIp(ip): ClientIp,
    jar: CookieJar,
    Form(form): Form<ResetForm>,
) -> AppResult<Response> {
    if state.rate_limiters.reset_ip.check_key(&ip).is_err() {
        let tpl = ResetPasswordTemplate {
            page: PageMeta::new(locale, "auth.reset_title", None),
            error_key: Some("error.rate_limited".to_string()),
        };
        return render_html(&tpl, StatusCode::TOO_MANY_REQUESTS);
    }

    match redeem_password_reset(
        &state.pool,
        &state.passwords,
        &form.token,
        &form.new_password,
        &form.confirm_password,
    )
    .await
    {
        Ok(()) => Ok(with_flash_redirect(
            jar,
            "/login",
            false,
            "flash.password_reset",
            state.config.cookie_secure,
        )),
        Err(err) => {
            let tpl = ResetPasswordTemplate {
                page: PageMeta::new(locale, "auth.reset_title", None),
                error_key: Some(err.i18n_key().to_string()),
            };
            render_html(&tpl, err.status_code())
        }
    }
}
