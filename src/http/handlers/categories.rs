use std::collections::HashMap;

use axum::Form;
use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum_extra::extract::CookieJar;
use serde::Deserialize;

use crate::app::AppState;
use crate::application::menus::{
    CategoryInput, create_category, delete_category, move_category, update_category,
};
use crate::domain::account::AccountRole;
use crate::error::{AppError, AppResult};
use crate::http::middleware::auth::{TenantContext, verify_csrf};
use crate::http::middleware::locale::RequestLocale;
use crate::http::responses::{take_flash, with_flash_redirect};
use crate::i18n::format_price;
use crate::infrastructure::database::queries::{
    find_category_for_est, find_media_asset_for_est, list_categories_for_est, list_products_for_est,
};
use crate::rendering::menu_editor::{
    CategoryFormTemplate, EditorCategoryView, EditorProductView, MenuEditorTemplate,
};
use crate::rendering::{PageMeta, render_html};

#[derive(Deserialize)]
pub struct CategoryForm {
    pub _csrf: String,
    pub name: String,
    #[serde(default)]
    pub name_en: String,
    #[serde(default)]
    pub name_es: String,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub description_en: String,
    #[serde(default)]
    pub description_es: String,
    #[serde(default)]
    pub is_visible: Option<String>,
}

#[derive(Deserialize)]
pub struct MoveForm {
    pub _csrf: String,
    pub direction: String,
}

#[derive(Deserialize)]
pub struct CsrfOnlyForm {
    pub _csrf: String,
}

pub async fn menu_editor_page(
    State(state): State<AppState>,
    RequestLocale(locale): RequestLocale,
    tenant: TenantContext,
    jar: CookieJar,
) -> AppResult<Response> {
    let est = &tenant.establishment;
    let cats = list_categories_for_est(&state.pool, &est.id).await?;
    let prods = list_products_for_est(&state.pool, &est.id).await?;

    let mut prod_views_by_cat: HashMap<String, Vec<EditorProductView>> = HashMap::new();
    for p in prods {
        let thumb_url = match &p.image_asset_id {
            Some(aid) => find_media_asset_for_est(&state.pool, &est.id, aid)
                .await?
                .map(|m| format!("/dashboard/media/{}", m.filename_thumb)),
            None => None,
        };
        let view = EditorProductView {
            id: p.id.clone(),
            name: p.effective_name(locale).to_string(),
            description: p.effective_description(locale).to_string(),
            price_centavos: p.price_centavos,
            price_display: format_price(p.price_centavos, locale),
            thumb_url,
            is_available: p.is_available,
            is_visible: p.is_visible,
        };
        prod_views_by_cat
            .entry(p.category_id.clone())
            .or_default()
            .push(view);
    }

    let categories = cats
        .into_iter()
        .map(|c| {
            let products = prod_views_by_cat.remove(&c.id).unwrap_or_default();
            EditorCategoryView {
                name: c.effective_name(locale).to_string(),
                id: c.id,
                is_visible: c.is_visible,
                products,
            }
        })
        .collect();

    let (jar, flash) = take_flash(jar);
    let tpl = MenuEditorTemplate {
        page: PageMeta::new(locale, "menu_editor.page_title", flash),
        brand_name: est.name.clone(),
        has_establishment: true,
        has_theme: true,
        is_admin: tenant.auth.account.role == AccountRole::Admin,
        active_nav: "menu",
        csrf_token: tenant.auth.csrf_token,
        has_unpublished_changes: est.has_unpublished_changes,
        categories,
    };
    Ok((jar, render_html(&tpl, StatusCode::OK)?).into_response())
}

pub async fn new_category_page(
    RequestLocale(locale): RequestLocale,
    tenant: TenantContext,
) -> AppResult<Response> {
    let tpl = CategoryFormTemplate {
        page: PageMeta::new(locale, "category_form.new_title", None),
        brand_name: tenant.establishment.name.clone(),
        has_establishment: true,
        has_theme: true,
        is_admin: tenant.auth.account.role == AccountRole::Admin,
        active_nav: "menu",
        csrf_token: tenant.auth.csrf_token,
        form_action: "/dashboard/categories".to_string(),
        name_value: String::new(),
        name_en: String::new(),
        name_es: String::new(),
        description_value: String::new(),
        description_en: String::new(),
        description_es: String::new(),
        is_visible: true,
        error_key: None,
    };
    render_html(&tpl, StatusCode::OK)
}

