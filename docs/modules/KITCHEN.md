# QR Kitchen

> **Status: Planned — not implemented, not sellable.** Nothing in this document is available in production. Every endpoint, table and event below is a design contract.

| Field | Value |
|---|---|
| Module code | `kitchen` |
| Commercial name | QR Kitchen |
| Phase | 6 |
| Lifecycle | `planned` (sale: never; grants: never — see `contracts/commercial/module-lifecycle.json`) |
| Monthly price (catalog version 1) | R$ 24,90 (`monthly_price_centavos: 2490`), effective 2026-11-24, editable until first sale (ADR-007) |
| Standalone | Yes (`standalone: true`, `requires_modules: []`) |
| Machine-readable contract | [`contracts/modules/kitchen.json`](../../contracts/modules/kitchen.json) (`contract_version: 1`) |

The JSON contracts are the single source of truth. Items marked **Proposed** are not in contract version 1 and must be added to the JSON (and its parity tests) before implementation. Commercial context: [Module catalog](../business/MODULE_CATALOG.md). Event semantics: [Event contracts](../architecture/EVENT_CONTRACTS.md).

## 1. Commercial objective

Offer a low-cost kitchen display system (KDS) that replaces paper production tickets on any tablet, TV or laptop browser. It raises revenue per establishment for QR Menu customers, is a component of the QR Business and QR Complete bundles, and runs on the existing single binary and SQLite database with no extra infrastructure, so the marginal cost per establishment stays near zero.

## 2. Target audience

- Small and medium restaurants, snack bars, burger shops, pizzerias, bakeries, cafés and açaí shops with one to six preparation stations.
- Kitchen staff using the planned `kitchen_operator` role, plus owners and managers acting as expeditors.
- Establishments that receive orders outside QR Cardápio (phone, counter, third-party marketplaces) and want only the production screen.
- Not targeted in the MVP: multi-unit central kitchens, chains needing production planning, hardware printer fleets.

## 3. Problems solved

| Problem | How QR Kitchen addresses it |
|---|---|
| Lost, illegible or out-of-order paper tickets | Digital production tickets per station, sorted by a deterministic priority |
| Items for different stations mixed on one slip | Routing rules split an order into one ticket per station |
| No visibility of preparation time | Per-ticket and per-item timers against target times |
| Late orders noticed only when the customer complains | Delay alerts on the display |
| Front of house does not know when food is ready | Ready column, plus `KitchenTicketStarted` and `KitchenTicketReady` for integrated channels |
| No record for disputes or staffing decisions | Ticket history with server timestamps for every item state |

## 4. MVP scope

- Kitchen sectors (for example "hot kitchen", "bar", "desserts") and prep stations inside each sector.
- Routing rules by product or category, with a mandatory default station.
- Station display (`GET /dashboard/kitchen/display/{station_id}`): full-screen, large type, touch targets of at least 48 px, columns for queued, in progress and ready.
- Station and routing configuration (`kitchen.station.configure`).
- Manual ticket entry with catalog products or free-text items (`kitchen.ticket.create`, `POST /dashboard/kitchen/tickets`), usable without any other module.
- Automatic tickets from `OrderAccepted`, created by the planned `system` role once Commerce Core and an ordering channel exist.
- Per-item states following the `preparation` state machine; ticket status derived from its items.
- Prioritization: rush flag, promised time, then arrival order.
- Timers rendered client-side from server timestamps; target time per station with optional per-product override.
- Delay alerts (visual, optional audible cue after a user gesture), acknowledged by staff.
- History: searchable list of finished tickets and a basic daily preparation-time summary per station.
- Full `pt-BR`, `en` and `es` UI with zero-reload switching.

## 5. Future scope

- Thermal printer output (ESC/POS) through a local bridge, and bump-bar keypads.
- Expeditor station that aggregates tickets from several stations of one order.
- Allergen and modifier highlighting once catalog variations and add-on groups ship in Commerce Core.
- Device pairing codes so kitchen screens do not need a member account.
- "Sold out" shortcut that toggles catalog availability from the display.
- Load balancing between identical stations and preparation-time prediction.
- Inventory or ingredient depletion stays out of scope (see `AGENTS.md`).

## 6. Domain entities

