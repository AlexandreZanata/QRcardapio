use std::collections::{BTreeMap, BTreeSet};
use std::io::Cursor;
use std::path::PathBuf;

use axum::Router;
use axum::body::Body;
use axum::http::header::CONTENT_TYPE;
use axum::http::{Request, StatusCode};
use http_body_util::BodyExt;
use image::{DynamicImage, ImageEncoder, codecs::png::PngEncoder};
use tempfile::TempDir;
use tower::ServiceExt;

use qrcardapio::app::{AppState, build_router};
use qrcardapio::application::auth::register_owner;
use qrcardapio::application::establishments::{CreateEstablishmentInput, create_establishment};
use qrcardapio::application::menus::{
    CategoryInput, ProductInput, create_category, create_product,
};
use qrcardapio::application::publishing::{PublishLocks, publish_establishment};
use qrcardapio::config::{Config, DbSynchronous, LogFormat};
use qrcardapio::i18n::Locale;
use qrcardapio::infrastructure::assets::AssetProcessor;
use qrcardapio::infrastructure::database::init_pool;
use qrcardapio::infrastructure::security::PasswordService;

const BASE_URL: &str = "https://qrcardapio.com.br";

#[derive(Debug)]
struct SitemapEntry {
    loc: String,
    alternates: Vec<(String, String)>,
    lastmod: String,
}

fn test_config(dir: &TempDir) -> Config {
    Config {
        asaas: None,
        mcp_enabled: false,
        google: None,
        bind_addr: "127.0.0.1:0".parse().unwrap(),
        public_base_url: BASE_URL.to_string(),
        data_dir: dir.path().join("data"),
        static_dir: PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("static"),
        db_max_connections: 4,
        db_synchronous: DbSynchronous::Full,
        cookie_secure: false,
        trust_proxy: false,
        log_format: LogFormat::Pretty,
        log_level: "warn".to_string(),
    }
}

fn png_bytes(w: u32, h: u32) -> Vec<u8> {
    let img = DynamicImage::new_rgba8(w, h);
    let rgba = img.to_rgba8();
    let mut out = Vec::new();
    PngEncoder::new(Cursor::new(&mut out))
        .write_image(&rgba, w, h, image::ExtendedColorType::Rgba8)
        .unwrap();
    out
}

async fn get(app: &Router, uri: &str) -> axum::response::Response {
    app.clone()
        .oneshot(Request::builder().uri(uri).body(Body::empty()).unwrap())
        .await
        .unwrap()
}

async fn body_bytes(res: axum::response::Response) -> Vec<u8> {
    res.into_body().collect().await.unwrap().to_bytes().to_vec()
}

async fn body_str(res: axum::response::Response) -> String {
    String::from_utf8(body_bytes(res).await).unwrap()
}

fn to_path(abs_url: &str) -> &str {
    abs_url.strip_prefix(BASE_URL).unwrap_or(abs_url)
}

fn parse_sitemap_entries(xml: &str) -> Vec<SitemapEntry> {
    let mut entries = Vec::new();
    let mut rest = xml;
    while let Some(start) = rest.find("<url>") {
        let after = &rest[start + 5..];
        let end = after.find("</url>").expect("closing </url>");
        let block = &after[..end];
        let loc = extract_between(block, "<loc>", "</loc>").expect("<loc>");
        let lastmod = extract_between(block, "<lastmod>", "</lastmod>").expect("<lastmod>");
        let mut alternates = Vec::new();
        let mut alt_rest = block;
        while let Some(idx) = alt_rest.find("<xhtml:link ") {
            let tag_end = alt_rest[idx..].find("/>").expect("closing xhtml:link");
            let tag = &alt_rest[idx..idx + tag_end];
            let lang = extract_attr(tag, "hreflang").expect("hreflang attr");
            let href = extract_attr(tag, "href").expect("href attr");
            alternates.push((lang, href));
            alt_rest = &alt_rest[idx + tag_end + 2..];
        }
        entries.push(SitemapEntry {
            loc,
            alternates,
            lastmod,
        });
        rest = &after[end + 6..];
    }
    entries
}

