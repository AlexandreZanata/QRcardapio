use axum::Form;
use axum::extract::{Multipart, Path, Query, State};
use axum::http::{HeaderMap, StatusCode};
use axum::response::{IntoResponse, Redirect, Response};
use axum_extra::extract::CookieJar;
use serde::Deserialize;

use super::menu_editor::{EditorPageQuery, render_editor};
use super::product_multipart::{ParsedProductMultipart, parse_product_multipart};
use crate::app::AppState;
use crate::application::menus::{
    ProductInput, create_product, delete_product, move_product, toggle_product_availability,
    update_product,
};
use crate::domain::account::AccountRole;
use crate::domain::product::format_price_input;
use crate::error::{AppError, AppResult};
use crate::http::middleware::auth::{TenantContext, verify_csrf};
use crate::http::middleware::locale::RequestLocale;
use crate::http::responses::{
    dialog_error, dialog_saved, prefers_dialog_response, with_flash_redirect,
};
use crate::infrastructure::database::queries::{
    find_media_asset_for_est, find_product_for_est, list_categories_for_est,
};
use crate::rendering::menu_editor::{CategoryOptionView, ProductFields, ProductFormTemplate};
use crate::rendering::{PageMeta, render_html};

#[derive(Deserialize)]
pub struct NewProductQuery {
    pub category_id: Option<String>,
    #[serde(default)]
    standalone: bool,
    #[serde(flatten)]
    editor: EditorPageQuery,
}

#[derive(Deserialize)]
pub struct MoveProductForm {
    pub _csrf: String,
    pub direction: String,
}

#[derive(Deserialize)]
pub struct CsrfOnlyForm {
    pub _csrf: String,
}

pub async fn new_product_page(
    State(state): State<AppState>,
    RequestLocale(locale): RequestLocale,
    tenant: TenantContext,
    Query(query): Query<NewProductQuery>,
    jar: CookieJar,
) -> AppResult<Response> {
    if !query.standalone {
        return render_editor(
            &state,
            locale,
            tenant,
            query.editor,
            jar,
            Some(query.category_id.as_deref().unwrap_or_default()),
        )
        .await;
    }
    let cats = list_categories_for_est(&state.pool, &tenant.establishment.id).await?;
    if cats.is_empty() {
        return Ok(Redirect::to("/dashboard/categories/new").into_response());
    }
    let editor = query.editor;
    let category_options: Vec<_> = cats.into_iter().map(CategoryOptionView::from).collect();
    let product = ProductFields::new(&category_options, query.category_id.as_deref());

    let tpl = ProductFormTemplate {
        page: PageMeta::new(locale, "product_form.new_title", None),
        brand_name: tenant.establishment.name.clone(),
        has_establishment: true,
        has_theme: true,
        is_admin: tenant.auth.account.role == AccountRole::Admin,
        active_nav: "menu",
        csrf_token: tenant.auth.csrf_token,
        form_action: editor.with_page("/dashboard/products"),
        menu_url: editor.menu_url(),
        category_options,
        product,
        error_key: None,
    };
    render_html(&tpl, StatusCode::OK)
}

