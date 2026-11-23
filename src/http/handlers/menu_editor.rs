use axum::extract::{Query, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum_extra::extract::CookieJar;
use serde::Deserialize;

use crate::app::AppState;
use crate::domain::account::AccountRole;
use crate::error::AppResult;
use crate::http::middleware::auth::TenantContext;
use crate::http::middleware::locale::RequestLocale;
use crate::http::responses::take_flash;
use crate::i18n::{format_price, pick_localized_text};
use crate::infrastructure::database::queries::menu_editor::list_page;
use crate::rendering::menu_editor::{
    EditorCategoryView, EditorPagination, EditorProductView, MenuEditorTemplate,
};
use crate::rendering::{PageMeta, render_html};

#[derive(Deserialize)]
pub struct EditorPageQuery {
    page: Option<String>,
}

impl EditorPageQuery {
    fn page_number(&self) -> i64 {
        self.page
            .as_deref()
            .and_then(|page| page.parse::<i64>().ok())
            .unwrap_or(1)
            .max(1)
    }

    pub fn with_page(&self, path: &str) -> String {
        let page = self.page_number();
        if page > 1 {
            format!("{path}?page={page}")
        } else {
            path.to_string()
        }
    }

    pub fn menu_url(&self) -> String {
        self.with_page("/dashboard/menu")
    }
}

pub async fn menu_editor_page(
    State(state): State<AppState>,
    RequestLocale(locale): RequestLocale,
    tenant: TenantContext,
    Query(query): Query<EditorPageQuery>,
    jar: CookieJar,
) -> AppResult<Response> {
    let est = &tenant.establishment;
    let page = list_page(&state.pool, &est.id, query.page_number()).await?;
    let categories = page
        .rows
        .chunk_by(|left, right| left.category_id == right.category_id)
        .map(|rows| {
            let category = &rows[0];
            let products = rows
                .iter()
                .filter_map(|row| {
                    Some(EditorProductView {
                        id: row.product_id.clone()?,
                        name: pick_localized_text(
                            locale,
                            &row.product_name,
                            &row.product_name_en,
                            &row.product_name_es,
                        )
                        .to_string(),
                        description: pick_localized_text(
                            locale,
                            &row.description,
                            &row.description_en,
                            &row.description_es,
                        )
                        .to_string(),
                        price_centavos: row.price_centavos,
                        price_display: format_price(row.price_centavos, locale),
                        thumb_url: row
                            .filename_thumb
                            .as_ref()
                            .map(|filename| format!("/dashboard/media/{filename}")),
                        is_available: row.is_available,
                        is_visible: row.is_visible,
                    })
                })
                .collect();
            EditorCategoryView {
                id: category.category_id.clone(),
                name: pick_localized_text(
                    locale,
                    &category.category_name,
                    &category.category_name_en,
                    &category.category_name_es,
                )
                .to_string(),
                is_visible: category.category_visible,
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
        pagination: EditorPagination {
            current: page.current,
            pages: page.pages,
        },
    };
    Ok((jar, render_html(&tpl, StatusCode::OK)?).into_response())
}
