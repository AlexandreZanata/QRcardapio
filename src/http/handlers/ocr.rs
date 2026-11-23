use axum::Form;
use axum::extract::{Multipart, Path, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum_extra::extract::CookieJar;
use serde::Deserialize;

use super::ocr_form::{build_review_template, parse_apply_form_pairs};

use crate::app::AppState;
use crate::application::ocr::{
    apply_import, discard_import, extract_from_pasted_text, extract_from_upload, get_import,
    list_recent_imports,
};
use crate::domain::account::AccountRole;
use crate::domain::ocr::{ApplyOcrItemInput, ApplyOcrOptions};
use crate::error::{AppError, AppResult};
use crate::http::middleware::auth::{TenantContext, verify_csrf};
use crate::http::middleware::locale::RequestLocale;
use crate::http::responses::{take_flash, with_flash_redirect};
use crate::infrastructure::ocr::MAX_OCR_PDF_BYTES;
use crate::rendering::ocr::{
    OcrImportSummaryRow, OcrIndexTemplate, rows_from_record, summary_rows_from_records,
};
use crate::rendering::{PageMeta, render_html};

#[derive(Deserialize)]
pub struct PasteTextForm {
    pub _csrf: String,
    pub raw_text: String,
}

#[derive(Deserialize)]
pub struct CsrfOnlyForm {
    pub _csrf: String,
}

pub async fn ocr_index_page(
    State(state): State<AppState>,
    RequestLocale(locale): RequestLocale,
    tenant: TenantContext,
    jar: CookieJar,
) -> AppResult<Response> {
    let (jar, flash) = take_flash(jar);
    let imports = build_summary_rows(&state, &tenant.establishment.id).await?;
    let tpl = OcrIndexTemplate {
        page: PageMeta::new(locale, "ocr.page_title", flash),
        brand_name: tenant.establishment.name.clone(),
        has_establishment: true,
        has_theme: true,
        is_admin: tenant.auth.account.role == AccountRole::Admin,
        active_nav: "ocr",
        csrf_token: tenant.auth.csrf_token,
        imports,
        error_key: None,
    };
    Ok((jar, render_html(&tpl, StatusCode::OK)?).into_response())
}

pub async fn ocr_extract_submit(
    State(state): State<AppState>,
    RequestLocale(locale): RequestLocale,
    tenant: TenantContext,
    jar: CookieJar,
    mut multipart: Multipart,
) -> AppResult<Response> {
    let mut csrf = String::new();
    let mut filename = String::from("cardapio");
    let mut file_bytes: Option<Vec<u8>> = None;

    while let Some(field) = multipart
        .next_field()
        .await
        .map_err(|_| AppError::Validation("error.ocr_unsupported_file"))?
    {
        let name = field.name().unwrap_or("").to_string();
        if name == "menu_file" {
            if let Some(orig) = field.file_name() {
                filename = orig.to_string();
            }
            let bytes = field
                .bytes()
                .await
                .map_err(|_| AppError::Validation("error.ocr_file_too_large"))?;
            if bytes.len() > MAX_OCR_PDF_BYTES {
                return render_index_error(
                    &state,
                    locale,
                    &tenant,
                    "error.ocr_file_too_large",
                    StatusCode::PAYLOAD_TOO_LARGE,
                )
                .await;
            }
            if !bytes.is_empty() {
                file_bytes = Some(bytes.to_vec());
            }
        } else if name == "_csrf" {
            csrf = field.text().await.unwrap_or_default();
        }
    }

    verify_csrf(&tenant.auth, &csrf)?;
    if state
        .rate_limiters
        .ocr_account
        .check_key(&tenant.auth.account.id)
        .is_err()
    {
        return render_index_error(
            &state,
            locale,
            &tenant,
            "error.rate_limited",
            StatusCode::TOO_MANY_REQUESTS,
        )
        .await;
    }

    let bytes = file_bytes.unwrap_or_default();
    match extract_from_upload(
        &state.pool,
        &state.ocr,
        &tenant.establishment.id,
        &filename,
        bytes,
        locale,
    )
    .await
    {
        Ok(rec) => Ok(with_flash_redirect(
            jar,
            &format!("/dashboard/ocr/{}", rec.id),
            false,
            "flash.ocr_extracted",
            state.config.cookie_secure,
        )),
        Err(err) => {
            render_index_error(&state, locale, &tenant, err.i18n_key(), err.status_code()).await
        }
    }
}

pub async fn ocr_text_submit(
    State(state): State<AppState>,
    RequestLocale(locale): RequestLocale,
    tenant: TenantContext,
    jar: CookieJar,
    Form(form): Form<PasteTextForm>,
) -> AppResult<Response> {
    verify_csrf(&tenant.auth, &form._csrf)?;
    if state
        .rate_limiters
        .ocr_account
        .check_key(&tenant.auth.account.id)
        .is_err()
    {
        return render_index_error(
            &state,
            locale,
            &tenant,
            "error.rate_limited",
            StatusCode::TOO_MANY_REQUESTS,
        )
        .await;
    }

    match extract_from_pasted_text(
        &state.pool,
        &tenant.establishment.id,
        &form.raw_text,
        locale,
    )
    .await
    {
        Ok(rec) => Ok(with_flash_redirect(
            jar,
            &format!("/dashboard/ocr/{}", rec.id),
            false,
            "flash.ocr_extracted",
            state.config.cookie_secure,
        )),
        Err(err) => {
            render_index_error(&state, locale, &tenant, err.i18n_key(), err.status_code()).await
        }
    }
}

pub async fn ocr_review_page(
    State(state): State<AppState>,
    RequestLocale(locale): RequestLocale,
    tenant: TenantContext,
    jar: CookieJar,
    Path(import_id): Path<String>,
) -> AppResult<Response> {
    let rec = get_import(&state.pool, &tenant.establishment.id, &import_id).await?;
    let (jar, flash) = take_flash(jar);
    let rows = rows_from_record(&rec);
    let tpl =
        build_review_template(&state, locale, &tenant, &rec, rows, true, true, flash, None).await?;
    Ok((jar, render_html(&tpl, StatusCode::OK)?).into_response())
}

pub async fn ocr_apply_submit(
    State(state): State<AppState>,
    RequestLocale(locale): RequestLocale,
    tenant: TenantContext,
    jar: CookieJar,
    Path(import_id): Path<String>,
    Form(pairs): Form<Vec<(String, String)>>,
) -> AppResult<Response> {
    let parsed_form = parse_apply_form_pairs(pairs);
    verify_csrf(&tenant.auth, &parsed_form.csrf)?;

    let rec = get_import(&state.pool, &tenant.establishment.id, &import_id).await?;
    let selected: Vec<ApplyOcrItemInput> = parsed_form
        .rows
        .iter()
        .filter(|r| r.include)
        .map(|r| ApplyOcrItemInput {
            category_name: r.category_name.clone(),
            name: r.name.clone(),
            description: r.description.clone(),
            price_raw: r.price_value.clone(),
            is_available: r.is_available,
        })
        .collect();

    let opts = ApplyOcrOptions {
        reuse_existing_categories: parsed_form.reuse_existing_categories,
        mark_visible: parsed_form.mark_visible,
    };

    match apply_import(
        &state.pool,
        &tenant.establishment.id,
        &import_id,
        &selected,
        opts,
        locale,
    )
    .await
    {
        Ok(_) => Ok(with_flash_redirect(
            jar,
            "/dashboard/menu",
            false,
            "flash.ocr_applied",
            state.config.cookie_secure,
        )),
        Err(err) => {
            let tpl = build_review_template(
                &state,
                locale,
                &tenant,
                &rec,
                parsed_form.rows,
                parsed_form.reuse_existing_categories,
                parsed_form.mark_visible,
                None,
                Some(err.i18n_key().to_string()),
            )
            .await?;
            render_html(&tpl, err.status_code())
        }
    }
}

pub async fn ocr_discard_submit(
    State(state): State<AppState>,
    tenant: TenantContext,
    jar: CookieJar,
    Path(import_id): Path<String>,
    Form(form): Form<CsrfOnlyForm>,
) -> AppResult<Response> {
    verify_csrf(&tenant.auth, &form._csrf)?;
    discard_import(&state.pool, &tenant.establishment.id, &import_id).await?;
    Ok(with_flash_redirect(
        jar,
        "/dashboard/ocr",
        false,
        "flash.ocr_discarded",
        state.config.cookie_secure,
    ))
}

async fn render_index_error(
    state: &AppState,
    locale: crate::i18n::Locale,
    tenant: &TenantContext,
    err_key: &str,
    status: StatusCode,
) -> AppResult<Response> {
    let imports = build_summary_rows(state, &tenant.establishment.id).await?;
    let tpl = OcrIndexTemplate {
        page: PageMeta::new(locale, "ocr.page_title", None),
        brand_name: tenant.establishment.name.clone(),
        has_establishment: true,
        has_theme: true,
        is_admin: tenant.auth.account.role == AccountRole::Admin,
        active_nav: "ocr",
        csrf_token: tenant.auth.csrf_token.clone(),
        imports,
        error_key: Some(err_key.to_string()),
    };
    render_html(&tpl, status)
}

async fn build_summary_rows(state: &AppState, est_id: &str) -> AppResult<Vec<OcrImportSummaryRow>> {
    let list = list_recent_imports(&state.pool, est_id).await?;
    Ok(summary_rows_from_records(list))
}
