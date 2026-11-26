use std::collections::HashSet;

use askama::Template;
use axum::Form;
use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Redirect, Response};
use chrono::Utc;
use serde::Deserialize;

use super::allowed;
use crate::app::AppState;
use crate::domain::account::AccountRole;
use crate::domain::commerce::catalog_options::{AddonGroup, AddonOption, ProductVariation};
use crate::domain::entitlements::Operation;
use crate::error::{AppError, AppResult};
use crate::http::handlers::categories::CsrfOnlyForm;
use crate::http::middleware::auth::{TenantContext, verify_csrf};
use crate::http::middleware::locale::RequestLocale;
use crate::i18n::Locale;
use crate::infrastructure::database::queries::{
    addon_groups, addon_options, find_product_for_est, menu_options, product_variations,
};
use crate::rendering::{PageMeta, render_html};

mod inputs;
use inputs::OptionForm;

const GROUPS_PATH: &str = "/dashboard/menu/addons";
const MISSING: AppError = AppError::NotFound("error.not_found");

#[derive(Template)]
#[template(path = "dashboard/commerce/catalog.html")]
struct CatalogTemplate {
    page: PageMeta,
    brand_name: String,
    csrf_token: String,
    is_admin: bool,
    has_establishment: bool,
    has_theme: bool,
    active_nav: &'static str,
    view: CatalogView,
}

enum CatalogView {
    Groups {
        groups: Vec<AddonGroup>,
        public_options_enabled: bool,
    },
    Group {
        group: AddonGroup,
        options: Vec<AddonOption>,
    },
    Product {
        id: String,
        name: String,
        variations: Vec<ProductVariation>,
        groups: Vec<ProductGroup>,
    },
}

struct ProductGroup {
    group: AddonGroup,
    linked: bool,
}

fn render(
    tenant: TenantContext,
    locale: Locale,
    title: &'static str,
    view: CatalogView,
) -> AppResult<Response> {
    render_html(
        &CatalogTemplate {
            page: PageMeta::new(locale, title, None),
            brand_name: tenant.establishment.name,
            csrf_token: tenant.auth.csrf_token,
            is_admin: tenant.auth.account.role == AccountRole::Admin,
            has_establishment: true,
            has_theme: true,
            active_nav: "menu",
            view,
        },
        StatusCode::OK,
    )
}

pub async fn groups(
    State(state): State<AppState>,
    tenant: TenantContext,
    RequestLocale(locale): RequestLocale,
) -> AppResult<Response> {
    allowed(&state, &tenant, Operation::EditMenuDraft).await?;
    let groups =
        addon_groups::list_for_establishment(&state.pool, &tenant.establishment.id).await?;
    let public_options_enabled =
        menu_options::enabled(&state.pool, &tenant.establishment.id).await?;
    render(
        tenant,
        locale,
        "commerce.catalog.groups",
        CatalogView::Groups {
            groups,
            public_options_enabled,
        },
    )
}

pub async fn group(
    State(state): State<AppState>,
    tenant: TenantContext,
    RequestLocale(locale): RequestLocale,
    Path(id): Path<String>,
) -> AppResult<Response> {
    allowed(&state, &tenant, Operation::EditMenuDraft).await?;
    let est = &tenant.establishment.id;
    let group = addon_groups::list_for_establishment(&state.pool, est)
        .await?
        .into_iter()
        .find(|group| group.id == id)
        .ok_or(MISSING)?;
    let options = addon_options::list_for_group(&state.pool, est, &id).await?;
    render(
        tenant,
        locale,
        "commerce.catalog.groups",
        CatalogView::Group { group, options },
    )
}

pub async fn product(
    State(state): State<AppState>,
    tenant: TenantContext,
    RequestLocale(locale): RequestLocale,
    Path(id): Path<String>,
) -> AppResult<Response> {
    allowed(&state, &tenant, Operation::EditMenuDraft).await?;
    let est = &tenant.establishment.id;
    let product = find_product_for_est(&state.pool, est, &id)
        .await?
        .ok_or(MISSING)?;
    let variations = product_variations::list_for_product(&state.pool, est, &id).await?;
    let links: HashSet<_> = addon_groups::linked_ids(&state.pool, est, &id)
        .await?
        .into_iter()
        .collect();
    let groups = addon_groups::list_for_establishment(&state.pool, est)
        .await?
        .into_iter()
        .map(|group| ProductGroup {
            linked: links.contains(&group.id),
            group,
        })
        .collect();
    render(
        tenant,
        locale,
        "commerce.catalog.product_options",
        CatalogView::Product {
            id,
            name: product.effective_name(locale).into(),
            variations,
            groups,
        },
    )
}

async fn write(state: &AppState, tenant: &TenantContext, csrf: &str) -> AppResult<()> {
    verify_csrf(&tenant.auth, csrf)?;
    allowed(state, tenant, Operation::EditMenuDraft).await
}

#[derive(Deserialize)]
pub struct PublicOptionsForm {
    _csrf: String,
    enabled: Option<String>,
}