| Entity | Purpose | Key attributes |
|---|---|---|
| `KitchenSector` | Physical or logical area grouping stations | `id`, `establishment_id`, `name` (+ `name_en`, `name_es`), `position`, `is_active` |
| `Station` | Prep station that owns a display and a queue | `id`, `establishment_id`, `sector_id`, `name`, `target_seconds` (default 900), `is_default`, `is_active` |
| `RoutingRule` | Maps a product or category to a station | `id`, `establishment_id`, `station_id`, `product_id` or `category_id` (exactly one), `position` |
| `KitchenTicket` | Production unit for one station | `id`, `establishment_id`, `station_id`, `source` (`manual`/`order`), `order_id` (nullable), `label`, `priority` (`normal`/`rush`), `promised_at`, `status` (derived), `version`, `created_at` |
| `KitchenTicketItem` | One line to prepare | `id`, `ticket_id`, `establishment_id`, `product_id` (nullable), `name_snapshot`, `quantity` (1..=99), `notes`, `state`, `target_seconds`, `started_at`, `ready_at`, `handed_off_at`, `cancelled_at`, `version` |
| `DelayAlert` | Raised when a ticket exceeds its target or risks its promised time | `id`, `establishment_id`, `ticket_id`, `kind` (`over_target`/`promise_at_risk`), `raised_at`, `acknowledged_at`, `acknowledged_by`, `resolved_at` |

## 7. Responsibilities

| QR Kitchen owns | QR Kitchen does not own |
|---|---|
| Sectors, stations and routing configuration | Order lifecycle, acceptance and cancellation (Commerce Core `order` machine) |
| Production tickets and per-item preparation state | Prices, totals and payments (tickets carry no money) |
| Timers, prioritization and delay alerts | Catalog data (it stores name snapshots only) |
| Kitchen history and preparation-time summaries | Consumer notifications, waiter calls and delivery dispatch |
| Publishing `KitchenTicketCreated`, `KitchenTicketStarted` and `KitchenTicketReady` | Any order engine of its own (forbidden; Commerce Core is the single Order aggregate) |

## 8. Invariants

1. Every row carries `establishment_id`; station, sector, product, category and order references are verified to belong to the same establishment.
2. A ticket belongs to exactly one station; a station belongs to exactly one sector.
3. For `source = order` there is at most one ticket per `(establishment_id, order_id, station_id)`.
4. A ticket's item set is fixed at creation. Changes to an order produce a new ticket, never an edit.
5. Item states change only through transitions of the `preparation` machine; `handed_off` and `cancelled` are final.
6. Ticket status is a projection of its items, recomputed in the same transaction as every item transition.
7. `KitchenTicketStarted` and `KitchenTicketReady` are each emitted at most once per ticket.
8. Tickets carry no prices and no customer personal data; item names are immutable snapshots.
9. Exactly one active default station exists per establishment before any ticket can be created.
10. All timestamps are server-side UTC; clients never supply state timestamps.

## 9. Business rules

- **Routing resolution**: product rule, then category rule, then the establishment default station. Free-text manual items go to the station chosen in the form, or the default.
- **Splitting**: an order whose items resolve to N stations yields N tickets sharing `order_id`.
- **Priority order on a display**: `priority` (`rush` first), then `promised_at` ascending (nulls last), then `created_at` ascending. Owners and managers may set `rush`.
- **Targets**: item target is the product override if set, else the station `target_seconds`. Ticket target is the largest item target.
- **Delay alerts**: `over_target` when `now > created_at + ticket target`; `promise_at_risk` when `promised_at - now` is lower than the ticket target and preparation has not finished. One open alert per `(ticket_id, kind)`. An alert resolves automatically when the ticket becomes ready or cancelled.
- **Manual tickets**: `label` up to 40 characters (for example "Table 4", "Marketplace 1234"); free-text item names up to 80 characters; notes up to 140 characters. The form warns that labels and notes must not contain customer personal data.
- **Cancellation**: `OrderCancelled` moves the order's `queued`, `in_progress` and `ready` items to `cancelled` (actor `system`); items already `handed_off` stay final. Owners and managers may cancel manual tickets.
- **Display retention**: tickets whose items are all final leave the active display after 120 seconds and remain in history.
- **History retention**: 12 months, then purged by a batched maintenance task.

## 10. State machines

QR Kitchen implements the `preparation` machine of [`contracts/commerce/state-machines.json`](../../contracts/commerce/state-machines.json) (aggregate `kitchen_ticket_item`, initial `queued`, terminal `handed_off` and `cancelled`):

