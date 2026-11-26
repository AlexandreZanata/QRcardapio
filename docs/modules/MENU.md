# QR Menu

| Field | Value |
|---|---|
| Module code | `menu` |
| Status | **Implemented**. It is the only module you can buy today. |
| Lifecycle | `available` ([module lifecycle](../../contracts/commercial/module-lifecycle.json)) |
| Phase | 1 |
| Monthly price | R$ 29,90 (`2990` centavos), catalog version 1, effective 2026-11-23 |
| Cycles | Monthly, semiannual (`500` bps, R$ 170,43), annual (`1000` bps, R$ 322,92), rounded half up per line |
| Capabilities | `menu.publish` |
| Machine contract | [contracts/modules/menu.json](../../contracts/modules/menu.json) |

This document describes behavior that exists in the code. Each statement carries one of these tags:

- **[Tested]**: implemented and covered by the automated test suite.
- **[Not homologated]**: implemented and tested locally with simulated Asaas webhooks. Homologation in the Asaas sandbox account is still pending.
- **[Planned]**: a design contract only. It is not implemented and cannot be sold.

The JSON contracts are authoritative. If this document disagrees with them, the contracts win and this document must be fixed. See also the [module catalog](../business/MODULE_CATALOG.md) and the [event contracts](../architecture/EVENT_CONTRACTS.md).

## 1. Commercial objective

- Sell the right to publish and republish a permanent, mobile-first public menu at R$ 29,90 per month, with 5% and 10% discounts on the semiannual and annual cycles.
- Keep onboarding free so owners see value before they pay. Drafts, preview, QR Codes, OCR import and unpublishing are free QR Core operations. Only publication (`menu.publish`) is charged.
- Serve as the entry offer that appears in every planned bundle: QR Start, QR Business and QR Complete (`contracts/commercial/bundles.json`, all `planned`).
- Remain affiliate eligible (`affiliate_eligible: true`) so the affiliate program earns commission on QR Menu contracts.

## 2. Target audience

| Persona | Need |
|---|---|
| Owner of a small food establishment (restaurant, café, snack bar, bakery, ice cream or açaí shop, pizzeria, burger shop, bar) | Publish and update a menu without an agency or a reprint. |
| Manager (`establishment_members.member_role = 'manager'`) | Edit the draft and publish on the owner's behalf. Can view billing but cannot change it. |
| Consumer who scans the QR Code | Read the menu quickly on any phone in pt-BR, en or es, without an account, app or tracking. |
| Owner who uses an AI assistant (ChatGPT, Claude, Google Antigravity) | Edit and publish through the MCP connector with the same rules as the dashboard. |

## 3. Problems solved

- Printed menus show outdated prices, and every change means a reprint. Here the printed QR Code points to a permanent URL, and the content changes behind it.
- PDF menus are heavy and hard to read on a phone. Here a static, responsive HTML menu with photos loads in a few milliseconds.
- Tourists cannot read the menu. Here the visitor switches between pt-BR, en and es without a reload, using translations the owner provides.
- Typing an existing menu by hand is slow. Here a local OCR or PDF import fills the draft after a mandatory review.
- Menus slow down at lunch and dinner peaks. Here public scans touch neither SQLite nor templates.
- Collecting Google reviews is hard. Here an optional review invitation links to the establishment's Google profile.

## 4. MVP scope

