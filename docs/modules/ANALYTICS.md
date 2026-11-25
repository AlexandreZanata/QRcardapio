# QR Analytics

> **Status: Planned — not implemented, not sellable.** Nothing in this document is available to customers. Endpoints, tables and jobs below are design contracts, not running code.

| Field | Value |
|---|---|
| Module code | `analytics` (commercial name "QR Analytics") |
| Status | Planned — not implemented, not sellable |
| Phase | 9 |
| Lifecycle | `planned` (sale: `never`, grants: `never`, per [module-lifecycle.json](../../contracts/commercial/module-lifecycle.json)) |
| Monthly price | R$ 19,90 (`monthly_price_centavos: 1990`, catalog version 1, effective 2026-11-24) |
| Bundles | `business` (QR Business) and `complete` (QR Complete) |
| Affiliate eligible | Yes |
| Standalone | Yes (`requires_modules: []`) |
| Module contract | [contracts/modules/analytics.json](../../contracts/modules/analytics.json) |

Source of truth: [modules.json](../../contracts/commercial/modules.json), [capabilities.json](../../contracts/entitlements/capabilities.json), [authorization-rules.json](../../contracts/entitlements/authorization-rules.json), [registry.json](../../contracts/events/registry.json). See also [MODULE_CATALOG.md](../business/MODULE_CATALOG.md) and [EVENT_CONTRACTS.md](../architecture/EVENT_CONTRACTS.md). If this document and a contract disagree, the contract wins and this document must be corrected.

## 1. Commercial objective

Sell **advanced, cross-channel analysis** as an independent add-on module: period comparisons, channel and payment mix, hourly patterns, product mix and a historical baseline imported from CSV. Analytics sells analysis only. It never sells access to information an establishment needs to operate another module; every module keeps its own basic operational reports regardless of Analytics.

## 2. Target audience

| Audience | Need |
|---|---|
| Owners (`owner`) | Decide prices, menu mix, opening hours and staffing from evidence across channels. |
| Managers (`manager`) | Compare shifts, weeks and channels; spot cancellation and refund trends. |
| Establishments migrating from another system | Load historical sales (CSV) so comparisons start on day one. |

Not targeted: consumers, couriers, cashiers, kitchen operators, integration credentials (Connect may expose reports later; see section 5).

## 3. Problems solved

| Problem | How Analytics addresses it |
|---|---|
| Each module only shows its own operational list. | Aggregates delivery, table, counter, manual and external orders in one view. |
| No baseline when starting on QR Cardápio. | `analytics.import` loads historical sales from CSV, kept separate from live data. |
| Owners cannot tell which hours or products drive revenue. | Hourly and product aggregates with period-over-period comparison. |
| Cancellations and refunds are noticed too late. | Cancellation and refund rates per channel and period. |
| Menu reach is unknown without tracking customers. | Public menu access counts without cookies or personal data. |

## 4. MVP scope

1. Idempotent consumption of the ten contracted events (section 15) into incremental aggregates: `DailyAggregate`, `HourlyAggregate`, `ProductAggregate`, `ChannelAggregate`.
2. Advanced report page `GET /dashboard/analytics` (operation `analytics.report.advanced`): period-over-period comparison, channel mix, payment-method mix, average ticket, hourly distribution, product ranking, cancellation and refund rates, loyalty points granted, menu publication timeline and public menu access counts.
3. Public menu access counting without cookies or PII (release gate).
4. CSV historical sales import `POST /dashboard/analytics/imports` (operation `analytics.import.run`, capability `analytics.import`) with validation, preview, atomic apply and revert.
5. CSV export of advanced report results while `analytics.reports.advanced` is active. The free owner-only `data.export` operation belongs to QR Core, not to Analytics.
6. Server-rendered SVG charts (Askama), no client chart library, strict CSP, full `pt-BR`/`en`/`es` parity.

## 5. Future scope

