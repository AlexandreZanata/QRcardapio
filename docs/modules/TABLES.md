# QR Tables

> **Status: Planned — not implemented, not sellable.** Nothing in this document exists in the code. No route, table, operation or event listed here is available. With lifecycle `planned`, the catalog never sells or grants this module.

| Field | Value |
|---|---|
| Module code | `tables` |
| Lifecycle | `planned` ([module lifecycle](../../contracts/commercial/module-lifecycle.json)) |
| Phase | 5 (after Commerce Core in phase 3 and QR Delivery in phase 4) |
| Monthly price | R$ 39,90 (`3990` centavos), catalog version 1, effective 2026-11-24. Prices of planned modules can still change until the version is first sold (ADR-007). |
| Cycles (indicative) | Monthly; semiannual `500` bps (R$ 227,43); annual `1000` bps (R$ 430,92) |
| Bundles | QR Business, QR Complete (both `planned`) |
| Capabilities | `tables.session.manage`, `tables.order.create`, `tables.bill.manage` |
| Machine contract | [contracts/modules/tables.json](../../contracts/modules/tables.json) |

Items marked **(proposed)** are design proposals that do not yet appear in any JSON contract. Each one needs a contract change, with parity tests, before implementation. If this document and a contract disagree, the contract wins. Commercial context: [Module catalog](../business/MODULE_CATALOG.md). Event semantics: [Event contracts](../architecture/EVENT_CONTRACTS.md).

## 1. Commercial objective

- Sell in-venue service operations: table sessions, tabs (comandas), orders placed at the table, waiter calls and bill closing, at R$ 39,90 per month, standalone or inside the QR Business and QR Complete bundles.
- Give QR Menu customers who serve at tables a natural upgrade, without requiring QR Menu.
- Reduce lost comandas, ordering errors, waiter round trips and disputes over bill splits.
- Remain affiliate eligible (`affiliate_eligible: true`).

## 2. Target audience

| Persona | Need |
|---|---|
| Owner or manager of a full-service restaurant, bar, pub or pizzeria | Occupancy at a glance, accurate tabs, fast bill closing |
| Snack bars and cafés with counter comandas | Tabs that are not tied to a table |
| Waiter (planned role `waiter`) | Open sessions, enter orders, answer calls, close bills from a phone |
| Cashier (planned role `cashier`) | Settle payments and close bills |
| Seated consumer (planned role `consumer`) | See the catalog, order and call the waiter without an account or app |

## 3. Problems solved

- Paper comandas get lost or misread, and orders reach the wrong table.
- Guests wait for a waiter just to order or to ask for the bill.
- Splitting a bill by hand causes centavo errors and disputes.
- Managers cannot see which tables are occupied or for how long.
- A photographed table QR Code would allow prank or remote orders. Sessions controlled by the establishment, with rotating join codes, prevent this.
- Items move between tables or guests (for example, a party that moves) and the totals drift.

## 4. MVP scope

| Capability area | MVP definition |
|---|---|
| Tables | Create, edit, deactivate and order tables with a label, an optional area and capacity. |
| Individual QR Codes | One opaque `TableQrToken` per table that opens `/t/{table_token}`. It can be revoked and reissued (reprint). It is separate from `/c/{slug}`. |
| Temporary service sessions | Staff open a `TableSession` on a free table. The session moves `open` → `bill_requested` → `closed`, or ends as `voided` when it has no items. |
| Rotating join codes | Each session gets a new `SessionJoinCode`. Staff can rotate it during the session. Consumers must present it before they can order. |
| Tabs (comandas) | One or more `Tab` records per session (per guest or per group). Orders and payments are attributed to tabs. |
| In-venue orders | `tables.order.place`: orders placed by consumers from a joined device, or entered by staff. They use the Commerce Core `Order` aggregate with the `table` channel. |
| Manual entry | Staff enter catalog items for any tab through `tables.order.place`. Open-price items and discounts are restricted to owners and managers **(proposed)**. |
| Waiter call | `tables.waiter.call`: a joined consumer raises a `WaiterCall` (call or help); staff acknowledge and resolve it. The bill request moves the session to `bill_requested`. |
| Bill closing | `tables.bill.close` once the balance is settled, or with an explicit authorization of a pending balance. Emits `TableBillClosed`. |
| Bill split | `BillSplit` by tab, into equal parts, by item, or into custom amounts. All arithmetic is exact in integer centavos. |
| Item transfer | `tables.item.transfer`: `ItemTransfer` between tabs of the same session, or between open sessions of the same establishment. The party can also move to another table. |
| Occupancy | A board of free, occupied and bill-requested tables, with time since opening and guest count. |
| Staff permissions | Owner, manager, waiter and cashier, according to the permission matrix in section 11. |
| Optional service charge | A percentage in basis points, configured per establishment, shown separately and removable by the consumer **(proposed)**. |