| Feature | Status | Notes |
|---|---|---|
| Establishment profile: name, slug, PT/EN/ES descriptions, accent color, logo, Google Maps and review links | [Tested] | `/dashboard/settings`, onboarding, MCP `update_account_settings` |
| Categories and products: CRUD, ordering, visibility, sold-out flag, photo, EN/ES translations, integer prices | [Tested] | Free QR Core `catalog` operations (`menu.draft.edit`) |
| Opt-in display of available variations and add-ons in the static menu | [Tested] | Draft setting in `/dashboard/menu/addons`. Publishing applies the setting and available options together. |
| Editor pagination (12 product or empty-category rows per page) | [Tested] | `/dashboard/menu?page=N` |
| Draft preview with a `noindex, nofollow` banner | [Tested] | `/dashboard/preview`, `/dashboard/theme.css` |
| Atomic static publication, republication and unpublishing | [Tested] | `src/application/publishing.rs` |
| Permanent QR Code (inline SVG, SVG and 1024 px PNG downloads, printable sheet) | [Tested] | Free QR Core operation (reclassified from `menu.qr.generate`) |
| Local OCR and PDF import with mandatory review before anything reaches the draft | [Tested] | `/dashboard/ocr`, ADR-006 |
| MCP connector with six tools, including `publish_menu` and `unpublish_menu` | [Tested] | `contracts/mcp/tools.json` |
| SEO for published menus: `FoodEstablishment` and `Menu` JSON-LD, sitemap inclusion while published | [Tested] | `docs/architecture/SEO.md` |
| Publication gate through `authorize(Operation::PublishMenu)` and the `menu.publish` capability | [Tested] | Commercial foundation (`src/application/entitlements.rs`, ADR-008) |
| Paid coverage through Asaas recurring checkout (monthly, semiannual, annual) | [Not homologated] | `docs/architecture/BILLING.md` |
| Backfill of "Plano Essencial" subscribers as QR Menu v1 contracts | Implemented; release gate pending | Migration `20261124000009_commercial_foundation.sql` |
| `CatalogPublished` v1 outbox event | [Tested] | Written by `core.publishing` to `domain_events`. No consumer exists yet. |
| Deploy-time `republish-all` that skips establishments with unpublished draft changes | Implemented; regression test required | `src/application/publishing.rs` (`republish_all`) |
| Administrative and security suspension through `qrcardapio operator suspend` and `reinstate` | Implemented | `src/application/operator.rs`, `OperationalStatus::blocks` |

## 5. Future scope

All items in this section are [Planned].

- Product variations, add-on groups and options. These depend on the `catalog` core component, whose status is `partial`.
- Display of business hours (`business_hours` core component) and of service modes (`commercial_settings`).
- Counting menu views without cookies or personal data and without adding database queries to `/c/{slug}`. This is an Analytics release gate.
- Consumption of `CatalogPublished` by Analytics and Connect once the outbox dispatcher ships with Commerce Core (phase 3).
- Custom domains through the `branding.custom_domain` add-on, plus a per-establishment PWA manifest. See [White-label architecture](../architecture/WHITE_LABEL.md).
- CDN purge on publish. Today staleness is bounded by `max-age=60, stale-while-revalidate=300`.
- A general admin audit log, a storage quota per establishment and a monthly OCR page quota.
- Explicitly out of scope for this module: ordering, carts, WhatsApp checkout and consumer payments. These belong to QR Delivery and QR Tables. The public menu stays read-only.

## 6. Domain entities

| Entity | Source | Key fields and rules |
|---|---|---|
| `Establishment` | `src/domain/establishment.rs`, table `establishments` | `slug` (`[a-z0-9-]`, 3–64 characters, locked once `published_version > 0`), `accent_color` (`#RRGGBB`), `is_published`, `published_version`, `has_unpublished_changes`, `last_published_at` |
| `Category` | `src/domain/category.rs` | Belongs to one establishment. Integer `position`. `is_visible`. Optional EN/ES names and descriptions. |
| `Product` | `src/domain/product.rs` | Belongs to one establishment and one category of the same establishment. `price_centavos` is an integer in `0..=99_999_999`. Also `is_available`, `is_visible` and an optional `image_asset_id`. |
| `MediaAsset` | `src/domain/publication.rs`, table `media_assets` | Content-addressed variants: logos as PNG 512/192, product photos as JPEG 800/400. EXIF is stripped. |
| `Publication` | Table `publications` | Audit row with `version`, `status` (`published`, `unpublished` or `failed`), `html_bytes`, `asset_count`, `duration_ms` and `error_message`. |
| `OcrImport` | Table `ocr_imports` | Staging record with `status` (`review`, `applied` or `discarded`). Never published directly. |

The supporting QR Core records are `establishment_members`, `mcp_grants`, `commercial_contracts`, `entitlement_grants`, `establishment_operational_states` and `domain_events`.

## 7. Responsibilities

QR Menu owns these things:

- turning the current catalog draft into an immutable static snapshot under `published/{slug}/`;
- the public delivery contract `GET /c/{slug}`, which makes zero SQLite queries;
- the draft preview, the QR Code and its print sheet for the permanent menu URL;
- recording each publication attempt and emitting `CatalogPublished`;
- the menu-related MCP tools.

QR Menu does not own these things:

- Catalog data (categories, products, prices). It belongs to the QR Core `catalog` component, and editing it is free.
- Identity, membership and tenancy (QR Core).
- Commercial contracts, prices and capability resolution (Commercial Engine and Entitlement Engine).
- SaaS billing with Asaas (`saas_billing`).

## 8. Invariants

1. A public `GET /c/{slug}` performs zero SQLite queries. It never checks billing, entitlements or operational state. [Tested]
2. `slug` cannot change once `published_version > 0`. Profile updates take the per-establishment publish lock and re-check the stored version, so a form opened before the first publication cannot rename a published path. [Tested]
3. Writes to `published/{slug}/index.html` happen only by atomic rename (temporary file, `fsync`, rename on the same filesystem). Referenced media and `theme-{hash8}.css` are written before the HTML. Stale files are deleted only after the HTML is live. [Tested]
4. A failed publication never removes or changes the live snapshot. [Tested]
5. `published_version` increases by exactly 1 on each successful publication. Unpublishing never decreases it. [Tested]
6. Prices are integer centavos end to end. Formatting uses integer arithmetic only. [Tested]
7. A snapshot contains only `is_visible = 1` categories and products. Products with `is_available = 0` appear as sold out. [Tested]
8. Publications and profile updates of the same establishment are serialized by `PublishLocks`. [Tested]
9. Every tenant query filters on `establishment_id`. `category_id` and `image_asset_id` are checked against the same establishment. [Tested]
10. Each successful publication appends exactly one `CatalogPublished` (`channel = "menu"`) in the transaction that records it. The registry dedupe rule is channel + establishment id + publication version, implemented as `CatalogPublished:menu:{establishment_id}:{version}`. A failed attempt emits nothing. [Tested]
11. QR Codes encode exactly `{PUBLIC_BASE_URL}/c/{slug}`, with no tracking or redirect hop. [Tested]
12. A system-initiated republication never makes public draft changes the owner did not publish. `republish-all` skips every establishment with `has_unpublished_changes = 1` and leaves its live snapshot untouched, so it never bypasses `menu.publish`. Implemented; see section 24 for the remaining follow-up.

## 9. Business rules

1. Every change to the profile, logo, a category or a product sets `has_unpublished_changes = 1`.
2. When billing is enforced, both the first publication and every republication require `menu.publish`. This applies to the dashboard and to MCP `publish_menu`.
3. Without Asaas configured, the deployment does not enforce billing, so publication is allowed. This keeps the behavior of earlier releases.
4. Unpublishing never requires a capability. It deletes `published/{slug}/`, sets `is_published = 0` and `has_unpublished_changes = 1`, and records `unpublished`. `/c/{slug}` then returns the localized `404`. Draft, version, slug and QR Code are kept. Republishing restores the same URL.
5. Preview, QR Codes, OCR import and draft editing are always available to members, before publication and after expiry.
6. OCR results go to a staging table. They reach the draft only after the owner reviews and applies them, and publication is never automatic.
7. MCP writes only touch the draft. Each edit carries the exact `updated_at` and an idempotent `operation_key` that is kept for 7 days. MCP publication also requires an active owner grant with the `menu.write` scope.
8. Cover photo: the first visible, available product with a photo, in menu order. Without one, the cover is illustrated in the brand color.
9. Review invitation: the direct review link takes precedence over the Maps profile link. If both are removed, the invitation disappears at the next publication.
10. Coverage requires a confirmed Asaas payment of exactly the contracted price. The period starts on the original due date and ends after 1, 6 or 12 months. The end date is exclusive and the calendar is UTC−03:00. Grace is `payment_grace_days: 0`.
11. Contract items copy the catalog version, monthly price, discount and final price. A later catalog change never changes an existing contract (ADR-007).
12. Existing "Plano Essencial" subscribers are migrated as QR Menu v1 contracts. The migration keeps price, cycle, Asaas identifiers and paid periods, and issues no new charge.
13. Canceling the renewal keeps the paid period. A full refund or a chargeback cancels coverage for the affected charge only.

## 10. State machines

`menu.json` declares `"state_machines": []`. QR Menu has no Commerce Core state machine. The publication status is derived from three columns on `establishments` and shown as dashboard badges:

| Derived state | `is_published` | `published_version` | `has_unpublished_changes` |
|---|---|---|---|
| Draft (never published) | 0 | 0 | any |
| Live, up to date | 1 | ≥ 1 | 0 |
| Live, pending changes | 1 | ≥ 1 | 1 |
| Offline (unpublished) | 0 | ≥ 1 | 1 |

| From | Action | To | Gate |
|---|---|---|---|
| Draft or Offline | Publish | Live, up to date | `menu.publish` |
| Live, up to date | Edit draft | Live, pending changes | none |
| Live, pending changes | Republish | Live, up to date | `menu.publish` |
| Live (any) | Unpublish | Offline | none |
| any | Failed publish | unchanged | records a `failed` publication |
| Live, up to date | `republish-all` (deploy CLI) | Live, up to date (version + 1) | none: the rendered content equals what the owner already published |
| Live, pending changes | `republish-all` (deploy CLI) | unchanged (skipped and logged) | never publishes the pending draft |

The commercial lifecycle of the module (`available`, which can move to `deprecated`) follows `contracts/commercial/module-lifecycle.json`. A sellable version is hash-locked in `commercial_catalog_versions`.

## 11. Permissions

The evaluation order comes from `authorization-rules.json`: `module_lifecycle`, `establishment_operational_state`, `member_permission`, `deployment_enforcement`, `commercial_entitlement`. Each denial has its own i18n key.

| Operation | Roles | Capability | After expiry | Surfaces |
|---|---|---|---|---|
| `menu.publish` | owner, manager | `menu.publish` | `deny` | dashboard, MCP (owners only, through the `menu.write` grant) |
| `menu.unpublish` | owner, manager | none | `allow` | dashboard, MCP |
| `menu.draft.edit` | owner, manager | none | `allow` | dashboard, MCP |
| `billing.view` | owner, manager | none | `allow` | dashboard |
| `billing.manage` | owner | none | `allow` | dashboard |

QR Code pages and downloads, preview and OCR require only `TenantContext` membership. `platform_admin` works through `/admin` (`AdminContext`) and never bypasses `TenantContext` on owner routes. Consumers are anonymous and only read static files. The planned roles (`cashier`, `waiter`, `kitchen_operator`, `courier`, `consumer`, `integration_credential`, `system`) hold no QR Menu operation.

Suspension follows the top-level `suspension` rule of `authorization-rules.json`: an administrative or security suspension blocks only operations whose `after_expiry` is `deny`, which here is `menu.publish`. `menu.unpublish`, `menu.draft.edit`, `billing.view` and `billing.manage` stay available (`OperationalStatus::blocks`). [Tested]

`republish-all` is a deploy-time CLI action, not an `authorize` operation. It is safe without a capability check only because it skips establishments with pending draft changes (invariant 12).

## 12. Commercial capabilities

| Capability | Gates | Sources |
|---|---|---|
| `menu.publish` | First publication and every republication, from the dashboard and from MCP | A paid commercial contract that covers today (view `commercial_entitlement_periods`), or an unrevoked `entitlement_grants` row (`trial`, `beta`, `internal_preview` or `courtesy`) |

These capabilities were reclassified in `capabilities.json`:

- `menu.update` was merged into `menu.publish`. Owners update a live menu by republishing.
- `menu.qr.generate` is now a free QR Core operation. Gating it would break QR Codes that customers have already printed.

Capabilities come from catalog offers, never from plan names. If a capability comes from more than one source, revoking one source keeps it while another source still covers it.

## 13. Technical dependencies

- QR Core (`core_dependencies`): `identity`, `establishments`, `members`, `tenancy`, `i18n`, `catalog`, `media`, `commercial`, `entitlements`, `saas_billing`, `events`. The module depends on no other commercial module.
- Runtime: Rust 1.100.0, Axum 0.8, SQLx 0.9 with bundled SQLite 3.54.1 (WAL, `synchronous=FULL`), and Askama templates compiled at build time.
- Filesystem: `published/.tmp/` and `published/{slug}/` must be on the same mount for an atomic `rename(2)`. The source media lives in the per-establishment media directory.
- Delivery: Caddy serves `/c/*` directly from `/srv/published`, returns `404` for `/c/.tmp`, and falls back to the Axum disk handler.
- Optional integrations (`optional_integrations`): `delivery`, `analytics`, `connect`. All are [Planned].

## 14. Published events

