/// Deterministic 12-hex content hash of all `static/{css,js,icons}` assets baked at compile time.
pub const ASSET_VERSION: &str = env!("QRCARDAPIO_ASSET_VERSION");

const VERSIONED_PREFIXES: &[&str] = &["/static/css/", "/static/js/", "/static/icons/"];

/// Appends `?v=<ASSET_VERSION>` to relative `href="/static/{css,js,icons}/..."` and
/// `src="/static/{css,js,icons}/..."` attributes (preserving `#fragment` anchors on SVG sprites)
/// so browsers and CDN edges (Cloudflare) never serve stale CSS/JS/SVG files after a deploy.
pub fn version_static_assets(html: String) -> String {
    version_static_assets_with(html, ASSET_VERSION)
}

fn version_static_assets_with(html: String, version: &str) -> String {
    if !html.contains("/static/") {
        return html;
    }

    let mut out = String::with_capacity(html.len() + 256);
    let mut rest = html.as_str();

    while let Some(pos) = find_next_attr(rest) {
        let (attr_start, url_start) = pos;
        out.push_str(&rest[..url_start]);
        let after_url = &rest[url_start..];
        let Some(quote_end) = after_url.find('"') else {
            out.push_str(after_url);
            return out;
        };
        let url = &after_url[..quote_end];
        let _ = attr_start;
        if should_version(url) {
            if let Some((path, fragment)) = url.split_once('#') {
                out.push_str(path);
                out.push_str("?v=");
                out.push_str(version);
                out.push('#');
                out.push_str(fragment);
            } else {
                out.push_str(url);
                out.push_str("?v=");
                out.push_str(version);
            }
        } else {
            out.push_str(url);
        }
        out.push('"');
        rest = &after_url[quote_end + 1..];
    }

    out.push_str(rest);
    out
}

fn find_next_attr(s: &str) -> Option<(usize, usize)> {
    let href = s.find("href=\"/static/").map(|i| (i, i + 6));
    let src = s.find("src=\"/static/").map(|i| (i, i + 5));
    match (href, src) {
        (Some(a), Some(b)) => Some(if a.0 <= b.0 { a } else { b }),
        (a, b) => a.or(b),
    }
}

fn should_version(url: &str) -> bool {
    !url.contains('?')
        && VERSIONED_PREFIXES
            .iter()
            .any(|prefix| url.starts_with(prefix))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn versions_css_js_and_svg_sprites_preserving_fragments() {
        let input = concat!(
            r#"<link rel="stylesheet" href="/static/css/landing-header.css">"#,
            r#"<script src="/static/js/app.js" defer></script>"#,
            r#"<use href="/static/icons/landing.svg#explore"></use>"#,
            r#"<img src="/static/icons/favicon.svg">"#,
            r#"<img src="/static/images/landing-dashboard.webp">"#,
            r#"<link rel="stylesheet" href="/static/css/already.css?v=old">"#,
            r#"<meta property="og:image" content="https://qrcardapio.com.br/static/images/og-pt-BR.png">"#,
        );
        let rendered = version_static_assets_with(input.to_owned(), "abc123def456");

        assert!(rendered.contains(r#"href="/static/css/landing-header.css?v=abc123def456""#));
        assert!(rendered.contains(r#"src="/static/js/app.js?v=abc123def456""#));
        assert!(rendered.contains(r#"href="/static/icons/landing.svg?v=abc123def456#explore""#));
        assert!(rendered.contains(r#"src="/static/icons/favicon.svg?v=abc123def456""#));
        assert!(rendered.contains(r#"src="/static/images/landing-dashboard.webp""#));
        assert!(rendered.contains(r#"href="/static/css/already.css?v=old""#));
        assert!(
            rendered.contains(r#"content="https://qrcardapio.com.br/static/images/og-pt-BR.png""#)
        );
    }

    #[test]
    fn asset_version_is_twelve_lowercase_hex_chars() {
        assert_eq!(ASSET_VERSION.len(), 12);
        assert!(
            ASSET_VERSION
                .bytes()
                .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
        );
    }
}
