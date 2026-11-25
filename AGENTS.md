# AGENTS.md — QR Cardápio

Authoritative guide for autonomous coding agents and human contributors working on **QR Cardápio** (`https://qrcardapio.com.br`).

## 1. What the Product Does

QR Cardápio is a Brazilian multi-tenant SaaS enabling food establishments (restaurants, cafés, snack bars, bakeries, ice cream and açaí shops) to create mobile-first digital menus and permanent printable QR Codes.

- **Core design**: Owner edits save drafts in SQLite. Publishing renders an atomic static HTML/CSS/media snapshot (`published/{slug}/index.html`). Public QR Code scans serve static files directly with **zero database queries**.
- **Modular commercial foundation**: a free QR Core plus independently sold modules. A versioned catalog compiled from `contracts/commercial/` prices consolidated subscriptions (Asaas); the Entitlement Engine grants capabilities from paid contracts or audited grants. Only QR Menu is sellable today.
- **Native i18n (`pt-BR`, `en`, `es`)**: Full UI & menu translation with zero-reload frontend switching (`static/js/i18n.js` on dynamic pages, embedded `#qr-menu-i18n` + `static/js/menu.js` on static public menus).

## 2. Product Scope and Phase Discipline

QR Cardápio is evolving into a modular platform (QR Core + nine commercial modules). Only **QR Menu** is implemented and sellable. The other modules (Delivery, Tables, Kitchen, Fleet, Loyalty, Analytics, POS, Connect) have contracts and specifications but are `planned`.

- Do NOT implement operational features of a planned module (ordering, carts, checkout for food, payments from consumers, KDS, POS, loyalty, delivery tracking, courier apps, public APIs) unless the task explicitly starts that module's phase in [MASTER_ROADMAP](docs/roadmap/MASTER_ROADMAP.md). Phase order: Commerce Core → Delivery → Tables → Kitchen → Fleet → Loyalty → Analytics → POS → Connect.
- Never create fictitious screens, functional buttons or endpoints for planned modules, and never document planned endpoints as available.
- Still out of scope: AI menu extraction via cloud APIs, microservices, Kubernetes, Kafka/RabbitMQ/Redis, per-tenant databases, Node.js at runtime, customer registration to view menus, platform commission on food orders.

## 3. Architecture & Directory Structure

Modular monolith written in **Rust 1.100.0 (Edition 2024)** with **Axum 0.8.9**, **SQLx 0.9.1** (`sqlite-bundled` -> **SQLite 3.54.1**), and **Askama 0.16.1**:

```text
├── AGENTS.md
├── contracts/                     # Machine-readable contracts (commercial catalog, entitlements, events, modules)
├── i18n/{pt-BR,en,es}.json        # Canonical translation keys (must stay 100% in parity)
├── migrations/                    # Versioned SQLx SQLite migrations
├── src/
│   ├── main.rs                    # CLI entrypoint (serve, db, admin, seed, bench-gen, republish-all)
│   ├── app.rs                     # AppState & Axum router composition
│   ├── config.rs                  # Environment-driven Config
│   ├── error.rs                   # Centralized AppError -> i18n key + HTTP status
│   ├── domain/                    # Pure domain types, invariants, centavos math, slug & color rules
│   │   ├── commercial/            # Versioned catalog (from contracts/commercial), cycles, quotes, lifecycle
│   │   └── entitlements/          # Capability, access resolution, MemberRole/Operation rules
│   ├── application/               # Use cases: auth, establishments, menus, ocr, publishing, billing, commercial, entitlements, operator
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

## 5. Commercial & Entitlement Rules

- **Prices live only in `contracts/commercial/*.json`.** Never hard-code a price, discount, module code or bundle composition in Rust, templates, SQL or JS. Read them through `domain::commercial::catalog()`; price with `CommercialCatalog::quote` / `apply_discount` (integer centavos, round half up per line).
- **Never branch on plan or bundle names.** Gate operations with `application::entitlements::authorize(pool, state.billing.enforcement(), est, tenant.role, Operation::X, billing_now().date_naive())`. `state.billing` is an `application::payments::BillingGateways`. Add new operations to `domain::entitlements::Operation` and `contracts/entitlements/authorization-rules.json` together.
- **Keep the five dimensions separate**: module lifecycle, establishment operational state, member permission, deployment enforcement, commercial entitlement. Each denial has its own i18n key.
- **Contracts are immutable snapshots.** Never update `commercial_contract_items` or recalculate past charges. Sold catalog versions cannot be edited (the start-up registry refuses it): add a new version.
- **The browser never sends amounts.** Checkout forms send offer codes (`offer`) and a cycle (`plan`) only.
- **Outbox**: when a state change has a registered event, append it with `queries::domain_events::append` in the same transaction; payloads carry ids and integer amounts only (no PII). Register new events in `contracts/events/registry.json` with a JSON Schema.
- **Migrations are expand-contract** (ADR-010): additive and idempotent first; destructive changes only in a later release. `init_pool` tolerates newer migrations.
- **Contract drift is a test failure**: update `contracts/`, `docs/modules/*.md` and `docs/business/MODULE_CATALOG.md` together; run `cargo test --test commercial_contracts`.
- SaaS billing (platform ↔ establishment) and Commerce Payments (consumer ↔ establishment) must never share credentials, beneficiaries, state or ledgers.
- **Payment providers** (Asaas, Stripe, Inter) are adapters: translate provider facts into `domain::payments::GatewayEvent` and apply them with `application::billing_events::apply_events`. Never branch coverage, entitlements or commissions on the provider. Store Stripe/Inter ids qualified (`stripe:`, `inter:`), verify webhooks (header token, Stripe HMAC, Inter secret path + mTLS re-fetch) and never log provider payloads or payer documents. See `docs/architecture/payments/`.

## 6. Multi-Tenant & Security Constraints

- Extract `TenantContext` on every owner endpoint; verify `establishment_members` server-side. Never trust browser-supplied tenant IDs.
- All tenant SQL queries MUST include `AND establishment_id = ?` (and cross-verify foreign keys like `category_id` belong to the same `establishment_id`).
- All authenticated `POST` forms must embed and verify `_csrf` via `verify_csrf`.
- Never alter `slug` after `published_version > 0`.
- Never log passwords, session tokens, or reset tokens.

## 7. Commands Required Before Finishing Any Task

```bash
cargo fmt --check
cargo clippy --all-targets --all-features -- -D warnings
cargo test --locked
```

When changing dependencies, run `cargo audit` and `cargo build --release --locked`.
When modifying architecture, schemas, routes, UI patterns, or performance paths, update the corresponding document under `docs/` and `docs/agents/PROJECT_STATE.md`.
Because `main` is protected by Pull Request and required GitHub CI checks (`branch-policy`, `seo-contracts`, `quality`), always create a branch conforming to [Branch Policy](docs/agents/BRANCH_POLICY.md), commit after passing all checks above, push the feature branch to GitHub (`origin <branch>`), and merge only via green Pull Request.

## 8. Authoritative Documentation Links

- [System Overview](docs/architecture/OVERVIEW.md) | [Dependencies](docs/architecture/DEPENDENCIES.md)
- [Database & SQLite](docs/architecture/DATABASE.md) | [Static Publishing](docs/architecture/PUBLISHING.md) | [HTTP Contracts](docs/architecture/HTTP.md) | [SEO & GEO](docs/architecture/SEO.md)
- [Domain Model](docs/domain/MODEL.md) | [Design System & i18n](docs/design/DESIGN_SYSTEM.md)
- [Security](docs/security/SECURITY.md) | [Performance & Benchmarks](docs/performance/PERFORMANCE.md)
- [Operations Runbook](docs/operations/RUNBOOK.md) | [Agent Workflow](docs/agents/WORKFLOW.md) | [Branch Policy](docs/agents/BRANCH_POLICY.md) | [Project State](docs/agents/PROJECT_STATE.md)
- [Modular Architecture](docs/architecture/MODULAR_ARCHITECTURE.md) | [Commercial Engine](docs/architecture/COMMERCIAL_ENGINE.md) | [Entitlement Engine](docs/architecture/ENTITLEMENT_ENGINE.md) | [Event Contracts](docs/architecture/EVENT_CONTRACTS.md) | [Multi-Tenancy](docs/architecture/MULTI_TENANCY.md) | [Commerce Core](docs/architecture/COMMERCE_CORE.md) | [White-label](docs/architecture/WHITE_LABEL.md) | [Scaling](docs/architecture/SCALING_STRATEGY.md)
- [Module Catalog](docs/business/MODULE_CATALOG.md) | [Commercial Model](docs/business/COMMERCIAL_MODEL.md) | [Subscription Rules](docs/business/SUBSCRIPTION_RULES.md) | [Pricing Policy](docs/business/PRICING_POLICY.md) | [Product Vision](docs/business/PRODUCT_VISION.md) | Module specs: `docs/modules/*.md`
- [Master Roadmap](docs/roadmap/MASTER_ROADMAP.md) | [Release Gates](docs/roadmap/RELEASE_GATES.md) | ADRs: `docs/decisions/`
