use askama::Template;
use axum::Form;
use axum::extract::State;
use axum::http::StatusCode;
use axum::response::{IntoResponse, Redirect, Response};
use chrono::{NaiveDate, Utc};
use serde::Deserialize;

use super::allowed;
use crate::app::AppState;
use crate::domain::account::AccountRole;
use crate::domain::commerce::business_hours::{
    CommercialSettings, SUPPORTED_ZONES, ServiceMode, Windows, zone,
};
use crate::domain::commerce::pricing::MAX_ORDER_TOTAL_CENTAVOS;
use crate::domain::entitlements::Operation;
use crate::domain::product::parse_price_centavos;
use crate::error::{AppError, AppResult};
use crate::http::middleware::auth::{TenantContext, verify_csrf};
use crate::http::middleware::locale::RequestLocale;
use crate::infrastructure::database::queries::business_hours;
use crate::rendering::{PageMeta, render_html};

const PATH: &str = "/dashboard/hours";
const INVALID: AppError = AppError::Validation("commerce.hours.invalid");

#[derive(Template)]
#[template(path = "dashboard/commerce/hours.html")]
struct HoursPage {
    page: PageMeta,
    brand_name: String,
    csrf_token: String,
    is_admin: bool,
    has_establishment: bool,
    has_theme: bool,
    active_nav: &'static str,
    zones: [&'static str; 5],
    settings: CommercialSettings,
    days: Vec<DayInput>,
    exceptions: Vec<(String, String)>,
    paused: bool,
    open: bool,
}

struct DayInput {
    field: &'static str,
    key: &'static str,
    windows: String,
}

pub async fn page(
    State(state): State<AppState>,
    tenant: TenantContext,
    RequestLocale(locale): RequestLocale,
) -> AppResult<Response> {
    allowed(&state, &tenant, Operation::ManageBusinessHours).await?;
    let schedule = business_hours::load(&state.pool, &tenant.establishment.id).await?;
    let time = Utc::now();
    let paused = schedule.settings.paused_at(time);
    let open = schedule.service_at(time, ServiceMode::Delivery).is_some()
        || schedule.service_at(time, ServiceMode::Pickup).is_some();
    let days = [
        ("mon", "commerce.hours.monday"),
        ("tue", "commerce.hours.tuesday"),
        ("wed", "commerce.hours.wednesday"),
        ("thu", "commerce.hours.thursday"),
        ("fri", "commerce.hours.friday"),
        ("sat", "commerce.hours.saturday"),
        ("sun", "commerce.hours.sunday"),
    ]
    .into_iter()
    .zip(schedule.weekly)
    .map(|((field, key), windows)| DayInput {
        field,
        key,
        windows: windows.to_string(),
    })
    .collect();
    render_html(
        &HoursPage {
            page: PageMeta::new(locale, "commerce.hours.title", None),
            brand_name: tenant.establishment.name,
            csrf_token: tenant.auth.csrf_token,
            is_admin: tenant.auth.account.role == AccountRole::Admin,
            has_establishment: true,
            has_theme: true,
            active_nav: "hours",
            zones: SUPPORTED_ZONES,
            settings: schedule.settings,
            days,
            exceptions: schedule
                .exceptions
                .into_iter()
                .map(|(date, windows)| (date.to_string(), windows.to_string()))
                .collect(),
            paused,
            open,
        },
        StatusCode::OK,
    )
}

#[derive(Deserialize)]
pub struct SettingsForm {
    _csrf: String,
    time_zone: String,
    delivery: Option<String>,
    pickup: Option<String>,
    minimum: String,
    acceptance_minutes: i64,
}

async fn write(state: &AppState, tenant: &TenantContext, csrf: &str) -> AppResult<()> {
    verify_csrf(&tenant.auth, csrf)?;
    allowed(state, tenant, Operation::ManageBusinessHours).await
}

pub async fn settings(
    State(state): State<AppState>,
    tenant: TenantContext,
    Form(form): Form<SettingsForm>,
) -> AppResult<Response> {
    write(&state, &tenant, &form._csrf).await?;
    let minimum_centavos = parse_price_centavos(&form.minimum)?;
    if zone(&form.time_zone).is_none()
        || !(3..=30).contains(&form.acceptance_minutes)
        || minimum_centavos > MAX_ORDER_TOTAL_CENTAVOS
    {
        return Err(INVALID);
    }
    business_hours::save_settings(
        &state.pool,
        &tenant.establishment.id,
        &CommercialSettings {
            time_zone: form.time_zone,
            delivery: form.delivery.is_some(),
            pickup: form.pickup.is_some(),
            minimum_centavos,
            acceptance_minutes: form.acceptance_minutes,
            ..Default::default()
        },
    )
    .await?;
    Ok(Redirect::to(PATH).into_response())
}

#[derive(Deserialize)]
pub struct WeeklyForm {
    _csrf: String,
    mon: String,
    tue: String,
    wed: String,
    thu: String,
    fri: String,
    sat: String,
    sun: String,
}

pub async fn weekly(
    State(state): State<AppState>,
    tenant: TenantContext,
    Form(form): Form<WeeklyForm>,
) -> AppResult<Response> {
    write(&state, &tenant, &form._csrf).await?;
    let raw = [
        form.mon, form.tue, form.wed, form.thu, form.fri, form.sat, form.sun,
    ];
    let week = raw
        .map(|value| value.parse::<Windows>())
        .into_iter()
        .collect::<Result<Vec<_>, _>>()
        .map_err(|_| INVALID)?;
    let week: [Windows; 7] = week.try_into().map_err(|_| INVALID)?;
    business_hours::save_weekly(&state.pool, &tenant.establishment.id, &week).await?;
    Ok(Redirect::to(PATH).into_response())
}

#[derive(Deserialize)]
pub struct ExceptionForm {
    _csrf: String,
    date: NaiveDate,
    #[serde(default)]
    windows: String,
}

pub async fn exception(
    State(state): State<AppState>,
    tenant: TenantContext,
    Form(form): Form<ExceptionForm>,
) -> AppResult<Response> {
    write(&state, &tenant, &form._csrf).await?;
    if form.date.to_string().len() != 10 {
        return Err(INVALID);
    }
    let windows = form.windows.parse().map_err(|_| INVALID)?;
    business_hours::save_exception(&state.pool, &tenant.establishment.id, form.date, &windows)
        .await?;
    Ok(Redirect::to(PATH).into_response())
}

pub async fn remove_exception(
    State(state): State<AppState>,
    tenant: TenantContext,
    Form(form): Form<ExceptionForm>,
) -> AppResult<Response> {
    write(&state, &tenant, &form._csrf).await?;
    business_hours::delete_exception(&state.pool, &tenant.establishment.id, form.date).await?;
    Ok(Redirect::to(PATH).into_response())
}

#[derive(Deserialize)]
pub struct PauseForm {
    _csrf: String,
    duration: String,
}

pub async fn pause(
    State(state): State<AppState>,
    tenant: TenantContext,
    Form(form): Form<PauseForm>,
) -> AppResult<Response> {
    write(&state, &tenant, &form._csrf).await?;
    let (paused, until) = match form.duration.as_str() {
        "resume" => (false, None),
        "indefinite" => (true, None),
        "15" | "30" | "60" => {
            let minutes: i64 = form.duration.parse().map_err(|_| INVALID)?;
            (true, Utc::now().timestamp().checked_add(minutes * 60))
        }
        _ => return Err(INVALID),
    };
    business_hours::set_pause(&state.pool, &tenant.establishment.id, paused, until).await?;
    Ok(Redirect::to(PATH).into_response())
}
