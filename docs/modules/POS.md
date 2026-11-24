# QR POS

> **Status: Planned — not implemented, not sellable.** Nothing in this document is available to customers. Endpoints, tables and flows below are design contracts, not running code.

| Field | Value |
|---|---|
| Module code | `pos` (commercial name "QR POS") |
| Status | Planned — not implemented, not sellable |
| Phase | 10 |
| Lifecycle | `planned` (sale: `never`, grants: `never`, per [module-lifecycle.json](../../contracts/commercial/module-lifecycle.json)) |
| Monthly price | R$ 69,90 (`monthly_price_centavos: 6990`, catalog version 1, effective 2026-11-24) |
| Bundles | `complete` (QR Complete) only |
| Affiliate eligible | Yes |
| Standalone | Yes (`requires_modules: []`) |
| Module contract | [contracts/modules/pos.json](../../contracts/modules/pos.json) |

Source of truth: [modules.json](../../contracts/commercial/modules.json), [capabilities.json](../../contracts/entitlements/capabilities.json), [authorization-rules.json](../../contracts/entitlements/authorization-rules.json), [registry.json](../../contracts/events/registry.json), [state-machines.json](../../contracts/commerce/state-machines.json). If this document and a contract disagree, the contract wins. Commercial context: [Module catalog](../business/MODULE_CATALOG.md). Event semantics: [Event contracts](../architecture/EVENT_CONTRACTS.md).

**Non-fiscal by design.** QR POS issues non-fiscal receipts only. It never issues, imitates or labels anything as NFC-e, SAT/CF-e, MFE or "cupom fiscal". Fiscal issuance is a separate future domain (section 5).

## 1. Commercial objective

Sell a browser-based front-counter module for registering counter sales, taking payments, running cash sessions with operational reconciliation, printing non-fiscal receipts and tickets, and keeping basic stock. It must run on any computer or tablet with a browser and a common thermal printer, with no local installation and no fiscal certificate.

## 2. Target audience

| Audience | Need |
|---|---|
| Snack bars, cafés, bakeries, ice cream and açaí shops | Fast counter sales with cash control. |
| Owners (`owner`) and managers (`manager`) | Open/close registers, approve discounts, cancellations and refunds, review differences. |
| Cashiers (`cashier`, planned role) | Register sales, take payments, record withdrawals and supplies, count the drawer. |
| Establishments with QR Tables | Settle table bills at the counter inside a cash session. |

Not targeted in this module: establishments that require fiscal documents at the counter until the fiscal domain exists; they must keep their fiscal system.

## 3. Problems solved

| Problem | How QR POS addresses it |
|---|---|
| Paper notes and calculator at the counter. | Product lookup from the shared catalog, integer-centavo totals, change calculation. |
| Cash drawer differences discovered days later. | Cash sessions with expected vs counted reconciliation at close. |
| Withdrawals and supplies untracked. | Immutable `CashMovement` records with operator and reason. |
| Counter sales invisible to other channels. | Sales are Commerce Core Orders (channel `counter`) visible to Kitchen, Loyalty, Analytics and Connect. |
| Stock-outs noticed only by customers. | Basic stock levels and movements with low-stock alerts. |

## 4. MVP scope

1. Operators bound to establishment members (`owner`, `manager`, `cashier`).
2. Cash registers and cash sessions: open with opening float (`pos.cash_register.open`), count and close with reconciliation (`pos.cash_register.close`, `cash_session` machine).
3. Counter sales as Commerce Core Orders with channel `counter` (`pos.sale.register`; no separate order engine).
4. Payments per sale: cash (with change), Pix and card on an external terminal confirmed manually by the operator, split across methods.
5. Withdrawals (sangria) and supplies (suprimento) while a session is `open` (`pos.cash_movement.register`).
6. Line and order discounts in integer centavos with a manager-approval threshold.
7. Cancellations (`pos.sale.cancel`) and refunds through the shared `order` and `order_payment` machines.
8. Non-fiscal receipts with per-establishment sequential numbers, reprint marking and cancellation receipt.
9. Ticket printing (pickup number or production ticket) through browser print CSS for 58 mm and 80 mm thermal printers.
10. Basic stock: stock items optionally linked to catalog products, append-only stock movements, low-stock alert.
11. Operational closing report per session (not a fiscal "Redução Z").

## 5. Future scope