| From | To | Actors | Event | Notes |
|---|---|---|---|---|
| `queued` | `in_progress` | `kitchen_operator`, `owner`, `manager` | none | |
| `in_progress` | `ready` | `kitchen_operator`, `owner`, `manager` | none | |
| `ready` | `handed_off` | `kitchen_operator`, `waiter`, `courier`, `owner`, `manager` | none | |
| `queued` | `cancelled` | `owner`, `manager`, `system` | none | |
| `in_progress` | `cancelled` | `owner`, `manager`, `system` | none | |
| `ready` | `cancelled` | `owner`, `manager`, `system` | none | Order cancelled after the item was ready but before hand-off |

Item transitions emit no event. The contract states that the ticket-level events `KitchenTicketCreated`, `KitchenTicketStarted` and `KitchenTicketReady` aggregate item states. Derived ticket status (module-internal projection, not a separate machine):

| Ticket status | Condition | Ticket-level event |
|---|---|---|
| `queued` | every non-cancelled item is `queued` | `KitchenTicketCreated` at insertion |
| `in_progress` | at least one item `in_progress`, not all ready | `KitchenTicketStarted` (once, when the first item leaves `queued`) |
| `ready` | every non-cancelled item is `ready` or `handed_off`, at least one exists | `KitchenTicketReady` (once) |
| `handed_off` | every non-cancelled item is `handed_off` | none |
| `cancelled` | every item is `cancelled` | none |

Concurrency follows Commerce Core: `UPDATE ... WHERE id = ? AND establishment_id = ? AND state = <expected> AND version = ?`; zero rows affected returns `409` with the current state.

## 11. Permissions

Roles come from `contracts/entitlements/authorization-rules.json`. `kitchen_operator`, `waiter`, `courier`, `cashier` and `system` are planned roles.

| Action | Operation | Roles | Capability | After expiry |
|---|---|---|---|---|
| Move an item between preparation states | `kitchen.ticket.progress` | `owner`, `manager`, `kitchen_operator`, `waiter`, `courier` | `kitchen.ticket.manage` | `complete_existing` |
| Create a manual ticket | `kitchen.ticket.create` | `owner`, `manager`, `kitchen_operator`, `waiter`, `cashier` | `kitchen.ticket.create` | `deny` |
| Create tickets from `OrderAccepted` | outbox consumer | `system` | `kitchen.ticket.create` (re-checked as of the event `occurred_at`) | `deny` |
| Configure sectors, stations, routing | `kitchen.station.configure` | `owner`, `manager` | `kitchen.station.manage` | `deny` |
| Cancel a manual ticket | covered by `kitchen.ticket.progress` | `owner`, `manager` | `kitchen.ticket.manage` | `complete_existing` |
| View display and history | read only, no contract operation | `owner`, `manager`, `kitchen_operator` | none | allowed |

The `preparation` machine narrows `kitchen.ticket.progress` further: `waiter` and `courier` may only perform `ready → handed_off`; only `owner`, `manager` and `system` may cancel. The `system` role is an idempotent outbox consumer acting for one establishment; it runs only when the triggering event was produced under a valid operation and re-checks Kitchen's own capability.

`authorize(...)` evaluates module lifecycle, establishment operational state, member permission, deployment enforcement and commercial entitlement, in that order. While the module is `planned`, every operation is denied even with an `entitlement_grants` row.

## 12. Commercial capabilities

| Capability | Summary (from `capabilities.json`) |
|---|---|
| `kitchen.ticket.create` | Create production tickets manually or from orders. Required by `kitchen.ticket.create` and by `system` ticket creation. |
| `kitchen.ticket.manage` | Progress production tickets and item states on the kitchen display. Required by `kitchen.ticket.progress`. |
| `kitchen.station.manage` | Configure kitchen sectors, stations and routing rules. Required by `kitchen.station.configure`. |

- Sold alone at 2490 centavos per month (catalog version 1), or inside QR Business (16990) and QR Complete (21990). Not part of QR Start.
- Billing cycles from `billing-cycles.json`: monthly, semiannual (`discount_bps: 500`), annual (`discount_bps: 1000`), rounding `half_up_per_line`.
- Affiliate eligible. Capabilities come from contract snapshots or explicit grants, never from plan names.

## 13. Technical dependencies