fn unescape_html_basic(s: &str) -> String {
    s.replace("&#38;", "&")
        .replace("&amp;", "&")
        .replace("&#39;", "'")
        .replace("&#x27;", "'")
        .replace("&quot;", "\"")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
}

fn extract_between(haystack: &str, open: &str, close: &str) -> Option<String> {
    let start = haystack.find(open)? + open.len();
    let end = haystack[start..].find(close)?;
    Some(haystack[start..start + end].to_string())
}

fn extract_attr(tag: &str, attr: &str) -> Option<String> {
    let needle = format!("{attr}=\"");
    let start = tag.find(&needle)? + needle.len();
    let end = tag[start..].find('"')?;
    Some(tag[start..start + end].to_string())
}

fn extract_meta_content(html: &str, key_attr: &str, key_val: &str) -> Option<String> {
    let mut rest = html;
    let needle = format!("{key_attr}=\"{key_val}\"");
    while let Some(idx) = rest.find("<meta ") {
        let end = rest[idx..].find('>')?;
        let tag = &rest[idx..idx + end + 1];
        if tag.contains(&needle) {
            return extract_attr(tag, "content");
        }
        rest = &rest[idx + end + 1..];
    }
    None
}

fn extract_link_hrefs(html: &str, rel_val: &str) -> Vec<(Option<String>, String)> {
    let mut out = Vec::new();
    let mut rest = html;
    let needle = format!("rel=\"{rel_val}\"");
    while let Some(idx) = rest.find("<link ") {
        let Some(end) = rest[idx..].find('>') else {
            break;
        };
        let tag = &rest[idx..idx + end + 1];
        if tag.contains(&needle)
            && let Some(href) = extract_attr(tag, "href")
        {
            out.push((extract_attr(tag, "hreflang"), href));
        }
        rest = &rest[idx + end + 1..];
    }
    out
}

fn assert_no_inline_scripts(html: &str, url: &str) {
    let mut rest = html;
    while let Some(idx) = rest.find("<script") {
        let end = rest[idx..].find('>').expect("closing >");
        let tag = &rest[idx..idx + end + 1];
        let allowed = tag.contains(" src=\"")
            || tag.contains("type=\"application/ld+json\"")
            || tag.contains("type=\"application/json\"");
        assert!(allowed, "disallowed inline <script> on {url}: {tag}");
        rest = &rest[idx + end + 1..];
    }
}

fn extract_json_lds(html: &str) -> Vec<serde_json::Value> {
    let mut out = Vec::new();
    let marker = "<script type=\"application/ld+json\">";
    let mut rest = html;
    while let Some(idx) = rest.find(marker) {
        let after = &rest[idx + marker.len()..];
        let end = after.find("</script>").expect("closing </script>");
        let parsed: serde_json::Value = serde_json::from_str(&after[..end]).expect("valid JSON-LD");
        out.push(parsed);
        rest = &after[end + 9..];
    }
    out
}

fn is_valid_iso_date(s: &str) -> bool {
    let b = s.as_bytes();
    b.len() == 10
        && b[4] == b'-'
        && b[7] == b'-'
        && b.iter()
            .enumerate()
            .all(|(i, c)| i == 4 || i == 7 || c.is_ascii_digit())
}

