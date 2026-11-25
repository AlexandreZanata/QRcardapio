# Commerce Core

## Status

**In progress, Increment 1.** Commerce Core belongs to QR Core, the free shared foundation listed in [`contracts/core/components.json`](../../contracts/core/components.json). This increment implements typed state machines, integer pricing, tenant-scoped catalog options, internal published snapshots, order persistence, and transactional order events. HTTP and UI surfaces, checkout, consumer payments, and the dispatcher are planned.

Implemented foundation it builds on: `src/domain/commercial/`, `src/domain/entitlements/` (`Capability`, `MemberRole`, `Operation`, `OperationalStatus`, `GrantKind`, `resolve`), `src/application/{commercial,entitlements,operator}.rs`, migration `20261124000009_commercial_foundation.sql` and the `domain_events` outbox (append only).

Authoritative contracts: [`contracts/commerce/state-machines.json`](../../contracts/commerce/state-machines.json), [`contracts/events/registry.json`](../../contracts/events/registry.json), [`contracts/entitlements/authorization-rules.json`](../../contracts/entitlements/authorization-rules.json), [`contracts/entitlements/capabilities.json`](../../contracts/entitlements/capabilities.json). When this document and a contract disagree, the contract wins and this document is fixed. See also [EVENT_CONTRACTS.md](EVENT_CONTRACTS.md) and the [module catalog](../business/MODULE_CATALOG.md). First consumer: [QR Delivery](../modules/DELIVERY.md).

## 1. Purpose and boundaries

Commerce Core is the single order engine shared by every selling channel. Modules (QR Delivery, QR Tables, QR POS, QR Connect) provide channel UX and channel rules; they never own a second order engine, a second price calculator or a second payment ledger.

| Core component | Status today | Commerce Core deliverable |
|---|---|---|
| `catalog` | partial (catalog rows, options and internal published snapshots) | Variations, add-on groups, add-on options, published catalog snapshots per channel |
| `business_hours` | planned | Weekly hours, date exceptions, holiday overrides, pause switch |
| `commercial_settings` | planned | Service modes, minimum order, fees, acceptance window, checkout settings |
| `customers`, `addresses` | planned | Establishment-scoped consumer records with LGPD export and erasure |
| `orders` | partial (aggregate, item snapshots, numbering, transition executor) | The `Order` aggregate and its state machine |
| `order_payments` | planned | Consumer-to-establishment payments, separate from SaaS billing |
| `notifications` | planned | Email and web push sent by outbox consumers, per-establishment templates |
| `events` | partial (append implemented) | The outbox dispatcher (section 11) |

Rules that keep the boundary:

- Modules depend only on core components, never on another commercial module (`contracts/core/components.json`). Cross-module effects travel only through outbox events consumed idempotently.
- Dependency direction stays `http -> application -> domain` and `infrastructure -> domain`. SQL lives only in `src/infrastructure/database/queries/<entity>.rs`.
- Module aggregates with their own machines (`table_session`, `kitchen_ticket_item`, `delivery`, `cash_session`) are persisted by their modules. Commerce Core owns the machine definitions and the shared transition executor.

Current and planned code layout (one responsibility per file, ~50–350 lines):

```text
src/domain/commerce/        state_machine.rs (contract loader + parity), order.rs, order_payment.rs,
                            pricing.rs, discount.rs, checkout.rs, catalog_options.rs,
                            catalog_snapshot.rs, customer.rs, address.rs, business_hours.rs, channel.rs
src/application/commerce/   checkout.rs, order_transitions.rs, payments.rs, refunds.rs,
                            customers.rs, privacy.rs, catalog_publication.rs
src/infrastructure/events/  dispatcher.rs, consumers.rs, backoff.rs
src/infrastructure/database/queries/
                            orders.rs, order_items.rs, order_payments.rs, payment_refunds.rs,
                            checkout_attempts.rs, customers.rs, customer_addresses.rs,
                            catalog_snapshots.rs, product_variations.rs, addon_groups.rs,
                            addon_options.rs, business_hours.rs, event_consumers.rs
```

## 2. One order engine for every channel

Every sale is an `Order` row with a `channel`. Channels select the creating operation and capability; the aggregate, its machine, its pricing and its payments are identical.

| `channel` | Created by | Creating operation (authorization-rules) | Capability |
|---|---|---|---|
| `delivery` | QR Delivery storefront | `delivery.order.place` | `delivery.order.create` |
| `pickup` | QR Delivery storefront | `delivery.order.place` | `delivery.order.create` |
| `table` | QR Tables session | `tables.order.place` | `tables.order.create` |
| `counter` | QR POS | `pos.sale.register` | `pos.sale.create` |
| `manual` | Staff entry from an order center (phone orders) | none defined yet | open decision |
| `external` | QR Connect API | `connect.api.call` | `connect.api.use` |

`fulfillment` (`delivery`, `pickup`, `in_venue`) is stored separately from `channel` because an `external` order can be delivered or picked up; `OrderReady` carries it in its payload.

Operations that expose later transitions (roles, capability and `after_expiry` come from `authorization-rules.json`):