| Core component | Status (`components.json`) | Use |
|---|---|---|
| `identity`, `establishments`, `members`, `tenancy`, `i18n` | implemented | Sessions, CSRF, `TenantContext`, catalogs |
| `catalog` | partial | Product and category ids and translated names for routing and snapshots |
| `commercial`, `entitlements` | implemented | `authorize(...)` and capability resolution |
| `events` | partial | Outbox append exists; consumption needs the dispatcher shipping with Commerce Core (phase 3) |
| `orders` | planned | Read port for accepted order items |

Optional integrations: `delivery`, `tables`, `pos`, `analytics`, `connect`. Prerequisites: planned roles require an expand migration, because `establishment_members.member_role` is constrained to `owner` and `manager`; the `system` actor must exist for outbox consumers; and the `complete_existing` expiry class must be modelled in `domain::entitlements`, which today has only `Deny` and `Allow`.

## 14. Published events

All events are appended to `domain_events` in the same transaction as the state change, with the standard envelope (`aggregate_type = kitchen_ticket`).

| Event | Trigger | Payload (registry v1) | Proposed `dedupe_key` |
|---|---|---|---|
| `KitchenTicketCreated` | Ticket inserted (manual or from an order) | `ticket_id`, `order_id` (`string\|null`), `station_id` | `kitchen_ticket:{ticket_id}:created` |
| `KitchenTicketStarted` | First item of the ticket moves `queued → in_progress` | `ticket_id`, `order_id` (`string\|null`) | `kitchen_ticket:{ticket_id}:started` |
| `KitchenTicketReady` | Derived ticket status becomes `ready` | `ticket_id`, `order_id` (`string\|null`) | `kitchen_ticket:{ticket_id}:ready` |

Manual tickets have no order, so `order_id` is null; the registry types it as `string|null` in all three events. Payloads never include labels, notes or item names. The registry sets no dedupe key for planned events; the keys above are design proposals.

## 15. Consumed events

| Event | Producer | Effect | Idempotency |
|---|---|---|---|
| `OrderAccepted` | Commerce Core | Read the order items through the `orders` read port, route them and create one ticket per station with `promised_at` | Dispatcher record by `event_id` plus unique `(establishment_id, order_id, station_id)` |
| `OrderCancelled` | Commerce Core | Cancel `queued`, `in_progress` and `ready` items of the order's tickets | Transitions are no-ops when the item is already final |

Both consumers run as the planned `system` role. The entitlement check for tickets created from orders uses the event `occurred_at`, so replaying the outbox gives the same result.

## 16. HTTP contracts

Contract endpoints (all **planned**, none available):

| Method | Path | Audience | Purpose |
|---|---|---|---|
| `GET` | `/dashboard/kitchen/display/{station_id}` | member | Station display, server-rendered, `Cache-Control: no-store` |
| `POST` | `/dashboard/kitchen/tickets` | member | Create a manual ticket (`kitchen.ticket.create`) |
| `POST` | `/dashboard/kitchen/items/{id}/transition` | member | Apply a `preparation` transition (`to`, `version`, `_csrf`) (`kitchen.ticket.progress`) |

**Proposed** routes (not in `kitchen.json` yet): `GET /dashboard/kitchen` (overview and history), `GET`/`POST /dashboard/kitchen/stations` and `/dashboard/kitchen/routing` (configuration through the contracted `kitchen.station.configure` operation), `POST /dashboard/kitchen/alerts/{id}/acknowledge`, and `GET /dashboard/kitchen/display/{station_id}/events` (Server-Sent Events carrying only a revision number).

Rules: `TenantContext` on every route; `station_id` and item `id` are resolved with `AND establishment_id = ?`, and a foreign id returns `404`; every `POST` verifies `_csrf`; stale `version` returns `409` with the current state; JSON responses only for `Accept: application/json` requests from `static/js/kitchen.js`, HTML otherwise; errors map to i18n keys.

## 17. Integration contracts