- **Fiscal domain (separate, not part of QR POS):** NFC-e, SAT/CF-e, MFE and any successor. It is subject to state (UF) legislation, SEFAZ homologation, A1 certificate custody, CSC tokens, contingency modes and specific integrations. It is not in `contracts/commercial/modules.json` and needs its own contract, ADR and legal review.
- Offline sales queue with idempotent replay.
- Direct ESC/POS printing and cash drawer kick (requires a local agent).
- TEF / integrated card terminals and Pix with automatic confirmation (gateway webhooks).
- Operator quick-switch by PIN on a shared terminal.
- Recipes (ficha técnica), purchase orders, suppliers and stock costing (CMV). Full inventory remains out of scope per `AGENTS.md`.

## 6. Domain entities

| Entity | Owner | Purpose |
|---|---|---|
| `Operator` | POS | POS profile bound 1:1 to an `establishment_members` row; display code, active flag. No shared logins. |
| `CashRegister` | POS | A physical counter position (name, active flag). |
| `CashSession` | POS | One shift of one register: opening float, state, expected and counted totals, difference. |
| `CashMovement` | POS | Immutable drawer movement: opening float, withdrawal, supply, cash sale, cash refund. |
| `Sale (Order with channel counter)` | Commerce Core | The shared Order aggregate; POS links it to a session, operator and receipt. |
| `NonFiscalReceipt` | POS | Immutable receipt snapshot with sequential number and non-fiscal label. |
| `StockItem` | POS | Countable item, optional link to a catalog product of the same establishment. |
| `StockMovement` | POS | Append-only quantity change (entry, sale, sale reversal, adjustment, loss). |

## 7. Responsibilities

POS **owns**: operators, registers, cash sessions, cash movements, the sale-to-session link, receipts, stock items and stock movements, counter UI and printing layouts.

POS **delegates to Commerce Core**: order creation, order lines, order totals, order transitions, payments, refunds and their events. POS must not create its own order or payment tables.

POS **reads, never writes**: the QR Core catalog (categories, products, `price_centavos`, availability).

POS **never**: issues fiscal documents, stores card data, talks to card terminals in MVP, or changes the published menu.

## 8. Invariants

1. Money is `i64` integer centavos; totals, discounts, change and differences use checked integer arithmetic; percentage discounts are basis points rounded half-up per line.
2. Every POS row has `establishment_id`; `cash_register_id`, `operator_id`, `order_id`, `product_id` are cross-verified to the same `establishment_id`.
3. At most one non-`closed` cash session per register (partial unique index).
4. Cash movements are immutable and can be created only while the session is `open`; corrections are new compensating movements.
5. Expected cash = opening float + supplies − withdrawals + cash received − change given − cash refunds, all within the session.
6. A sale belongs to exactly one session and one operator; registering a sale requires an `open` session.
7. Confirmed payments of a sale never exceed its total; `sum(refunds) <= confirmed amount` (order_payment invariant).
8. Receipt numbers are unique and monotonic per establishment; a receipt is never edited; reprints are marked as copies.
9. Every receipt and ticket carries the non-fiscal label in the active locale (`pt-BR`, `en`, `es`).
10. Stock balance equals the sum of its movements; the cached balance is updated in the same transaction as the movement.

## 9. Business rules

| Topic | Rule |
|---|---|
| Opening | `pos.cash_register.open`. Opening float ≥ 0 centavos; emits `CashRegisterOpened` at creation. |
| Sale | `pos.sale.register`. Lines from the catalog with price snapshot; total must be > 0 unless an approved 100% discount. Idempotency key per submission. |
| Discount | Cannot exceed the line or order total; above the establishment threshold requires `owner`/`manager` approval recorded on the order. |
| Payment | Cash records tendered amount and change; Pix and external-card payments are confirmed by the operator (`pending → confirmed`, actors include `cashier`, `owner`, `manager`). Online gateway confirmation is not in MVP. |
| Withdrawal | `pos.cash_movement.register`. Cannot exceed expected cash; reason required; above threshold requires `owner`/`manager`. |
| Supply | `pos.cash_movement.register`. Positive amount; reason required. |
| Cancellation | `pos.sale.cancel` (`owner`/`manager`). Uses the shared `order` machine; `accepted`/`preparing`/`ready` → `cancelled`, with the compensations contracted per source state (refund confirmed payments, cancel kitchen tickets, cancel pending deliveries). A `completed` order cannot be cancelled; it is refunded. |
| Refund | `owner`/`manager`; cash refunds require an open session and create a cash movement. |
| Counting | `open → counting` freezes sales and movements; counted amounts entered per method. |
| Closing | `pos.cash_register.close`. `counting → closed` stores expected, counted and difference; emits `CashRegisterClosed`. Differences are recorded, never auto-corrected. |
| Reopen | `counting → open` only by `owner`/`manager`. |
| Stock | Sale creates `sale` movements; cancellation creates `sale reversal`. Negative stock is allowed with a warning so the counter never stops. Stock never edits the menu automatically. |
| Receipt | Shows establishment, lines, discounts, payments, change, operator code, session, number, timestamp and "Documento sem valor fiscal" / "Non-fiscal document" / "Documento sin valor fiscal". No CPF field, no fiscal key, no SEFAZ QR Code. |
| Closing report | Per-method expected vs counted, movements, sales count and totals; labelled operational, non-fiscal. |

