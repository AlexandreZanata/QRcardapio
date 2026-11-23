use reqwest::Url;

pub fn validate_origin(base: &str, secure_cookie: bool) -> Result<(), String> {
    let valid = Url::parse(base).is_ok_and(|url| {
        url.scheme() == "https"
            && url.host_str().is_some()
            && url.origin().ascii_serialization() == base
    });
    if !valid || !secure_cookie {
        return Err(
            "MCP_ENABLED requires an HTTPS PUBLIC_BASE_URL origin and COOKIE_SECURE=true".into(),
        );
    }
    Ok(())
}
