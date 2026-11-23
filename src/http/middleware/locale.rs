use std::convert::Infallible;

use axum::extract::{FromRequestParts, Query};
use axum::http::header::ACCEPT_LANGUAGE;
use axum::http::request::Parts;
use axum_extra::extract::CookieJar;
use serde::Deserialize;

use crate::i18n::Locale;

pub const LANG_COOKIE_NAME: &str = "qr_lang";

#[derive(Debug, Clone, Copy)]
pub struct RequestLocale(pub Locale);

/// Resolves locale only from explicit `?lang=` or `qr_lang` cookie (ignoring `Accept-Language`),
/// keeping `/` deterministic for search crawlers unless a visitor explicitly picked a language.
#[derive(Debug, Clone, Copy)]
pub struct ExplicitRequestLocale(pub Option<Locale>);

#[derive(Debug, Deserialize)]
struct LangQuery {
    lang: Option<String>,
}

impl<S> FromRequestParts<S> for ExplicitRequestLocale
where
    S: Send + Sync,
{
    type Rejection = Infallible;

    async fn from_request_parts(parts: &mut Parts, state: &S) -> Result<Self, Self::Rejection> {
        if let Ok(Query(q)) = Query::<LangQuery>::from_request_parts(parts, state).await
            && let Some(raw) = q.lang
            && let Some(locale) = Locale::parse(&raw)
        {
            return Ok(Self(Some(locale)));
        }

        let jar = CookieJar::from_headers(&parts.headers);
        if let Some(cookie) = jar.get(LANG_COOKIE_NAME)
            && let Some(locale) = Locale::parse(cookie.value())
        {
            return Ok(Self(Some(locale)));
        }

        Ok(Self(None))
    }
}

impl<S> FromRequestParts<S> for RequestLocale
where
    S: Send + Sync,
{
    type Rejection = Infallible;

    async fn from_request_parts(parts: &mut Parts, state: &S) -> Result<Self, Self::Rejection> {
        if let Ok(Query(q)) = Query::<LangQuery>::from_request_parts(parts, state).await
            && let Some(raw) = q.lang
            && let Some(locale) = Locale::parse(&raw)
        {
            return Ok(Self(locale));
        }

        let jar = CookieJar::from_headers(&parts.headers);
        if let Some(cookie) = jar.get(LANG_COOKIE_NAME)
            && let Some(locale) = Locale::parse(cookie.value())
        {
            return Ok(Self(locale));
        }

        if let Some(accept) = parts
            .headers
            .get(ACCEPT_LANGUAGE)
            .and_then(|v| v.to_str().ok())
            && let Some(locale) = Locale::from_accept_language(accept)
        {
            return Ok(Self(locale));
        }

        Ok(Self(Locale::PtBr))
    }
}