## 10. State machines

POS uses three machines from [state-machines.json](../../contracts/commerce/state-machines.json):

| Machine | States | POS usage |
|---|---|---|
| `cash_session` | `open`, `counting`, `closed` | Owned by POS. `open → counting` (`cashier`, `owner`, `manager`), `counting → open` (`owner`, `manager`), `counting → closed` (`cashier`, `owner`, `manager`, event `CashRegisterClosed`). `CashRegisterOpened` is emitted at creation. |
| `order` | `placed`, `accepted`, `rejected`, `preparing`, `ready`, `completed`, `cancelled` | Owned by Commerce Core. A counter sale is created `placed` and must reach `completed`. |
| `order_payment` | `pending`, `confirmed`, `failed`, `expired`, `cancelled`, `partially_refunded`, `refunded`, `chargeback_open`, `chargeback_lost` | Owned by Commerce Core. Counter payments go `pending → confirmed` by a staff actor (`cashier`, `owner`, `manager`) with the collected method. |

Counter path in the `order` machine: `placed → accepted` (owner, manager, cashier, waiter), then the contracted fast path `accepted → ready` (owner, manager, cashier, waiter; "for establishments without QR Kitchen, including counter (POS) sales"), then `ready → completed` (owner, manager, cashier, waiter, courier, system). When QR Kitchen routes the counter order, the order instead moves `accepted → preparing` on `KitchenTicketStarted` and `preparing → ready` when every ticket is ready, both as the planned `system` role. Cancellation (`accepted`/`preparing`/`ready → cancelled`) is limited to owner and manager. Transitions use the contracted optimistic concurrency (`... AND state = ? AND version = ?`, `409` on conflict).

## 11. Permissions

| Operation | Roles | Capability | After expiry | Source |
|---|---|---|---|---|
| `pos.sale.register` | `owner`, `manager`, `cashier` | `pos.sale.create` | `deny` | authorization-rules.json |
| `pos.cash_register.open` | `owner`, `manager`, `cashier` | `pos.cash_register.manage` | `deny` | authorization-rules.json |
| `pos.cash_movement.register` | `owner`, `manager`, `cashier` | `pos.cash_register.manage` | `complete_existing` | authorization-rules.json (withdrawals and supplies on an open session) |
| `pos.cash_register.close` | `owner`, `manager`, `cashier` | none | `allow` | authorization-rules.json |
| `pos.sale.cancel` | `owner`, `manager` | none | `allow` | authorization-rules.json |

Open decisions (no operation in the contract yet): refunds of a `completed` counter sale (a dedicated operation or an extension of `pos.sale.cancel`, following the `delivery.order.refund` precedent of no capability and `allow`); discount approval above the threshold; stock adjustments; reopening a session (`counting → open`, owner and manager). Each needs an `authorization-rules.json` row and parity tests before implementation.

The `cashier` role is `planned` in authorization-rules.json and must be implemented in `domain::entitlements::MemberRole` before launch, together with the `system` role used for automatic order transitions driven by QR Kitchen events. All checks go through `application::entitlements::authorize(...)` in the contracted order.

## 12. Commercial capabilities

| Capability | Summary (capabilities.json) |
|---|---|
| `pos.sale.create` | Register counter sales with non-fiscal receipts. Required by `pos.sale.register`. |
| `pos.cash_register.manage` | Open, reconcile and close cash register sessions. Required by `pos.cash_register.open` and `pos.cash_movement.register`; closing (`pos.cash_register.close`) needs no capability. |

Granted by `pos` version 1 (R$ 69,90, `6990` centavos) and the `complete` bundle. Cycle discounts come only from [billing-cycles.json](../../contracts/commercial/billing-cycles.json) through the Commercial Engine. Contracted after-expiry behavior: "No new sales. Open cash sessions can be counted and closed; refunds and history stay available."

