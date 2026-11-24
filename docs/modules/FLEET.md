# QR Fleet

> **Status: Planned — not implemented, not sellable.** Nothing in this document is available in production. Every endpoint, table and event below is a design contract.

| Field | Value |
|---|---|
| Module code | `fleet` |
| Commercial name | QR Fleet |
| Phase | 7 |
| Lifecycle | `planned` (sale: never; grants: never — see `contracts/commercial/module-lifecycle.json`) |
| Monthly price (catalog version 1) | R$ 29,90 (`monthly_price_centavos: 2990`), effective 2026-11-24, editable until first sale (ADR-007) |
| Standalone | Yes (`standalone: true`, `requires_modules: []`) |
| Machine-readable contract | [`contracts/modules/fleet.json`](../../contracts/modules/fleet.json) (`contract_version: 1`) |

The JSON contracts are the single source of truth (overview in [MODULE_CATALOG.md](../business/MODULE_CATALOG.md), events in [EVENT_CONTRACTS.md](../architecture/EVENT_CONTRACTS.md)). Items marked **Open decision** are not in the contracts yet and must be added to the JSON (and its parity tests) before implementation. In this document "QR Delivery" is the ordering module and "delivery" (lowercase) is the Fleet aggregate that moves food to an address.

## 1. Commercial objective

Give establishments with their own couriers a simple dispatch board, a courier web app and an operational payout record, sold at R$ 29,90 per month or inside the QR Business and QR Complete bundles. QR Fleet does not intermediate courier hiring and never processes payments: it organizes the establishment's own delivery operation at near-zero marginal infrastructure cost.

## 2. Target audience

- Restaurants, pizzerias, burger and açaí shops with one to fifteen own or freelance couriers.
- Owners and managers who dispatch deliveries; couriers using the planned `courier` role.
- Establishments whose orders come from phone, counter or third-party marketplaces and who only need dispatch control.
- Not targeted in the MVP: courier marketplaces, last-mile logistics companies, route optimization for large fleets.

## 3. Problems solved

| Problem | How QR Fleet addresses it |
|---|---|
| Dispatch tracked on paper or chat groups | One board of pending, assigned, picked-up and finished deliveries |
| Unclear who is available | Courier availability set by the courier or the manager |
| Disputes about whether food was delivered | Proof of delivery (photo, code or note) with server timestamps |
| Failed deliveries handled ad hoc | Incidents with reason codes, return flow and reassignment |
| End-of-shift courier fees computed by hand | Append-only payout ledger in integer centavos |
| Orders from other systems cannot be dispatched | Manual and external deliveries without any QR Cardápio order |

## 4. MVP scope

- Courier registry with availability (`available`, `paused`, `offline`) and shift check-in/check-out.
- Delivery registration: manual, external (with a reference such as a marketplace code) or automatic from `OrderReady` when the order fulfillment is delivery.
- Assignment, unassignment and reassignment by owners and managers; courier decline.
- Pickup, delivery and failure reported by the courier; return of failed deliveries.
- Proof of delivery when configured: confirmation code, photo (EXIF stripped) or note.
- Incidents with reason codes; required when a delivery fails.
- Delivery history with full transition trail.
- Operational payout control: per-delivery courier fee, adjustments and settlement records; balances per courier. Record-keeping only.
- Courier PWA at `/courier`: installable, server-rendered pages with small vanilla JS, offline queue for status changes.
- Full `pt-BR`, `en` and `es` UI with zero-reload switching.

## 5. Future scope

- Live GPS tracking of couriers and consumer-facing tracking links.
- Route optimization, batching several deliveries per trip and ETA prediction.
- Delivery-fee calculation by distance (requires maps with external costs, billed through the `external_usage` add-on).
- Automatic assignment rules and courier acceptance timeouts.
- Payout exports for accounting; integration with payment providers stays out of scope.
- Courier hiring, marketplaces or intermediation remain out of scope.