#[tokio::test]
async fn seo_contract_sitemap_reciprocity_metadata_and_schema() {
    let dir = TempDir::new().unwrap();
    let cfg = test_config(&dir);
    let pool = init_pool(&cfg).await.unwrap();

    let pw = PasswordService::new();
    let assets = AssetProcessor::new(cfg.media_dir());
    let locks = PublishLocks::default();
    let auth = register_owner(
        &pool,
        &pw,
        "contrato@bistro.com.br",
        "SenhaForte123",
        "SenhaForte123",
        Locale::PtBr,
    )
    .await
    .unwrap();
    let est = create_establishment(
        &pool,
        &auth.account.id,
        CreateEstablishmentInput {
            name: "Bistrô <script>alert(1)</script> Contrato",
            slug: "bistro-contrato",
            description: "Cozinha sazonal & carta digital.",
            accent_color: "#263be8",
        },
    )
    .await
    .unwrap();
    let cat = create_category(
        &pool,
        &est.id,
        CategoryInput {
            name: "Principais",
            name_en: "Mains",
            name_es: "Principales",
            description: "",
            description_en: "",
            description_es: "",
            is_visible: true,
        },
    )
    .await
    .unwrap();
    create_product(
        &pool,
        &assets,
        &est.id,
        ProductInput {
            category_id: &cat.id,
            name: "Risoto Especial",
            name_en: "Special Risotto",
            name_es: "Risotto Especial",
            description: "Arroz arbóreo e cogumelos.",
            description_en: "Arborio rice and mushrooms.",
            description_es: "Arroz arbóreo y hongos.",
            price_raw: "42,90",
            is_available: true,
            is_visible: true,
        },
        Some(png_bytes(480, 480)),
    )
    .await
    .unwrap();
    publish_establishment(&pool, &cfg, &locks, &est.id)
        .await
        .unwrap();

    let app = build_router(AppState::new(cfg, pool.clone()));
    let sitemap_xml = body_str(get(&app, "/sitemap.xml").await).await;
    let entries = parse_sitemap_entries(&sitemap_xml);
    assert_eq!(entries.len(), 58, "57 static localized pages + 1 menu");

    let mut seen_locs = BTreeSet::new();
    let mut seen_titles = BTreeSet::new();
    let mut seen_descs = BTreeSet::new();
    let mut hreflang_matrix: BTreeMap<String, BTreeMap<String, String>> = BTreeMap::new();

    for entry in &entries {
        assert!(
            seen_locs.insert(entry.loc.clone()),
            "duplicate loc {}",
            entry.loc
        );
        assert!(entry.loc.starts_with(BASE_URL));
        assert!(!entry.loc.contains('?') && !entry.loc.contains('#'));
        if entry.loc != format!("{BASE_URL}/") {
            assert!(!entry.loc.ends_with('/'), "trailing slash {}", entry.loc);
        }
        assert!(
            is_valid_iso_date(&entry.lastmod),
            "bad lastmod {}",
            entry.lastmod
        );

        let res = get(&app, to_path(&entry.loc)).await;
        assert_eq!(res.status(), StatusCode::OK, "url {}", entry.loc);
        assert!(
            res.headers()[CONTENT_TYPE]
                .to_str()
                .unwrap()
                .starts_with("text/html")
        );
        let csp = res
            .headers()
            .get("content-security-policy")
            .unwrap()
            .to_str()
            .unwrap();
        assert!(csp.contains("script-src 'self'") && csp.contains("style-src 'self'"));

        let html = body_str(res).await;
        assert!(!html.contains("style=\""), "inline style on {}", entry.loc);
        assert_no_inline_scripts(&html, &entry.loc);
        assert_eq!(html.matches("<h1").count(), 1, "single h1 on {}", entry.loc);

        let canonical = extract_link_hrefs(&html, "canonical");
        assert_eq!(canonical.len(), 1, "single canonical on {}", entry.loc);
        assert_eq!(canonical[0].1, entry.loc);

        let robots = extract_meta_content(&html, "name", "robots").expect("robots meta");
        assert!(robots.contains("index, follow"));

        let lds = extract_json_lds(&html);
        assert_eq!(lds.len(), 1, "single JSON-LD block on {}", entry.loc);
        let raw_ld = lds[0].to_string();
        assert!(!raw_ld.contains("\"aggregateRating\""), "no fake ratings");
        assert!(!raw_ld.contains("\"@type\":\"Review\""), "no fake reviews");
        assert_eq!(lds[0]["@context"], "https://schema.org");
        let graph = lds[0]["@graph"].as_array().expect("@graph array");
        assert!(!graph.is_empty());

        if entry.loc == format!("{BASE_URL}/c/bistro-contrato") {
            assert!(!html.contains("<script>alert(1)</script>"));
            assert_eq!(graph[0]["@type"], "FoodEstablishment");
            assert_eq!(graph[1]["@type"], "Menu");
            assert_eq!(
                graph[1]["hasMenuSection"][0]["hasMenuItem"][0]["offers"]["priceCurrency"],
                "BRL"
            );
            assert_eq!(
                graph[1]["hasMenuSection"][0]["hasMenuItem"][0]["offers"]["price"],
                "42.90"
            );
            continue;
        }

        let title =
            unescape_html_basic(&extract_between(&html, "<title>", "</title>").expect("<title>"));
        let title_len = title.chars().count();
        assert!(
            (30..=68).contains(&title_len),
            "title length {title_len} out of [30,68] on {}: {title}",
            entry.loc
        );
        assert!(seen_titles.insert(title.clone()), "duplicate title {title}");

        let desc = unescape_html_basic(
            &extract_meta_content(&html, "name", "description").expect("description"),
        );
        let desc_len = desc.chars().count();
        assert!(
            (110..=165).contains(&desc_len),
            "desc length {desc_len} out of [110,165] on {}: {desc}",
            entry.loc
        );
        assert!(seen_descs.insert(desc.clone()), "duplicate desc {desc}");

        assert_eq!(
            extract_meta_content(&html, "property", "og:title")
                .map(|s| unescape_html_basic(&s))
                .as_deref(),
            Some(title.as_str())
        );
        assert_eq!(
            extract_meta_content(&html, "property", "og:description")
                .map(|s| unescape_html_basic(&s))
                .as_deref(),
            Some(desc.as_str())
        );
        assert_eq!(
            extract_meta_content(&html, "property", "og:url").as_deref(),
            Some(entry.loc.as_str())
        );
        assert_eq!(
            extract_meta_content(&html, "property", "og:image:width").as_deref(),
            Some("1200")
        );
        assert_eq!(
            extract_meta_content(&html, "property", "og:image:height").as_deref(),
            Some("630")
        );
        assert_eq!(
            extract_meta_content(&html, "name", "twitter:card").as_deref(),
            Some("summary_large_image")
        );

        let html_alts: BTreeMap<String, String> = extract_link_hrefs(&html, "alternate")
            .into_iter()
            .filter_map(|(lang, href)| lang.map(|l| (l, href)))
            .collect();
        let xml_alts: BTreeMap<String, String> = entry.alternates.iter().cloned().collect();
        assert_eq!(html_alts.len(), 4, "4 hreflang links on {}", entry.loc);
        assert_eq!(
            html_alts, xml_alts,
            "HTML/XML hreflang parity on {}",
            entry.loc
        );
        assert_eq!(html_alts.get("x-default"), html_alts.get("pt-BR"));
        hreflang_matrix.insert(entry.loc.clone(), html_alts);

        for node in graph {
            if node["@type"] == "FAQPage" {
                for q in node["mainEntity"].as_array().unwrap() {
                    let q_name = q["name"].as_str().unwrap();
                    assert!(
                        unescape_html_basic(&html).contains(q_name),
                        "FAQ schema mismatch on {}: {q_name}",
                        entry.loc
                    );
                }
            }
            if node["@type"] == "BreadcrumbList" {
                let items = node["itemListElement"].as_array().unwrap();
                for (idx, item) in items.iter().enumerate() {
                    assert_eq!(item["position"].as_u64(), Some((idx + 1) as u64));
                    assert!(item["item"].as_str().unwrap().starts_with("https://"));
                }
            }
        }
    }

    for (page_a, alts_a) in &hreflang_matrix {
        for (lang, page_b) in alts_a {
            if lang == "x-default" {
                continue;
            }
            let alts_b = hreflang_matrix.get(page_b).unwrap_or_else(|| {
                panic!("missing hreflang target {page_b} referenced by {page_a}")
            });
            assert!(
                alts_b.values().any(|target| target == page_a),
                "broken reciprocity: {page_a} -> {page_b} ({lang}) missing back-link"
            );
        }
    }

    pool.close().await;
}