## 13. Technical dependencies

| Dependency | Status | Why |
|---|---|---|
| `identity`, `establishments`, `members`, `tenancy`, `i18n` | Implemented | Login, `TenantContext`, roles, translations. |
| `catalog` | Partial | Products and prices; variations and add-on groups come with Commerce Core. |
| `commercial`, `entitlements` | Implemented | Catalog, contracts, `authorize(...)`. |
| `events` | Partial | Outbox append; dispatcher ships with Commerce Core (phase 3). |
| `orders`, `order_payments` | Planned (Commerce Core, phase 3) | Hard prerequisite: POS cannot ship before Commerce Core. |
| Optional: `tables`, `kitchen`, `loyalty`, `analytics`, `connect` | Planned | Integrations in section 23. |

Printing uses `@media print` stylesheets under `static/css/`; no native drivers, no Node, no local agent in MVP.

## 14. Published events

| Event | Emitted by | When |
|---|---|---|
| `PosSaleRegistered` | POS (aggregate `order`) | Sale registered in a session; payload `order_id`, `cash_session_id`, `total_centavos`. |
| `CashRegisterOpened` | POS (aggregate `cash_session`) | Session created; `opening_float_centavos`. |
| `CashRegisterClosed` | POS (aggregate `cash_session`) | `counting → closed`; `expected_centavos`, `counted_centavos`. |
| `OrderCreated` | Commerce Core on POS request | Counter order created; `channel` = `counter`. |
| `OrderCompleted` | Commerce Core on POS request | Counter order `ready → completed`; `completed_at`. |
| `OrderCancelled` | Commerce Core on POS request | `pos.sale.cancel`; `reason_code`, `cancelled_by`. |
| `PaymentConfirmed` | Commerce Core on POS request | Counter payment confirmed; `method`, `amount_centavos`. |
| `PaymentRefunded` | Commerce Core on POS request | Refund registered at the counter. |

All are appended in the same transaction as the state change. `OrderCreated`, `OrderCompleted`, `OrderCancelled`, `PaymentConfirmed` and `PaymentRefunded` have producers `commerce` / `commerce.payments` in the registry; per the registry rule, module `publishes` lists include Commerce Core events written on the module's behalf. POS triggers them through Commerce Core use cases and never writes them directly. Cash movements, receipts and stock movements emit no events in contract version 1.

## 15. Consumed events

| Event | Producer | Handling |
|---|---|---|
| `TableBillClosed` | `tables` | Attributes counter-confirmed payments of the closed bill (`session_id`, `total_centavos`) to the open cash session of the operator who settled it; if no session is open, the bill is listed as unattributed in the next closing report. Idempotent by `event_id`. |

## 16. HTTP contracts

All routes are **planned and not available**. No OpenAPI document is published.

| Method | Path | Audience | Status | Behavior |
|---|---|---|---|---|
| `GET` | `/dashboard/pos` | `member` | Planned | Counter screen (Askama SSR + small vanilla JS module); `TenantContext`; `no-store`. |
| `POST` | `/dashboard/pos/sessions` | `member` | Planned | Opens a cash session (`pos.cash_register.open`); `_csrf`. |

Open decision: sale, payment, cancellation, movement, count, close, receipt and stock routes are not in the module contract yet and must be added to `contracts/modules/pos.json` (Axum 0.8 `{id}` syntax, for example `/dashboard/pos/sessions/{id}/close`) before implementation. Their operations already exist (`pos.sale.register`, `pos.sale.cancel`, `pos.cash_movement.register`, `pos.cash_register.close`). These are internal app routes, not QR Connect APIs.

## 17. Integration contracts

- **Commerce Core port.** POS calls application functions to create a counter order (lines with catalog price snapshot, discounts), add payments, confirm, cancel and refund. Every call passes `establishment_id` from `TenantContext` and an idempotency key.
- **Catalog port.** Read-only lookup of visible and available products by name or code, scoped to the establishment.
- **Kitchen.** Kitchen reacts to `OrderAccepted` of counter orders (tickets created by the `system` role); `KitchenTicketStarted` and `KitchenTicketReady` move the order through `preparing` and `ready`. POS does not call Kitchen.
- **Tables.** Through `TableBillClosed` only.
- **Printing.** Receipt and ticket templates are Askama pages with print CSS; the browser print dialog is the only printer interface.
- **Fiscal domain (future).** Will consume completed counter orders through events; POS data keeps line-level price, quantity and discount so a future fiscal module can operate without changing POS records.