| Operation | Transitions | Capability | After expiry |
|---|---|---|---|
| `delivery.order.progress` | Staff order transitions and pay-on-delivery confirmation for delivery and pickup orders | `delivery.order.manage` | `complete_existing` |
| `delivery.order.cancel` | Consumer `placed → cancelled` | none | `allow` |
| `delivery.order.refund` | `order_payment` refunds | none | `allow` |
| `pos.sale.cancel` | Counter sale cancellation | none | `allow` |
| `tables.bill.close` | `table_session` → `closed` | `tables.bill.manage` | `complete_existing` |
| `kitchen.ticket.progress` | Kitchen item transitions; order moves follow through `system` | `kitchen.ticket.manage` | `complete_existing` |
| `fleet.delivery.complete` | `delivery` `picked_up → delivered` | none | `allow` |

Suspension (top-level `suspension` rule) blocks only `deny` operations, so every row above keeps working for existing orders while an establishment is suspended; only creating operations (`delivery.order.place`, `tables.order.place`, `pos.sale.register`, `connect.api.call`) stop.

## 3. State machines

Six independent machines are defined in `contracts/commerce/state-machines.json`. `domain::commerce::state_machine` embeds the JSON with `include_str!`, parses the definitions once into typed states and actors, and exposes transition lookup. `tests/commerce_contracts.rs` checks every combination of machine, source state, target state and actor, plus the initial and terminal states and event names. Increment 1 persists only the `order` machine. **A transition in one machine never implies a transition in another.**

| Fact | Never implies |
|---|---|
| Delivery `delivered` | Order `completed` (only the `DeliveryCompleted` consumer moves the order) or payment `confirmed` |
| Order `completed` | Payment `confirmed`; a completed pay-on-delivery order can still owe money |
| Payment `confirmed` | Order `accepted` or preparation finished |
| Order `cancelled` / `rejected` | Payment `refunded` / `cancelled` (a compensation consumer does that) |
| Kitchen item `ready` | Order `ready` (only when every ticket of the order is ready) |

Actors: `owner`, `manager`, `cashier`, `waiter`, `kitchen_operator`, `courier` are establishment roles; `consumer` is an anonymous storefront or table guest bound to the order by its unguessable public id; `system` is the planned platform-scoped role of `authorization-rules.json`: a timer or idempotent outbox consumer acting for one establishment, which runs only when the triggering event was produced under a valid operation and re-checks the consumer module's own capability; `gateway` is an authenticated payment-provider webhook for the establishment's own financial account. The effective actor set of a transition is the intersection of the contract `actors` with the roles of the module operation that exposes it, after `authorize(...)` passes. Any `(from, to)` pair not listed below is forbidden and returns `409`.

### 3.1 `order` (aggregate `order`, initial `placed`, terminal `rejected`, `completed`, `cancelled`)

| From → To | Actors | Event | Preconditions and side effects |
|---|---|---|---|
| `placed` → `accepted` | owner, manager, cashier, waiter | `OrderAccepted` | Within acceptance window; sets `accepted_at`, `promised_at` |
| `placed` → `rejected` | owner, manager, cashier, system | `OrderRejected` | `reason_code` required; system only when the acceptance window expires |
| `placed` → `cancelled` | consumer | `OrderCancelled` | Only while `placed` (`delivery.order.cancel`); `cancelled_by = consumer` |
| `accepted` → `preparing` | owner, manager, kitchen_operator, system | `OrderPreparing` | System on `KitchenTicketStarted` |
| `preparing` → `ready` | owner, manager, kitchen_operator, system | `OrderReady` | System when every kitchen ticket of the order emitted `KitchenTicketReady` (the ticket set is known from `KitchenTicketCreated`) |
| `accepted` → `ready` | owner, manager, cashier, waiter | `OrderReady` | Fast path for establishments without QR Kitchen, including counter (POS) sales |
| `ready` → `completed` | owner, manager, cashier, waiter, courier, system | `OrderCompleted` | System on `DeliveryCompleted` or pickup confirmation |
| `accepted` → `cancelled` | owner, manager | `OrderCancelled` | Compensations: refund confirmed payments, cancel kitchen tickets, cancel pending deliveries |
| `preparing` → `cancelled` | owner, manager | `OrderCancelled` | Same compensations as above |
| `ready` → `cancelled` | owner, manager | `OrderCancelled` | Refund confirmed payments, cancel pending deliveries (no kitchen compensation in the contract; see section 16) |

Every transition into `rejected` or `cancelled` releases the order's coupon redemption in the same transaction (coupons are order-owned rows, not a state machine).

Automatic loyalty points are credited only after `OrderCompleted`, so cancellation never needs a loyalty compensation. A refund after completion reverses points through `PaymentRefunded`.

Explicitly forbidden examples: `completed → cancelled` (money goes back through `order_payment` refunds), `rejected → accepted`, `placed → preparing`, `ready → preparing`, consumer cancellation after `accepted`.

### 3.2 `order_payment` (aggregate `order_payment`, initial `pending`, terminal `failed`, `expired`, `cancelled`, `refunded`, `chargeback_lost`)

