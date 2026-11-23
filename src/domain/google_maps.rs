use crate::error::{AppError, AppResult};

pub fn validate_google_maps_link(input: &str) -> AppResult<Option<&str>> {
    let url = input.trim();
    if url.is_empty() {
        return Ok(None);
    }

    let invalid = || AppError::Validation("error.invalid_google_maps_link");
    if url.len() > 2048
        || url.contains('\\')
        || url.chars().any(|ch| ch.is_whitespace() || ch.is_control())
    {
        return Err(invalid());
    }

    let (host, suffix) = url
        .strip_prefix("https://")
        .and_then(|address| address.split_once('/'))
        .ok_or_else(invalid)?;
    let path = suffix.split(['?', '#']).next().unwrap_or_default();
    if path.split('/').any(|part| {
        matches!(
            part.to_ascii_lowercase().as_str(),
            "." | ".." | "%2e" | ".%2e" | "%2e." | "%2e%2e"
        )
    }) {
        return Err(invalid());
    }
    let valid = match host.to_ascii_lowercase().as_str() {
        "www.google.com" | "google.com" | "www.google.com.br" | "google.com.br" => {
            path == "maps" || path.starts_with("maps/")
        }
        "maps.google.com" | "maps.google.com.br" => {
            path.is_empty() || path == "maps" || path.starts_with("maps/")
        }
        "maps.app.goo.gl" | "g.page" => !path.is_empty(),
        "goo.gl" => path.starts_with("maps/"),
        "search.google.com" => path == "local/writereview",
        _ => false,
    };
    if valid { Ok(Some(url)) } else { Err(invalid()) }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accepts_profile_and_review_links_only_from_google_hosts() {
        for url in [
            "https://www.google.com/maps/place/Cafe",
            "https://maps.app.goo.gl/abc123",
            "https://g.page/r/example/review",
            "https://search.google.com/local/writereview?placeid=example",
            "https://maps.google.com/?cid=123",
        ] {
            assert_eq!(validate_google_maps_link(url).unwrap(), Some(url));
        }
        assert_eq!(validate_google_maps_link("  ").unwrap(), None);
        for url in [
            "http://www.google.com/maps/abc",
            "https://www.google.com.evil.test/maps/abc",
            "https://www.google.com@evil.test/maps/abc",
            "https://www.google.com/url?q=https://example.test",
            "https://www.google.com/maps/../url?q=https://example.test",
            "https://www.google.com/maps/%2e%2e/url?q=https://example.test",
            "https://maps.app.goo.gl:444/abc",
            "https://maps.app.goo.gl/abc\nxyz",
            "javascript:alert(1)",
        ] {
            assert!(validate_google_maps_link(url).is_err());
        }
    }
}