pub async fn set_public_options(
    State(state): State<AppState>,
    tenant: TenantContext,
    Form(form): Form<PublicOptionsForm>,
) -> AppResult<Response> {
    write(&state, &tenant, &form._csrf).await?;
    let enabled = match form.enabled.as_deref() {
        Some("1") => true,
        None => false,
        _ => return Err(AppError::Validation("commerce.catalog.invalid_options")),
    };
    menu_options::set_enabled(
        &state.pool,
        &tenant.establishment.id,
        enabled,
        &Utc::now().to_rfc3339(),
    )
    .await?;
    Ok(Redirect::to(GROUPS_PATH).into_response())
}

pub async fn save_group(
    State(state): State<AppState>,
    tenant: TenantContext,
    Form(form): Form<OptionForm>,
) -> AppResult<Response> {
    write(&state, &tenant, &form._csrf).await?;
    let group = form.group()?;
    addon_groups::save(
        &state.pool,
        &tenant.establishment.id,
        &group,
        &Utc::now().to_rfc3339(),
    )
    .await?;
    Ok(Redirect::to(&format!("{GROUPS_PATH}/{}", group.id)).into_response())
}

pub async fn delete_group(
    State(state): State<AppState>,
    tenant: TenantContext,
    Path(id): Path<String>,
    Form(form): Form<CsrfOnlyForm>,
) -> AppResult<Response> {
    write(&state, &tenant, &form._csrf).await?;
    if !addon_groups::delete(
        &state.pool,
        &tenant.establishment.id,
        &id,
        &Utc::now().to_rfc3339(),
    )
    .await?
    {
        return Err(MISSING);
    }
    Ok(Redirect::to(GROUPS_PATH).into_response())
}

pub async fn save_option(
    State(state): State<AppState>,
    tenant: TenantContext,
    Path(id): Path<String>,
    Form(form): Form<OptionForm>,
) -> AppResult<Response> {
    write(&state, &tenant, &form._csrf).await?;
    addon_options::save(
        &state.pool,
        &tenant.establishment.id,
        &id,
        &form.addon()?,
        &Utc::now().to_rfc3339(),
    )
    .await?;
    Ok(Redirect::to(&format!("{GROUPS_PATH}/{id}")).into_response())
}

pub async fn delete_option(
    State(state): State<AppState>,
    tenant: TenantContext,
    Path((group_id, id)): Path<(String, String)>,
    Form(form): Form<CsrfOnlyForm>,
) -> AppResult<Response> {
    write(&state, &tenant, &form._csrf).await?;
    if !addon_options::delete(
        &state.pool,
        &tenant.establishment.id,
        &group_id,
        &id,
        &Utc::now().to_rfc3339(),
    )
    .await?
    {
        return Err(MISSING);
    }
    Ok(Redirect::to(&format!("{GROUPS_PATH}/{group_id}")).into_response())
}

pub async fn save_variation(
    State(state): State<AppState>,
    tenant: TenantContext,
    Path(id): Path<String>,
    Form(form): Form<OptionForm>,
) -> AppResult<Response> {
    write(&state, &tenant, &form._csrf).await?;
    product_variations::save(
        &state.pool,
        &tenant.establishment.id,
        &id,
        &form.variation()?,
        &Utc::now().to_rfc3339(),
    )
    .await?;
    Ok(Redirect::to(&format!("/dashboard/products/{id}/options")).into_response())
}

pub async fn delete_variation(
    State(state): State<AppState>,
    tenant: TenantContext,
    Path((product_id, id)): Path<(String, String)>,
    Form(form): Form<CsrfOnlyForm>,
) -> AppResult<Response> {
    write(&state, &tenant, &form._csrf).await?;
    if !product_variations::delete(
        &state.pool,
        &tenant.establishment.id,
        &product_id,
        &id,
        &Utc::now().to_rfc3339(),
    )
    .await?
    {
        return Err(MISSING);
    }
    Ok(Redirect::to(&format!("/dashboard/products/{product_id}/options")).into_response())
}

#[derive(Deserialize)]
pub struct LinkForm {
    _csrf: String,
    group_id: String,
    position: Option<i64>,
}

pub async fn set_link(
    State(state): State<AppState>,
    tenant: TenantContext,
    Path(id): Path<String>,
    Form(form): Form<LinkForm>,
) -> AppResult<Response> {
    write(&state, &tenant, &form._csrf).await?;
    let est = &tenant.establishment.id;
    let now = Utc::now().to_rfc3339();
    if let Some(position) = form.position {
        if !(0..=1_000_000).contains(&position) {
            return Err(AppError::Validation("commerce.catalog.invalid_options"));
        }
        addon_groups::link_to_product(&state.pool, est, &id, &form.group_id, position, &now)
            .await?;
    } else if !addon_groups::unlink_from_product(&state.pool, est, &id, &form.group_id, &now)
        .await?
    {
        return Err(MISSING);
    }
    Ok(Redirect::to(&format!("/dashboard/products/{id}/options")).into_response())
}