| From → To | Actors | Event | Notes |
|---|---|---|---|
| `pending` → `confirmed` | gateway, owner, manager, cashier, waiter, courier | `PaymentConfirmed` | Pay-on-delivery confirmed by staff with the collected method; online methods only by authenticated gateway webhooks |
| `pending` → `failed` | gateway | none | |
| `pending` → `expired` | system | none | Pix dinâmico expiry |
| `pending` → `cancelled` | system, owner, manager | none | System on `OrderRejected` / `OrderCancelled` |
| `confirmed` → `partially_refunded` | owner, manager, gateway | `PaymentRefunded` | |
| `partially_refunded` → `partially_refunded` | owner, manager, gateway | `PaymentRefunded` | Further partial refund |
| `confirmed` → `refunded` | owner, manager, gateway | `PaymentRefunded` | |
| `partially_refunded` → `refunded` | owner, manager, gateway | `PaymentRefunded` | |
| `confirmed` → `chargeback_open` | gateway | none | |
| `chargeback_open` → `confirmed` | gateway | none | Dispute won |
| `chargeback_open` → `chargeback_lost` | gateway | `PaymentRefunded` | Dispute lost; `refund_id` is the chargeback reference |
| `partially_refunded` → `chargeback_open` | gateway | none | Dispute on a partially refunded payment |

Invariants: `sum(refunds) <= confirmed amount`; amounts are integer centavos; gateway events are deduplicated by `(environment, provider event id)` and ordered by provider timestamp (a stale event older than the last applied one for the same payment is recorded and ignored; at equal timestamps a reversal wins, mirroring [BILLING.md](BILLING.md)).

### 3.3 `preparation` (aggregate `kitchen_ticket_item`, initial `queued`, terminal `handed_off`, `cancelled`) — owned by QR Kitchen

| From → To | Actors | Event |
|---|---|---|
| `queued` → `in_progress` | kitchen_operator, owner, manager | none |
| `in_progress` → `ready` | kitchen_operator, owner, manager | none |
| `ready` → `handed_off` | kitchen_operator, waiter, courier, owner, manager | none |
| `queued` → `cancelled` | owner, manager, system | none |
| `in_progress` → `cancelled` | owner, manager, system | none |
| `ready` → `cancelled` | owner, manager, system | none (order cancelled after the item was ready but before hand-off) |

Ticket-level `KitchenTicketCreated`, `KitchenTicketStarted` and `KitchenTicketReady` aggregate item states; their `order_id` is `null` for manual tickets. `handed_off` items are never cancelled. Hand-off is exposed through `kitchen.ticket.progress`, whose roles include `waiter` and `courier`.

### 3.4 `delivery` (aggregate `delivery`, initial `pending`, terminal `delivered`, `returned`, `cancelled`) — owned by QR Fleet

| From → To | Actors | Event | Notes |
|---|---|---|---|
| `pending` → `assigned` | owner, manager | `DeliveryAssigned` | |
| `assigned` → `pending` | owner, manager, courier | none | Unassign or courier declines |
| `assigned` → `picked_up` | courier | none | |
| `picked_up` → `delivered` | courier, owner, manager | `DeliveryCompleted` | Proof of delivery when configured; owners and managers may record on the courier's behalf |
| `picked_up` → `failed` | courier | `DeliveryFailed` | |
| `failed` → `assigned` | owner, manager | `DeliveryAssigned` | Retry |
| `failed` → `returned` | courier, owner, manager | none | |
| `pending` → `cancelled` | owner, manager, system | none | |
| `assigned` → `cancelled` | owner, manager, system | none | |

`DeliveryCreated` is emitted at creation. `DeliveryCreated`, `DeliveryCompleted` and `DeliveryFailed` carry `order_id`, which is `null` for deliveries registered outside QR Cardápio (`fleet.delivery.register`). A `picked_up` delivery cannot be cancelled; it must go through `failed → returned`.

### 3.5 `table_session` (initial `open`, terminal `closed`, `voided`) — owned by QR Tables

| From → To | Actors | Event | Notes |
|---|---|---|---|
| `open` → `bill_requested` | consumer, waiter, owner, manager | none | |
| `bill_requested` → `open` | waiter, owner, manager | none | |
| `bill_requested` → `closed` | waiter, cashier, owner, manager | `TableBillClosed` | Requires settled balance or explicit pending-balance authorization |
| `open` → `closed` | waiter, cashier, owner, manager | `TableBillClosed` | Requires settled balance or explicit pending-balance authorization |
| `open` → `voided` | owner, manager | none | Only sessions without items |

`TableSessionOpened` is emitted at creation.

### 3.6 `cash_session` (initial `open`, terminal `closed`) — owned by QR POS

| From → To | Actors | Event |
|---|---|---|
| `open` → `counting` | cashier, owner, manager | none |
| `counting` → `open` | owner, manager | none |
| `counting` → `closed` | cashier, owner, manager | `CashRegisterClosed` |

`CashRegisterOpened` is emitted at creation (`pos.cash_register.open`). Withdrawals (sangria) and supplies (suprimento) are immutable movements while `open` (`pos.cash_movement.register`). Counting and closing use `pos.cash_register.close`, which needs no capability.