pub async fn create_category_submit(
    State(state): State<AppState>,
    RequestLocale(locale): RequestLocale,
    tenant: TenantContext,
    jar: CookieJar,
    Form(form): Form<CategoryForm>,
) -> AppResult<Response> {
    verify_csrf(&tenant.auth, &form._csrf)?;
    let visible = form.is_visible.is_some();

    match create_category(
        &state.pool,
        &tenant.establishment.id,
        CategoryInput {
            name: &form.name,
            name_en: &form.name_en,
            name_es: &form.name_es,
            description: &form.description,
            description_en: &form.description_en,
            description_es: &form.description_es,
            is_visible: visible,
        },
    )
    .await
    {
        Ok(_) => Ok(with_flash_redirect(
            jar,
            "/dashboard/menu",
            false,
            "flash.category_created",
            state.config.cookie_secure,
        )),
        Err(err) => {
            let tpl = CategoryFormTemplate {
                page: PageMeta::new(locale, "category_form.new_title", None),
                brand_name: tenant.establishment.name.clone(),
                has_establishment: true,
                has_theme: true,
                is_admin: tenant.auth.account.role == AccountRole::Admin,
                active_nav: "menu",
                csrf_token: tenant.auth.csrf_token,
                form_action: "/dashboard/categories".to_string(),
                name_value: form.name,
                name_en: form.name_en,
                name_es: form.name_es,
                description_value: form.description,
                description_en: form.description_en,
                description_es: form.description_es,
                is_visible: visible,
                error_key: Some(err.i18n_key().to_string()),
            };
            render_html(&tpl, err.status_code())
        }
    }
}

pub async fn edit_category_page(
    State(state): State<AppState>,
    RequestLocale(locale): RequestLocale,
    tenant: TenantContext,
    Path(cat_id): Path<String>,
) -> AppResult<Response> {
    let cat = find_category_for_est(&state.pool, &tenant.establishment.id, &cat_id)
        .await?
        .ok_or(AppError::NotFound("error.not_found"))?;

    let tpl = CategoryFormTemplate {
        page: PageMeta::new(locale, "category_form.edit_title", None),
        brand_name: tenant.establishment.name.clone(),
        has_establishment: true,
        has_theme: true,
        is_admin: tenant.auth.account.role == AccountRole::Admin,
        active_nav: "menu",
        csrf_token: tenant.auth.csrf_token,
        form_action: format!("/dashboard/categories/{}", cat.id),
        name_value: cat.name,
        name_en: cat.name_en,
        name_es: cat.name_es,
        description_value: cat.description,
        description_en: cat.description_en,
        description_es: cat.description_es,
        is_visible: cat.is_visible,
        error_key: None,
    };
    render_html(&tpl, StatusCode::OK)
}

pub async fn update_category_submit(
    State(state): State<AppState>,
    RequestLocale(locale): RequestLocale,
    tenant: TenantContext,
    jar: CookieJar,
    Path(cat_id): Path<String>,
    Form(form): Form<CategoryForm>,
) -> AppResult<Response> {
    verify_csrf(&tenant.auth, &form._csrf)?;
    let visible = form.is_visible.is_some();

    match update_category(
        &state.pool,
        &tenant.establishment.id,
        &cat_id,
        CategoryInput {
            name: &form.name,
            name_en: &form.name_en,
            name_es: &form.name_es,
            description: &form.description,
            description_en: &form.description_en,
            description_es: &form.description_es,
            is_visible: visible,
        },
    )
    .await
    {
        Ok(()) => Ok(with_flash_redirect(
            jar,
            "/dashboard/menu",
            false,
            "flash.category_updated",
            state.config.cookie_secure,
        )),
        Err(err) => {
            let tpl = CategoryFormTemplate {
                page: PageMeta::new(locale, "category_form.edit_title", None),
                brand_name: tenant.establishment.name.clone(),
                has_establishment: true,
                has_theme: true,
                is_admin: tenant.auth.account.role == AccountRole::Admin,
                active_nav: "menu",
                csrf_token: tenant.auth.csrf_token,
                form_action: format!("/dashboard/categories/{cat_id}"),
                name_value: form.name,
                name_en: form.name_en,
                name_es: form.name_es,
                description_value: form.description,
                description_en: form.description_en,
                description_es: form.description_es,
                is_visible: visible,
                error_key: Some(err.i18n_key().to_string()),
            };
            render_html(&tpl, err.status_code())
        }
    }
}

pub async fn delete_category_submit(
    State(state): State<AppState>,
    tenant: TenantContext,
    jar: CookieJar,
    Path(cat_id): Path<String>,
    Form(form): Form<CsrfOnlyForm>,
) -> AppResult<Response> {
    verify_csrf(&tenant.auth, &form._csrf)?;
    delete_category(
        &state.pool,
        &state.assets,
        &tenant.establishment.id,
        &cat_id,
    )
    .await?;
    Ok(with_flash_redirect(
        jar,
        "/dashboard/menu",
        false,
        "flash.category_deleted",
        state.config.cookie_secure,
    ))
}

pub async fn move_category_submit(
    State(state): State<AppState>,
    tenant: TenantContext,
    Path(cat_id): Path<String>,
    Form(form): Form<MoveForm>,
) -> AppResult<Response> {
    verify_csrf(&tenant.auth, &form._csrf)?;
    let move_up = form.direction == "up";
    move_category(&state.pool, &tenant.establishment.id, &cat_id, move_up).await?;
    Ok(axum::response::Redirect::to("/dashboard/menu").into_response())
}
