# ADR-004: Server-Rendered Askama HTML, Strict CSP & Native Zero-Reload i18n

- **Status**: Accepted (2026-11-23)
- **Context**: Customers on budget smartphones and spotty 3G need instant page loads without heavy SPA JS bundles; owners and travelers need seamless Portuguese, English, and Spanish (`pt-BR`, `en`, `es`) switching without losing unsaved form inputs or reloading pages.
- **Decision**: Use build-time Askama SSR templates paired with lightweight vanilla JS modules (`i18n.js`, `menu.js`, `app.js`), strict CSP (`script-src 'self'; style-src 'self'`), immutable cached `/static/i18n/{locale}.json` catalogs for dynamic pages, and an embedded ~1.6 KB `#qr-menu-i18n` JSON bundle + `data-pt`/`data-en`/`data-es` attributes inside static published menus.
- **Consequences**:
  - Zero Node.js process in development or production.
  - Instant first paint with SSR in the visitor's preferred language and zero-reload language toggling in place.
  - Static public menus switch languages completely offline-ready with zero extra HTTP requests.