### 3.7 Compensations

Compensations listed on order cancellation run as idempotent outbox consumers of `OrderCancelled` (and `OrderRejected` where relevant) acting as the `system` role, never inline in the cancelling transaction.

| Compensation | Consumer | Behavior |
|---|---|---|
| Refund confirmed payments | `commerce.payments` | Online payment: provider refund with idempotency key = refund public id. Offline payment (cash, card machine): marks a `refund_due` item for staff; staff records it via the module refund operation (for example `delivery.order.refund`, no capability, `allow`), which emits `PaymentRefunded`. Pending payments move to `cancelled` (system). |
| Cancel kitchen tickets | `kitchen` | `queued`, `in_progress` and `ready` (not yet handed off) items → `cancelled`; `handed_off` items are unchanged |
| Cancel pending deliveries | `fleet` | `pending`/`assigned` → `cancelled`; `picked_up` raises an incident for staff |

There is no loyalty compensation (section 3.1). QR Loyalty consumes `OrderCancelled` only to skip crediting a cancelled order and `PaymentRefunded` to reverse points already credited.

## 4. Concurrency control

- Every write runs in a `BEGIN IMMEDIATE` transaction (`begin_immediate()`, `busy_timeout = 5000`), so writers queue on SQLite's lock instead of failing on snapshot upgrade.
- Every state-machine aggregate row carries `state` and `version INTEGER NOT NULL DEFAULT 1`. A transition is exactly: `UPDATE <table> SET state = ?, version = version + 1, <timestamps> WHERE id = ? AND establishment_id = ? AND state = <expected> AND version = ?`. Zero rows affected returns `409` with the current state and version (contract rule).
- In the same transaction: append a row to the aggregate's transition log, write side-effect rows owned by the same order (for example the coupon redemption release), and append the outbox event. The HTTP response is sent only after commit.
- Dashboard forms carry `expected_version` next to `_csrf`; two staff members accepting the same order produce exactly one `OrderAccepted` and one `409`.
- Planned system actors (event consumers, timers) treat "already in the target state or a later state" as success: they record the event receipt and change nothing, so replays and races with staff never surface errors.
- Long work (rendering, provider HTTP calls, notification sends) never runs inside a write transaction.

## 5. Shared catalog evolution

Increment 1 adds options to the shared draft catalog as new tables (expand-only, ADR-010). The existing QR Menu publication path continues to render from categories and products. The byte-for-byte republish regression checks the public menu and `republish-all`.

| Concept | Rule |
|---|---|
| Variation (`product_variations`) | One mutually exclusive axis per product (for example size). Each variation has an absolute `price_centavos` that replaces the product price. A product with at least one active variation requires exactly one selection. |
| Add-on group (`addon_groups`) | Establishment-scoped, reusable, translated (`name`, `name_en`, `name_es`), `min_select`/`max_select` with `0 <= min <= max <= 20`; `min_select > 0` means required. Linked to products through `product_addon_groups (product_id, group_id, position)`. |
| Add-on option (`addon_options`) | Belongs to one group; `price_centavos >= 0` (removals such as "sem cebola" are free options); `max_quantity >= 1`; `is_available`. |
| Tenancy | Every row has `establishment_id`; linking verifies product, group and option belong to the same establishment. |
| Draft vs published | Edits mark `has_unpublished_changes = 1`. Ordering channels never read draft tables (section 8). |
| QR Menu | May render variations and options read-only in the static menu on next publication. |

## 6. Customers and addresses

- `customers` are scoped to one establishment; the same person ordering from two restaurants is two unrelated records. There is no cross-establishment consumer identity and no consumer password.
- Identity key: `(establishment_id, phone_e164)`, unique among non-erased rows. Name is required (1–80 chars); email is optional and unused in the MVP.
- `customer_addresses` belong to one customer and one establishment and reference a delivery zone of the same establishment. Orders copy the address into `order_delivery_details`, so later edits never change a placed order.
- The establishment is the LGPD controller of its consumers' data; QR Cardápio is the processor. Event payloads never contain names, phones, emails or addresses (registry privacy rule), so erasure never rewrites `domain_events`.
- **Export**: the owner (`data.export`, a QR Core operation: owner only, no capability, `allow`, never tied to a module lifecycle and available during suspension) downloads one JSON document with the profile, addresses and that customer's orders.
- **Erasure**: in one transaction, PII columns in `customers`, `customer_addresses` and `order_delivery_details` are nulled, `erased_at` is set and addresses are deleted. **Retention exceptions**: order amounts, items, dates, states, payment and refund identifiers are kept de-identified for legal and accounting obligations; the delivery zone and neighborhood label are kept for reports. Erased data disappears from backups as they rotate out of the runbook retention window.

## 7. Order financial invariants