pub async fn create_product_submit(
    State(state): State<AppState>,
    RequestLocale(locale): RequestLocale,
    tenant: TenantContext,
    Query(editor): Query<EditorPageQuery>,
    jar: CookieJar,
    headers: HeaderMap,
    multipart: Multipart,
) -> AppResult<Response> {
    let dialog_submission = prefers_dialog_response(&headers);
    let mut parsed = match checked_product_multipart(&state, &tenant, multipart).await {
        Ok(parsed) => parsed,
        Err(err) if dialog_submission => return Ok(dialog_error(state.catalog, locale, &err)),
        Err(err) => return Err(err),
    };

    let res = create_product(
        &state.pool,
        &state.assets,
        &tenant.establishment.id,
        ProductInput {
            category_id: &parsed.category_id,
            name: &parsed.name,
            name_en: &parsed.name_en,
            name_es: &parsed.name_es,
            description: &parsed.description,
            description_en: &parsed.description_en,
            description_es: &parsed.description_es,
            price_raw: &parsed.price,
            is_available: parsed.is_available,
            is_visible: parsed.is_visible,
        },
        parsed.image_bytes.take(),
    )
    .await;

    match res {
        Ok(_) if dialog_submission => Ok(dialog_saved(
            jar,
            "flash.product_created",
            state.config.cookie_secure,
        )),
        Ok(_) => Ok(with_flash_redirect(
            jar,
            &editor.menu_url(),
            false,
            "flash.product_created",
            state.config.cookie_secure,
        )),
        Err(err) if dialog_submission => Ok(dialog_error(state.catalog, locale, &err)),
        Err(err) => {
            let cats = list_categories_for_est(&state.pool, &tenant.establishment.id).await?;
            let category_options = cats.into_iter().map(CategoryOptionView::from).collect();
            let tpl = ProductFormTemplate {
                page: PageMeta::new(locale, "product_form.new_title", None),
                brand_name: tenant.establishment.name.clone(),
                has_establishment: true,
                has_theme: true,
                is_admin: tenant.auth.account.role == AccountRole::Admin,
                active_nav: "menu",
                csrf_token: tenant.auth.csrf_token,
                form_action: editor.with_page("/dashboard/products"),
                menu_url: editor.menu_url(),
                category_options,
                product: parsed.into(),
                error_key: Some(err.i18n_key().to_string()),
            };
            render_html(&tpl, err.status_code())
        }
    }
}

async fn checked_product_multipart(
    state: &AppState,
    tenant: &TenantContext,
    multipart: Multipart,
) -> AppResult<ParsedProductMultipart> {
    let parsed = parse_product_multipart(multipart).await?;
    verify_csrf(&tenant.auth, &parsed.csrf)?;
    if parsed.image_bytes.is_some()
        && state
            .rate_limiters
            .upload_account
            .check_key(&tenant.auth.account.id)
            .is_err()
    {
        return Err(AppError::RateLimited);
    }
    Ok(parsed)
}

pub async fn edit_product_page(
    State(state): State<AppState>,
    RequestLocale(locale): RequestLocale,
    tenant: TenantContext,
    Query(editor): Query<EditorPageQuery>,
    Path(prod_id): Path<String>,
) -> AppResult<Response> {
    let prod = find_product_for_est(&state.pool, &tenant.establishment.id, &prod_id)
        .await?
        .ok_or(AppError::NotFound("error.not_found"))?;
    let cats = list_categories_for_est(&state.pool, &tenant.establishment.id).await?;
    let category_options = cats.into_iter().map(CategoryOptionView::from).collect();

    let existing_image_url = match &prod.image_asset_id {
        Some(aid) => find_media_asset_for_est(&state.pool, &tenant.establishment.id, aid)
            .await?
            .map(|m| format!("/dashboard/media/{}", m.filename_thumb)),
        None => None,
    };

    let tpl = ProductFormTemplate {
        page: PageMeta::new(locale, "product_form.edit_title", None),
        brand_name: tenant.establishment.name.clone(),
        has_establishment: true,
        has_theme: true,
        is_admin: tenant.auth.account.role == AccountRole::Admin,
        active_nav: "menu",
        csrf_token: tenant.auth.csrf_token,
        form_action: editor.with_page(&format!("/dashboard/products/{}", prod.id)),
        menu_url: editor.menu_url(),
        category_options,
        product: ProductFields {
            selected_category_id: prod.category_id,
            name_value: prod.name,
            name_en: prod.name_en,
            name_es: prod.name_es,
            price_value: format_price_input(prod.price_centavos),
            description_value: prod.description,
            description_en: prod.description_en,
            description_es: prod.description_es,
            existing_image_url,
            is_available: prod.is_available,
            is_visible: prod.is_visible,
        },
        error_key: None,
    };
    render_html(&tpl, StatusCode::OK)
}