- Exposure of aggregates through QR Connect (`/api/v1/...`), only after Connect is implemented and the read scope is contracted.
- Scheduled report emails (depends on the planned `notifications` component).
- Kitchen preparation-time and delivery-time analysis (requires consuming `KitchenTicketReady`, `DeliveryCompleted` and friends; not in the contract today).
- Forecasting and anomaly alerts.
- Anonymized cross-establishment benchmarks (requires legal review and explicit opt-in).
- XLSX export (only CSV in MVP to avoid new dependencies).

## 6. Domain entities

| Entity | Purpose | Key fields (conceptual) |
|---|---|---|
| `DailyAggregate` | Per-day totals per establishment and source. | `establishment_id`, `local_date`, `source` (`live`/`import`), order counts, `gross_centavos`, `refunded_centavos`, `menu_views` |
| `HourlyAggregate` | Per-hour distribution for heatmaps. | `establishment_id`, `local_date`, `hour` (0–23), order count, `gross_centavos`, `menu_views` |
| `ProductAggregate` | Product mix per day. | `establishment_id`, `local_date`, `product_id` (nullable for imports), `product_label`, `quantity`, `gross_centavos` |
| `ChannelAggregate` | Channel and payment-method mix per day. | `establishment_id`, `local_date`, `channel`, `payment_method`, counts, amounts |
| `ImportJob` | One CSV historical import. | `establishment_id`, `file_sha256`, row counts, status, `created_by` |
| `ExportJob` | One generated export file. | `establishment_id`, kind, period, `expires_at`, `created_by` |

Supporting technical records (not domain entities): the per-consumer processed-event ledger and the `order_id → channel` index described in section 18.

## 7. Responsibilities

Analytics **owns**: aggregates, import jobs, export jobs, metric definitions and the advanced report UI.

Analytics **does not own and never writes**: orders, order items, payments, catalog, cash sessions, loyalty ledgers, menu publications, entitlements. It reads other modules only through their events and through application-layer read functions, never through direct SQL on their tables.

Analytics **never**: emits domain events, creates Orders from imports, produces fiscal figures, decides authorization, or hides a module's basic operational report.

## 8. Invariants

1. Money is `i64` integer centavos with checked arithmetic; ratios (rates, shares) are integer basis points computed at read time; no floats anywhere in aggregation or rendering.
2. Every aggregate, job and ledger row carries `establishment_id`; every query filters by it; `product_id` is cross-verified to belong to the same `establishment_id` before use.
3. An event is applied at most once per consumer: the processed-event row and the aggregate increments commit in the same `BEGIN IMMEDIATE` transaction (release gate "incremental aggregation idempotent by event_id").
4. Aggregates are derived data. They are rebuildable from source events, Commerce Core history and import rows, and are never the source of truth for any other module.
5. A counter sale is counted once: `OrderCreated` (channel `counter`) creates the order fact; `PosSaleRegistered` for the same `order_id` only adds the cash-session dimension.
6. Imported rows are tagged `source = import`, never mixed silently with live data, never create Orders and never emit events.
7. No personal data is stored: no names, emails, phones, addresses, IPs, user agents, `wallet_id` or customer identifiers.
8. Basic operational reports of other modules never depend on Analytics tables or capabilities.

## 9. Business rules