#[tokio::test]
async fn seo_contract_assets_llms_links_and_private_noindex() {
    let dir = TempDir::new().unwrap();
    let cfg = test_config(&dir);
    let pool = init_pool(&cfg).await.unwrap();
    let app = build_router(AppState::new(cfg, pool.clone()));

    for (path, w, h) in [
        ("/static/images/og-pt-BR.png", 1200, 630),
        ("/static/images/og-en.png", 1200, 630),
        ("/static/images/og-es.png", 1200, 630),
        ("/apple-touch-icon.png", 180, 180),
        ("/static/icons/icon-192.png", 192, 192),
        ("/static/icons/icon-512.png", 512, 512),
        ("/static/icons/icon-maskable-512.png", 512, 512),
    ] {
        let res = get(&app, path).await;
        assert_eq!(res.status(), StatusCode::OK, "image {path}");
        let img = image::load_from_memory(&body_bytes(res).await).expect("valid PNG");
        assert_eq!((img.width(), img.height()), (w, h), "dims for {path}");
    }

    let llms = body_str(get(&app, "/llms.txt").await).await;
    let mut rest = llms.as_str();
    let mut checked_llms = 0usize;
    while let Some(idx) = rest.find("](https://qrcardapio.com.br") {
        let after = &rest[idx + 2..];
        let end = after.find(')').expect("closing )");
        let url = &after[..end];
        assert_eq!(
            get(&app, to_path(url)).await.status(),
            StatusCode::OK,
            "llms link {url}"
        );
        checked_llms += 1;
        rest = &after[end + 1..];
    }
    assert!(checked_llms >= 18);

    for home_path in ["/", "/en", "/es"] {
        let html = body_str(get(&app, home_path).await).await;
        assert!(
            html.contains("<link rel=\"preload\" as=\"image\""),
            "LCP preload on {home_path}"
        );
        let footer_start = html.find("<footer").expect("footer");
        let mut f_rest = &html[footer_start..];
        while let Some(idx) = f_rest.find("href=\"/") {
            let after = &f_rest[idx + 6..];
            let end = after.find('"').expect("closing quote");
            let target = after[..end].split('#').next().unwrap_or("/");
            assert_eq!(
                get(&app, target).await.status(),
                StatusCode::OK,
                "footer link {target}"
            );
            f_rest = &after[end + 1..];
        }
    }

    let robots = body_str(get(&app, "/robots.txt").await).await;
    for disallow in [
        "/dashboard",
        "/onboarding",
        "/admin",
        "/login",
        "/register",
        "/logout",
        "/reset-password",
        "/mcp",
        "/webhooks/",
        "/healthz",
        "/readyz",
        "/api/tools/",
    ] {
        assert!(robots.contains(&format!("Disallow: {disallow}")));
    }
    for bot in [
        "OAI-SearchBot",
        "ChatGPT-User",
        "GPTBot",
        "ClaudeBot",
        "Claude-SearchBot",
        "PerplexityBot",
        "Google-Extended",
    ] {
        assert!(robots.contains(bot));
    }

    let sitemap_xml = body_str(get(&app, "/sitemap.xml").await).await;
    for priv_path in [
        "/login",
        "/register",
        "/reset-password",
        "/nonexistent-seo-404",
    ] {
        let html = body_str(get(&app, priv_path).await).await;
        assert_eq!(
            extract_meta_content(&html, "name", "robots").as_deref(),
            Some("noindex, nofollow"),
            "missing noindex on {priv_path}"
        );
        assert!(!sitemap_xml.contains(&format!("{BASE_URL}{priv_path}<")));
    }

    pool.close().await;
}