1. **Backend pricing only.** Unit prices come from the published catalog snapshot. Client-sent prices are never trusted; the client sends only ids, quantities and `expected_total_centavos`, used solely to detect changes.
2. **Integer money.** All amounts are `i64` centavos with checked arithmetic (`checked_mul`, `checked_add`); overflow or a total above the order cap of 1,000,000 centavos is a validation error. See [ADR-012](../decisions/ADR-012-commerce-core-increment-1.md). JSON amounts are integers; no floats anywhere, including discount math.
3. **Line math.** `unit = (variation or product price) + Σ(option price × option quantity)`; `line_total = unit × quantity`; `items_subtotal = Σ line_total`.
4. **Deterministic discounts.** At most one coupon per order. Percentage: `round_half_up(items_subtotal × bps / 10000)`; fixed: `min(amount, items_subtotal)`. Discounts never apply to the delivery fee and never make a total negative.
5. **Backend delivery fee.** Derived from the selected zone of the same establishment and the order subtotal; a client-sent fee is ignored.
6. **Total identity.** `total = items_subtotal − discount + delivery_fee`, enforced by a `CHECK` constraint and by the domain constructor.
7. **Availability.** Every product, variation and option must be present and available in the snapshot used for pricing.
8. **Tenant isolation.** Every referenced id must belong to the establishment resolved server-side from the slug or session; foreign ids return the same error as unknown ids.
9. **Add-on validation.** Options belong to a group linked to the product; per-group selection count within `min_select..=max_select`; per-option quantity within `1..=max_quantity`; no duplicate options.
10. **Idempotent checkout.** `(establishment_id, idempotency_key)` is unique. The same key with the same request hash returns the original order; a different hash returns `422`.
11. **Immutable snapshots.** `order_items` and `order_item_options` copy names (all locales), unit prices and quantities at placement. No later catalog edit changes them.
12. **Payment bounds.** `Σ confirmed payments − Σ refunds <= order total`, and refunds per payment never exceed its confirmed amount.

## 8. Checkout against the published snapshot

Increment 1 provides an internal publication transaction for `catalog_snapshots`. The calling operation resolves the catalog channel server-side. The transaction promotes only the channel’s tenant-scoped visible catalog rows into an immutable canonical JSON payload with SHA-256 and an incremental version. The checkout and storefront increments will wire this publication boundary into authorized operations and add matching static files. Checkout reads only the **current published snapshot** of that channel, never draft tables. Sold-out toggles from an order center create a new snapshot version derived from the current one with only availability changed, so unpublished draft edits never leak.

1. Resolve the establishment from the slug server-side and call `authorize(...)` for the channel operation (lifecycle, operational state, role, enforcement, capability).
2. Validate business hours, pause switch, service mode, zone and minimum order.
3. Price the cart against the current snapshot (section 7).
4. If any line is unavailable or the computed total differs from `expected_total_centavos`, return `409` with code `commerce.checkout_repriced`, the current `catalog_version` and a fresh quote. Nothing is persisted; the client must show the differences and get explicit reconfirmation.
5. `BEGIN IMMEDIATE`: re-check that the snapshot version is still current (else back to step 4), insert `checkout_attempts`, upsert the customer, consume the coupon with a conditional `UPDATE`, insert order, items, options, delivery details and a `pending` payment, append `OrderCreated` (`dedupe_key` = order public id), commit.
6. Respond `201` with the order public id and tracking path.

## 9. SaaS billing vs Commerce Payments

| Aspect | SaaS billing (implemented) | Commerce Payments (planned) |
|---|---|---|
| Who pays whom | Establishment pays the platform | Consumer pays the establishment |
| Provider account | Platform's central Asaas account | Per-establishment financial account; never the platform's central account |
| Credentials | Platform environment configuration | Per establishment, encrypted at rest, separate webhook secrets |
| Code | `application::billing`, `application::commercial` | `application::commerce::payments` with distinct credential types |
| Tables | `billing_*`, `commercial_contract*`, affiliate ledger | `order_payments`, `payment_refunds`, `payment_gateway_events`, `financial_accounts` |
| State | Coverage and entitlements | `order_payment` machine |
| Platform take | Subscription price | None: no commission, no split, no platform balance |

They never share credentials, beneficiaries, state, webhook endpoints or ledgers. A test asserts that no commerce payment can be created against the platform account.

