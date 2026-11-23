# Design System & Native i18n Frontend

## Visual Tokens (`static/css/tokens.css`)

- **Palette**: Warm off-white background (`#FAF7F2`), pure card surface (`#FFFFFF`), warm sand border (`#E7E0D5`), espresso charcoal ink (`#1C1917`), warm stone secondary ink (`#57534E`), default terracotta coral accent (`#D9532F`).
- **Brand Theming**: Published menus and owner previews load a generated external stylesheet (`theme-{hash8}.css` or `/dashboard/theme.css`) setting `--brand-accent`, `--brand-accent-hover`, `--brand-accent-ink` (computed via sRGB luminance for WCAG AA contrast), and `--brand-soft`.
- **Typography**: System UI sans-serif stack for instant first paint, zero external font requests, and crisp legibility on budget Android smartphones.
- **Touch Targets & Accessibility**: Interactive controls enforce `min-height: 44px`, keyboard `:focus-visible` outlines, skip links, semantic landmarks (`<header>`, `<main>`, `<nav>`, `<footer>`), and `aria-live="polite"` alerts.

## Zero Inline Scripts or Styles (Strict CSP)

`Content-Security-Policy` sets `script-src 'self'; style-src 'self'`. Consequently:
- No `style="..."` attributes exist in any HTML template (palette swatches use `.swatch-*` CSS classes).
- No executable inline `<script>` tags exist anywhere (only inert `<script type="application/json" id="qr-menu-i18n">` in static menus).

## Full Native Zero-Reload Multilingual i18n (`pt-BR`, `en`, `es`)

1. **Server-Side Resolution (`src/http/middleware/locale.rs`)**: `?lang=` query > `qr_lang` cookie > `Accept-Language` quality header > `pt-BR` default. Every page renders SSR in the active locale with zero flash of untranslated text.
2. **Dynamic Pages (`static/js/i18n.js`)**:
   - Header `PT | EN | ES` controls intercept clicks (`ev.preventDefault()`), fetch `/static/i18n/{locale}.json?v={hash}` once (in-memory + immutable HTTP cache), update all `[data-i18n]`, `[data-i18n-placeholder]`, `[data-i18n-aria]`, `[data-i18n-title]`, `[data-price-centavos]`, `document.title`, and `<html lang>` **in place without reloading the page**, preserving unsaved form inputs and persisting `qr_lang` cookie + `localStorage`.
   - Without JS, links still resolve cleanly via `?lang={locale}`.
3. **Static Public Menus (`/c/{slug}`, `static/js/menu.js`)**:
   - Published HTML embeds ~1.6 KB `#qr-menu-i18n` JSON for chrome strings and `data-pt` / `data-en` / `data-es` attributes for owner-provided translations.
   - Switching `PT | EN | ES` updates both UI chrome, formatted BRL prices, and translated dishes **instantly with zero page reload and zero network requests**.