## 5. Future scope

- A native staff app (Kotlin Multiplatform) that reuses the same versioned API. It is not started; see [White-label architecture](../architecture/WHITE_LABEL.md).
- Printing kitchen and bar tickets without QR Kitchen (thermal printers).
- Online payment at the table through the gateway once `order_payments` supports gateway methods.
- A floor-plan editor with drag-and-drop table layout.
- Out of scope for this module: reservations, fiscal documents (NFC-e, which stays a separate future domain under POS), inventory and delivery.

## 6. Domain entities

| Entity | Purpose | Key attributes |
|---|---|---|
| `Table` | A physical table or seat group | `id`, `establishment_id`, `label`, `area`, `capacity`, `is_active`, `position` |
| `TableQrToken` | The printed QR identity of a table | Opaque random token of at least 128 bits, stored as a SHA-256 hash, `revoked_at` |
| `TableSession` | One period of service at a table | `table_id`, `state`, `opened_by`, `guest_count`, `opened_at`, `closed_at`, `version` |
| `SessionJoinCode` | Short code that admits consumer devices to a session | `session_id`, code hash, `issued_at`, `rotated_at`, `expires_at` |
| `Tab` | A comanda inside a session, or a standalone one | `session_id` (nullable for a counter comanda), `label`, `state` |
| `WaiterCall` | A request for staff attention | `session_id`, `reason` (`call` or `help`), `acknowledged_at`, `resolved_at` |
| `BillSplit` | How a session's balance is divided | `session_id`, `method`, parts with `amount_centavos` |
| `ItemTransfer` | Audited move of order items | from and to tab or session, `order_item_id`, `quantity`, `actor`, `reason` |
| `SessionParticipant` **(proposed)** | A consumer device admitted with the join code | Hash of the participant token, `session_id`, `tab_id`, `expires_at` |

Commerce Core provides `Order`, `OrderItem` and `OrderPayment` through the `orders` and `order_payments` components.

## 7. Responsibilities

QR Tables owns these things:

- the table registry and table QR tokens;
- sessions, join codes and participant admission;
- tabs, waiter calls, bill splits and item transfers;
- the occupancy board and the in-venue consumer page.

QR Tables does not own these things:

- The `Order` aggregate and the `order` and `order_payment` state machines (Commerce Core).
- The catalog (QR Core `catalog`).
- Production tickets (QR Kitchen) and cash register sessions (QR POS).
- Loyalty points (QR Loyalty) and notification delivery (QR Core `notifications`).

## 8. Invariants