| Rule | Definition |
|---|---|
| Reporting day | Buckets use the establishment's local time zone (`America/Sao_Paulo` until the planned `business_hours` component stores one). |
| Orders | Counted on `OrderCreated` in the bucket of `occurred_at`. |
| Completed orders | Counted on `OrderCompleted`. |
| Cancelled orders | Counted on `OrderCancelled`; excluded from completed counts. |
| Rejected orders | Counted on `OrderRejected` (`reason_code`); excluded from completed counts; feed the rejection rate per channel. |
| Gross sales | Sum of `PaymentConfirmed.amount_centavos`, attributed to the order's channel. |
| Refunds | Sum of `PaymentRefunded.amount_centavos`, booked on the refund's own date (history is never rewritten). |
| Net sales | Gross sales minus refunds for the same period. |
| Average ticket | Net sales divided by completed orders, integer division rounded half-up, shown as centavos. |
| Loyalty | Count and sum of `LoyaltyPointsGranted.points` and `LoyaltyPointsReversed.points` (positive magnitudes; the event type gives the direction); net points = granted minus reversed. Redemptions, debits and expirations are not consumed, so the report never shows balances or liability. No wallet identifiers kept. |
| Menu reach | Hourly count of successful `GET /c/{slug}` origin responses (section 17). Labelled approximate because browser and CDN caching serve repeat views. |
| Comparisons | Only between periods of equal length; up to 366 days per query. |
| Imports | CSV, UTF-8, header row required; whitelisted columns only (date, optional hour, optional channel, optional product label, quantity, total). Prices parsed by `parse_price_centavos`. Preview before apply; duplicate file (same SHA-256) rejected; an applied import can be reverted as a whole. |
| Exports | CSV with formula-injection escaping; files expire after 7 days. |
| Upsell | Module pages may show a non-blocking link to Analytics; they never replace operational data with a paywall. |

## 10. State machines

`state_machines` is empty in the module contract. Analytics owns no machine in [state-machines.json](../../contracts/commerce/state-machines.json) and drives no transition of `order` or `order_payment`; it only observes their events. `ImportJob` and `ExportJob` use an internal processing status that emits no events and is not a contracted state machine. If job status is ever exposed through Connect, it must first be added to the module contract.

## 11. Permissions

| Operation | Roles | Capability | After expiry | Source |
|---|---|---|---|---|
| `analytics.report.advanced` | `owner`, `manager` | `analytics.reports.advanced` | `deny` | authorization-rules.json |
| `analytics.import.run` | `owner`, `manager` | `analytics.import` | `deny` | authorization-rules.json |

`data.export` (`owner`, no capability, `allow`) is a QR Core operation for LGPD portability, never tied to a module lifecycle; it is not an Analytics operation and works whether or not Analytics is purchased.

All checks run through `application::entitlements::authorize(...)` in the contracted order: module lifecycle, establishment operational state, member permission, deployment enforcement, commercial entitlement. While the module is `planned`, every Analytics operation is denied by the first check.

## 12. Commercial capabilities

| Capability | Summary (capabilities.json) |
|---|---|
| `analytics.reports.advanced` | Advanced cross-channel reports and period comparisons. |
| `analytics.import` | Import historical sales from CSV. |

Both are granted by `analytics` version 1 (R$ 19,90, `1990` centavos) and by the `business` and `complete` bundles. Billing cycles and their discounts come only from [billing-cycles.json](../../contracts/commercial/billing-cycles.json); the Commercial Engine (`src/domain/commercial/`, `src/application/commercial.rs`) computes totals, never this module. Contracts snapshot the price and capabilities at signature (ADR-007). Explicit `entitlement_grants` (`trial`, `beta`, `internal_preview`, `courtesy`) can also grant them once the lifecycle allows grants.

Contracted after-expiry behavior: "Advanced reports and imports stop. Basic operational reports inside each module and the QR Core data.export operation remain."

## 13. Technical dependencies

| Dependency | Kind | Status today | Why |
|---|---|---|---|
| `identity`, `establishments`, `members`, `tenancy`, `i18n` | QR Core | Implemented | Sessions, `TenantContext`, roles, translations. |
| `commercial`, `entitlements` | QR Core | Implemented | Catalog, contracts, `authorize(...)`. |
| `events` | QR Core | Partial | Outbox append exists; the dispatcher ships with Commerce Core (phase 3). Analytics cannot launch before it. |
| `menu`, `delivery`, `tables`, `kitchen`, `fleet`, `loyalty`, `pos`, `connect` | Optional modules | Only `menu` implemented | Event sources and future API exposure. |
| Commerce Core `orders` / `order_payments` | QR Core (planned) | Planned | Producer of order and payment events and of order lines for `ProductAggregate`. |
| Caddy access log | Infrastructure | Not configured | Menu access counting (section 17). |

