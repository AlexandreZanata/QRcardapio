pub mod admin;
pub mod auth;
pub mod billing;
pub mod dashboard;
pub mod error_page;
pub mod landing;
pub mod mcp;
pub mod menu_editor;
pub mod ocr;
pub mod public_menu;
pub mod qr;
pub mod seo;

use askama::Template;
use axum::http::StatusCode;
use axum::response::{Html, IntoResponse, Response};

use crate::error::{AppError, AppResult};
use crate::i18n::{Catalog, Locale, format_price};
use crate::seo::SeoMeta;

#[derive(Debug, Clone)]
pub struct FlashMessage {
    pub is_error: bool,
    pub key: String,
}

#[derive(Debug, Clone)]
pub struct PageMeta {
    pub locale: Locale,
    pub locale_code: &'static str,
    pub i18n_version: String,
    pub title_key: &'static str,
    pub flash: Option<FlashMessage>,
    pub catalog: &'static Catalog,
    pub seo: SeoMeta,
}

impl PageMeta {
    pub fn new(locale: Locale, title_key: &'static str, flash: Option<FlashMessage>) -> Self {
        Self::with_seo(
            locale,
            title_key,
            flash,
            SeoMeta::noindex("landing.meta_description"),
        )
    }

    pub fn with_seo(
        locale: Locale,
        title_key: &'static str,
        flash: Option<FlashMessage>,
        seo: SeoMeta,
    ) -> Self {
        let catalog = Catalog::global();
        Self {
            locale,
            locale_code: locale.code(),
            i18n_version: catalog.version_hash().to_string(),
            title_key,
            flash,
            catalog,
            seo,
        }
    }

    pub fn t<'a>(&'a self, key: &'a str) -> &'a str {
        self.catalog.t(self.locale, key)
    }

    pub fn seo_title(&self) -> &str {
        self.seo
            .title_override
            .as_deref()
            .unwrap_or_else(|| self.t(self.title_key))
    }

    pub fn seo_description(&self) -> &str {
        self.seo
            .description_override
            .as_deref()
            .unwrap_or_else(|| self.t(self.seo.description_key))
    }

    pub fn timestamp(&self, timestamp: &str) -> String {
        chrono::DateTime::parse_from_rfc3339(timestamp).map_or_else(
            |_| timestamp.to_owned(),
            |date| date.to_utc().format("%Y-%m-%d %H:%M UTC").to_string(),
        )
    }

    pub fn price(&self, centavos: i64) -> String {
        format_price(centavos, self.locale)
    }
}

pub fn render_html<T: Template>(tpl: &T, status: StatusCode) -> AppResult<Response> {
    let body = tpl
        .render()
        .map_err(|e| AppError::Internal(format!("template render failed: {e}")))?;
    Ok((status, Html(body)).into_response())
}