1. A table has at most one session in a non-terminal state (`open` or `bill_requested`).
2. An order attaches only to a session in state `open`. In `bill_requested` new orders are rejected until staff reopen the session.
3. A consumer order requires a participant that joined with the session's current join code, has not expired, and belongs to the same `establishment_id` and `session_id` as the request.
4. Join codes are never reused across sessions. Each new session issues a new code, and rotating it invalidates the previous code for new joins.
5. Table QR tokens are random and independent of the slug. `/c/{slug}` and its printed QR Codes are never reused, redirected or changed by this module.
6. All amounts are integer centavos. Session total = sum of the non-cancelled order totals = sum of the tab totals. The parts of a split sum exactly to the amount being split. A transfer leaves the combined total of the affected sessions unchanged.
7. Order items snapshot their unit price from the server catalog when the order is created. A client-supplied price is never accepted.
8. Every row carries `establishment_id`. Table, session, tab, order, product and participant references are checked against the same establishment, at database level with composite foreign keys.
9. `closed` and `voided` sessions are immutable. After closing, only refunds through the `order_payment` machine are allowed.
10. Only sessions without items can be voided.

## 9. Business rules

1. **Sessions are opened only by staff.** A consumer can never create a session. Scanning a table QR Code with no open session shows the catalog read-only.
2. **Join codes.** 6 characters from an unambiguous alphabet (no `0`, `O`, `1`, `I`), shown on the waiter's device or the table display. Staff tell the code to the guests. Attempts are limited per table token and per client IP, and comparison runs in constant time.
3. **Rotation.** A new code for every session (release gate). Staff may rotate it at any time. Rotation keeps admitted participants but blocks new joins with the old code. Closing the session expires every participant.
4. **Staff-only mode** **(proposed)**: an establishment setting turns off consumer self-ordering. The table page then shows the catalog and the waiter call only.
5. **Consumer orders** are created in state `placed` and must be accepted by staff before production, unless the establishment enables auto-acceptance for joined participants **(proposed)**.
6. **Availability and prices** are revalidated when the order is created. A product that became unavailable, or a stale `catalog_version`, returns `409` with current data.
7. **Waiter calls**: at most one open call per session and reason. A repeated call refreshes it instead of duplicating it. Open calls are resolved automatically when the session closes.
8. **Bill request**: a consumer or staff moves the session to `bill_requested`. Staff can return it to `open`.
9. **Bill closing**: from `open` or `bill_requested`, allowed when confirmed payments minus refunds cover the session total, or when an owner or manager explicitly authorizes the pending balance (actor and reason recorded). Emits `TableBillClosed` with `total_centavos`.
10. **Splitting**:
    - by tab: each tab pays its own orders;
    - equal parts: `n` parts of `floor(total / n)`, and the first `total mod n` parts get one extra centavo;
    - by item: an item shared between parts is divided with the same remainder rule;
    - custom amounts: the parts must add up exactly.
    A service charge is computed once on the base amount (rounded half up) and then distributed with the same remainder rule.
11. **Item transfer** (`tables.item.transfer`): only items of non-cancelled orders, only between open sessions or tabs of the same establishment, with integer quantities (a partial quantity splits the line). Each transfer is recorded with the actor.
12. **Party transfer**: moving a session to another table requires the target table to be free. The session id, its tabs and its join code stay the same.
13. **Manual entry**: waiters enter catalog items. Open-price items and discounts (in centavos or bps) require an owner or manager and a recorded reason **(proposed)**.
14. **Occupancy**: derived from session state. A table with no session is free. `open` means occupied. `bill_requested` means waiting for the bill.

## 10. State machines

**`table_session`** (from `contracts/commerce/state-machines.json`, which is `planned`). The initial state is `open`. The terminal states are `closed` and `voided`.

| From | To | Actors | Event | Notes |
|---|---|---|---|---|
| `open` | `bill_requested` | consumer, waiter, owner, manager | — | |
| `bill_requested` | `open` | waiter, owner, manager | — | |
| `bill_requested` | `closed` | waiter, cashier, owner, manager | `TableBillClosed` | Requires a settled balance or an explicit pending-balance authorization |
| `open` | `closed` | waiter, cashier, owner, manager | `TableBillClosed` | Requires a settled balance or an explicit pending-balance authorization |
| `open` | `voided` | owner, manager | — | Only for sessions without items |