| Event | Version | Producer | Aggregate | Dedupe key | Status |
|---|---|---|---|---|---|
| `CatalogPublished` | 1 | `core.publishing` | `establishment` | channel + establishment id + publication version (`CatalogPublished:menu:{establishment_id}:{version}`) | [Tested] (append only) |

`CatalogPublished` is listed under `menu.json` `publishes` because the registry counts events written on the module's behalf; the row itself is written by the shared `core.publishing` component, which QR Delivery will reuse with `channel = "storefront"`.

The envelope follows `contracts/events/registry.json`: `event_id`, `event_type`, `event_version`, `establishment_id`, `aggregate_type`, `aggregate_id`, `correlation_id`, `causation_id`, `occurred_at`, `payload`. The payload follows `schemas/CatalogPublished.v1.json` and has no other properties:

| Field | Type | QR Menu value |
|---|---|---|
| `channel` | `"menu"` or `"storefront"` | `"menu"` |
| `publication_version` | integer ≥ 1 | the new `published_version` |
| `html_bytes` | integer ≥ 0 | size of `index.html` |
| `asset_count` | integer ≥ 0 | number of staged media files |

Unpublishing and failed attempts emit no event. `republish-all` goes through the same publication path, so it emits one event per establishment it re-renders and none for the establishments it skips because of pending draft changes. Consumers must not read those events as owner actions. The dispatcher is [Planned] for phase 3. Until it exists, `domain_events` is an append-only audit trail.

## 15. Consumed events

None (`"consumes": []`). QR Menu reacts to no event. Changes to subscriptions, grants and suspensions are evaluated synchronously by `authorize` when an owner publishes.

## 16. HTTP contracts

These are the routes declared in `menu.json`. All of them are implemented.

| Method | Path | Audience | Behavior |
|---|---|---|---|
| `GET` | `/c/{slug}` | public | Static `index.html` from disk with an `ETag`, `304` and `Cache-Control: public, max-age=60, stale-while-revalidate=300`. Returns the branded `404` when unpublished. Zero database queries. |
| `GET` | `/dashboard/menu` | member | Editor with `?page=N`, 12 rows per page, tenant-scoped. |
| `POST` | `/dashboard/menu/addons/public-options` | owner, manager | `_csrf` plus `menu.draft.edit`. Saves the display preference in the draft and redirects to the add-on groups page. |
| `POST` | `/dashboard/publish` | member | `_csrf`, then `authorize(menu.publish)`. On success, redirects to `/dashboard` with `flash.published`. An entitlement denial redirects to `/dashboard/billing`. A rendering failure redirects with `error.publish_failed` and leaves the previous snapshot intact. |
| `POST` | `/dashboard/unpublish` | member | `_csrf`. Removes the snapshot and redirects with `flash.unpublished`. |
| `GET` | `/dashboard/qr` | member | QR preview for `/c/{slug}`. Sub-routes: `/dashboard/qr/print`, `/dashboard/qr.svg`, `/dashboard/qr.png`. |
| `POST` | `/mcp` | mcp_client | JSON-RPC over Streamable HTTP. Bearer token, 128 KiB body limit, `menu.read` and `menu.write` scopes. |

These supporting routes are also implemented and documented in [HTTP Routes](../architecture/HTTP.md): `/c/{slug}/{*file}` (immutable CSS and media), `/dashboard/preview`, `/dashboard/theme.css` (`no-store`), `/dashboard/media/{filename}` (`private`, tenant-scoped), the category and product routes, `/dashboard/ocr/*` and `/dashboard/settings`. Every authenticated `POST` checks `_csrf` in constant time. Dynamic pages send `Cache-Control: no-store`.

## 17. Integration contracts

- **MCP** ([tools](../../contracts/mcp/tools.json), [clients](../../contracts/mcp/clients.json)): `get_account_settings` and `get_menu_draft` use `menu.read`. `update_account_settings`, `apply_menu_changes`, `publish_menu` and `unpublish_menu` use `menu.write`. Prices travel as integer `price_centavos`. `get_account_settings` reports `billing_enabled` and `can_publish`. When `publish_menu` is denied, the response points the owner to `/dashboard/billing`.
- **Snapshot layout** (consumed by Caddy and by the planned CDN): `published/{slug}/index.html`, `theme-{hash8}.css` and `media/{hash16}_{size}.{jpg|png}`. The layout and the cache headers are a stable contract.
- **CLI**: `qrcardapio republish-all` re-renders every published establishment whose draft has no unpublished changes and skips (and logs) the others. The deploy agent runs it after each cutover, and `restore.sh` runs it after a restore. `qrcardapio operator grant|revoke|suspend|reinstate` (audited, requires an existing admin account) manages explicit `entitlement_grants` and the operational state, and appends `EntitlementGranted`, `EntitlementRevoked`, `EstablishmentSuspended` or `EstablishmentReinstated`.
- **Outbox**: `CatalogPublished` is the only integration point for planned consumers (Analytics, Connect).
- **SaaS billing**: Asaas webhooks update coverage. QR Menu never calls Asaas directly.

