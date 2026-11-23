# SEO & GEO Architecture

QR Cardápio implements a full technical SEO, international multilingual routing, and Generative Engine Optimization (GEO) stack compiled directly into the Rust binary with strict Content Security Policy compliance.

## 1. Crawl Control & Machine-Readable Endpoints

| Endpoint | Content Type | Purpose |
|---|---|---|
| `GET /robots.txt` | `text/plain; charset=utf-8` | Allows public landing, segments, tools, guides, and `/c/*` menus for `*`, `OAI-SearchBot`, `ChatGPT-User`, `GPTBot`, `ClaudeBot`, `Claude-SearchBot`, `PerplexityBot`, and `Google-Extended`. Disallows `/dashboard`, `/onboarding`, `/admin`, `/login`, `/register`, `/logout`, `/reset-password`, `/mcp`, `/webhooks/`, `/healthz`, `/readyz`, and `/api/tools/`. Advertises `Sitemap: {PUBLIC_BASE_URL}/sitemap.xml`. |
| `GET /sitemap.xml` | `application/xml; charset=utf-8` | Dynamic XML sitemap with strong `ETag` (`304 Not Modified`), full `xmlns:xhtml` bidirectional `hreflang` links (`pt-BR`, `en`, `es`, `x-default`) across all public routes, plus live published `/c/{slug}` menus queried from SQLite (`is_published = 1`). |
| `GET /llms.txt` | `text/plain; charset=utf-8` | Structured Markdown overview for language models and AI search assistants linking to localized landing pages, vertical blueprints, free utilities, and guides. |
| `GET /manifest.webmanifest` | `application/manifest+json; charset=utf-8` | Web App Manifest referencing `192x192`, `512x512`, and maskable PNG icons. |
| `GET /favicon.ico`, `GET /apple-touch-icon.png` | `image/x-icon`, `image/png` | Standard root icon endpoints served from `static/icons/`. |

## 2. Multilingual Clusters & Zero-Reload Navigation

- Canonical localized entry points: `/` (`pt-BR`), `/en` (`en`), and `/es` (`es`), with `301 Permanent Redirect` from `/en/` and `/es/`.
- `ExplicitRequestLocale` resolves `/` only from `?lang=` or the `qr_lang` cookie (never `Accept-Language`) so search crawlers consistently receive `pt-BR` on `/` while browser language selections remain preserved.
- `/?lang=en` or `/?lang=es` consolidates its `<link rel="canonical">` and Open Graph URLs to `/en` or `/es`.
- On the landing page, `static/js/i18n.js` updates the visible path via `history.replaceState` when switching languages without reloading, and updates `<meta name="description">`.
- All SEO subpages (`segments`, `tools`, `guides`) form fully connected 3-language `RouteCluster` maps with self-referencing canonical URLs and bidirectional `hreflang` + `x-default`.

## 3. Structured Data (`JSON-LD @graph`)

All JSON-LD payloads are serialized through `src/seo/schema.rs` using `safe_json_ld`, escaping `<`, `>`, and `&` into `\u003c`, `\u003e`, and `\u0026` to uphold strict XSS hygiene inside `<script type="application/ld+json">`.

- **Landing (`/`, `/en`, `/es`)**: `Organization`, `WebSite`, `SoftwareApplication` (including multi-cycle `Offer` entries across `BillingPlan::ALL` — Monthly, Semiannual, and Annual — when plans are shown), `WebPage`, `HowTo`, and `FAQPage` (synced to the 8 visible `<details>` items in `#faq`).
- **Solutions by Vertical**: `Organization`, `WebSite`, `WebPage`, `BreadcrumbList`, `FAQPage`.
- **Free Tools**: `Organization`, `WebSite`, `WebApplication` (`isAccessibleForFree: true`, `price: "0"`), `BreadcrumbList`, `FAQPage`.
- **Guides**: `Organization`, `WebSite`, `Article`, `BreadcrumbList`, `FAQPage`.
- **Published Static Menus (`/c/{slug}`)**: `FoodEstablishment` + `Menu` (`MenuSection` -> `MenuItem` -> `Offer` formatted via exact integer centavos math). Draft previews (`/dashboard/preview`) emit `noindex, nofollow` and omit canonical/JSON-LD.

## 4. Programmatic SEO (pSEO), Free Tools & GEO Content

- **8 Business Vertical Pages** (`src/seo/segments_{pt,en,es}.rs`): Restaurants, Burger Shops, Coffee Shops, Snack Bars, Pizzerias, Açaí & Ice Cream Shops, Bakeries, and Bars & Pubs — each with Answer-First GEO summaries, tailored pain points, sample category blueprints, tabletop tips, and FAQs.
- **3 Free Zero-Signup Tools** (`src/seo/tools.rs`, `src/http/handlers/seo/tools.rs`, `static/js/seo-tools.js`):
  1. Menu QR Code Generator (live preview + vector SVG and 1024 px PNG downloads via rate-limited `/api/tools/qr.{svg,png}`).
  2. Food Cost & Menu Pricing Calculator (integer centavos math in the browser with target CMV presets).
  3. Printable Tabletop QR Tent Generator (with dedicated `@media print` sheet).
- **4 Operational Guides** (`src/seo/guides_{pt,en,es}.rs`): Creation guide, PDF vs. Digital Menu comparison table, Google Maps reviews playbook, and Menu Engineering & QR printing standards.

## 5. Social Previews, Icons & Core Web Vitals

- Localized 1200×630 PNG social cards (`static/images/og-{pt-BR,en,es}.png`) power `og:image` and `twitter:card` (`summary_large_image`).
- `templates/landing/index.html` preloads the responsive hero LCP dashboard WebP image (`<link rel="preload" as="image" imagesrcset=...>`).
- Zero inline `<script>` execution and zero `style="..."` attributes preserve strict `script-src 'self'; style-src 'self'` CSP compliance across all SEO pages.