No new runtime services: no Redis, no Kafka, no Node, no separate analytics database. Aggregates live in the same SQLite file.

## 14. Published events

None. `events.publishes` is empty in [analytics.json](../../contracts/modules/analytics.json). Analytics is a pure consumer and must not append to `domain_events`.

## 15. Consumed events

| Event | Producer | Used for |
|---|---|---|
| `CatalogPublished` | `core.publishing` (implemented; channel `menu`, later `storefront`) | Publication timeline (`publication_version`, `channel`). |
| `OrderCreated` | `commerce` | Order counts; `order_id → channel` index; `total_centavos`, `item_count`. |
| `OrderRejected` | `commerce` | Rejection counts and rates (`reason_code`). |
| `OrderCompleted` | `commerce` | Completed counts; trigger to read order lines for `ProductAggregate`. |
| `OrderCancelled` | `commerce` | Cancellation counts and rates (`reason_code`). |
| `PaymentConfirmed` | `commerce.payments` | Gross sales and payment-method mix. |
| `PaymentRefunded` | `commerce.payments` | Refund totals and rates. |
| `PosSaleRegistered` | `pos` | Cash-session dimension of counter sales (never a second sale count). |
| `LoyaltyPointsGranted` | `loyalty` | Points granted per period (counts and sums only). |
| `LoyaltyPointsReversed` | `loyalty` | Points reversed per period; net points. |

Consumer rules: the consumer runs as the planned `system` role for one establishment; idempotent by `event_id`; no global ordering assumed. A payment event whose `order_id` is not yet indexed returns a retryable error so the dispatcher retries it later, instead of attributing it to a guessed channel. Unknown `event_version` values are rejected to the dead-letter state, never partially applied.

## 16. HTTP contracts

All routes are **planned and not available**. No OpenAPI document is published for them.

| Method | Path | Audience | Status | Behavior |
|---|---|---|---|---|
| `GET` | `/dashboard/analytics` | `member` | Planned | Askama SSR page; `TenantContext`; `authorize(analytics.report.advanced)`; `Cache-Control: no-store`; query `from`, `to`, `compare`. |
| `POST` | `/dashboard/analytics/imports` | `member` | Planned | Multipart CSV upload; `_csrf` verified; `authorize(analytics.import.run)`; returns to a preview page. |

**Open decision**: apply, revert and export endpoints are not in the module contract. They must be added to `contracts/modules/analytics.json` (with Axum 0.8 `{id}` paths, e.g. `/dashboard/analytics/imports/{id}/apply`, under `analytics.import.run`) before implementation. Denials map to distinct i18n error keys ("not permitted", "suspended", "not purchased").

## 17. Integration contracts

- **Event consumer.** Registered with the outbox dispatcher as consumer `analytics` with its own checkpoint; processing follows section 15.
- **Commerce Core read port.** On `OrderCompleted`, Analytics calls an application-layer function of Commerce Core that returns order lines (`product_id`, label snapshot, quantity, line total in centavos) filtered by `order_id` and `establishment_id`. No direct SQL on Commerce Core tables.
- **Menu access log.** Caddy serves `/c/{slug}` from disk with zero SQLite queries, so the app never sees those requests. A JSON access log restricted to timestamp, path, status and a bot flag is aggregated per slug and hour by a periodic job; IP and user agent are dropped before storage. The public request path stays at zero database queries. This mechanism requires an ADR before implementation.
- **QR Connect.** Future read scope over aggregates only, never raw events. Not part of contract version 1.
- **Other modules.** Analytics never calls them synchronously and never blocks their writes.

## 18. Conceptual persistence model

