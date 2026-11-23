# Design System & Native i18n Frontend

## Visual Tokens (`static/css/tokens.css`)

- **Palette**: Warm off-white background (`#FAF7F2`), pure card surface (`#FFFFFF`), warm sand border (`#E7E0D5`), espresso charcoal ink (`#1C1917`), warm stone secondary ink (`#57534E`), default terracotta coral accent (`#D9532F`).
- **Brand Theming**: Published menus and owner previews load a generated external stylesheet (`theme-{hash8}.css` or `/dashboard/theme.css`) setting `--brand-accent`, `--brand-accent-hover`, `--brand-accent-ink` (computed via sRGB luminance for WCAG AA contrast), and `--brand-soft`.
- **Typography**: System UI sans-serif stack for instant first paint, zero external font requests, and crisp legibility on budget Android smartphones.
- **Touch Targets & Accessibility**: Interactive controls enforce `min-height: 44px`, keyboard `:focus-visible` outlines, skip links, semantic landmarks (`<header>`, `<main>`, `<nav>`, `<footer>`), and `aria-live="polite"` alerts.

## Landing Page and Dashboard

The landing page uses its own cream, blue, yellow, and pink palette. The device illustration uses real browser captures of the owner dashboard and public mobile menu for a fictional establishment. The enlarged WebP files are captured natively at 3840 × 2160 and 2160 × 3840 pixels. The landing selects smaller WebP derivatives through `srcset` and uses translated captions and alternative text. Keep personal information out of captures. The three original food photos under `static/images/` can be used to recreate the fictional menu.

Dashboard pages share a dark sidebar, a workspace header with a language switcher, and an active link marked with `aria-current="page"`. On narrow screens, `static/js/app.js` enables a collapsible sidebar with keyboard dismissal. The navigation remains visible without JavaScript. The operations page adds shortcuts to its establishment and publication tables.

The dashboard uses the original `static/icons/dashboard.svg` illustrations for navigation, metrics, and actions. The Guide button opens a keyboard-accessible dialog and highlights controls annotated by `components/guide_hint.html`. It scrolls each control into view, reveals collapsed fields for its explanation, and restores the previous view on close. The component reads translated instructions from the page catalog.

The Google Maps landing section compares two simulated 30-day periods for the fictional Café Pingo. It uses an original storefront photo generated from the brief for a Brazilian café with cream walls, cobalt window frames, and a yellow awning. The comparison illustrates review and profile-visit counts without presenting them as measured results.

The brand mark and icon sprites live under `static/icons/`. The landing's features and how-it-works sections share the illustrated SVG sprite, with bold outlines and fills from the landing palette. Keep decorative icons hidden from assistive technology when the adjacent text already names the action. Landing assets are served from `static/images/` and CSS stays scoped to the landing and dashboard layouts.

## Public Menu

The public menu uses a branded cover and a centered establishment identity. The cover uses the first visible, available product photo in menu order when the snapshot provides one. An illustrated cover uses the same brand color for menus without photos. The layout supports the shared static stylesheet on previously published snapshots, including empty menus and menus with no logo. A sticky search and category strip precedes product cards. Cards use one column on phones and two on wider screens.

## Illustrated States

Menu editing, import history, and operations show an illustrated empty state when there are no records. The error page uses the same chef character and the artwork for missing pages, restricted access, or service failures. The status code remains visible and the text is translated through the existing catalog.

## Zero Inline Scripts or Styles (Strict CSP)

`Content-Security-Policy` sets `script-src 'self'; style-src 'self'`. Consequently:
- No `style="..."` attributes exist in any HTML template (palette swatches use `.swatch-*` CSS classes).
- No executable inline `<script>` tags exist anywhere (only inert `<script type="application/json" id="qr-menu-i18n">` in static menus).

## Full Native Zero-Reload Multilingual i18n (`pt-BR`, `en`, `es`)

1. **Server-Side Resolution (`src/http/middleware/locale.rs`)**: `?lang=` query > `qr_lang` cookie > `Accept-Language` quality header > `pt-BR` default. Every page renders SSR in the active locale with zero flash of untranslated text.
2. **Dynamic Pages (`static/js/i18n.js`)**:
   - The illustrated language picker shows the active language in a native disclosure. Choosing an option fetches `/static/i18n/{locale}.json?v={hash}` once and updates page text, prices, the document language, and the active icon in place. It preserves unsaved form inputs and stores the choice in the `qr_lang` cookie and local storage.
   - Without JS, links still resolve cleanly via `?lang={locale}`.
3. **Static Public Menus (`/c/{slug}`, `static/js/menu.js`)**:
   - Published HTML embeds ~1.6 KB `#qr-menu-i18n` JSON for chrome strings and `data-pt` / `data-en` / `data-es` attributes for owner-provided translations.
   - Switching `PT | EN | ES` updates the interface, formatted BRL prices, and translated dishes in place from the embedded catalog.
   - Existing snapshots adopt the same compact picker when `static/js/menu.js` enhances their previous language controls.