## 18. Conceptual persistence model

| Store | Content | Tenant key |
|---|---|---|
| `establishments` | Profile, brand, publication columns | `id` |
| `establishment_members` | `(establishment_id, account_id, member_role)` | `establishment_id` |
| `categories`, `products`, `media_assets` | Catalog draft and media metadata | `establishment_id` (plus `category_id` checked against the same establishment) |
| `publications` | Publication attempt log | `establishment_id` |
| `ocr_imports` | OCR staging | `establishment_id` |
| `mcp_grants`, `mcp_menu_operations` | MCP grants and idempotency | `establishment_id` |
| `commercial_contracts`, `commercial_contract_items`, `commercial_contract_capabilities` | Contract snapshots (price, cycle, capabilities) | `establishment_id` |
| `commercial_catalog_versions`, `commercial_catalog_offers` | Hashes of sellable versions and the lifecycle registry | platform |
| `entitlement_grants`, `establishment_operational_states`, view `commercial_entitlement_periods` | Capability sources and operational state | `establishment_id` |
| `billing_checkouts`, `billing_payments`, `billing_coverage` | Legacy gateway source of truth, kept under expand-contract (ADR-010) | `establishment_id` |
| `domain_events` | Outbox | `establishment_id` |
| `DATA_DIR/media/{establishment_id}/` | Private source media | path |
| `DATA_DIR/published/{slug}/` | Public static snapshot | path |

## 19. Security requirements

- `TenantContext` resolves the establishment only from `establishment_members`. Browser-supplied tenant IDs are never trusted.
- CSRF is checked in constant time on every authenticated `POST`. MCP uses OAuth 2.1 with PKCE S256, single-use codes and resource-bound tokens.
- Strict CSP (`script-src 'self'; style-src 'self'`). There is no inline script except the inert `#qr-menu-i18n` and JSON-LD blocks, and no `style` attribute. Brand colors come only from the generated `theme-*.css`.
- Askama escapes all owner text automatically. JSON-LD is serialized through `safe_json_ld`.
- `Slug::validate_existing` and `resolve_safe_subpath` block path traversal. Caddy returns `404` for `/c/.tmp`.
- Uploads: magic-byte validation, dimension bounds, re-encoding with EXIF stripped, logos capped at 2 MiB, product photos capped at 5 MiB (6 MiB multipart body), OCR requests capped at 16 MiB (`components.json` `free_tier_limits`). Rate limits: 20 uploads per minute and 10 OCR requests per minute per account. OCR subprocesses run with `Semaphore(1)`, timeouts and no shell.
- Billing actions require the owner role, `_csrf` and at most six actions per minute per establishment. Webhooks use a separate token compared in constant time.
- Passwords, session tokens, OAuth codes and gateway payloads are never logged.

## 20. Privacy requirements

- Public menus create no account, collect no form data and make no database writes. The only client-side state is the visitor's language choice (`qr_lang` cookie and `localStorage` key), which is a platform-wide preference with no establishment data.
- The page fetches no Google data. The review link opens only when the visitor chooses it.
- Owner data is limited to email, password hash, locale and public business data. Card and payer data stay at Asaas.
- EXIF metadata, including GPS, is stripped from every image.
- Event payloads carry only identifiers and integer amounts, never emails, names, phone numbers or tokens.

## 21. Performance requirements

