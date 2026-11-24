use crate::i18n::Locale;

use super::guides::all_guides;
use super::routes::RouteCluster;
use super::segments::all_segments;
use super::tools::all_tools;
use super::{SEO_LASTMOD, absolute_url, normalized_base_url};

#[derive(Debug, Clone)]
pub struct PublishedMenuEntry {
    pub slug: String,
    pub lastmod: String,
}

pub fn build_robots_txt(base_url: &str) -> String {
    let base = normalized_base_url(base_url);
    let shared_rules = "\
Allow: /
Disallow: /dashboard
Disallow: /onboarding
Disallow: /admin
Disallow: /login
Disallow: /register
Disallow: /logout
Disallow: /reset-password
Disallow: /mcp
Disallow: /webhooks/
Disallow: /healthz
Disallow: /readyz
Disallow: /api/tools/
";

    let mut out = String::new();
    for agent in [
        "*",
        "OAI-SearchBot",
        "ChatGPT-User",
        "GPTBot",
        "ClaudeBot",
        "Claude-SearchBot",
        "PerplexityBot",
        "Google-Extended",
    ] {
        out.push_str(&format!("User-agent: {agent}\n{shared_rules}\n"));
    }
    out.push_str(&format!("Sitemap: {base}/sitemap.xml\n"));
    out
}

pub fn build_sitemap_xml(base_url: &str, published_menus: &[PublishedMenuEntry]) -> String {
    let base = normalized_base_url(base_url);
    let mut xml = String::from(
        "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n\
         <urlset xmlns=\"http://www.sitemaps.org/schemas/sitemap/0.9\" \
         xmlns:xhtml=\"http://www.w3.org/1999/xhtml\">\n",
    );

    for (cluster, changefreq, priority) in RouteCluster::all_sitemap_clusters() {
        let alternates = cluster.alternates(base);
        for locale in Locale::ALL {
            let loc = absolute_url(base, cluster.path_for(locale));
            xml.push_str("  <url>\n");
            xml.push_str(&format!("    <loc>{}</loc>\n", escape_xml(&loc)));
            for alt in &alternates {
                xml.push_str(&format!(
                    "    <xhtml:link rel=\"alternate\" hreflang=\"{}\" href=\"{}\"/>\n",
                    escape_xml(alt.hreflang),
                    escape_xml(&alt.href),
                ));
            }
            xml.push_str(&format!("    <lastmod>{SEO_LASTMOD}</lastmod>\n"));
            xml.push_str(&format!("    <changefreq>{changefreq}</changefreq>\n"));
            xml.push_str(&format!("    <priority>{priority}</priority>\n"));
            xml.push_str("  </url>\n");
        }
    }

    for menu in published_menus {
        let loc = format!("{base}/c/{}", menu.slug);
        let lastmod = normalize_date(&menu.lastmod);
        xml.push_str("  <url>\n");
        xml.push_str(&format!("    <loc>{}</loc>\n", escape_xml(&loc)));
        xml.push_str(&format!("    <lastmod>{}</lastmod>\n", escape_xml(lastmod)));
        xml.push_str("    <changefreq>weekly</changefreq>\n");
        xml.push_str("    <priority>0.7</priority>\n");
        xml.push_str("  </url>\n");
    }

    xml.push_str("</urlset>\n");
    xml
}

pub fn build_llms_txt(base_url: &str) -> String {
    let base = normalized_base_url(base_url);
    let mut md = format!(
        "# QR Cardápio\n\n\
         > Plataforma brasileira para bares, restaurantes, cafeterias, hamburguerias, pizzarias, lanchonetes, padarias e açaiterias criarem cardápios digitais responsivos com QR Code permanente e tradução nativa em Português, Inglês e Espanhol.\n\n\
         > Brazilian digital menu SaaS built for fast mobile QR scanning, instant draft-to-static HTML publishing, built-in OCR PDF/photo menu import, remote MCP draft editing (ChatGPT, Claude, Antigravity via OAuth PKCE), and Google Maps review invitations.\n\n\
         ## Idiomas / Languages\n\n\
         - [Português (Brasil)]({base}/)\n\
         - [English]({base}/en)\n\
         - [Español]({base}/es)\n\n\
         ## Soluções por segmento\n\n\
         - [Todas as soluções]({base}/solucoes)\n"
    );
    for seg in all_segments(Locale::PtBr) {
        let url = absolute_url(base, seg.id.full_path(Locale::PtBr));
        md.push_str(&format!("- [{}]({url}): {}\n", seg.name, seg.short_desc));
    }

    md.push_str(&format!(
        "\n## Ferramentas gratuitas\n\n- [Todas as ferramentas]({base}/ferramentas)\n"
    ));
    for tool in all_tools(Locale::PtBr) {
        let url = absolute_url(base, tool.id.full_path(Locale::PtBr));
        md.push_str(&format!("- [{}]({url}): {}\n", tool.name, tool.short_desc));
    }

    md.push_str(&format!(
        "\n## Guias práticos\n\n- [Todos os guias]({base}/guias)\n"
    ));
    for guide in all_guides(Locale::PtBr) {
        let url = absolute_url(base, guide.id.full_path(Locale::PtBr));
        md.push_str(&format!(
            "- [{}]({url}): {}\n",
            guide.title, guide.short_desc
        ));
    }
    md
}

pub fn build_manifest_json() -> String {
    serde_json::json!({
        "name": "QR Cardápio",
        "short_name": "QR Cardápio",
        "description": "Crie um cardápio digital bonito com QR Code permanente e atualize fotos e preços quando quiser.",
        "lang": "pt-BR",
        "dir": "ltr",
        "start_url": "/",
        "scope": "/",
        "display": "browser",
        "background_color": "#fff9ee",
        "theme_color": "#263be8",
        "icons": [
            {
                "src": "/static/icons/icon-192.png",
                "sizes": "192x192",
                "type": "image/png",
                "purpose": "any"
            },
            {
                "src": "/static/icons/icon-512.png",
                "sizes": "512x512",
                "type": "image/png",
                "purpose": "any"
            },
            {
                "src": "/static/icons/icon-maskable-512.png",
                "sizes": "512x512",
                "type": "image/png",
                "purpose": "maskable"
            }
        ]
    })
    .to_string()
}

fn normalize_date(raw: &str) -> &str {
    let trimmed = raw.trim();
    if trimmed.len() >= 10 && trimmed.as_bytes()[4] == b'-' && trimmed.as_bytes()[7] == b'-' {
        &trimmed[..10]
    } else {
        SEO_LASTMOD
    }
}

fn escape_xml(raw: &str) -> String {
    let mut out = String::with_capacity(raw.len());
    for ch in raw.chars() {
        match ch {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&apos;"),
            _ => out.push(ch),
        }
    }
    out
}