| Table (proposed) | Key | Notes |
|---|---|---|
| `analytics_daily_aggregates` | `(establishment_id, local_date, source)` | Integer counters and centavos sums. |
| `analytics_hourly_aggregates` | `(establishment_id, local_date, hour)` | `hour` 0–23; live data only. |
| `analytics_product_aggregates` | `(establishment_id, local_date, product_key, source)` | `product_key` is `product_id` or a normalized import label. |
| `analytics_channel_aggregates` | `(establishment_id, local_date, channel, payment_method)` | Channel from the order index. |
| `analytics_order_index` | `(establishment_id, order_id)` | Channel and creation bucket only; pruned after 400 days. |
| `analytics_processed_events` | `(consumer, event_id)` | Written in the same transaction as the increments. |
| `analytics_import_jobs` / `analytics_import_rows` | `id`; `(establishment_id, import_id, row_no)` | Rows kept to allow revert; no free-text columns beyond whitelist. |
| `analytics_export_jobs` | `id` | File stored under a tenant-scoped private directory via `resolve_safe_subpath`. |

Queries live in `src/infrastructure/database/queries/analytics_*.rs` (one file per entity). Schema changes follow ADR-010 expand-contract rules.

## 19. Security requirements

- `TenantContext` on every route; `establishment_id` never taken from the browser.
- `authorize(...)` before every read of advanced reports and before every import; denial keys distinguish permission, suspension and missing capability.
- `_csrf` on every `POST`; per-account rate limit on imports and exports.
- CSV upload: size cap, UTF-8 validation, row cap, header whitelist, no evaluation of formulas, parsing on `spawn_blocking` with a bounded semaphore.
- CSV export: cells starting with `=`, `+`, `-`, `@`, tab or carriage return are escaped.
- Export files are private, tenant-scoped, served only to the authorized owner, and deleted at expiry.
- Logs never include file contents, report figures per customer or session tokens.

## 20. Privacy requirements

- Aggregates contain no personal data (LGPD data minimization); event payloads already exclude PII (ADR-009).
- `LoyaltyPointsGranted.wallet_id` is never persisted by Analytics.
- Menu access counting uses no cookies, no fingerprinting, no IP or user-agent retention.
- Import columns outside the whitelist (for example customer names or CPF) are rejected, not stored.
- Deleting an establishment deletes its aggregates, jobs and export files.
- The QR Core `data.export` operation exports the establishment's own data for portability and does not depend on Analytics being purchased.

## 21. Performance requirements

- Report page reads only pre-aggregated rows: p95 under 150 ms for 366 days on the 1 vCPU / 1 GB reference VPS; never scans orders.
- Event application is O(1) per event: one processed-event insert plus a bounded number of upserts.
- Import of 50,000 rows completes in one bounded background job without blocking request handlers; memory stays bounded by streaming parsing.
- Zero added queries on `/c/{slug}`; access-log aggregation runs off the request path.
- Indexes start with `establishment_id`; storage cost target under 2 KiB per establishment per day.

## 22. Behavior without other modules

Analytics works standalone. With no other commercial module, the establishment can import historical sales and use every advanced report on the imported baseline. Data sources that are not active (no orders, no menu publications) show explicit empty states naming the source, not errors. With only the free QR Core and no published menu, there are no `CatalogPublished` events and no menu access counts; reports still render.

## 23. Behavior integrated with other modules

| Module | Effect |
|---|---|
| QR Menu | Publication timeline and menu access counts. |
| QR Delivery | Delivery and pickup orders, payments, refunds, rejections, cancellations. |
| QR Tables | Table orders and their payments through Commerce Core events. |
| QR POS | Counter orders plus cash-session dimension via `PosSaleRegistered`. |
| QR Kitchen / QR Fleet | No consumed events in contract version 1; only their effect on order events is visible. |
| QR Loyalty | Points granted and reversed per period. |
| QR Connect | Future read exposure of aggregates. |

## 24. Behavior after suspension or cancellation