`TableSessionOpened` is emitted when a session is created.

**`order`** and **`order_payment`** are reused unchanged from Commerce Core. Every transition updates with `WHERE id = ? AND establishment_id = ? AND state = ? AND version = ?`. If no row changes, the call returns `409` with the current state. The transitions that table service relies on:

| Machine | Transition | Actors (contract) | Table use |
|---|---|---|---|
| `order` | `placed → accepted` | owner, manager, cashier, waiter | Staff accept consumer orders |
| `order` | `accepted → preparing` | owner, manager, kitchen_operator, system | `system` on `KitchenTicketStarted` when QR Kitchen routes the order |
| `order` | `preparing → ready` | owner, manager, kitchen_operator, system | `system` once every ticket of the order emitted `KitchenTicketReady` |
| `order` | `accepted → ready` | owner, manager, cashier, waiter | Fast path without QR Kitchen |
| `order` | `ready → completed` | owner, manager, cashier, waiter, courier, system | Waiter marks the order served |
| `order` | `accepted`/`preparing`/`ready → cancelled` | owner, manager | Compensations: refund confirmed payments, cancel kitchen tickets (not from `ready`), cancel pending deliveries |
| `order_payment` | `pending → confirmed` | gateway, owner, manager, cashier, waiter, courier | Staff confirm payments collected at the table with the method used |

**`WaiterCall`** has no contract state machine. The proposed lifecycle is `open` → `acknowledged` → `resolved`, with `cancelled` when the session closes **(proposed)**.

## 11. Permissions

Operations in `authorization-rules.json` (all `planned`):

| Operation | Roles | Capability | After expiry |
|---|---|---|---|
| `tables.session.open` | owner, manager, waiter | `tables.session.manage` | `deny` |
| `tables.order.place` | consumer, waiter, owner, manager | `tables.order.create` | `deny` |
| `tables.waiter.call` | consumer | `tables.session.manage` | `deny` |
| `tables.item.transfer` | waiter, owner, manager | `tables.bill.manage` | `complete_existing` |
| `tables.bill.close` | owner, manager, waiter, cashier | `tables.bill.manage` | `complete_existing` |

State transitions of the `order` and `order_payment` machines are additionally limited to the actors listed in section 10. Automatic transitions (`accepted → preparing` on `KitchenTicketStarted`, `preparing → ready`, resolving open waiter calls when a session closes) run as the planned `system` role: an idempotent outbox consumer that acts only when the triggering event was produced under a valid operation and that re-checks the consumer module's own capability.

Open decisions (no operation in the contract yet; each needs an `authorization-rules.json` row and parity tests before implementation):

- Which operation authorizes staff progressing table orders (`placed → accepted`, `accepted → ready`, `ready → completed`); QR Delivery uses `delivery.order.progress`, QR Tables has no equivalent.
- Bill split: a separate operation, or part of `tables.bill.close`.
- Party transfer to another table, join code rotation and waiter call acknowledgement or resolution.
- Session void (`open → voided`) and bill request (`open → bill_requested`).
- Table, QR token and settings configuration (expected owner and manager, no capability, `allow`).

The roles `waiter`, `cashier`, `consumer` and `system` are `planned`. The `establishment_members.member_role` CHECK accepts only `owner` and `manager` today, and no staff invitation flow exists yet. In `authorization-rules.json` the `consumer` role is scoped to `storefront`. For Tables, a consumer is a participant admitted to a single session. The evaluation order is the same five steps as every other module.

## 12. Commercial capabilities

| Capability | Summary (from `capabilities.json`) | Gates |
|---|---|---|
| `tables.session.manage` | Open, transfer and close table sessions. | `tables.session.open`, `tables.waiter.call` |
| `tables.order.create` | Create in-venue orders from a controlled table session. | `tables.order.place` (consumer orders and manual entry) |
| `tables.bill.manage` | Close, split and settle table bills. | `tables.bill.close`, `tables.item.transfer` |