pub async fn update_product_submit(
    State(state): State<AppState>,
    RequestLocale(locale): RequestLocale,
    tenant: TenantContext,
    Query(editor): Query<EditorPageQuery>,
    jar: CookieJar,
    Path(prod_id): Path<String>,
    multipart: Multipart,
) -> AppResult<Response> {
    let mut parsed = checked_product_multipart(&state, &tenant, multipart).await?;

    let res = update_product(
        &state.pool,
        &state.assets,
        &tenant.establishment.id,
        &prod_id,
        ProductInput {
            category_id: &parsed.category_id,
            name: &parsed.name,
            name_en: &parsed.name_en,
            name_es: &parsed.name_es,
            description: &parsed.description,
            description_en: &parsed.description_en,
            description_es: &parsed.description_es,
            price_raw: &parsed.price,
            is_available: parsed.is_available,
            is_visible: parsed.is_visible,
        },
        parsed.image_bytes.take(),
        parsed.remove_image,
    )
    .await;

    match res {
        Ok(()) => Ok(with_flash_redirect(
            jar,
            &editor.menu_url(),
            false,
            "flash.product_updated",
            state.config.cookie_secure,
        )),
        Err(err) => {
            let cats = list_categories_for_est(&state.pool, &tenant.establishment.id).await?;
            let category_options = cats.into_iter().map(CategoryOptionView::from).collect();
            let tpl = ProductFormTemplate {
                page: PageMeta::new(locale, "product_form.edit_title", None),
                brand_name: tenant.establishment.name.clone(),
                has_establishment: true,
                has_theme: true,
                is_admin: tenant.auth.account.role == AccountRole::Admin,
                active_nav: "menu",
                csrf_token: tenant.auth.csrf_token,
                form_action: editor.with_page(&format!("/dashboard/products/{prod_id}")),
                menu_url: editor.menu_url(),
                category_options,
                product: parsed.into(),
                error_key: Some(err.i18n_key().to_string()),
            };
            render_html(&tpl, err.status_code())
        }
    }
}

pub async fn toggle_product_submit(
    State(state): State<AppState>,
    tenant: TenantContext,
    Query(editor): Query<EditorPageQuery>,
    Path(prod_id): Path<String>,
    Form(form): Form<CsrfOnlyForm>,
) -> AppResult<Response> {
    verify_csrf(&tenant.auth, &form._csrf)?;
    toggle_product_availability(&state.pool, &tenant.establishment.id, &prod_id).await?;
    Ok(Redirect::to(&editor.menu_url()).into_response())
}

pub async fn delete_product_submit(
    State(state): State<AppState>,
    tenant: TenantContext,
    Query(editor): Query<EditorPageQuery>,
    jar: CookieJar,
    Path(prod_id): Path<String>,
    Form(form): Form<CsrfOnlyForm>,
) -> AppResult<Response> {
    verify_csrf(&tenant.auth, &form._csrf)?;
    delete_product(
        &state.pool,
        &state.assets,
        &tenant.establishment.id,
        &prod_id,
    )
    .await?;
    Ok(with_flash_redirect(
        jar,
        &editor.menu_url(),
        false,
        "flash.product_deleted",
        state.config.cookie_secure,
    ))
}

pub async fn move_product_submit(
    State(state): State<AppState>,
    tenant: TenantContext,
    Query(editor): Query<EditorPageQuery>,
    Path(prod_id): Path<String>,
    Form(form): Form<MoveProductForm>,
) -> AppResult<Response> {
    verify_csrf(&tenant.auth, &form._csrf)?;
    let move_up = form.direction == "up";
    move_product(&state.pool, &tenant.establishment.id, &prod_id, move_up).await?;
    Ok(Redirect::to(&editor.menu_url()).into_response())
}