## 18. Conceptual persistence model

| Table (proposed) | Key / constraints | Notes |
|---|---|---|
| `pos_operators` | `id`; unique `(establishment_id, account_id)` | FK to `establishment_members`. |
| `pos_cash_registers` | `id`; unique `(establishment_id, name)` | |
| `pos_cash_sessions` | `id`; partial unique `(establishment_id, cash_register_id) WHERE state <> 'closed'` | `state`, `version`, float, expected, counted, difference. |
| `pos_cash_movements` | `id`; `(establishment_id, cash_session_id)` | Kind, `amount_centavos`, reason, operator; no UPDATE/DELETE. |
| `pos_sales` | `order_id` PK; unique `(establishment_id, idempotency_key)` | Links Commerce Core order to session, operator, receipt. |
| `pos_receipts` | `id`; unique `(establishment_id, receipt_number)` | Immutable rendered snapshot, copy counter. |
| `pos_receipt_sequences` | `establishment_id` PK | Incremented inside the sale transaction. |
| `pos_stock_items` / `pos_stock_movements` | `id`; `(establishment_id, stock_item_id)` | Integer quantities in base units; movements append-only. |
| `pos_processed_events` | `(consumer, event_id)` | Idempotent consumption. |

Queries go in `src/infrastructure/database/queries/pos_*.rs`, one file per entity. Writes use `BEGIN IMMEDIATE`. Migrations follow ADR-010.

## 19. Security requirements

- `TenantContext` on every route; no browser-supplied tenant ids; all FKs cross-verified.
- `_csrf` on every `POST`; `authorize(...)` on every operation; manager approvals re-authenticated server-side, never trusted from the client.
- Per-establishment and per-account rate limits on sale registration and movements.
- No card data, no PAN, no CVV ever entered or stored; external terminals are confirmed by method only.
- Receipts and reports are tenant-private; printing pages carry `no-store`.
- Audit: every movement, discount approval, cancellation and refund stores actor and timestamp; outbox rows double as audit trail.
- Strict CSP: counter JS is a static module under `static/js/`; no inline scripts or styles.

## 20. Privacy requirements

- POS stores no customer personal data in MVP: no names, CPF, phones or emails on sales or receipts.
- Operator data is the member link only; event payloads carry identifiers and amounts only.
- Deleting an establishment deletes POS data; `data.export` includes sessions, movements and receipts.

## 21. Performance requirements

- Sale registration (order, lines, payments, sale link, receipt, stock movements, outbox events) in one transaction, p95 under 100 ms on the 1 vCPU reference VPS.
- Product lookup p95 under 50 ms for catalogs of 1,000 products.
- Closing report computed from session rows only, under 200 ms for 2,000 sales per session.
- No polling loops on the counter screen; no impact on `/c/{slug}` static delivery.

## 22. Behavior without other modules

QR POS works standalone on top of QR Core and Commerce Core: it uses the free draft catalog (no published menu required), creates counter orders, takes payments, runs cash sessions, prints receipts and tracks stock. Without QR Menu, Delivery, Tables, Kitchen, Loyalty, Analytics or Connect, no feature of the counter is missing; its own operational closing report and sales history are always available and never depend on Analytics.

## 23. Behavior integrated with other modules

| Module | Effect |
|---|---|
| QR Tables | Table bills settled at the counter enter the session via `TableBillClosed`. |
| QR Kitchen | Counter orders produce kitchen tickets from `OrderAccepted`; printed tickets become optional; the order follows the Kitchen-driven `preparing → ready` path instead of the counter fast path. |
| QR Loyalty | Loyalty credits points from `OrderCompleted` / `PaymentConfirmed`; reversals on refunds (`PaymentRefunded`). |
| QR Analytics | Consumes `PosSaleRegistered` for the cash-session dimension, plus the order and payment events. |
| QR Connect | Webhooks for `OrderCreated`, `OrderCompleted`, `OrderCancelled`, `PaymentConfirmed`, `PaymentRefunded`; cash events are not in Connect's v1 consumed list. |
| QR Delivery | Independent channel; delivery orders never enter cash sessions unless paid at the counter. |

## 24. Behavior after suspension or cancellation