- The public path must stay free of database queries, entitlement checks and template rendering. A measured baseline on `/c/{slug}` in Axum: 28,794 req/s, p95 2.15 ms at 32 connections. During five concurrent republications: 26,880 req/s, p95 2.35 ms. Caddy limited to 1 vCPU served 6,469 req/s, p95 10.65 ms (`docs/performance/PERFORMANCE.md`).
- Publication holds a database connection only for the single snapshot read and for the final recording transaction, which also appends the outbox row. All file I/O happens outside SQLite transactions.
- `authorize` runs only on authenticated member and MCP paths, such as the publish action and the `can_publish` indicator. It must not add more than one indexed read per data source.
- Caching: HTML `max-age=60, stale-while-revalidate=300`. Theme and media `max-age=31536000, immutable`.
- Total `republish-all` time must fit the deploy agent's smoke-check window. Track it in the performance baseline.

## 22. Behavior without other modules

QR Menu is `standalone: true` and needs only QR Core. With no other module active, it provides a read-only public menu, a QR Code, a preview, OCR import and MCP editing. It has no cart, no ordering, no consumer identity and no table sessions.

## 23. Behavior integrated with other modules

All integrations are [Planned].

- **QR Delivery**: the storefront lives at `/s/{slug}` with its own snapshot, and `core.publishing` appends `CatalogPublished` with `channel = "storefront"` on its behalf. `/c/{slug}` never redirects to the storefront and never gains ordering. A link from the menu to the storefront is rendered only at publication time, so the storefront must handle "ordering unavailable" itself when Delivery has lapsed.
- **QR Tables**: table QR Codes use `/t/{table_token}` and never reuse the printed `/c/{slug}` code. See [QR Tables](TABLES.md).
- **QR Analytics**: consumes `CatalogPublished`. Menu views must be counted without cookies or personal data, from access logs or an equivalent source, and never by adding database work to `/c/{slug}`.
- **QR Connect**: consumes `CatalogPublished` for webhooks. MCP grants issued before Connect keep their `menu.read` and `menu.write` scopes.
- **Bundles**: QR Start, QR Business and QR Complete pin `menu` version 1. A bundle contract grants `menu.publish` exactly as the standalone module does.

## 24. Behavior after suspension or cancellation

| Situation | Effect |
|---|---|
| Renewal canceled | The paid period stays valid until its exclusive end date. |
| Contract expired, no grant | Publishing and republishing are denied (dashboard and MCP). The published snapshot stays online at the same URL and QR Code (legacy policy) until the owner unpublishes it. Drafts, preview, QR Codes, OCR and unpublishing stay available. Renewing restores publication immediately. |
| Full refund or chargeback | Coverage of the affected charge is cancelled. Coverage from other charges still applies. |
| Administrative or security suspension (`establishment_operational_states`, `qrcardapio operator suspend`) | Checked at step 2 of `authorize`, before role and payment, with its own error key (`error.establishment_suspended`). Only `after_expiry: deny` operations are blocked: `menu.publish` is refused, while unpublishing, draft editing and billing stay available. The static snapshot is not removed automatically, because public delivery never reads SQLite. Taking an abusive menu offline needs an explicit unpublish; there is no operator unpublish command yet (open decision). Emits `EstablishmentSuspended` and `EstablishmentReinstated`. |
| Module `deprecated` | Contracts and renewals keep granting `menu.publish`. No new sales. |
| Module `retired` | `menu.publish` is never granted. Snapshots that are already published stay online. |
| Asaas not configured | Publication is not commercially gated. |

**System republication (fixed).** `republish_all` re-renders from the current draft (`load_menu_snapshot`), so it previously published pending draft edits after every deploy, including for establishments without `menu.publish`. It now skips every establishment whose draft has unpublished changes, logs the skip and leaves the live snapshot untouched. Establishments without pending changes are re-rendered, which only refreshes markup and the embedded `?v={ASSET_VERSION}` of the shared CSS and JavaScript.

Open items:

- **Re-render from a persisted per-publication snapshot.** Skipped establishments keep the previous template markup and asset version until the owner republishes. The follow-up is to persist the catalog input of each publication and re-render from it, so pending-draft menus also receive template upgrades. This also removes the short window between the `has_unpublished_changes` check and the draft read, during which a concurrent catalog edit could still be rendered.
- **Assets deleted under cached HTML.** Stale `theme-*.css` and media files are deleted right after the new HTML is live, but browsers and CDNs may keep the previous HTML for up to `max-age + stale-while-revalidate` (360 s) and then request deleted files (`404`). The previous version's immutable files must be kept for at least that window. See [White-label architecture](../architecture/WHITE_LABEL.md), section 10.