- **Expiry or cancellation of Analytics**: advanced reports and imports stop; basic operational reports inside each module and the QR Core `data.export` operation remain (contracted `after_expiry`).
- **Open decision**: whether aggregation keeps running after expiry. Continuing is cheap, contains no PII and keeps history continuous for a renewal, but the `system` role rule says consumers re-check the consumer module's own capability, which would stop it. Continuing needs an ADR and an explicit contract note; otherwise a renewal rebuilds aggregates from history.
- **Payment grace**: `payment_grace_days` is `0`; access ends on the first uncovered day.
- **Administrative or security suspension**: the contract `suspension` rule blocks only `after_expiry: deny` operations (`OperationalStatus::blocks`), so `analytics.report.advanced` and `analytics.import.run` are refused while `data.export` (`allow`) stays available.
- **Module retirement**: `retired` grants nothing; exports of aggregates must be offered before retirement.

## 25. Acceptance criteria

1. Every event in section 15 is applied exactly once even when redelivered; replaying the outbox yields identical aggregates.
2. A counter sale is counted once in order totals.
3. Gross, refunds and net reconcile to the centavo against Commerce Core payments for the same period.
4. `/c/{slug}` keeps zero SQLite queries after menu access counting is enabled.
5. Users without `analytics.reports.advanced` keep every module's basic operational report.
6. Imports never create Orders or events, and revert removes their contribution exactly.
7. All UI strings exist in `pt-BR`, `en` and `es`; no inline scripts or styles.
8. `cargo fmt --check`, `cargo clippy ... -D warnings` and `cargo test --locked` pass.

## 26. Test scenarios

| ID | Scenario | Expected |
|---|---|---|
| AN-01 | Same `PaymentConfirmed` delivered twice | One increment. |
| AN-02 | `PaymentConfirmed` before its `OrderCreated` | Retried; attributed to the right channel after indexing. |
| AN-03 | Establishment A requests report while event belongs to B | No cross-tenant rows read or written. |
| AN-04 | `manager` opens `/dashboard/analytics` without capability | Denied with the "not purchased" key; module reports unaffected. |
| AN-05 | `cashier` opens `/dashboard/analytics` | Denied with the "not permitted" key. |
| AN-06 | Module still `planned` with a `courtesy` grant | Denied by the lifecycle check. |
| AN-07 | CSV with `=cmd` cell and CPF column | CPF column rejected; exported cell escaped. |
| AN-08 | Import applied then reverted | Aggregates return to pre-import values. |
| AN-09 | Counter sale with `OrderCreated` and `PosSaleRegistered` | Sale counted once with cash-session dimension. |
| AN-10 | Refund on a later day | Booked on refund date; earlier day unchanged. |
| AN-11 | Load 10,000 public menu hits | No SQLite queries on the request path; hourly counts updated by job. |
| AN-12 | Suspended establishment with an active Analytics contract | Report and import denied with the "suspended" key; owner `data.export` allowed. |
| AN-13 | `OrderRejected` for a delivery order | Counted in rejections; absent from completed counts. |
| AN-14 | `LoyaltyPointsGranted` 100 then `LoyaltyPointsReversed` 40 | Granted 100, reversed 40, net 60. |
| AN-15 | `manager` runs a CSV import without `analytics.import` | Denied with the "not purchased" key. |

## 27. Commercial launch requirements

1. Contracted release gates pass: "incremental aggregation idempotent by event_id" and "public menu access counting without cookies or PII".
2. Commerce Core and the outbox dispatcher are in production.
3. Open decisions closed in the contracts: apply, revert and export endpoints listed in `analytics.json`; the Commerce Core order-line read port for `ProductAggregate`; aggregation after expiry (section 24); the menu access log ADR (section 17).
4. Lifecycle follows `planned → development → internal_preview → beta → available`; from `beta` onward the version content hash is immutable (ADR-007).
5. i18n parity, `docs/agents/PROJECT_STATE.md` and the HTTP and database docs updated; landing copy states that Analytics never removes operational reports.
6. Load test on the reference VPS meets section 21 with 10,000 establishments of synthetic data.