- **Expiry or cancellation of POS**: no new sales (`pos.sale.register` is `deny`) and no new sessions (`pos.cash_register.open` is `deny`); withdrawals and supplies in a session opened before expiry stay possible (`pos.cash_movement.register` is `complete_existing`); open sessions can be counted and closed (`pos.cash_register.close` is `allow`); sales can be cancelled (`pos.sale.cancel` is `allow`); refunds and history stay available.
- **Payment grace**: `payment_grace_days` is `0`.
- **Administrative or security suspension**: per the `suspension` rule of `authorization-rules.json`, the operational-state check blocks only `deny` operations (`pos.sale.register`, `pos.cash_register.open`). `pos.cash_movement.register`, `pos.cash_register.close` and `pos.sale.cancel` stay available, so open sessions can be counted and closed during a suspension.
- **Module retirement**: `retired` grants nothing, so every open session must be closed before the transition.

## 25. Acceptance criteria

1. Receipts are labeled non-fiscal in all three locales (release gate).
2. Cash session totals reconcile to the centavo (release gate).
3. Fiscal issuance stays a separate future domain (release gate); no fiscal terms appear in POS UI or receipts.
4. Every counter sale is a Commerce Core Order with channel `counter`; there is no POS order table.
5. A double-submitted sale creates one order (idempotency key).
6. Cross-tenant access to sessions, sales and receipts is impossible.
7. After expiry, sales and new sessions are denied while movements, counting and closing of an already open session succeed.
8. During an administrative or security suspension, sales and session openings are refused while closing and cancellation succeed.
9. A counter sale without QR Kitchen reaches `completed` through `placed → accepted → ready → completed` performed by a `cashier`.
10. Roles, capabilities and after-expiry behavior of the five POS operations match `authorization-rules.json` (parity test).
11. i18n parity and strict CSP hold; `cargo fmt`, `clippy -D warnings` and `cargo test --locked` pass.

## 26. Test scenarios

| ID | Scenario | Expected |
|---|---|---|
| POS-01 | Open session, 3 cash sales, 1 supply, 1 withdrawal, close with exact count | Difference 0; `CashRegisterOpened` and `CashRegisterClosed` emitted once. |
| POS-02 | Counted cash 500 centavos short | Difference −500 recorded; no auto-adjustment. |
| POS-03 | Second session on the same register | Rejected by the partial unique constraint. |
| POS-04 | Withdrawal after `open → counting` | Rejected. |
| POS-05 | Sale split 1000 cash + 990 Pix on a 1990 total | Two confirmed payments; change computed in integers. |
| POS-06 | `cashier` cancels an accepted order | Denied (`pos.sale.cancel` roles and state-machine actors). |
| POS-07 | `manager` refunds a cash payment with no open session | Rejected; refund by another method allowed. |
| POS-08 | Same sale submitted twice | One order, one receipt number. |
| POS-09 | Session of establishment A accessed by member of B | `404`, no data leak. |
| POS-10 | Capability expired with session open | Sale and new session denied; withdrawal, count and close succeed. |
| POS-11 | `TableBillClosed` redelivered | Attributed once. |
| POS-12 | Stock reaches −1 | Sale succeeds; low-stock warning shown; menu unchanged. |
| POS-13 | `cashier` registers a sale without QR Kitchen and completes it | `placed → accepted → ready → completed`; `OrderCreated`, `PosSaleRegistered`, `PaymentConfirmed`, `OrderCompleted` emitted once each. |
| POS-14 | Establishment suspended with a session open | Sale and new session refused; count, close and `pos.sale.cancel` succeed. |
| POS-15 | `manager` cancels a ready counter order with a confirmed cash payment | `OrderCancelled` and `PaymentRefunded`; cash refund movement in the open session. |

## 27. Commercial launch requirements

1. Contracted release gates: "receipts are labeled non-fiscal", "cash session totals reconcile to the centavo", "fiscal issuance stays a separate future domain".
2. Commerce Core (`orders`, `order_payments`, dispatcher) in production.
3. Remaining contract and code work: the open decisions in section 11 (refund of completed sales, discount approval, stock adjustment, session reopen) and the routes in section 16 added to the contracts with parity tests; `cashier` and `system` roles and the `complete_existing` expiry class implemented in `domain::entitlements`.
4. Legal review of receipt wording and of marketing copy (no claim of fiscal compliance).
5. Lifecycle `planned → development → internal_preview → beta → available`; immutable version content from `beta` (ADR-007).
6. Tested on 58 mm and 80 mm thermal printers in Chrome and Firefox; i18n parity; HTTP, database and `PROJECT_STATE.md` docs updated.
