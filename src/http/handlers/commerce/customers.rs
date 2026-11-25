use askama::Template;
use axum::extract::{Path, Query, State};
use axum::http::{HeaderValue, StatusCode, header};
use axum::response::{IntoResponse, Redirect, Response};
use axum::{Form, Json};
use chrono::Utc;
use serde::Deserialize;
use uuid::Uuid;

use super::allowed;
use crate::app::AppState;
use crate::domain::account::AccountRole;
use crate::domain::commerce::customer::{CustomerProfile, FEDERAL_UNITS, PostalAddress};
use crate::domain::entitlements::Operation;
use crate::error::{AppError, AppResult};
use crate::http::handlers::categories::CsrfOnlyForm;
use crate::http::middleware::auth::{TenantContext, verify_csrf};
use crate::http::middleware::locale::RequestLocale;
use crate::i18n::Locale;
use crate::infrastructure::database::queries::{customer_addresses, customer_privacy, customers};
use crate::rendering::{PageMeta, render_html};

const PATH: &str = "/dashboard/customers";

#[derive(Template)]
#[template(path = "dashboard/commerce/customers.html")]
struct CustomerPage {
    page: PageMeta,
    brand_name: String,
    csrf_token: String,
    is_admin: bool,
    has_establishment: bool,
    has_theme: bool,
    active_nav: &'static str,
    view: CustomerView,
}

enum CustomerView {
    Index {
        customers: Vec<customers::Customer>,
        previous: Option<i64>,
        next: Option<i64>,
    },
    Detail(Box<CustomerDetail>),
}

