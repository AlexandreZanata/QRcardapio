use std::{fmt, net::IpAddr};

use reqwest::Url;

pub const CALLBACK_PATH: &str = "/auth/google/callback";

#[derive(Clone)]
pub struct GoogleConfig {
    pub client_id: String,
    client_secret: String,
    pub redirect_uri: String,
}

impl fmt::Debug for GoogleConfig {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("GoogleConfig")
            .field("client_id", &self.client_id)
            .field("redirect_uri", &self.redirect_uri)
            .finish_non_exhaustive()
    }
}

impl GoogleConfig {
    pub fn new(id: &str, secret: &str, origin: &str, secure_cookie: bool) -> Result<Self, String> {
        if !id
            .strip_suffix(".apps.googleusercontent.com")
            .is_some_and(|prefix| {
                !prefix.is_empty()
                    && prefix
                        .bytes()
                        .all(|b| b.is_ascii_alphanumeric() || b == b'-')
            })
            || id.len() > 300
        {
            return Err("GOOGLE_CLIENT_ID must be a Google OAuth web client ID".into());
        }
        if secret.is_empty()
            || secret.len() > 512
            || !secret.bytes().all(|byte| byte.is_ascii_graphic())
        {
            return Err("GOOGLE_CLIENT_SECRET must be a nonempty token".into());
        }
        let valid_origin = Url::parse(origin).is_ok_and(|url| {
            let local = url.host_str().is_some_and(|host| {
                host == "localhost"
                    || host
                        .trim_matches(['[', ']'])
                        .parse::<IpAddr>()
                        .is_ok_and(|ip| ip.is_loopback())
            });
            url.origin().ascii_serialization() == origin
                && ((url.scheme() == "https" && secure_cookie) || (url.scheme() == "http" && local))
        });
        if !valid_origin {
            return Err("Google login requires HTTPS PUBLIC_BASE_URL with COOKIE_SECURE=true, or an HTTP loopback origin for development".into());
        }
        Ok(Self {
            client_id: id.into(),
            client_secret: secret.into(),
            redirect_uri: format!("{origin}{CALLBACK_PATH}"),
        })
    }

    pub(super) fn from_env(origin: &str, secure_cookie: bool) -> Result<Option<Self>, String> {
        let id = std::env::var("GOOGLE_CLIENT_ID").unwrap_or_default();
        let secret = std::env::var("GOOGLE_CLIENT_SECRET").unwrap_or_default();
        if id.is_empty() && secret.is_empty() {
            return Ok(None);
        }
        Self::new(&id, &secret, origin, secure_cookie).map(Some)
    }

    pub fn secret(&self) -> &str {
        &self.client_secret
    }
}