- **Commerce Core `orders` read port**: `accepted_order_items(establishment_id, order_id)` returns item lines (product id, translated name, quantity, notes) and `promised_at`. Kitchen never writes orders.
- **Order transitions**: Commerce Core consumes `KitchenTicketStarted` and moves the order `accepted → preparing` (actor `system`), and consumes `KitchenTicketReady` and moves the order `preparing → ready` when every ticket of the order is ready (actor `system`). Events with a null `order_id` are ignored. Without QR Kitchen, staff use the `accepted → ready` fast path (owner, manager, cashier, waiter).
- **QR Delivery** consumes `KitchenTicketCreated`, `KitchenTicketStarted` and `KitchenTicketReady` to show preparation progress and signal pickup. **QR Tables** consumes `KitchenTicketReady` to signal service. **QR POS** reaches Kitchen only through orders.
- **QR Analytics and QR Connect**: no Kitchen events are consumed in their v1 contracts.

## 18. Conceptual persistence model

| Table | Notes |
|---|---|
| `kitchen_sectors` | `UNIQUE (establishment_id, name)`, index `(establishment_id, position)` |
| `kitchen_stations` | FK to sector in the same establishment; partial unique index on `(establishment_id) WHERE is_default = 1` |
| `kitchen_routing_rules` | `CHECK` exactly one of `product_id`/`category_id`; unique per `(establishment_id, product_id)` and `(establishment_id, category_id)` |
| `kitchen_tickets` | Partial unique `(establishment_id, order_id, station_id) WHERE order_id IS NOT NULL`; index `(establishment_id, station_id, status, priority, promised_at, created_at)` |
| `kitchen_ticket_items` | `state` `CHECK` over the five `preparation` states; `quantity` `CHECK` 1..=99; index `(establishment_id, ticket_id)` |
| `kitchen_delay_alerts` | Partial unique `(ticket_id, kind) WHERE resolved_at IS NULL` |

Ids are `TEXT`, writes use `BEGIN IMMEDIATE`, and migrations follow ADR-010 (expand only, `IF NOT EXISTS`). SQL lives in `src/infrastructure/database/queries/kitchen_*.rs`, one file per entity.

## 19. Security requirements

- Strict tenant isolation on every query and cross-check of `station_id`, `product_id`, `category_id` and `order_id` ownership.
- CSRF on every authenticated `POST`; optimistic `version` on transitions.
- Strict CSP: no inline scripts or styles; timers and colors through classes and `data-*` attributes; SSE stays same-origin under `default-src 'self'`.
- Per-member rate limit on transitions (for example 120 per minute) and on manual ticket creation (30 per minute).
- Shared kitchen tablets: session cookies stay `HttpOnly`, `Secure`, `SameSite=Lax`; logout from the display is one tap; no credentials in URLs.
- Free text is escaped by Askama and length-bounded server-side.

## 20. Privacy requirements

- Tickets and events contain no customer names, phones, emails or addresses. Order-derived tickets copy only item data.
- Manual labels and notes are free text; the UI discourages personal data and the 12-month purge bounds retention.
- Member actions are attributed by member id for accountability; history exports include member ids, not emails.

## 21. Performance requirements

- Display refresh under 2 s after a change without polling storms (release gate): SSE pushes a per-station revision from an in-process map; the client fetches the board only when the revision changes. Fallback polling is at most every 5 s with `ETag`/`304` answered from memory without SQLite reads.
- Timers tick client-side; no per-second server traffic.
- A transition is one short `BEGIN IMMEDIATE` transaction (item update, ticket projection, optional outbox row); p95 under 20 ms on 1 vCPU.
- At most 10 display connections per establishment; each costs only a task and a broadcast receiver.
- Board queries read only non-final tickets of one station through the composite index.

## 22. Behavior without other modules

Fully usable with QR Core only: configure stations (`kitchen.station.configure`), type manual tickets (`kitchen.ticket.create`, catalog products or free text), progress items (`kitchen.ticket.progress`) and read history. `KitchenTicketCreated`, `KitchenTicketStarted` and `KitchenTicketReady` are still emitted with a null `order_id`; no consumer requires them.

## 23. Behavior integrated with other modules

- With QR Delivery, QR Tables or QR POS orders, `OrderAccepted` creates routed tickets automatically (`system` role); manual entry remains available.
- `KitchenTicketStarted` lets Commerce Core move the order to `preparing` and lets Delivery show progress.
- `KitchenTicketReady` lets Commerce Core mark the order ready and lets Delivery and Tables alert staff.
- Waiters and couriers hand off ready items (`ready → handed_off`) from their own devices.
- `OrderCancelled` cancels every item that is not yet handed off.
- With QR Fleet, readiness reaches Fleet only through Commerce Core's `OrderReady`; Kitchen never calls Fleet.

