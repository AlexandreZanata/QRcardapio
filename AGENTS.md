# AGENTS.md — QR Cardápio

Authoritative guide for autonomous coding agents and human contributors working on **QR Cardápio** (`https://qrcardapio.com.br`).

## 1. What the Product Does

QR Cardápio is a Brazilian multi-tenant SaaS enabling food establishments (restaurants, cafés, snack bars, bakeries, ice cream and açaí shops) to create mobile-first digital menus and permanent printable QR Codes.

- **Core design**: Owner edits save drafts in SQLite. Publishing renders an atomic static HTML/CSS/media snapshot (`published/{slug}/index.html`). Public QR Code scans serve static files directly with **zero database queries**.
- **Native i18n (`pt-BR`, `en`, `es`)**: Full UI & menu translation with zero-reload frontend switching (`static/js/i18n.js` on dynamic pages, embedded `#qr-menu-i18n` + `static/js/menu.js` on static public menus).

## 2. Strictly Out of Scope (MVP)

Do NOT implement unless explicitly asked: online ordering, shopping carts, WhatsApp checkout, payments, subscription billing, AI menu extraction, loyalty programs, KDS/kitchen displays, POS integrations, inventory, delivery tracking, microservices, Kubernetes, or customer registration to view menus.

## 3. Architecture & Directory Structure

Modular monolith written in **Rust 1.100.0 (Edition 2024)** with **Axum 0.8.9**, **SQLx 0.9.1** (`sqlite-bundled` -> **SQLite 3.54.1**), and **Askama 0.16.1**:

```text
├── AGENTS.md
├── i18n/{pt-BR,en,es}.json        # Canonical translation keys (must stay 100% in parity)
├── migrations/                    # Versioned SQLx SQLite migrations
├── src/
│   ├── main.rs                    # CLI entrypoint (serve, db, admin, seed, bench-gen, republish-all)
│   ├── app.rs                     # AppState & Axum router composition
│   ├── config.rs                  # Environment-driven Config
│   ├── error.rs                   # Centralized AppError -> i18n key + HTTP status
│   ├── domain/                    # Pure domain types, invariants, centavos math, slug & color rules
│   ├── application/               # Use cases: auth, establishments, menus, ocr, publishing, admin, seed, bench_gen
│   ├── infrastructure/
│   │   ├── database/              # SQLite pool, PRAGMAs, VACUUM INTO backup/restore
│   │   │   └── queries/           # Standardized per-entity query submodules (NEVER a monolith)
│   │   ├── security/              # Bounded Argon2id, tokens, constant-time compare, governor rate limits
│   │   ├── assets/                # Magic-byte validation, bounded resize/re-encode (EXIF stripped)
│   │   ├── ocr/                   # Hybrid local PDF/image OCR (pdftotext/pdftoppm/tesseract + preprocessing)
│   │   └── storage/               # Path traversal guards & same-FS atomic temp+fsync+rename
│   ├── http/
│   │   ├── handlers/              # Thin per-feature Axum handlers
│   │   ├── middleware/            # Auth/Tenant/Admin extractors, locale resolution, strict CSP headers
│   │   └── responses/             # Flash cookies & ETag/304 helpers
│   ├── i18n/                      # Embedded compile-time Catalog & integer price formatting
│   ├── rendering/                 # Modular Askama view models & static HTML/CSS snapshot builder
│   └── qr/                        # High-contrast SVG & >=1024px PNG QR generation
├── static/{css,js,icons}/         # Design tokens, app/menu/print CSS, progressive zero-reload JS
├── templates/                     # Askama HTML templates (zero inline scripts or inline styles)
├── scripts/                       # dev.sh, backup.sh, restore.sh, bench_run.sh
├── tests/                         # Integration & E2E tests
└── docs/                          # Authoritative technical documentation & ADRs
```

## 4. Coding Rules & Anti-King-File Standard

- **No "king files"**: Keep source modules concise (~50–350 lines), cohesive, and named by single responsibility. Put new SQL queries into the matching `src/infrastructure/database/queries/<entity>.rs` file, not a catch-all file.
- **Dependency direction**: `http -> application -> domain` and `infrastructure -> domain`. Handlers stay thin; SQL never leaks into handlers or templates; domain never depends on Axum/SQLx.
- **Edition 2024**: Never use `gen` as an identifier. Never call `std::env::set_var` in tests (inject `Config` directly into `AppState::new`).
- **Axum 0.8 routing**: Always use `{id}` / `{slug}` path syntax (never `:id`).
- **Money**: Integer `price_centavos: i64` only (`parse_price_centavos`). Never use floats for money.
- **Strict CSP**: Zero inline `<script>` (except inert `type="application/json"`) and zero `style="..."` attributes in templates. Put dynamic brand variables in `theme-*.css` or `/dashboard/theme.css`.
- **i18n**: Every UI string must be added to ALL THREE catalogs (`i18n/pt-BR.json`, `i18n/en.json`, `i18n/es.json`) and tagged with `data-i18n` in templates.

## 5. Multi-Tenant & Security Constraints

- Extract `TenantContext` on every owner endpoint; verify `establishment_members` server-side. Never trust browser-supplied tenant IDs.
- All tenant SQL queries MUST include `AND establishment_id = ?` (and cross-verify foreign keys like `category_id` belong to the same `establishment_id`).
- All authenticated `POST` forms must embed and verify `_csrf` via `verify_csrf`.
- Never alter `slug` after `published_version > 0`.
- Never log passwords, session tokens, or reset tokens.

## 6. Commands Required Before Finishing Any Task

```bash
cargo fmt --check
cargo clippy --all-targets --all-features -- -D warnings
cargo test --locked
```

When changing dependencies, run `cargo audit` and `cargo build --release --locked`.
When modifying architecture, schemas, routes, UI patterns, or performance paths, update the corresponding document under `docs/` and `docs/agents/PROJECT_STATE.md`.
After passing all required checks above, **always commit and push the changes to GitHub (`origin main`)** before finishing any task.

## 7. Authoritative Documentation Links

- [System Overview](docs/architecture/OVERVIEW.md) | [Dependencies](docs/architecture/DEPENDENCIES.md)
- [Database & SQLite](docs/architecture/DATABASE.md) | [Static Publishing](docs/architecture/PUBLISHING.md) | [HTTP Contracts](docs/architecture/HTTP.md)
- [Domain Model](docs/domain/MODEL.md) | [Design System & i18n](docs/design/DESIGN_SYSTEM.md)
- [Security](docs/security/SECURITY.md) | [Performance & Benchmarks](docs/performance/PERFORMANCE.md)
- [Operations Runbook](docs/operations/RUNBOOK.md) | [Agent Workflow](docs/agents/WORKFLOW.md) | [Project State](docs/agents/PROJECT_STATE.md)