## 6. Domain entities

| Entity | Purpose | Key attributes |
|---|---|---|
| `Courier` | Establishment-scoped courier profile | `id`, `establishment_id`, `account_id` (nullable, links to identity for PWA access), `display_name`, `phone` (optional), `vehicle_kind` (`motorcycle`/`bicycle`/`car`/`on_foot`), `fee_per_delivery_centavos`, `is_active` |
| `CourierAvailability` | Availability and shift intervals | `id`, `establishment_id`, `courier_id`, `status`, `started_at`, `ended_at`, `set_by` |
| `Delivery` | One trip from pickup to an address | `id`, `establishment_id`, `source` (`order`/`manual`/`external`), `order_id` (nullable), `external_reference`, `dropoff_address_id` or address snapshot, `recipient_label`, `notes`, `collect_centavos` (informational), `courier_id`, `courier_fee_centavos` (snapshot at assignment), `state`, `proof_required`, `version` |
| `DeliveryProof` | Evidence of delivery | `id`, `establishment_id`, `delivery_id`, `kind` (`code`/`photo`/`note`), `media_asset_id`, `code_verified`, `captured_at`, `captured_by` |
| `DeliveryIncident` | Problem reported on a delivery | `id`, `establishment_id`, `delivery_id`, `reason_code` (`customer_absent`/`wrong_address`/`refused`/`damaged`/`vehicle_problem`/`other`), `note`, `reported_by`, `reported_at` |
| `CourierPayoutEntry` | Append-only payout ledger line | `id`, `establishment_id`, `courier_id`, `kind` (`delivery_fee`/`adjustment`/`settlement`/`reversal`), `amount_centavos` (signed `i64`), `delivery_id`, `reverses_entry_id`, `note`, `created_by`, `created_at` |

## 7. Responsibilities

| QR Fleet owns | QR Fleet does not own |
|---|---|
| Couriers, availability and assignment | Order lifecycle and order payments (Commerce Core) |
| The `delivery` state machine, proofs and incidents | Delivery fees charged to consumers (Commerce Core and QR Delivery settings) |
| Operational payout records per courier | Moving money, payroll, taxes or labor relationships |
| Courier PWA | Consumer addresses as master data (`addresses` component) |
| Publishing `DeliveryCreated`, `DeliveryAssigned`, `DeliveryCompleted`, `DeliveryFailed` | Hiring or rating couriers on behalf of the establishment |

## 8. Invariants

1. Every row carries `establishment_id`; courier, order, address and media references belong to the same establishment.
2. A delivery has at most one current courier, and only an active courier of the same establishment.
3. For `source = order` there is at most one active (not `cancelled` or `returned`) delivery per `(establishment_id, order_id)`.
4. `delivered` requires a `DeliveryProof` when `proof_required` is set.
5. `failed` requires a `DeliveryIncident`; its `reason_code` is the `DeliveryFailed` payload value.
6. Payout entries are never updated or deleted; corrections are `reversal` entries referencing the original.
7. A courier balance is the sum of its entries; at most one `delivery_fee` entry per delivery.
8. All amounts are integer centavos; Fleet never initiates a transfer.
9. Courier PWA responses only contain deliveries assigned to the authenticated courier in the selected establishment.

## 9. Business rules