## 25. Acceptance criteria

1. An unpublished establishment returns `404` at `/c/{slug}`. After publication, it returns `200` with zero SQLite queries.
2. With billing enforced and no coverage or grant, `POST /dashboard/publish` and MCP `publish_menu` are refused, and the live snapshot stays byte-identical.
3. With paid coverage or an active grant, publication succeeds, `published_version` increases by 1, a `published` row is recorded and one `CatalogPublished` with `channel = "menu"` is appended.
4. After expiry, `/c/{slug}` keeps serving the same snapshot. Unpublishing, preview, QR downloads and draft edits still work.
5. A forced write failure before the rename leaves the previous HTML and assets intact and records a `failed` row with no event.
6. The slug cannot change after the first publication, through the dashboard or MCP.
7. No member of establishment A can read or change anything in establishment B.
8. Legacy subscribers keep publication access with no new charge after the backfill. Coverage parity is verified for every row in `billing_checkouts`.
9. A suspended establishment cannot publish, but can still unpublish, edit the draft and view or manage billing.
10. `republish-all` leaves the snapshot of an establishment with pending draft changes byte-identical and appends no `CatalogPublished` for it.
11. `cargo test --locked` passes, including the contract parity tests for capabilities, operations, events and catalog.

## 26. Test scenarios

| Scenario | Expected | Coverage |
|---|---|---|
| Atomic publish, XSS escaping, zero-DB delivery, recovery after failure | Previous snapshot preserved, owner text escaped | `tests/publishing_and_qr.rs` |
| Category and product ordering, images, logo lifecycle, concurrent edits | No `SQLITE_BUSY`, correct order | `tests/menu_crud.rs` |
| Tenant isolation and slug locking | Cross-tenant access refused, slug locked | `tests/auth_and_isolation.rs` |
| Billing coverage, discounted cycles, webhook redelivery | Exact periods and prices, idempotent processing | `tests/billing.rs` |
| OCR review, apply and discard, XSS, tenant isolation | Nothing reaches the draft without review | `tests/ocr_import.rs` |
| MCP publish and unpublish with the subscription gate, slug lock, isolation | Same rules as the dashboard | `tests/mcp.rs`, `tests/mcp/account_and_publish.rs` |
| Published menu SEO (JSON-LD, sitemap, `noindex` on preview) | Contracts hold | `tests/seo_contracts.rs` |
| Catalog validity, hash lock of sellable versions, lifecycle transitions | Invalid catalog refused at start-up | `tests/commercial_contracts.rs` (ADR-007) |
| `authorize` order: lifecycle, suspension, role, enforcement, capability | Each denial has its own key | Required for the commercial foundation |
| Suspension blocks only `deny` operations | `menu.publish` refused; `menu.unpublish`, `menu.draft.edit`, `billing.view`, `billing.manage` allowed | `OperationalStatus::blocks` unit test; HTTP test required |
| Grant revoked while a paid contract still covers | `menu.publish` kept | Required |
| `CatalogPublished` dedupe on retry of the same version | One row | Required |
| `republish-all` with pending drafts and an expired contract | Establishment skipped, snapshot byte-identical, no event | Implemented; regression test required |
| `republish-all` with no pending drafts | Re-rendered with the current `ASSET_VERSION`, one event, `published_version` + 1 | Required |

## 27. Commercial launch requirements

The release gates in `menu.json`:

1. **Entitlement parity with legacy coverage**: for every establishment, the result of `authorize(menu.publish)` matches the previous `publication_allowed` on production data before cutover.
2. **Asaas sandbox homologation**: checkout, all three cycles, renewal, cancellation, refund and chargeback validated in the sandbox account. Status: pending.
3. **`republish-all` after deploy**: runs cleanly and skips establishments with pending drafts (fixed, regression test required). Remaining follow-ups, tracked in section 24: re-rendering from a persisted per-publication snapshot so pending-draft menus still receive template upgrades, and keeping the previous immutable assets while cached HTML may still reference them.

Additional conditions:

- The catalog version 1 hash is recorded in `commercial_catalog_versions`, and the landing page, dashboard, JSON-LD offers and Asaas payloads read prices from `contracts/commercial`.
- i18n parity for every new denial key across `pt-BR`, `en` and `es`.
- The rollback procedure of ADR-010 is rehearsed.