Sources are the same as for every module: a paid contract that covers the date, or an explicit unrevoked grant. A grant is possible only from lifecycle `internal_preview` onward, and only explicitly. With the current `planned` lifecycle, no source can grant these capabilities.

## 13. Technical dependencies

- QR Core (`core_dependencies`): `identity`, `establishments`, `members`, `tenancy`, `i18n`, `catalog`, `commercial`, `entitlements`, `events`, `orders`, `order_payments`, `notifications`.
- `orders`, `order_payments` and `notifications` are still `planned`, and the outbox dispatcher is part of phase 3. QR Tables cannot ship before Commerce Core.
- Optional integrations: `menu`, `kitchen`, `loyalty`, `analytics`, `pos`, `connect`. No commercial module is required.

## 14. Published events

| Event | Producer in the registry | Payload (registry) | Proposed dedupe key |
|---|---|---|---|
| `TableSessionOpened` | `tables` | `session_id`, `table_id` | session id |
| `TableBillClosed` | `tables` | `session_id`, `total_centavos` | session id |
| `OrderCreated` | `commerce` (channel `table`) | `order_id`, `channel`, `total_centavos`, `item_count`, `catalog_version` | order id |
| `OrderCancelled` | `commerce` | `order_id`, `reason_code`, `cancelled_by` | order id |
| `PaymentConfirmed` | `commerce.payments` | `payment_id`, `order_id`, `amount_centavos`, `method` | payment id |
| `PaymentRefunded` | `commerce.payments` | `payment_id`, `order_id`, `amount_centavos`, `refund_id` | refund id |

The registry lists `producer` as the component that writes the row; the `publishes` list of `tables.json` includes the Commerce Core events written on behalf of Tables operations. QR Tables triggers the order and payment events through Commerce Core, in the same SQLite transaction as the state change (ADR-009). Payloads carry only identifiers and integer amounts. Dedupe keys are design proposals; the registry sets none for planned events.

## 15. Consumed events

| Event | Reaction | Without the producer |
|---|---|---|
| `KitchenTicketReady` | Notify the waiter that the ticket for this order is ready to be served, and highlight it on the board. `order_id` is `string\|null`: manual tickets (null) and orders of other channels are ignored. Idempotent by `event_id`. | QR Kitchen absent: the event never arrives, and staff use the `accepted → ready` fast path and mark items as served manually. |

## 16. HTTP contracts

Routes declared in `tables.json`. All are `planned` and none is available.

| Method | Path | Audience | Behavior (design) |
|---|---|---|---|
| `GET` | `/t/{table_token}` | consumer | Resolves the token to a table. Shows the brand, table label and catalog. Offers the join form while a session is open. `no-store`, `noindex`, never in the sitemap. Unknown or revoked token: localized `404`. |
| `POST` | `/api/tables/v1/sessions/{session_id}/orders` | consumer | Operation `tables.order.place`. JSON `{ idempotency_key, catalog_version, items: [{ product_id, quantity, notes }] }`. Returns `201` with server-computed integer totals. `403` when the device has not joined, or when the session is closed or not `open`. `409` on stale catalog or unavailable item. `429` when rate limited. |
| `POST` | `/api/tables/v1/sessions/{session_id}/waiter-calls` | consumer | Operation `tables.waiter.call`. JSON `{ reason }`. Returns `201`, or `200` when an open call is refreshed. |
| `GET` | `/dashboard/tables` | member | Occupancy board and session management. Conditional `GET` with `ETag`. |

Routes still to add to the contract **(proposed)**:

- `POST /t/{table_token}/join` (join code, which sets the participant cookie);
- `POST /dashboard/tables`, `POST /dashboard/tables/{id}`, `POST /dashboard/tables/{id}/qr/rotate`;
- `POST /dashboard/tables/sessions` (`tables.session.open`), `POST /dashboard/tables/sessions/{id}/transition` (including `tables.bill.close`), `POST /dashboard/tables/sessions/{id}/join-code/rotate`;
- `POST /dashboard/tables/sessions/{id}/orders` (`tables.order.place`), `/transfers` (`tables.item.transfer`), `/splits`;
- `GET /dashboard/tables/{id}/qr.svg` and `.png`.

The `session_id` in a URL is never trusted on its own. It must match the participant binding, or the member's `TenantContext`.

## 17. Integration contracts

- **QR Menu** (optional): `/t/{table_token}` may link to the published `/c/{slug}`. A menu QR Code never opens a session, and a table QR Code never resolves under `/c/`.
- **QR Kitchen** (optional): consumes `OrderAccepted` and `OrderCancelled` to create or cancel tickets (as the `system` role). Commerce Core moves the order `accepted → preparing` on `KitchenTicketStarted` and `preparing → ready` once every ticket is ready. QR Tables consumes `KitchenTicketReady`. The waiter carries out `ready → handed_off` in the `preparation` machine (`kitchen.ticket.progress` includes the waiter role).
- **QR POS** (optional): consumes `TableBillClosed` to reconcile the bill in the open cash session.
- **QR Loyalty** (optional): consumes `OrderCompleted`, `PaymentConfirmed`, `PaymentRefunded` and `OrderCancelled` from table orders.
- **QR Analytics** and **QR Connect** (optional): consume the order and payment events. Connect exposes them through the versioned API and webhooks.
- **Notifications core**: web push or an in-app signal to staff devices for waiter calls and ready tickets.

## 18. Conceptual persistence model

| Table (proposed name) | Key columns and constraints |
|---|---|
| `dining_tables` | `PRIMARY KEY (id)`, `UNIQUE (establishment_id, id)`, `UNIQUE (establishment_id, label)` |
| `table_qr_tokens` | `token_hash UNIQUE`, `(establishment_id, table_id)` composite FK, `revoked_at` |
| `table_sessions` | `state CHECK`, `version`, composite FK to the table, partial unique index on `(establishment_id, table_id) WHERE state IN ('open','bill_requested')` |
| `table_session_join_codes` | `(establishment_id, session_id)` FK, `code_hash`, `issued_at`, `rotated_at` |
| `table_session_participants` | `token_hash UNIQUE`, `(establishment_id, session_id)` FK, `expires_at` |
| `tabs` | `(establishment_id, session_id)` FK (nullable for counter comandas), `state` |
| `waiter_calls` | `(establishment_id, session_id)` FK, `reason`, partial unique index on open calls per reason |
| `bill_splits`, `bill_split_parts` | `amount_centavos INTEGER CHECK (amount_centavos >= 0)`, a check that the parts sum to the split amount, enforced in the application transaction |
| `item_transfers` | Append only: from/to tab and session, `order_item_id`, `quantity`, actor |
| Commerce Core `orders`, `order_items`, `order_payments` | `channel = 'table'`, `table_session_id`, `tab_id` |

All writes use short `BEGIN IMMEDIATE` transactions with an optimistic `version`.

## 19. Security requirements

- **Remote-order prevention**: ordering requires an open session plus an admitted participant. Staff open sessions, and the join code changes with every session. A photographed QR Code alone can only display the catalog. No geolocation is used (`Permissions-Policy: geolocation=()`).
- **Brute force**: join attempts are limited per table token and per IP, with a lockout and constant-time comparison. Codes are stored as hashes.
- **Tokens**: table QR tokens and participant tokens are random, hashed at rest and revocable. The participant cookie is `__Host-`-prefixed, `HttpOnly`, `Secure`, `SameSite=Strict`, and expires when the session closes.
- **CSRF on consumer JSON**: the API requires `Content-Type: application/json`, an `Origin` equal to the public origin, and a SameSite-Strict cookie. Staff routes use `TenantContext` plus `_csrf`.
- **Idempotency**: `idempotency_key` per order deduplicates double taps and retries.
- **Bounds**: 16 KiB request bodies, at most 50 lines per order and at most 99 units per line **(proposed)**, plus a per-session order rate limit.
- **Tenancy**: every query filters on `establishment_id`, with composite foreign keys.
- **CSP**: the strict CSP stays in place. The table page uses external JavaScript and the generated brand CSS only.