- **MVP (pay on delivery)**: methods `cash` (with optional `change_for_centavos >= total`), `card_on_delivery`, `pix_on_delivery` (the establishment's own key shown as text). Staff confirm collection; no money touches the platform.
- **Future online payments**: each establishment connects a financial account it owns (for example a provider subaccount under its own KYC, or its own provider credentials). Pix dinâmico charges carry the exact outstanding amount and expire (`pending → expired`); confirmation only by authenticated webhook, never by the browser. Card payments use provider-hosted or tokenized forms so card data never reaches our servers. Daily reconciliation per financial account compares provider records with `order_payments` and opens issues on mismatch; it never silently adjusts amounts.

## 10. Outbox usage

Implemented today: `domain_events` with the registry envelope, a global `seq` (ordering is guaranteed per aggregate only, through the `(aggregate_type, aggregate_id, seq)` index) and a nullable unique `dedupe_key`. No dispatcher-only column exists yet (ADR-009). The existing SaaS and QR Menu events remain implemented, including `CatalogPublished:menu:{establishment_id}:{version}`. Commerce Core appends `OrderCreated`, `OrderAccepted`, `OrderRejected`, `OrderPreparing`, `OrderReady`, `OrderCompleted`, and `OrderCancelled`.

Commerce Core appends every machine event in the transaction of its transition, with optional typed correlation and causation IDs supplied by the calling operation. Checkout attempts and dispatcher receipts arrive in later increments. The registry `producer` names the component that writes the row (`commerce`, `commerce.payments`); module contracts still list these events under `publishes` when their operations cause them. Point amounts in loyalty events are positive magnitudes and the event type gives the direction. Event-level details live in [EVENT_CONTRACTS.md](EVENT_CONTRACTS.md).

## 11. Planned dispatcher

- **Single in-process task** (Tokio) polling `domain_events` by `seq` every 250 ms when idle and immediately after a commit notification; batch size 200. No broker.
- **Per-consumer checkpoints**: `event_consumer_checkpoints(consumer, last_seq)`. Each consumer reads events of its subscribed types with `seq > last_seq` in `seq` order.
- **Idempotent consumers**: the handler's writes, an `event_consumer_receipts(consumer, event_id)` row and the checkpoint advance commit in one `BEGIN IMMEDIATE` transaction, so a crash replays without duplicate effects. Handlers with external effects (push, email, provider calls) store an outbound record keyed by `event_id` before calling out.
- **Authorization**: consumers act as the planned `system` role for the event's establishment. They run only for events produced under a valid operation and re-check the consumer module's own capability before creating new work (for example Kitchen tickets from `OrderAccepted`). Compensations of existing work (cancelling tickets or deliveries, reversing points) follow the `allow` and `complete_existing` rules and keep running after expiry.
- **Order engine consumers** (registered today under the channel module's `consumes`, for example `delivery.json`): `KitchenTicketCreated`, `KitchenTicketStarted` and `KitchenTicketReady` drive `accepted → preparing → ready`; `DeliveryCompleted` drives `ready → completed`; events with `order_id = null` are ignored.
- **Retries**: exponential backoff with jitter (1 s, 5 s, 30 s, 2 min, 10 min, 1 h), up to 10 attempts, recorded in `event_consumer_failures`.
- **Dead letter**: after the last attempt the event is marked `dead` for that consumer, later events of the **same aggregate** are parked for that consumer to preserve per-aggregate order, and other aggregates continue. Platform admins can replay or discard from `/admin` (planned).
- **`published_at`** (new nullable column, added by an expand-only migration with the dispatcher): set when every subscribed consumer's checkpoint has passed the event (low-water mark), or immediately when nobody subscribes.
- **Observability**: per-consumer lag (`max(seq) − last_seq`), dead-letter count and oldest pending age exposed to `/admin` and `/readyz` warnings.

## 12. Conceptual SQLite persistence model

The catalog and order tables shown below scope tenant rows and parent references by `establishment_id`. Increment 1 migrations cover options, snapshots, orders, order items and options, and the order transition log. Remaining rows in the table describe planned DDL. See [DATABASE.md](DATABASE.md#commerce-core).

| Table | Key columns | Constraints and indexes |
|---|---|---|
| `product_variations` | product_id, name (+en/es), price_centavos, position, is_available | Index `(establishment_id, product_id, position)` |
| `addon_groups` | name (+en/es), min_select, max_select, position | `CHECK (0 <= min_select AND min_select <= max_select AND max_select <= 20)` |
| `addon_options` | group_id, name (+en/es), price_centavos, max_quantity, is_available | Index `(establishment_id, group_id, position)` |
| `product_addon_groups` | product_id, group_id, position | PK `(product_id, group_id)` |
| `catalog_snapshots` | channel, version, content_hash, snapshot_json, published_at, is_current | UNIQUE `(establishment_id, channel, version)`; partial UNIQUE `(establishment_id, channel) WHERE is_current = 1` |
| `business_hours`, `business_hour_exceptions` | weekday or date, opens_at_min, closes_at_min, closed | Index `(establishment_id, weekday)` / `(establishment_id, date)` |
| `commercial_settings` | service modes, min_order_centavos, acceptance_window_s, timezone, paused | PK `establishment_id` |
| `customers` | public_id, name, phone_e164, erased_at, last_order_at | Partial UNIQUE `(establishment_id, phone_e164) WHERE erased_at IS NULL` |
| `customer_addresses` | customer_id, zone_id, street, number, complement, neighborhood, city, uf, cep, reference | Index `(establishment_id, customer_id)` |
| `checkout_attempts` | idempotency_key, request_hash, order_id, outcome, created_at | UNIQUE `(establishment_id, idempotency_key)`; pruned after 7 days |
| `orders` | public_id (128-bit random), order_number, business_date, channel, fulfillment, state, version, catalog_version, customer_id, items_subtotal/discount/delivery_fee/total_centavos, coupon_code, reason_code, placed/accepted/promised/ready/completed/cancelled_at | UNIQUE `public_id`; UNIQUE `(establishment_id, business_date, order_number)`; index `(establishment_id, state, placed_at DESC)`; total `CHECK` |
| `order_items` | order_id, product_id, variation_id, names (pt-BR/en/es), unit_price_centavos, quantity, line_total_centavos | Index `(order_id)`; `CHECK quantity BETWEEN 1 AND 99` |
| `order_item_options` | order_item_id, option_id, group name, option name, unit_price_centavos, quantity | Index `(order_item_id)` |
| `order_delivery_details` | order_id, recipient name, phone, address fields, zone_id, neighborhood_label, erased_at | PK `order_id` |
| `order_transitions` | aggregate (order/payment), from_state, to_state, actor_kind, actor_member_id, reason_code, at | Append-only; index `(establishment_id, order_id, at)` |
| `order_payments` | public_id, order_id, method, state, version, amount_centavos, refunded_centavos, change_for_centavos, provider, environment, provider_payment_id, financial_account_id, expires_at | `CHECK refunded_centavos <= amount_centavos`; partial UNIQUE `(environment, provider, provider_payment_id)` |
| `payment_refunds` | payment_id, amount_centavos, reason_code, provider_refund_id | Append-only |
| `payment_gateway_events` | environment, provider, provider_event_id, provider_timestamp, payment_id | UNIQUE `(environment, provider, provider_event_id)` |
| `financial_accounts` (future) | provider, environment, account ref, encrypted credentials, status | UNIQUE `(establishment_id, provider, environment)` |
| `event_consumer_checkpoints` | consumer, last_seq | PK `consumer` |
| `event_consumer_receipts` | consumer, event_id, processed_at | PK `(consumer, event_id)` |
| `event_consumer_failures` | consumer, event_id, seq, aggregate, attempts, next_attempt_at, status | Index `(consumer, status, next_attempt_at)` |

`domain_events` additionally gets the `published_at` column and a partial index `WHERE published_at IS NULL` for the dispatcher.

## 13. Benchmarks required before launch

The static-menu results in [PERFORMANCE.md](../performance/PERFORMANCE.md) (about 28k req/s for `/c/{slug}` with zero queries) **do not prove order capacity**: orders are durable writes with `synchronous=FULL`. The closest existing measurement is 1,000 unique Asaas webhooks at 201 req/s, p95 165 ms, on one CPU. Each order costs roughly seven write transactions over its life (checkout, four or five transitions, payment confirmation).

| Benchmark | Workload (1 vCPU pinned, `synchronous=FULL`, release build) | Proposed pass criteria |
|---|---|---|
| Concurrent order writes | 200 establishments, 64 concurrent consumers, 10 min, 10% retries with the same idempotency key, concurrent staff transitions | ≥ 30 orders/s sustained; checkout p95 ≤ 300 ms, p99 ≤ 1 s; 0 5xx; 0 `SQLITE_BUSY` surfaced; 0 duplicate orders; totals reconcile to the centavo |
| Mixed read/write | Same as above plus static menu and storefront traffic | Static p95 within 2× the published baseline |
| Event throughput | 100k-event backlog, no-op consumers; then steady state | Drain ≥ 1,000 events/s; steady-state lag ≤ 2 s; crash mid-batch causes no duplicate effect |
| Notifications | 1,000 web push sends per minute with a fake endpoint | Bounded concurrency; checkout latency unaffected |
| Payments | 1,000 unique gateway events, then full redelivery | ≥ 150 events/s, p95 ≤ 250 ms; redelivery is a no-op |

Results go into `docs/performance/PERFORMANCE.md` with the script (`scripts/bench_commerce.sh`, planned).

## 14. Phase 3 completion criteria

1. Rust parity tests pass for all six machines in `contracts/commerce/state-machines.json` (states, transitions, actors, events, terminal states).
2. `order` and `order_payment` are implemented with optimistic concurrency; a concurrency test yields exactly one success and one `409`.
3. Variations and add-ons are editable in the dashboard with pt-BR/en/es parity, strict CSP and tenant cross-verification; `republish-all` output is byte-identical for menus without options.
4. Catalog snapshots, checkout with idempotency and the repricing response are covered by unit and property tests on pricing and rounding.
5. Customers and addresses support LGPD export and erasure with retention exceptions tested.
6. The dispatcher has checkpoints, retries, dead letter and per-aggregate parking, with a crash-recovery test.
7. Every emitted event is `implemented` in `contracts/events/registry.json` with a JSON Schema, and payloads contain no PII.
8. Benchmarks in section 13 are recorded and meet the agreed thresholds.
9. All migrations are expand-only and the previous binary starts on the migrated database.
10. No module becomes sellable as a side effect; QR Delivery stays `planned` or `development`.
11. `cargo fmt --check`, `cargo clippy --all-targets --all-features -- -D warnings`, `cargo test --locked` pass; docs (`DATABASE.md`, `HTTP.md`, `SECURITY.md`, `PROJECT_STATE.md`) are updated.

## 15. Ordered task list to start Commerce Core

Items 1–4, 6 and 9 are implemented in Increment 1. Item 17 is implemented for events emitted in this increment.

1. Create a `feat/` branch per [BRANCH_POLICY.md](../agents/BRANCH_POLICY.md).
2. `domain::commerce::state_machine`: load `state-machines.json` with `include_str!`, typed states and actors, `can_transition(from, to, actor)`; parity test `tests/commerce_contracts.rs`.
3. `domain::commerce::pricing` and `discount`: line math, half-up basis-point rounding, checked arithmetic, order cap; unit and property tests.
4. Expand migration for options; query files `product_variations.rs`, `addon_groups.rs`, `addon_options.rs` with tenant cross-checks.
5. Dashboard UI for variations and add-on groups (Askama, `data-i18n`, three catalogs, CSRF, no inline script or style); optional read-only rendering in the static menu.
6. `catalog_snapshots` builder producing canonical JSON with content hash and version; availability-only derived versions.
7. `business_hours` and `commercial_settings` core components with timezone-aware "open now" evaluation.
8. `customers` and `customer_addresses` with phone normalization, export and erasure services.
9. Orders schema, `Order` aggregate, transition executor (expected-state UPDATE, transition log, event append) and `order_number` per business date.
10. Checkout service: idempotency table, revalidation, repricing `409`, coupon consumption hook.
11. `order_payments` for offline methods: create, confirm, cancel, refund, invariants.
12. Event dispatcher with checkpoints, receipts, backoff, dead letter, admin view and `/readyz` lag warning.
13. Compensation consumers: payments (`refund_due`, pending cancellation) and the acceptance-window timer.
14. Notifications component: outbound records, web push (VAPID) sender as a dispatcher consumer; run `cargo audit` for the new dependency.
15. Add planned operations to `domain::entitlements::Operation` only as they become implemented; add `AfterExpiry::CompleteExisting` (the code has only `Deny` and `Allow`), the storefront-scoped `consumer` actor and the platform-scoped `system` role, keeping `OperationalStatus::blocks` limited to `deny` operations.
16. Benchmark script and results (section 13).
17. Register emitted events as `implemented` with JSON Schemas and keep [EVENT_CONTRACTS.md](EVENT_CONTRACTS.md) (referenced by ADR-009) in sync.
18. Update `DATABASE.md`, `HTTP.md`, `SECURITY.md`, `PROJECT_STATE.md`; open the PR.

### Next increments

**Increment 2**

1. Add owner and manager dashboard editing for variations, reusable add-on groups, product links, and option availability. Test session and role checks, CSRF, tenant ownership, and `pt-BR`/`en`/`es` parity under the existing CSP.
2. Implement weekly hours, date exceptions, holiday overrides, pause, service modes, minimum order and acceptance settings. Define behavior for overnight intervals and their business dates.
3. Add tenant-scoped customers and addresses with phone normalization, export, erasure and tested retention exceptions.

**Increment 3**

1. Add checkout with idempotency, snapshot revalidation, repricing, and atomic order creation. Cover same-key retries, changed request hashes, and concurrent checkout against an updated snapshot.
2. Implement the offline consumer payment ledger, including independent confirmation, cancellation and refunds, with invariants for exact balances.
3. Add outbox checkpoints, receipts, retries, dead letter and per-aggregate parking. Add acceptance expiry and payment compensations with crash recovery tests.
4. Add the lifecycle and operation rules required by the implemented entry points, complete the Phase 3 notification component and permission coverage, and record section 13 benchmarks and rollback evidence.
5. Close the section 14 gates before starting Phase 4. QR Delivery then wires the shared foundation into the storefront, cart, delivery and pickup settings, tracking and order center. Asaas homologation remains required before `beta` or `available`.

## 16. Decisions still open

- Gating of the `manual` channel (no creating operation exists in `authorization-rules.json`).
- Which operation lets a `courier` confirm a pay-on-delivery payment: `courier` is an actor of `order_payment` `pending → confirmed`, but no operation grants it (`delivery.order.progress` excludes couriers).
- Operations exposing staff order transitions outside QR Delivery: QR Tables has no staff progression operation for `accepted → ready` and `ready → completed`, QR POS has no refund operation for completed counter sales, and `kitchen_operator` moves orders only indirectly through `system`.
- Order `ready → cancelled` lists no "cancel kitchen tickets" compensation, while the `preparation` machine allows `ready → cancelled` for items not yet handed off; the two should be aligned.
- A won dispute (`chargeback_open → confirmed`) on a payment that was `partially_refunded` returns it to `confirmed`; the contract has no way back to `partially_refunded`.
- Whether `system` consumers that create new work (Kitchen tickets from orders, Fleet deliveries from `OrderReady`) are blocked during suspension; the `suspension` rule covers operations only.
- `tables.json` and `pos.json` do not list the Kitchen events under `consumes`, although their orders follow the same Kitchen-driven `accepted → preparing → ready` path; decide whether order-engine consumers are listed per module or only here.
- Whether online orders appear in the order center before `PaymentConfirmed`.
- Financial account model (provider subaccount vs establishment-owned credentials) and provider choice.
- Real-time transport for order centers (ETag polling vs server-sent events, which needs a contract addition).
- Customer data retention period for inactive customers; half-and-half pizza pricing rule; per-establishment timezone default (`America/Sao_Paulo`).
- Final benchmark thresholds in section 13.