## 24. Behavior after suspension or cancellation

- **Establishment suspended** (`establishment_operational_states`): per the `suspension` rule of `authorization-rules.json`, the operational-state check blocks only `deny` operations. `kitchen.ticket.create`, `kitchen.station.configure` and `system` ticket creation from `OrderAccepted` are refused, and the display shows a translated suspension notice. `kitchen.ticket.progress` (`complete_existing`) stays available, so existing tickets can be finished, handed off or cancelled. Reinstatement restores operation with no data loss.
- **Subscription expired or cancelled** (contract `after_expiry`): "No new tickets. Tickets already queued can be finished or cancelled." `kitchen.ticket.progress` is `complete_existing`; `kitchen.ticket.create` and `system` ticket creation are denied; `kitchen.station.configure` is denied, so configuration becomes read-only; history and export (QR Core `data.export`) stay available.
- **Module `retired`**: no operations are granted; history remains exportable.
- Data is kept after cancellation and follows the account deletion policy.

## 25. Acceptance criteria

1. An establishment with only QR Core and an active Kitchen entitlement creates a manual ticket and takes it to `handed_off`.
2. An order with items for two stations produces exactly two tickets, even when `OrderAccepted` is delivered twice.
3. Every `preparation` transition outside the table in section 10 returns `409` or `422` and changes nothing.
4. `KitchenTicketCreated`, `KitchenTicketStarted` and `KitchenTicketReady` each appear once per ticket in `domain_events`.
5. A second display reflects a transition in under 2 s.
6. A member of another establishment receives `404` for any station, ticket or item id.
7. After expiry, queued tickets can still be finished and new manual tickets are refused with a distinct i18n error.
8. During a suspension, ticket creation and station configuration are refused while existing tickets can still be progressed.
9. Roles and after-expiry behavior of `kitchen.ticket.progress`, `kitchen.ticket.create` and `kitchen.station.configure` match `authorization-rules.json` (parity test).
10. All UI strings exist in `pt-BR`, `en` and `es`; templates have no inline scripts or styles.

## 26. Test scenarios

| Scenario | Type |
|---|---|
| Routing precedence: product rule beats category rule beats default station | unit (domain) |
| Derived status for every combination of item states, including all-cancelled | unit (domain) |
| Duplicate `OrderAccepted` delivery creates no extra ticket | integration |
| `OrderCancelled` cancels queued, in-progress and ready items, leaves handed-off items untouched | integration |
| First item start emits `KitchenTicketStarted` once; a second item start emits nothing | integration |
| Commerce Core moves the order `accepted → preparing` on `KitchenTicketStarted` and ignores a null `order_id` | integration |
| `waiter` hands off a ready item but cannot start or cancel one | integration |
| Suspended establishment: manual ticket refused, transition on an existing item succeeds | integration |
| Concurrent transitions on the same item: one wins, the other gets `409` | integration |
| Cross-tenant `station_id` on display and `POST /dashboard/kitchen/tickets` returns `404` | integration |
| Missing or wrong `_csrf` on transition returns `403` | integration |
| `planned` lifecycle denies every operation even with an `entitlement_grants` row | integration |
| `kitchen_operator` cannot configure stations | integration |
| Delay alert raised once, resolved on ready | unit plus integration |
| i18n key parity for every `kitchen.*` key | contract |
| Display update latency under load with 10 connections | benchmark |

## 27. Commercial launch requirements

- Release gates from the contract: "display refresh under 2 s without polling storms", "idempotent ticket creation per order", "manual tickets without any other module".
- Contract work left before implementation: JSON Schemas for `KitchenTicketCreated`, `KitchenTicketStarted` and `KitchenTicketReady` under `contracts/events/schemas/` (with `order_id` nullable) when they move to `implemented`; the proposed routes in section 16 added to `kitchen.json`.
- Prerequisites shipped: event dispatcher, planned member roles (`kitchen_operator`, `waiter`, `courier`, `cashier`) and the `system` actor, `complete_existing` enforcement.
- Lifecycle progression `planned → development → internal_preview` (explicit grants only) `→ beta → available`, each a reviewed change validated at start-up.
- Price version hashed once sellable (ADR-007); module page and landing copy in three languages; runbook entry; `docs/agents/PROJECT_STATE.md` updated.
- Pilot with at least three establishments using manual tickets for two weeks without data loss.