## 20. Privacy requirements

- Consumers need no account, name, phone number or email. An optional tab nickname lives only on the tab and is anonymized after the retention period.
- IP addresses are used only transiently, in memory, for rate limiting, and are never persisted.
- Event payloads carry only identifiers and integer amounts.
- Customer identification happens only through QR Loyalty and the `customers` core component, with LGPD export and erasure.

## 21. Performance requirements

- The workload is modest (about 100 tables with one order per minute is under 2 orders per second). The target on 1 vCPU with 1 GB is an order write p95 ≤ 200 ms at a sustained 20 orders per second with `synchronous=FULL`.
- The staff board refreshes through conditional `GET` requests (an `ETag` derived from the establishment's latest session or order change) every 5 s or less. A `304` must cost a single indexed read. Server-sent events can come later inside the same process.
- `/c/{slug}` performance must not change. QR Tables never takes the publication lock and never writes under `published/`.
- The catalog for `/t/{table_token}` is rendered server-side with bounded queries and cached per `(establishment_id, catalog version)`.

## 22. Behavior without other modules

QR Tables is `standalone: true`.

- **Without QR Menu**: the table page renders the QR Core catalog. No menu publication is needed.
- **Without QR Delivery**: unaffected.
- **Without QR Kitchen**: staff (owner, manager, cashier or waiter) move orders `accepted → ready` through the contract fast path for establishments without QR Kitchen.
- **Without QR POS**: waiters, cashiers, managers or owners confirm payments (`pending → confirmed`) with the method they collected (cash, card terminal, PIX).
- **Without QR Loyalty and QR Analytics**: nothing changes.

## 23. Behavior integrated with other modules

| Module | Behavior |
|---|---|
| QR Menu | The table page links to the published `/c/{slug}`. The menu stays read-only and keeps its own QR Code. |
| QR Kitchen | Accepted table orders become station tickets. `KitchenTicketReady` alerts the waiter. The `system` role moves orders to `preparing` on `KitchenTicketStarted` and to `ready` when every ticket is ready. |
| QR POS | `TableBillClosed` enters the open cash session. Payments are reconciled to the centavo. |
| QR Loyalty | Points on completed and paid table orders, reversed automatically on refund. |
| QR Analytics | Table, tab and channel aggregates built from events. |
| QR Connect | Table order events go out through webhooks and the versioned API. |

## 24. Behavior after suspension or cancellation

| Situation | Effect |
|---|---|
| Renewal canceled | The module stays active until the paid period ends. |
| Expired (no capability) | Per `tables.json`: no new sessions (`tables.session.open` is `deny`), no new orders, including in sessions already open (`tables.order.place` is `deny`), and no new waiter calls (`tables.waiter.call` is `deny`). Sessions opened while the capability was active can still be settled and closed (`tables.bill.close`) and have items transferred (`tables.item.transfer`), both `complete_existing`. History stays available. Grace is `payment_grace_days: 0`, so the dashboard must warn before the period ends. |
| Table page after expiry | `/t/{table_token}` shows a neutral "table ordering unavailable" page and links to `/c/{slug}` if a menu is published. |
| Administrative or security suspension | Checked at step 2 of `authorize`. Per the `suspension` rule of `authorization-rules.json`, it blocks only `deny` operations: `tables.session.open`, `tables.order.place` and `tables.waiter.call`. `tables.bill.close` and `tables.item.transfer` (`complete_existing`) stay available, so open sessions can be settled and closed during a suspension. |
| Module `retired` | No capability is granted. Data stays exportable. |

## 25. Acceptance criteria

1. Without an open session and an admitted participant, the consumer order endpoint refuses every request (release gate).
2. Each session issues a new join code. A code from a previous session is rejected (release gate).
3. Splits and transfers keep integer totals exact for any number of parts, including totals that do not divide evenly (release gate).
4. A table never has two non-terminal sessions, even under concurrent open requests.
5. No price sent by the client is ever persisted. Order totals equal the server catalog prices at creation time.
6. Nothing at `/c/{slug}`, its snapshot or its printed QR Code changes when QR Tables is enabled, used or expired.
7. After expiry, open sessions can be closed but cannot receive new orders or waiter calls.
8. During an administrative or security suspension, `tables.session.open`, `tables.order.place` and `tables.waiter.call` are refused while `tables.bill.close` and `tables.item.transfer` succeed.
9. Roles and after-expiry behavior of the five Tables operations match `authorization-rules.json` (parity test).
10. Everything works with QR Tables as the only module contracted, including the `accepted → ready` fast path without QR Kitchen.
11. No request can read or change another establishment's tables, sessions or orders.
12. Every new UI string exists in `pt-BR`, `en` and `es`.

## 26. Test scenarios

| Scenario | Expected |
|---|---|
| Scan a table QR Code with no open session, then submit an order | Catalog shown; order `403` |
| Join with a wrong code repeatedly | Lockout after the limit, constant-time responses |
| Join with the previous session's code | Rejected |
| Rotate the code during a session | Admitted devices keep working; old code blocked for new joins |
| Two staff open the same table at once | One succeeds; the other gets `409` |
| Order with a product sold out after the catalog loaded | `409` with current availability |
| Double-submit with the same `idempotency_key` | One order |
| Equal split of 10000 into 3 parts | 3334 + 3333 + 3333 |
| Item split of 1001 centavos between 2 parts | 501 + 500 |
| Transfer of 2 of 3 units between tabs | Line split, combined session total unchanged |
| Close with an unsettled balance as waiter, then as manager with authorization | Refused; then closed with the authorization recorded |
| Close directly from `open` with a settled balance | Closed; `TableBillClosed` emitted once |
| Waiter accepts a consumer order, marks it ready without QR Kitchen, then completed | `placed → accepted → ready → completed` succeed |
| Waiter confirms a cash payment at the table | `pending → confirmed`; `PaymentConfirmed` emitted |
| Void a session that has items | Refused |
| Expiry during service | New orders and waiter calls `403`; item transfer and close allowed |
| Suspension during service | New sessions, orders and waiter calls refused; close allowed |
| `KitchenTicketReady` delivered twice | One notification |
| `KitchenTicketReady` with a null `order_id` (manual ticket) | Ignored |
| Cross-tenant `session_id` in the URL | `403` or `404`, no data leak |
| Module enabled without QR Menu | Full flow works |

## 27. Commercial launch requirements

Release gates in `tables.json`:

1. Session join codes rotate per session.
2. Remote ordering is blocked outside an open session.
3. Split and transfer keep integer totals exact.

Prerequisites:

- Commerce Core is complete (`orders`, `order_payments`, `notifications`, outbox dispatcher).
- The planned roles `waiter`, `cashier`, `consumer` and `system` exist in `MemberRole` (or the equivalent actor model) and in the members schema, along with a staff invitation flow. `complete_existing` is modelled in `domain::entitlements`, which today has only `Deny` and `Allow`.
- The open decisions in section 11 (staff order progression, split, party transfer, code rotation, void, configuration) are added to `authorization-rules.json`, and the proposed routes in section 16 to `tables.json`, with parity tests.
- The lifecycle follows `planned → development → internal_preview → beta → available`. The price is reviewed before the version is first sold and becomes hash-locked.
- A load test confirms the targets in section 21 on the reference VPS, and an LGPD review is completed.