- **Registration**: manual and external deliveries require a dropoff address and accept an optional `external_reference` (up to 40 characters) and `recipient_label` (up to 40 characters, first name or table-style label).
- **Automatic creation**: `OrderReady` with `fulfillment = delivery` creates a `source = order` delivery using the order's address id; pickup and dine-in fulfillments are ignored.
- **Assignment**: only `available` couriers appear first in the picker; managers may still assign a `paused` courier with confirmation. Assigning snapshots `fee_per_delivery_centavos` into `courier_fee_centavos`.
- **Decline**: a courier may decline an assignment (`assigned → pending`) before pickup.
- **Proof**: establishment setting `proof_mode` = `none`, `code`, `photo` or `any`. Codes are four digits shown on the order receipt (or handed over by staff for external deliveries); the server stores only a hash and compares in constant time, with at most five attempts per delivery.
- **Failure and return**: a failed delivery is either reassigned (`failed → assigned`) or returned (`failed → returned`) once the food is back at the establishment.
- **Payouts**: `delivered` appends one `delivery_fee` entry equal to the snapshot. `adjustment` entries need a note. `settlement` entries (negative) record that the establishment paid the courier outside QR Cardápio, with a free label such as "cash" or "PIX"; no account numbers or PIX keys are stored.
- **Cash collection**: `collect_centavos` only tells the courier what to collect; confirming an order payment is a Commerce Core `order_payment` transition (`courier` is an allowed actor of `pending → confirmed`), not a Fleet action. No operation grants it to couriers yet, so staff confirm collected payments in QR Delivery (open decision shared with [DELIVERY.md](DELIVERY.md#11-permissions)).
- **Kitchen hand-off**: with QR Kitchen, a courier may mark ticket items `ready → handed_off` at pickup (`kitchen.ticket.progress` includes `courier`); Fleet itself does not consume Kitchen events.
- **Address visibility**: the courier sees the full address from assignment until two hours after the delivery reaches a final state.

## 10. State machines

QR Fleet implements the `delivery` machine of [`contracts/commerce/state-machines.json`](../../contracts/commerce/state-machines.json) (aggregate `delivery`, initial `pending`, terminal `delivered`, `returned`, `cancelled`):

| From | To | Actors | Event | Notes |
|---|---|---|---|---|
| `pending` | `assigned` | `owner`, `manager` | `DeliveryAssigned` | |
| `assigned` | `pending` | `owner`, `manager`, `courier` | none | Unassign or courier declines |
| `assigned` | `picked_up` | `courier` | none | |
| `picked_up` | `delivered` | `courier`, `owner`, `manager` | `DeliveryCompleted` | Requires proof when configured; owners and managers may record on the courier's behalf |
| `picked_up` | `failed` | `courier` | `DeliveryFailed` | Requires an incident |
| `failed` | `assigned` | `owner`, `manager` | `DeliveryAssigned` | Reassignment |
| `failed` | `returned` | `courier`, `owner`, `manager` | none | |
| `pending` | `cancelled` | `owner`, `manager`, `system` | none | `system` on `OrderCancelled` |
| `assigned` | `cancelled` | `owner`, `manager`, `system` | none | `system` on `OrderCancelled` |

`system` is the planned platform role for idempotent outbox consumers acting for one establishment (`authorization-rules.json`). `CourierAvailability.status` is a module-internal attribute, not a contract state machine. Transitions use optimistic concurrency (`... AND state = ? AND version = ?`); zero rows affected returns `409` with the current state.

## 11. Permissions

Operations from [`authorization-rules.json`](../../contracts/entitlements/authorization-rules.json) (all `planned`), evaluated by `authorize(...)` in order: module lifecycle, establishment operational state, member permission, deployment enforcement, commercial entitlement.

| Action | Operation | Roles | Capability | After expiry |
|---|---|---|---|---|
| Register a manual or external delivery | `fleet.delivery.register` | `owner`, `manager` | `fleet.delivery.create` | `deny` |
| Create a delivery from `OrderReady` | outbox consumer (no operation) | `system` | `fleet.delivery.create`, re-checked at consumption | treated as `deny` (same as `fleet.delivery.register`) |
| Assign, reassign, unassign | `fleet.delivery.assign` | `owner`, `manager` | `fleet.delivery.assign` | `complete_existing` |
| Complete a delivery (`picked_up → delivered`) | `fleet.delivery.complete` | `courier`, `owner`, `manager` | none | `allow` |
| Manage couriers and availability | `fleet.courier.manage` | `owner`, `manager` | `fleet.driver.manage` | `deny` |
| Cancel a delivery on `OrderCancelled` | outbox consumer (no operation) | `system` | none | always applied |
| View board, history and payout balances | read only (membership) | `owner`, `manager` | none | allowed |

The `system` role (planned) covers idempotent outbox consumers acting for one establishment; they run only when the triggering event was produced under a valid operation and re-check Fleet's own capability. After expiry, `complete_existing` on `fleet.delivery.assign` is read as: reassignment of a delivery whose first assignment happened while the capability was active is allowed; first assignment of a still-pending delivery is denied, matching "No new deliveries or assignments". While the module is `planned`, every operation is denied even with an `entitlement_grants` row.

**Open decisions** (no operation in the contracts yet):

- Courier progress transitions (pick up, decline, report failure, return): either a dedicated operation (candidate `fleet.delivery.progress`, `courier` plus `owner`/`manager` for return, no capability, `allow`) or an explicit statement that `fleet.delivery.complete` covers them.
- Manual cancellation of a delivery by owners and managers (candidate: no capability, `allow`).
- Payout adjustments and settlements (candidate: `owner`, `manager`, `fleet.driver.manage`, `complete_existing` so past deliveries can still be settled after expiry).
- Couriers setting their own availability: `fleet.courier.manage` allows only `owner` and `manager`.

## 12. Commercial capabilities

| Capability | Summary (from `capabilities.json`) | Required by |
|---|---|---|
| `fleet.delivery.create` | Register deliveries, including ones originating outside QR Cardápio. | `fleet.delivery.register`; automatic creation from `OrderReady` |
| `fleet.delivery.assign` | Assign deliveries to couriers and track their progress. | `fleet.delivery.assign` |
| `fleet.driver.manage` | Manage couriers, availability and operational payouts. | `fleet.courier.manage` |

`fleet.delivery.complete` requires no capability.

- Sold alone at 2990 centavos per month (catalog version 1), or inside QR Business (16990) and QR Complete (21990). Not part of QR Start.
- Billing cycles: monthly, semiannual (`discount_bps: 500`), annual (`discount_bps: 1000`), rounding `half_up_per_line`. Affiliate eligible.
- No per-courier or per-delivery charge in version 1.

## 13. Technical dependencies

| Core component | Status (`components.json`) | Use |
|---|---|---|
| `identity`, `establishments`, `members`, `tenancy`, `i18n` | implemented | Courier accounts, sessions, CSRF, `TenantContext` |
| `commercial`, `entitlements` | implemented | `authorize(...)` |
| `media` | implemented | Proof photos through the existing magic-byte and EXIF-stripping pipeline |
| `events` | partial | Outbox append exists; consumption needs the dispatcher (Commerce Core, phase 3) |
| `addresses` | planned | Dropoff addresses for order deliveries and stored manual addresses |
| `notifications` | planned | Web push to couriers on assignment (degrades to in-app refresh) |
| `orders` | planned | Read port for `OrderReady` handling (fulfillment, address id, amount to collect) |

Optional integrations: `delivery`, `analytics`, `connect`. Prerequisite: an expand migration for the planned `courier` role (today `member_role` and `MemberRole` allow only `owner` and `manager`).

## 14. Published events

All events are appended to `domain_events` in the same transaction as the transition (`aggregate_type = delivery`).

| Event | Trigger | Payload (registry v1) | `dedupe_key` (design) |
|---|---|---|---|
| `DeliveryCreated` | Delivery inserted | `delivery_id`, `order_id` (nullable), `source` | `delivery:{delivery_id}:created` |
| `DeliveryAssigned` | `pending → assigned` or `failed → assigned` | `delivery_id`, `courier_id` | `delivery:{delivery_id}:assigned:{assignment_no}` |
| `DeliveryCompleted` | `picked_up → delivered` | `delivery_id`, `order_id` (nullable), `completed_at` | `delivery:{delivery_id}:completed` |
| `DeliveryFailed` | `picked_up → failed` | `delivery_id`, `order_id` (nullable), `reason_code` | `delivery:{delivery_id}:failed:{attempt_no}` |

The registry producer is `fleet`. `order_id` is null for manual and external deliveries and lets QR Delivery correlate order deliveries without a lookup. The registry defines dedupe keys only for implemented events; the keys above become contract values when each event's JSON Schema is added. Payloads never contain addresses, recipient labels, phones or courier names.

## 15. Consumed events

| Event | Producer | Effect | Idempotency |
|---|---|---|---|
| `OrderReady` | Commerce Core | If `fulfillment = delivery`, create a `source = order` delivery | Dispatcher record by `event_id` plus the unique active delivery per order |
| `OrderCancelled` | Commerce Core | Cancel `pending` and `assigned` deliveries of the order (actor `system`); a `picked_up` delivery is flagged for return and the courier is notified | Cancelling an already-final delivery is a no-op |

## 16. HTTP contracts

Contract endpoints (all **planned**, none available):

| Method | Path | Audience | Operation | Purpose |
|---|---|---|---|---|
| `GET` | `/dashboard/fleet` | member | membership | Dispatch board, couriers and availability |
| `POST` | `/dashboard/fleet/deliveries` | member | `fleet.delivery.register` | Register a manual or external delivery |
| `GET` | `/courier` | courier | membership (courier) | Courier PWA shell and assigned deliveries |

**Open decision** (endpoints not in the contract yet): `POST /dashboard/fleet/deliveries/{id}/transition` (assign and unassign under `fleet.delivery.assign`; cancel, return and on-behalf completion), `GET`/`POST /dashboard/fleet/couriers` and `/dashboard/fleet/couriers/{id}` (`fleet.courier.manage`), `GET`/`POST /dashboard/fleet/payouts` (entries per courier), `POST /courier/deliveries/{id}/transition` and `POST /courier/deliveries/{id}/proof` (multipart; 5 MiB photo and 6 MiB multipart body, matching the QR Core product-photo caps), `POST /courier/availability`, `GET /courier/manifest.webmanifest` and `GET /courier/sw.js`.

Rules: `TenantContext` on dashboard routes and a courier-scoped context on `/courier` routes; ids resolved with `AND establishment_id = ?` (and `AND courier_id = ?` for courier routes), foreign ids return `404`; `_csrf` on every `POST`; courier transitions carry a client-generated idempotency key so offline replays apply once; stale `version` returns `409`.

## 17. Integration contracts

- **Commerce Core `orders` read port**: `delivery_order_view(establishment_id, order_id)` returns fulfillment, address id and amount to collect. Fleet never writes orders.
- **Order completion**: the QR Delivery consumer of `DeliveryCompleted` applies the Commerce Core order transition `ready → completed` as `system`, using the payload `order_id`.
- **QR Delivery** consumes `DeliveryCreated`, `DeliveryCompleted` and `DeliveryFailed` (null `order_id` ignored) to link deliveries to orders and update the order screen; it never assigns couriers.
- **QR Connect** consumes `DeliveryCompleted` for webhooks. **QR Analytics** lists Fleet as an optional integration but consumes no Fleet event in its v1 contract.
- **External systems**: in the MVP, external deliveries are registered by hand. API registration requires QR Connect and a future endpoint.

## 18. Conceptual persistence model

| Table | Notes |
|---|---|
| `fleet_couriers` | `UNIQUE (establishment_id, account_id)` when `account_id` is set; index `(establishment_id, is_active)` |
| `fleet_courier_availability` | Append-only intervals; index `(establishment_id, courier_id, started_at DESC)` |
| `fleet_deliveries` | Partial unique `(establishment_id, order_id) WHERE order_id IS NOT NULL AND state NOT IN ('cancelled', 'returned')`; index `(establishment_id, state, created_at)` and `(establishment_id, courier_id, state)` |
| `fleet_delivery_transitions` | Append-only history (`from_state`, `to_state`, `actor_member_id`, `idempotency_key` unique per delivery, `at`) |
| `fleet_delivery_proofs` | `code_hash`, `media_asset_id` in the same establishment |
| `fleet_delivery_incidents` | Index `(establishment_id, delivery_id)` |
| `fleet_courier_payout_entries` | No `UPDATE`/`DELETE` paths in queries; partial unique `(delivery_id) WHERE kind = 'delivery_fee'`; index `(establishment_id, courier_id, created_at)` |

Ids are `TEXT`; writes use `BEGIN IMMEDIATE`; migrations are expand-only (ADR-010); SQL lives in `src/infrastructure/database/queries/fleet_*.rs`.

## 19. Security requirements

- Tenant isolation on every query; a courier linked to several establishments only sees the establishment selected in the session, and switching clears client caches.
- Courier PWA isolated per establishment (release gate): cache names include the establishment id, the service worker caches only static assets and the shell, never addresses.
- CSRF on every `POST`; per-account rate limits on courier transitions (60 per minute) and proof uploads (10 per minute).
- Proof photos pass magic-byte validation, size caps and EXIF stripping, removing embedded GPS coordinates; they are served only through tenant-scoped private media routes.
- Confirmation codes are hashed, compared in constant time and attempt-limited.
- Strict CSP: no inline scripts or styles; local photo previews avoid `blob:` URLs unless `img-src` is extended for `/courier` only.

## 20. Privacy requirements

- Couriers: collect display name and optionally a phone; no CPF, documents or bank data in the MVP. No location data is collected.
- Consumers: addresses come from the `addresses` component or a manual snapshot; recipient labels are limited to a first name or label. Couriers lose access two hours after the final state.
- Manual address snapshots of finished deliveries are reduced to neighborhood and city after 90 days; history keeps timestamps, states and amounts.
- Events and payout entries carry only identifiers and centavos.
- Courier data export and erasure follow LGPD; erasure anonymizes the courier profile while keeping payout entries under a pseudonymous id for the establishment's records.

## 21. Performance requirements

- Courier PWA shell under 60 KiB compressed (HTML, CSS and JS), usable on low-end Android over 3G.
- A courier transition is one short `BEGIN IMMEDIATE` transaction (delivery update, transition row, optional payout entry and outbox row); p95 under 25 ms on 1 vCPU.
- Dispatch board reads only non-final deliveries through the composite index; history is paginated.
- Board refresh reuses the Kitchen pattern: revision counter in memory, SSE or `ETag`/`304` polling no faster than every 5 s.
- No external API calls on the request path in the MVP.

## 22. Behavior without other modules

Fully usable with QR Core only: register manual or external deliveries with an address typed by staff, assign couriers, follow pickup and delivery in the PWA, collect proofs, record incidents and keep the payout ledger. `DeliveryCreated` is emitted with `source = manual` or `external` and a null `order_id`.

## 23. Behavior integrated with other modules

- With QR Delivery orders, `OrderReady` creates the delivery automatically; QR Delivery consumes `DeliveryCreated`, `DeliveryCompleted` and `DeliveryFailed`, and `DeliveryCompleted` completes the order (`ready → completed`, `system`) through Commerce Core.
- `OrderCancelled` cancels pending or assigned deliveries (`system`); a picked-up delivery must be failed and returned by the courier.
- With QR Kitchen, readiness reaches Fleet only through `OrderReady`; the modules never call each other. Couriers may record the kitchen hand-off (`ready → handed_off`) under `kitchen.ticket.progress`.
- With QR Connect, `DeliveryCompleted` is forwarded to subscribed webhooks.

## 24. Behavior after suspension or cancellation

- **Establishment suspended** (administrative or security): per the `suspension` rule of `authorization-rules.json` (`OperationalStatus::blocks`), only `deny` operations are blocked: `fleet.delivery.register`, `fleet.courier.manage` and automatic creation from `OrderReady`. `fleet.delivery.assign` (`complete_existing`) and `fleet.delivery.complete` (`allow`) stay available, so deliveries in progress can be reassigned and finished; the board and PWA show a translated notice, and payout records stay readable.
- **Subscription expired or cancelled** (contract `after_expiry`): "No new deliveries or assignments. Assigned deliveries can be completed; payout records stay available." Completion, return and payout records remain; registration, automatic creation, courier management and first assignment are denied.
- **Module `retired`**: no operations are granted; history and payout records remain exportable.

## 25. Acceptance criteria

1. With QR Core only, a manager registers an external delivery, assigns it, and the courier completes it in the PWA with a photo proof.
2. Duplicate `OrderReady` delivery creates exactly one delivery.
3. A transition outside section 10 returns `409` or `422` and changes nothing; `delivered` without required proof is refused.
4. Each delivered delivery yields exactly one `delivery_fee` payout entry; corrections appear as reversals.
5. A courier never sees deliveries of another courier or another establishment.
6. Proof photos stored on disk contain no EXIF metadata.
7. After expiry, assigned deliveries can be completed and new registrations are refused with a distinct i18n error.
8. Under administrative or security suspension, registration and courier management are refused while assigned deliveries can still be reassigned and completed.
9. Owners and managers can record `picked_up → delivered` on a courier's behalf; `DeliveryCompleted` carries the `order_id` for order deliveries and null for manual or external ones.
10. All UI strings exist in `pt-BR`, `en` and `es`; templates have no inline scripts or styles.

## 26. Test scenarios

| Scenario | Type |
|---|---|
| Every allowed and forbidden `delivery` transition | unit (domain) |
| Payout balance equals the sum of entries after fees, adjustments, reversals and settlements | unit (domain) |
| External delivery without any order flows to `delivered` | integration |
| Duplicate `OrderReady` and duplicate courier transition with the same idempotency key | integration |
| `OrderCancelled` cancels pending and assigned deliveries, leaves picked-up ones | integration |
| Courier of establishment A requests a delivery of establishment B: `404` | integration |
| Courier linked to two establishments switches and sees no stale data | integration |
| Confirmation code brute force stops after five attempts | integration |
| `planned` lifecycle denies every operation even with a grant | integration |
| Suspended establishment: `fleet.delivery.register` denied; `fleet.delivery.assign` reassignment and `fleet.delivery.complete` allowed | integration |
| Capability expired: first assignment of a pending delivery denied, reassignment of a previously assigned one allowed | integration |
| Manager records `picked_up → delivered` on behalf of a courier | integration |
| `DeliveryCreated`/`DeliveryCompleted`/`DeliveryFailed` payloads: `order_id` set for `source = order`, null for manual and external | contract |
| i18n key parity for every `fleet.*` key | contract |
| PWA shell size and transition latency on 1 vCPU | benchmark |

## 27. Commercial launch requirements

- Release gates from the contract: "external deliveries without any order", "courier PWA isolated per establishment", "payout ledger is append-only".
- Open decisions in sections 11 and 16 settled in the contracts (courier progress transitions, manual cancellation, payout entries, courier self-availability, remaining endpoints) and a JSON Schema in `contracts/events/schemas/` for every Fleet event.
- Prerequisites shipped: event dispatcher, `orders` read port, `addresses`, the `courier` and `system` roles, `complete_existing` enforcement.
- Terms of use state that QR Cardápio does not employ, hire or pay couriers and does not process courier payouts.
- Lifecycle progression `planned → development → internal_preview → beta → available`; price version hashed once sellable (ADR-007); docs and `docs/agents/PROJECT_STATE.md` updated.
- Pilot with at least three establishments and real couriers on Android and iOS browsers.
