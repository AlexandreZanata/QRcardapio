# Project State

Last updated: 2026-11-23

## Current Status — MVP Complete

All 5 implementation phases of the **QR Cardápio MVP** (including full native zero-reload `pt-BR` / `en` / `es` i18n across both owner panels and static published menus) are implemented, tested, benchmarked, and documented:

- **Phase 1 — Technical Foundation**: Complete & verified (`Cargo.toml`, `rust-toolchain.toml` pinned to `1.100.0`, Edition 2024, bundled SQLite `3.54.1` via `sqlx 0.9.1`, WAL + `synchronous=FULL`, migrations, structured logging, health/readiness endpoints).
- **Phase 2 — Domain, Security & Multi-Tenancy**: Complete & verified (Argon2id with bounded semaphore, CSPRNG SHA-256 hashed sessions with rotation, constant-time CSRF, IP & account rate limiting, `TenantContext` isolation, `BEGIN IMMEDIATE` write transactions, standardized per-entity submodules under `src/infrastructure/database/queries/`, EXIF-stripping image resizing).
- **Phase 3 — Premium Interface & Zero-Reload i18n**: Complete & verified (design tokens, responsive Askama templates, strict CSP with zero inline scripts/styles, `/static/i18n/{locale}.json` immutable endpoints, `static/js/i18n.js` + `static/js/menu.js` zero-reload switching).
- **Phase 4 — Static Publishing & QR Codes**: Complete & verified (atomic temp+fsync+rename publishing to `published/{slug}/`, content-addressed assets, failure rollback preserving previous live menu, unpublishing, zero-DB disk delivery, high-contrast SVG & >=1024px PNG QR generation, printable sheet).
- **Phase 5 — Operations, Benchmarks & AI-First Docs**: Complete & verified (`Dockerfile`, `compose.yaml`, `Caddyfile`, `scripts/{dev,backup,restore,bench_run}.sh`, full `AGENTS.md` + `docs/` + 6 ADRs, real benchmarks at 100..100k establishments).
- **Phase 6 — Local Hybrid OCR & PDF Menu Pre-Registration**: Complete & verified (`migrations/20261123000002_ocr_imports.sql`, pure-Rust menu parser under `src/domain/ocr_parser/`, hybrid `pdftotext` -> `pdftoppm` + pure-Rust image preprocessing + `tesseract` engine under `src/infrastructure/ocr/` with `Semaphore(1)` and timeouts, `ocr_imports` staging persistence, `/dashboard/ocr` upload/paste/review/bulk-apply/discard flow with full `pt-BR`/`en`/`es` i18n and strict multi-tenant + CSRF + rate-limit protection, ADR-006).

## Landing and Panel Refresh

The homepage includes native 4K browser captures of the owner dashboard and public mobile menu, with responsive WebP variants on the landing. The owner dashboard and operational administration share responsive side navigation. Operational tables include translated statuses and empty states.

The three how-it-works cards use custom storefront, menu book, and QR printer illustrations in the same style as the feature icons.

The public menu uses a branded cover, sticky search and categories, and responsive product cards. The shared CSS updates published snapshots in place. A photo cover is staged during publication when the visible menu has an available product photo.

The landing, dashboard, and public menu share an illustrated disclosure that shows the active language and opens the PT, EN, and ES choices. Previously-published menus gain the picker through their existing menu script.

The dashboard uses original icons and a contextual tutorial on the owner and platform workflows. Empty collections and browser error pages use coordinated illustrations. Owners can save a Google Maps profile or direct review link and publish an optional review invitation in their menu. The landing comparison uses public Maps profile fields and labeled synthetic review data for the fictional Brasa 128 hamburger restaurant. The landing style has a transferable implementation guide under `docs/design/`.

Settings groups business details, Google Maps links, and the current logo. Dashboard forms share translated Askama field components for settings, onboarding, categories, products, and menu import and review. Logo, product photo, and menu imports share an illustrated file picker with image preview, local format and size checks, and a native input fallback.

## Verification Snapshot

- `cargo fmt --check`: Passed
- `cargo clippy --all-targets --all-features -- -D warnings`: Passed (0 warnings)
- `cargo test --locked`: Passed (17 unit tests + 11 integration tests across 8 integration suites, including real `pdftotext` + `tesseract` E2E OCR tests)
- `cargo audit`: Passed (0 vulnerabilities; 2 unmaintained advisory notices: `basic-toml 0.1.10` transitive, `chrono 0.4.45`)
- `cargo build --release --locked`: Passed
- Live smoke test (`admin create`, `seed`, `backup.sh`, `restore.sh`, HTTP SSR + static `/c/cafe-da-praca`, `/dashboard/ocr`): Passed

## Next Recommended Actions (Pilot Validation)

1. Provision a 1 vCPU / 1 GB Linux VPS with DNS pointing `qrcardapio.com.br` and run `compose.yaml` + `Caddyfile` over HTTPS.
2. Run Lighthouse audits on a live HTTPS deployment (`/` and `/c/{slug}`) and verify physical smartphone QR scanning on iOS and Android cameras.
3. Onboard the first 5–10 pilot restaurants and collect real feedback before evaluating out-of-scope features (e.g., WhatsApp ordering or custom domains).