struct CustomerDetail {
    customer: customers::Customer,
    addresses: Vec<customer_addresses::CustomerAddress>,
    blank_address: PostalAddress,
    states: [&'static str; 27],
    can_export: bool,
    can_erase: bool,
}

fn render(tenant: TenantContext, locale: Locale, view: CustomerView) -> AppResult<Response> {
    let mut response = render_html(
        &CustomerPage {
            page: PageMeta::new(locale, "commerce.customer.title", None),
            brand_name: tenant.establishment.name,
            csrf_token: tenant.auth.csrf_token,
            is_admin: tenant.auth.account.role == AccountRole::Admin,
            has_establishment: true,
            has_theme: true,
            active_nav: "customers",
            view,
        },
        StatusCode::OK,
    )?;
    response
        .headers_mut()
        .insert(header::CACHE_CONTROL, HeaderValue::from_static("no-store"));
    Ok(response)
}

#[derive(Deserialize)]
pub struct CustomerQuery {
    page: Option<i64>,
}

pub async fn index(
    State(state): State<AppState>,
    tenant: TenantContext,
    RequestLocale(locale): RequestLocale,
    Query(query): Query<CustomerQuery>,
) -> AppResult<Response> {
    allowed(&state, &tenant, Operation::ManageCustomers).await?;
    let page = query.page.unwrap_or(1).clamp(1, u32::MAX.into());
    let mut customers = customers::page(&state.pool, &tenant.establishment.id, page).await?;
    let next = (customers.len() > customers::PAGE_SIZE as usize).then_some(page + 1);
    customers.truncate(customers::PAGE_SIZE as usize);
    render(
        tenant,
        locale,
        CustomerView::Index {
            customers,
            previous: (page > 1).then_some(page - 1),
            next,
        },
    )
}

pub async fn detail(
    State(state): State<AppState>,
    tenant: TenantContext,
    RequestLocale(locale): RequestLocale,
    Path(id): Path<String>,
) -> AppResult<Response> {
    allowed(&state, &tenant, Operation::ManageCustomers).await?;
    let est = &tenant.establishment.id;
    let customer = customers::find(&state.pool, est, &id).await?;
    let addresses = customer_addresses::list(&state.pool, est, &id).await?;
    let can_export = Operation::ExportData.permits(tenant.role);
    let can_erase = Operation::EraseCustomer.permits(tenant.role);
    render(
        tenant,
        locale,
        CustomerView::Detail(Box::new(CustomerDetail {
            customer,
            addresses,
            blank_address: PostalAddress::default(),
            states: FEDERAL_UNITS,
            can_export,
            can_erase,
        })),
    )
}

#[derive(Deserialize)]
pub struct ProfileForm {
    _csrf: String,
    #[serde(flatten)]
    profile: CustomerProfile,
}

async fn write(
    state: &AppState,
    tenant: &TenantContext,
    csrf: &str,
    operation: Operation,
) -> AppResult<()> {
    verify_csrf(&tenant.auth, csrf)?;
    allowed(state, tenant, operation).await
}

pub async fn create(
    State(state): State<AppState>,
    tenant: TenantContext,
    Form(form): Form<ProfileForm>,
) -> AppResult<Response> {
    write(&state, &tenant, &form._csrf, Operation::ManageCustomers).await?;
    let id = customers::create(
        &state.pool,
        &tenant.establishment.id,
        &form.profile.normalized()?,
        &Utc::now().to_rfc3339(),
    )
    .await?;
    Ok(Redirect::to(&format!("{PATH}/{id}")).into_response())
}

pub async fn update(
    State(state): State<AppState>,
    tenant: TenantContext,
    Path(id): Path<String>,
    Form(form): Form<ProfileForm>,
) -> AppResult<Response> {
    write(&state, &tenant, &form._csrf, Operation::ManageCustomers).await?;
    customers::update(
        &state.pool,
        &tenant.establishment.id,
        &id,
        &form.profile.normalized()?,
        &Utc::now().to_rfc3339(),
    )
    .await?;
    Ok(Redirect::to(&format!("{PATH}/{id}")).into_response())
}

#[derive(Deserialize)]
pub struct AddressForm {
    _csrf: String,
    #[serde(default)]
    id: String,
    #[serde(flatten)]
    address: PostalAddress,
}

pub async fn save_address(
    State(state): State<AppState>,
    tenant: TenantContext,
    Path(id): Path<String>,
    Form(form): Form<AddressForm>,
) -> AppResult<Response> {
    write(&state, &tenant, &form._csrf, Operation::ManageCustomers).await?;
    let address_id = if form.id.is_empty() {
        None
    } else {
        Some(
            Uuid::parse_str(&form.id)
                .map_err(|_| AppError::NotFound("error.not_found"))?
                .to_string(),
        )
    };
    customer_addresses::save(
        &state.pool,
        &tenant.establishment.id,
        &id,
        address_id.as_deref(),
        &form.address.normalized()?,
    )
    .await?;
    Ok(Redirect::to(&format!("{PATH}/{id}")).into_response())
}

pub async fn remove_address(
    State(state): State<AppState>,
    tenant: TenantContext,
    Path((id, address_id)): Path<(String, String)>,
    Form(form): Form<CsrfOnlyForm>,
) -> AppResult<Response> {
    write(&state, &tenant, &form._csrf, Operation::ManageCustomers).await?;
    customer_addresses::delete(&state.pool, &tenant.establishment.id, &id, &address_id).await?;
    Ok(Redirect::to(&format!("{PATH}/{id}")).into_response())
}

pub async fn export(
    State(state): State<AppState>,
    tenant: TenantContext,
    Path(id): Path<String>,
    Form(form): Form<CsrfOnlyForm>,
) -> AppResult<Response> {
    write(&state, &tenant, &form._csrf, Operation::ExportData).await?;
    let export = customer_privacy::export(&state.pool, &tenant.establishment.id, &id).await?;
    Ok((
        [
            (header::CACHE_CONTROL, "no-store"),
            (
                header::CONTENT_DISPOSITION,
                "attachment; filename=\"customer-export.json\"",
            ),
        ],
        Json(export),
    )
        .into_response())
}

#[derive(Deserialize)]
pub struct EraseForm {
    _csrf: String,
    confirm: Option<String>,
}

pub async fn erase(
    State(state): State<AppState>,
    tenant: TenantContext,
    Path(id): Path<String>,
    Form(form): Form<EraseForm>,
) -> AppResult<Response> {
    write(&state, &tenant, &form._csrf, Operation::EraseCustomer).await?;
    if form.confirm.as_deref() != Some("1") {
        return Err(AppError::Validation("commerce.customer.confirm_erase"));
    }
    customer_privacy::erase(
        &state.pool,
        &tenant.establishment.id,
        &id,
        &Utc::now().to_rfc3339(),
    )
    .await?;
    Ok(Redirect::to(PATH).into_response())
}
