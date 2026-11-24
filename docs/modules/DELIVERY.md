# QR Delivery

**Status: Planned — not implemented, not sellable.** Every endpoint, table, file and event in this document is a specification. None is available.

| Field | Value |
|---|---|
| Module code | `delivery` |
| Phase | 4 (requires [Commerce Core](../architecture/COMMERCE_CORE.md), phase 3) |
| Lifecycle | `planned` ([`contracts/commercial/modules.json`](../../contracts/commercial/modules.json)) |
| Monthly price | R$ 69,90 (`monthly_price_centavos: 6990`, catalog version 1, effective 2026-11-24) |
| Module contract | [`contracts/modules/delivery.json`](../../contracts/modules/delivery.json) (single source of truth; this document follows it). Overview: [MODULE_CATALOG.md](../business/MODULE_CATALOG.md); events: [EVENT_CONTRACTS.md](../architecture/EVENT_CONTRACTS.md) |
| Standalone | Yes. Works without QR Menu. |

## 1. Commercial objective

- Give each establishment its own white-label ordering channel for delivery and pickup at a fixed monthly price, with **no commission per order** and no platform share of food sales.
- Replace hand-typed WhatsApp and phone orders with structured orders whose prices, fees and discounts are calculated by the backend.
- Keep the consumer relationship with the establishment: its brand, its URL (`/s/{slug}`), no listing of competitors.
- Sell standalone (R$ 69,90/month) or in the QR Start bundle with QR Menu (R$ 84,90/month), and as part of QR Business and QR Complete.
- Pilot success signals: share of orders placed through the storefront instead of WhatsApp, median acceptance time, orders with item or address errors, monthly retention.

## 2. Target audience

| Audience | Profile | Needs |
|---|---|---|
| Establishments | Small and medium single-location restaurants, burger shops, pizzerias, snack bars, açaí shops, bakeries and cafés already delivering with their own or contracted couriers | Cheap, reliable order intake; no commission; simple setup reusing the existing catalog |
| Staff | Owner or manager on a phone or desktop browser; cashier and waiter (planned roles) at the counter or floor | One screen with new orders, sound alert, clear next action |
| Consumers | Mobile users on mid-range Android or iPhone over 4G | Order in under two minutes, no app install, no account, pay on delivery |

Not targeted in the MVP: multi-store chains, marketplaces, establishments needing the platform to issue fiscal documents.

## 3. Problems solved

| Problem today | QR Delivery answer |
|---|---|
| Orders typed by hand in chat; wrong items, prices and totals | Structured cart; backend-priced quote and order |
| Marketplace percentage commissions | Fixed monthly price; zero commission |
| Consumers must install an app or create an account | Installable PWA; guest checkout with name and phone |
| Delivery fee calculated mentally per neighborhood | Neighborhood zones with backend-calculated fees |
| Orders lost or forgotten during rush hours | Order center with sound alert, states and acceptance timeout |
| "Where is my order?" messages | Tracking view with live state |
| Menu, storefront and prices drifting apart | One shared catalog published as versioned snapshots |

## 4. MVP scope

| Area | Included in the MVP |
|---|---|
| Storefront PWA | White-label page at `/s/{slug}/` with establishment logo, accent color, name and hours; per-establishment `manifest.webmanifest` (`id`, `start_url` and `scope` = `/s/{slug}/`, icons from the logo); service worker at `/s/{slug}/sw.js` with scope `/s/{slug}/`; pt-BR/en/es zero-reload switching |
| Shopping catalog | Categories, products, photos, descriptions, "from R$" prices for products with variations, sold-out badge, search, sticky category navigation |
| Variations and add-ons | One variation axis per product (absolute prices); add-on groups with min/max selection and option quantities (Commerce Core catalog) |
| Cart | `localStorage`, keyed by establishment id and catalog version; never mixes establishments |
| Checkout | Guest: name and phone; no account and no app install before the first order |
| Addresses | Street, number, complement, neighborhood (zone), city, UF, optional CEP, reference point; optionally remembered on the device |
| Business hours | Weekly intervals, date exceptions, holiday overrides, manual pause |
| Minimum order | Global value with optional per-zone override |
| Simple coupons | Percentage or fixed amount, validity window, minimum subtotal, usage limits |
| Service modes | Delivery and/or pickup |
| Fees by neighborhood | Fee, ETA range and optional free-delivery threshold per zone |
| Pay on delivery | Cash (with change), card machine, Pix to the establishment's own key; confirmed by staff |
| Order center | Live list by state, sound alert, accept/reject, preparing, ready, complete, cancel, printable ticket |
| Tracking and history | Consumer tracking view and device-local order history; staff history with filters and CSV export |
| Notifications | Order-center sound and web push for staff; optional web push on the tracking view; WhatsApp click-to-chat link (no API) |

## 5. Future scope

- Online payment (Pix dinâmico, card) through per-establishment financial accounts (Commerce Core section 9).
- Scheduled orders, half-and-half pizzas, distance-based fees with a maps provider (external cost), CEP lookup.
- Loyalty redemption at checkout, ratings, campaigns.
- Courier dispatch through QR Fleet (integration already specified); custom domain add-on (`branding.custom_domain`).
- WhatsApp Business API or SMS notifications under the `external_usage` add-on.

Out of scope permanently or until explicitly decided: an own courier network (not initially), platform commission on orders, a marketplace listing several establishments.

## 6. Domain entities

| Entity (contract name) | Owner | Key attributes | Notes |
|---|---|---|---|
| `Storefront` | Delivery | establishment_id, status (`draft`, `published`, `unpublished`), published_version, accepted payment methods, Pix key text, WhatsApp contact, pickup instructions | One per establishment |
| `StorefrontSnapshot` | Commerce Core (`catalog_snapshots`, channel `storefront`) | version, content hash, canonical JSON | Immutable; the only pricing source for checkout |
| `Cart (client-side, per establishment)` | Storefront JS | lines, catalog version, idempotency key | Never stored on the server |
| `Order` | Commerce Core | public id, order number, channel `delivery` or `pickup`, state, version, totals | Shared order engine |
| `OrderItem` | Commerce Core | product/variation ids, copied names, unit price, quantity, line total | Immutable snapshot |
| `OrderItemOption` | Commerce Core | option id, copied names, unit price, quantity | Immutable snapshot |
| `OrderPayment` | Commerce Core | method, state, amount, change_for | `order_payment` machine |
| `Customer` | Commerce Core | name, phone (E.164), blocked_at, erased_at | Scoped to one establishment |
| `Address` | Commerce Core | street, number, complement, neighborhood, city, UF, CEP, reference, zone | Copied into the order |
| `DeliveryZone` | Delivery | neighborhood, city, fee, min order override, free-delivery threshold, ETA range | Unique per normalized neighborhood |
| `Coupon` | Delivery | code, kind, value, validity, limits, redemption count | Not published in the snapshot |
| `CheckoutAttempt` | Commerce Core | idempotency key, request hash, outcome | Unique per establishment and key |

## 7. Responsibilities

| Concern | QR Delivery | Commerce Core | Other |
|---|---|---|---|
| Storefront rendering, PWA files, publication | Owns | Provides snapshot store | QR Menu untouched |
| Zones, coupons, payment-method settings | Owns | Hours, modes, minimum order (`business_hours`, `commercial_settings`) | |
| Pricing, discounts math, fee application, totals | Supplies inputs | Owns | |
| Order and payment state, transitions, events | Exposes UI and endpoints | Owns machines and executor | |
| Customers, addresses, LGPD export/erasure | Collects at checkout | Owns | |
| Outbox dispatch, notifications sending | Defines templates | Owns | |
| Preparation tickets, couriers | | | QR Kitchen, QR Fleet |

## 8. Invariants

1. Every Delivery row has `establishment_id`; every query filters `AND establishment_id = ?`; zones, coupons, products, variations and options referenced by an order belong to that establishment.
2. Money is `i64` centavos everywhere (storefront JSON, quotes, orders, coupons, fees); no floats.
3. Checkout prices only against the **current published storefront snapshot**, never draft catalog tables.
4. `total = items_subtotal − discount + delivery_fee` and every component is `>= 0`.
5. All thresholds (minimum order, coupon minimum, free delivery) compare `items_subtotal` before discount and without fee.
6. A discount never exceeds `items_subtotal` and never applies to the delivery fee; at most one coupon per order.
7. One order per `(establishment_id, idempotency_key)`.
8. Order items and options are immutable after placement.
9. Orders are accepted only while the establishment is open, not paused, serving the chosen mode and zone.
10. A cart only contains lines of one establishment; storage keys include the establishment id.
11. The service worker of one establishment never controls pages of another (`scope` = `/s/{slug}/`).
12. The storefront slug is the establishment slug. The slug locks after the first menu **or** storefront publication (extends the current `published_version > 0` rule), and `/c/{slug}` URLs and printed QR codes are never changed by Delivery.
13. Delivery `delivered` (Fleet) does not imply order `completed` or payment `confirmed`; order `completed` does not imply `confirmed`.
14. `Σ confirmed − Σ refunds <= order total`; refunds never exceed a payment's confirmed amount.
15. Event payloads contain identifiers and integer amounts only.

## 9. Business rules

### 9.1 Service modes and pause

- `delivery` and `pickup` are enabled independently; at least one must be on to publish.
- A manual pause (`paused_until`, preset 15/30/60 min or until reopened) applies immediately to quotes and checkout.

### 9.2 Business hours

- Weekly intervals per weekday (several per day); an interval crossing midnight belongs to the day it opens.
- Date exceptions (closed or special hours) override the weekly schedule; holidays are exceptions.
- Evaluation is server-side in the establishment timezone (default `America/Sao_Paulo`). The storefront shows hours from the snapshot as advice only.
- Business date for order numbering cuts over at 04:00 local time so late shifts keep one sequence.

### 9.3 Minimum order and limits

- Effective minimum = zone override, else global minimum; pickup uses the global minimum.
- Up to 50 lines per order, quantity 1–99 per line, line note ≤ 140 characters, order note ≤ 280 characters, order total ≤ 99 999 999 centavos.

### 9.4 Fees by neighborhood

- The consumer picks a neighborhood from the establishment's active zones; no geolocation and no maps lookup.
- Fee = zone fee, or 0 when `items_subtotal >= free_delivery_from_centavos` of that zone.
- Pickup has no fee. Unknown or inactive zones are refused with `delivery.zone_not_served`.
- Zone, fee and minimum changes apply to checkout immediately and produce a derived storefront snapshot so the page shows the same values.

### 9.5 Coupons

- Code: uppercase `A–Z0–9`, 3–20 characters, unique per establishment.
- Kind: `percent_bps` (1–10000) or `amount_centavos` (> 0). Percentage uses half-up rounding on `items_subtotal × bps / 10000`.
- Constraints: validity window (local time), minimum subtotal, maximum redemptions, maximum per customer (by phone), service modes.
- Redemption is consumed by a conditional `UPDATE` inside the checkout transaction and released in the same transaction as a rejection or cancellation.
- Invalid, expired or exhausted codes return one generic message (no enumeration hints).

### 9.6 Acceptance, preparation and promised time

- Acceptance window: default 10 minutes (3–30). On expiry the system rejects with `acceptance_timeout`.
- Accepting sets prep minutes (default from settings); `promised_at = accepted_at + prep + zone ETA max` (pickup: no ETA).
- Without QR Kitchen, owners and managers move `accepted → preparing → ready`, or owners, managers, cashiers and waiters use the `accepted → ready` fast path.
- With QR Kitchen, the planned `system` role moves the order `accepted → preparing` on its first `KitchenTicketStarted` and `preparing → ready` once every kitchen ticket of the order emitted `KitchenTicketReady`.

### 9.7 Rejection and cancellation

- Rejection reasons: `closed`, `item_unavailable`, `address_not_served`, `too_busy`, `suspected_fraud`, `other`; `acceptance_timeout` is system-only.
- Consumers may cancel only while `placed` (`delivery.order.cancel`), using the cancel token returned at checkout.
- Staff cancellation after acceptance requires owner or manager and a reason: `consumer_request`, `item_unavailable`, `delivery_failed`, `payment_issue`, `other`.

### 9.8 Pay on delivery

- Methods: `cash` (optional `change_for_centavos`, which must be `>= total`), `card_on_delivery`, `pix_on_delivery` (the establishment's own Pix key shown as text; no platform involvement).
- A `pending` payment of the full total is created with the order. Staff (owner, manager, cashier or waiter) confirm it with the collected method; online methods would be confirmed only by authenticated gateway webhooks (future scope). Completed orders with an unconfirmed payment are flagged in the order center and in the daily summary.

## 10. State machines

Delivery uses the `order` and `order_payment` machines of [`contracts/commerce/state-machines.json`](../../contracts/commerce/state-machines.json) unchanged (full tables in [COMMERCE_CORE.md](../architecture/COMMERCE_CORE.md#3-state-machines)).

Effective actors are the intersection of the transition's contract actors and the roles of the Delivery operation that triggers it (section 11). Contract actors that hold no Delivery operation act through their own module, whose events Delivery consumes as the planned `system` role.

| Transition | Delivery trigger | Effective actors in Delivery | Event |
|---|---|---|---|
| `placed → accepted` | Order center "Accept" | owner, manager, cashier, waiter | `OrderAccepted` |
| `placed → rejected` | Order center "Reject"; acceptance timer | owner, manager, cashier; system (timeout only) | `OrderRejected` |
| `placed → cancelled` | Tracking view "Cancel" (`delivery.order.cancel`) | consumer | `OrderCancelled` |
| `accepted → preparing` | Order center "Start"; `KitchenTicketStarted` | owner, manager; system (kitchen_operator acts through QR Kitchen) | `OrderPreparing` |
| `preparing → ready` | Order center "Ready"; every kitchen ticket ready | owner, manager; system | `OrderReady` |
| `accepted → ready` | Order center "Ready" (fast path without Kitchen) | owner, manager, cashier, waiter | `OrderReady` |
| `ready → completed` | "Delivered"/"Picked up"; `DeliveryCompleted` | owner, manager, cashier, waiter; system (courier acts through QR Fleet) | `OrderCompleted` |
| `accepted`/`preparing`/`ready` → `cancelled` | Order center "Cancel" | owner, manager | `OrderCancelled` |
| payment `pending → confirmed` | "Payment collected" | owner, manager, cashier, waiter | `PaymentConfirmed` |
| payment `pending → cancelled` | Consumer of `OrderRejected` / `OrderCancelled` | system (owner and manager are contract actors without a Delivery endpoint) | none |
| payment `confirmed`/`partially_refunded` → `partially_refunded`/`refunded` | "Record refund" (`delivery.order.refund`) | owner, manager | `PaymentRefunded` |

Chargeback transitions (`confirmed`/`partially_refunded` → `chargeback_open`, `chargeback_open → confirmed`/`chargeback_lost`) and `pending → failed` are gateway-only and apply only once online payment exists (future scope). Cancellation compensations follow the contract: refund confirmed payments, cancel kitchen tickets (from `accepted` and `preparing`) and cancel pending deliveries; Delivery adds the coupon release (section 9.5). No loyalty compensation is needed because automatic points are credited only after completion.

The consumer tracking view maps states to labels; for delivery orders `ready` reads "ready / on the way" unless QR Fleet supplies finer delivery states.

## 11. Permissions

Operations from [`authorization-rules.json`](../../contracts/entitlements/authorization-rules.json) (all `planned`), evaluated by `authorize(...)` in order: module lifecycle, establishment operational state, member permission, deployment enforcement, commercial entitlement.

| Operation | Roles | Capability | After expiry | Covers |
|---|---|---|---|---|
| `delivery.storefront.publish` | owner, manager | `delivery.storefront.publish` | deny | Publish, republish and derived snapshots |
| `delivery.order.place` | consumer | `delivery.order.create` | deny | Quotes and checkout |
| `delivery.order.progress` | owner, manager, cashier, waiter | `delivery.order.manage` | complete_existing | Accept, reject, progress, complete, staff cancel, pay-on-delivery payment confirmation |
| `delivery.order.refund` | owner, manager | none | allow | Recording refunds |
| `delivery.settings.update` | owner, manager | `delivery.settings.manage` | deny | Hours, modes, minimum, zones, fees, coupons, payment methods |
| `delivery.order.cancel` | consumer | none | allow | Consumer cancellation while the order is still `placed` |

- A transition requires both the operation role and membership in the transition's contract actors (section 10); staff cancellation therefore stays owner/manager only.
- Automatic transitions (acceptance timeout, `KitchenTicketStarted`, all tickets ready, `DeliveryCompleted`, payment cancellation on rejection or cancellation) run as the planned `system` role: the acceptance timer and idempotent outbox consumers that run only when the triggering event was produced under a valid operation and re-check `delivery.order.manage` (existing orders pass under `complete_existing`).
- Viewing the order center and history needs establishment membership only and stays available after expiry.
- Customer data export uses the QR Core `data.export` operation (owner only, no capability, `allow`).
- Administrative and security suspension block only the `deny` operations (`delivery.storefront.publish`, `delivery.order.place`, `delivery.settings.update`); see section 24.

Open decisions:

- Courier payment confirmation: `courier` is a contract actor of payment `pending → confirmed`, but holds neither `delivery.order.progress` nor a Fleet operation that confirms payments, so cash collected at the door is confirmed by staff until an operation is assigned.
- Storefront unpublishing has no operation yet (candidate: `delivery.storefront.unpublish`, owner and manager, no capability, `allow`, mirroring `menu.unpublish`).

## 12. Commercial capabilities

| Capability | Summary (capabilities.json) | Gates |
|---|---|---|
| `delivery.storefront.publish` | Publish the white-label ordering storefront and its versioned catalog snapshot. | `delivery.storefront.publish` |
| `delivery.order.create` | Accept new delivery or pickup orders from consumers. | `delivery.order.place` |
| `delivery.order.manage` | Accept, reject and progress delivery and pickup orders. | `delivery.order.progress` |
| `delivery.settings.manage` | Manage delivery zones, fees, minimum order, coupons and service modes. | `delivery.settings.update` |

`delivery.order.refund` and `delivery.order.cancel` require no capability. Capabilities come from catalog offers, never from plan names. Prices below are computed from the contracts (months × monthly price minus the cycle discount, half-up per line); planned prices stay editable until the offer first becomes sellable (ADR-007).

| Offer | Monthly | Semiannual (−5%) | Annual (−10%) |
|---|---|---|---|
| QR Delivery v1 | 6990 (R$ 69,90) | 39843 (R$ 398,43) | 75492 (R$ 754,92) |
| QR Start v1 (`menu` + `delivery`) | 8490 (R$ 84,90) | 48393 (R$ 483,93) | 91692 (R$ 916,92) |

QR Business (16990) and QR Complete (21990) also include `delivery` v1. Delivery is affiliate-eligible.

## 13. Technical dependencies

| Core component (delivery.json) | Status today | Use |
|---|---|---|
| `identity`, `establishments`, `members`, `tenancy`, `i18n`, `media` | implemented | Staff auth, slug and branding, roles, `TenantContext`, translations, images |
| `commercial`, `entitlements` | implemented | Contracts, capabilities, `authorize(...)` |
| `catalog` | partial | Variations and add-ons arrive with Commerce Core |
| `events` | partial | Append implemented; dispatcher arrives with Commerce Core |
| `business_hours`, `commercial_settings`, `customers`, `addresses`, `orders`, `order_payments`, `notifications` | planned | Delivered by Commerce Core (phase 3) |

Infrastructure: Axum 0.8 (`{slug}` paths), SQLx SQLite WAL with `BEGIN IMMEDIATE`, Askama, vanilla JS modules, Caddy. New: a Caddy `handle_path /s/*` block mirroring `/c/*` (including the `/.tmp` 404 rule) and a Web Push (VAPID) crate, which requires `cargo audit`. No Node runtime, broker, maps API or per-tenant database.

## 14. Published events

Appended to `domain_events` in the transaction of the state change; payloads per [`registry.json`](../../contracts/events/registry.json) (see also [EVENT_CONTRACTS.md](../architecture/EVENT_CONTRACTS.md)). As the registry states, the module `publishes` list includes Commerce Core events written on Delivery's behalf; the registry `producer` names the component that writes the row (`core.publishing`, `commerce`, `commerce.payments`).

| Event | Producer | When | Aggregate | Payload v1 | Dedupe key |
|---|---|---|---|---|---|
| `CatalogPublished` | `core.publishing` | Storefront snapshot published (manual or derived) | establishment | `channel: "storefront"`, publication_version, html_bytes, asset_count | Contract: channel + establishment id + publication version (`CatalogPublished:storefront:{establishment_id}:{version}`, mirroring the implemented `CatalogPublished:menu:{establishment_id}:{version}`) |
| `OrderCreated` | `commerce` | Checkout commits | order | order_id, channel, total_centavos, item_count, catalog_version | Design: order public id |
| `OrderAccepted` | `commerce` | `placed → accepted` | order | order_id, promised_at | Design: order id + state |
| `OrderRejected` | `commerce` | `placed → rejected` | order | order_id, reason_code | Design: order id + state |
| `OrderPreparing` | `commerce` | `accepted → preparing` (staff or system on `KitchenTicketStarted`) | order | order_id | Design: order id + state |
| `OrderReady` | `commerce` | `preparing → ready` or `accepted → ready` | order | order_id, fulfillment | Design: order id + state |
| `OrderCompleted` | `commerce` | `ready → completed` | order | order_id, completed_at | Design: order id + state |
| `OrderCancelled` | `commerce` | Any transition into `cancelled` | order | order_id, reason_code, cancelled_by | Design: order id + state |
| `PaymentConfirmed` | `commerce.payments` | Payment `pending → confirmed` | order_payment | payment_id, order_id, amount_centavos, method | Design: payment id + state |
| `PaymentRefunded` | `commerce.payments` | Each refund or lost chargeback | order_payment | payment_id, order_id, amount_centavos, refund_id | Design: refund id |

The registry defines dedupe keys only for implemented events; the "Design" keys above become contract values when each event's JSON Schema is added.

## 15. Consumed events

| Event | Producer | Delivery reaction |
|---|---|---|
| `KitchenTicketCreated` | `kitchen` | Records the ticket id against the order so "every ticket ready" is computable; tickets with a null `order_id` (manual tickets) are ignored |
| `KitchenTicketStarted` | `kitchen` | First started ticket of an `accepted` order: system `accepted → preparing` (emits `OrderPreparing`); null `order_id` ignored |
| `KitchenTicketReady` | `kitchen` | When every ticket recorded for the order is ready, system `preparing → ready`; null `order_id` ignored |
| `DeliveryCreated` | `fleet` | Links the Fleet delivery to the order for the order center and tracking view; external or manual deliveries (null `order_id`) ignored |
| `DeliveryCompleted` | `fleet` | System `ready → completed` for the payload `order_id`; null `order_id` ignored |
| `DeliveryFailed` | `fleet` | No state change; order-center alert so staff reassign in Fleet or cancel with `delivery_failed`; null `order_id` ignored |

`order_id` is `string|null` in `KitchenTicketCreated`, `KitchenTicketStarted`, `KitchenTicketReady`, `DeliveryCreated`, `DeliveryCompleted` and `DeliveryFailed`. Consumers are idempotent by `event_id`; an order already in the target or a later state is a no-op.

## 16. HTTP contracts

### 16.1 Endpoints in the contract (all planned)

| Method | Path | Audience | Operation | Behavior |
|---|---|---|---|---|
| `GET` | `/s/{slug}` | consumer | none (static) | Static storefront from disk, zero DB queries; `301` to `/s/{slug}/`; `max-age=60, stale-while-revalidate=300` + ETag |
| `GET` | `/s/{slug}/catalog.v{version}.json` | consumer | none (static) | Static immutable snapshot (`max-age=31536000, immutable`); last 3 versions kept |
| `POST` | `/api/storefront/v1/{slug}/quotes` | consumer | `delivery.order.place` | Prices a cart; persists nothing |
| `POST` | `/api/storefront/v1/{slug}/orders` | consumer | `delivery.order.place` | Idempotent checkout; `201` with order id and cancel token |
| `GET` | `/api/storefront/v1/{slug}/orders/{order_id}` | consumer | none | Tracking by 128-bit public id; ETag/`304`; no PII in the body |
| `POST` | `/api/storefront/v1/{slug}/orders/{order_id}/cancel` | consumer | `delivery.order.cancel` | Body: cancel token; `200` when `placed → cancelled`, `403` on a wrong token, `409` once the order left `placed` |
| `GET` | `/dashboard/delivery/orders` | member | membership | Order center and history (`?state=`, `?date=`); fragment polling with ETag |
| `POST` | `/dashboard/delivery/orders/{id}/transition` | member | `delivery.order.progress` | Form: `_csrf`, `to`, `expected_version`, `reason_code`, `prep_minutes`; `303` or `409` with current state |
| `POST` | `/dashboard/delivery/orders/{id}/payments/{payment_id}/confirm` | member | `delivery.order.progress` | Form: `_csrf`, collected `method`, `expected_version`; payment `pending → confirmed`; `409` on a stale version |
| `POST` | `/dashboard/delivery/orders/{id}/refunds` | member | `delivery.order.refund` | Form: `_csrf`, `payment_id`, `amount_centavos`; refused when refunds would exceed the confirmed amount |
| `GET` | `/dashboard/delivery/settings` | member | membership | Settings, zones, coupons, storefront status |
| `POST` | `/dashboard/delivery/settings` | member | `delivery.settings.update` | Hours, modes, minimum order, payment methods; a derived storefront snapshot when published values change |
| `POST` | `/dashboard/delivery/publish` | member | `delivery.storefront.publish` | Publishes or republishes the storefront snapshot; emits `CatalogPublished` (`channel: "storefront"`) |

Static files published next to `index.html` under `/s/{slug}/`: `manifest.webmanifest` (`max-age=300`), `sw.js` (`no-cache`), `theme-{hash8}.css` and `media/*` (immutable), `icon-192.png`, `icon-512.png`.

### 16.2 Quote and order bodies

Quote request: `catalog_version`, `fulfillment` (`delivery`/`pickup`), `zone_id` (delivery), optional `coupon_code`, `lines[]` of `{product_id, variation_id, quantity, options[{option_id, quantity}], note}`. Quote response: per-line `unit_price_centavos`, `line_total_centavos`, `available`; `items_subtotal_centavos`, `discount_centavos`, `delivery_fee_centavos`, `total_centavos`, `minimum_order_centavos`, `coupon.status`, `open_now`, `eta_minutes[min,max]`, `issues[]`.

Order request: header `Idempotency-Key` (UUID generated when checkout opens, reused on retries); body = quote request + `expected_total_centavos`, `customer{name, phone}`, `address{street, number, complement, neighborhood_zone_id, city, uf, cep, reference}` (delivery only), `payment{method, change_for_centavos}`, `note`.

| Status | Code | Meaning |
|---|---|---|
| `201` | — | Order placed (same body on an idempotent replay) |
| `400` | `validation.*` | Malformed body or limits exceeded |
| `404` | `delivery.storefront_not_found` | Unknown or unpublished slug |
| `409` | `commerce.checkout_repriced` | Snapshot, price, availability or coupon changed; body has current `catalog_version` and a fresh quote; client must show differences and reconfirm |
| `422` | `delivery.closed`, `delivery.below_minimum`, `delivery.zone_not_served`, `delivery.mode_unavailable`, `commerce.idempotency_mismatch` | Business rule refused |
| `403` | `delivery.ordering_unavailable` | Authorization denied (expired capability, suspension, lifecycle); localized message only |
| `429` | `error.rate_limited` | Rate limit |

### 16.3 Open decision: endpoints not yet in `delivery.json`

Decide whether these become separate contract endpoints or fold into the ones above: `POST /dashboard/delivery/zones[/{id}]` and `POST /dashboard/delivery/coupons[/{id}]` (`delivery.settings.update`), `POST /dashboard/delivery/unpublish` (needs the storefront unpublish operation, section 11), `POST /dashboard/delivery/availability` (sold-out toggles; a QR Core catalog edit or a Delivery operation), `GET /dashboard/delivery/orders.csv`, `POST /dashboard/delivery/push-subscriptions`. All authenticated `POST`s verify `_csrf`.

## 17. Integration contracts

- **Commerce Core (in-process)**: Delivery handlers call `application::commerce::checkout::{quote, place_order}`, `order_transitions::transition(order, to, actor, expected_version)`, `payments::{confirm, refund}` and `catalog_publication::publish(channel = storefront)`. Delivery never writes order tables directly.
- **Storefront client**: snapshot JSON embedded as inert `<script type="application/json" id="qr-storefront-data">` (establishment id, catalog version, slug, hours, zones); shared JS from `/static/js/` with compile-time asset versioning.
- **Cart storage**: `localStorage` key `qrc:cart:{establishment_id}:v{catalog_version}` holding lines, idempotency key and `updated_at` (expires after 24 h). On a newer version the client re-quotes the lines, moves them to the new key and deletes older keys of the **same** establishment only. Device-local history: `qrc:orders:{establishment_id}` (last 20 public ids and cancel tokens). Remembered customer data: `qrc:customer:{establishment_id}`, only after explicit opt-in, with a "forget" button. All storefronts share one origin, so these prefixes are the isolation boundary.
- **Service worker**: caches named `qrc-s-{establishment_id}-v{n}`; deletes only caches with its own prefix; network-first for `index.html`; cache-first for versioned immutable files; never caches `/api/` responses; offline shows the cached catalog with ordering disabled.
- **Events**: section 14 and 15; all cross-module effects go through the outbox.

## 18. Conceptual persistence model

Delivery-owned tables (Commerce Core tables are in [COMMERCE_CORE.md](../architecture/COMMERCE_CORE.md#12-conceptual-sqlite-persistence-model)):

| Table | Key columns | Constraints and indexes |
|---|---|---|
| `storefronts` | establishment_id (PK), status, published_version, has_unpublished_changes, last_published_at, payment_methods, pix_key_display, contact_whatsapp, pickup_instructions | `status IN ('draft','published','unpublished')` |
| `storefront_publications` | version, kind (`manual`/`derived`), status (`published`/`failed`/`unpublished`), html_bytes, catalog_bytes, duration_ms, published_at | Append-only; index `(establishment_id, published_at DESC)` |
| `delivery_zones` | neighborhood, city, normalized_key, fee_centavos, min_order_centavos, free_delivery_from_centavos, eta_min_minutes, eta_max_minutes, is_active, position | UNIQUE `(establishment_id, normalized_key)`; fees `>= 0` |
| `coupons` | code, kind, percent_bps, amount_centavos, min_subtotal_centavos, valid_from, valid_until, max_redemptions, max_per_customer, redemption_count, modes, is_active | UNIQUE `(establishment_id, code)`; `CHECK` exactly one of percent/amount; `redemption_count <= max_redemptions` |
| `coupon_redemptions` | coupon_id, order_id, customer_id, status (`consumed`/`released`) | UNIQUE `order_id`; index `(establishment_id, coupon_id, customer_id)` |

Uses from Commerce Core: `catalog_snapshots` (channel `storefront`), `commercial_settings`, `business_hours`, `customers` (with `blocked_at`), `customer_addresses`, `checkout_attempts`, `orders` (with a hashed consumer cancel token), `order_items`, `order_item_options`, `order_delivery_details`, `order_payments`, `payment_refunds`, `order_transitions`.

Disk layout, separate from `published/` so menu and storefront publications never touch each other:

```text
DATA_DIR/storefronts/
├── .tmp/                       # same-filesystem staging, 404 at the edge
└── {slug}/
    ├── index.html              # written last, atomic rename
    ├── catalog.v{version}.json # last 3 versions
    ├── manifest.webmanifest
    ├── sw.js
    ├── theme-{hash8}.css
    ├── icon-192.png, icon-512.png
    └── media/{hash16}_{400|800}.jpg
```

Publication follows [PUBLISHING.md](../architecture/PUBLISHING.md): the shared per-establishment `PublishLocks`, one short snapshot read, staged files, `index.html` last, then one `BEGIN IMMEDIATE` transaction that swaps the current snapshot, updates `storefronts`, appends `storefront_publications` and `CatalogPublished`.

## 19. Security requirements

- **Tenancy**: slug resolves to `establishment_id` on the server; every id in quotes and orders is cross-verified against that establishment and its current snapshot; foreign ids get the same error as unknown ids.
- **Consumer API**: no cookies or sessions; same-origin only (no CORS); `Content-Type: application/json` required; `POST`s with `Sec-Fetch-Site: cross-site` or a foreign `Origin` are rejected; body limit 32 KiB.
- **Rate limits** (governor): quotes 60/min per IP; orders 5/min per IP and 3 per 10 min per phone and establishment; tracking 120/min per IP; cancel 5/min per IP; honeypot field on checkout.
- **Abuse**: staff can block a phone per establishment; blocked phones receive a generic refusal. Pay-on-delivery prank orders are bounded by rate limits and manual acceptance.
- **IDOR**: order public ids are 128-bit CSPRNG values; the cancel token is separate and stored hashed, so a shared tracking link cannot cancel.
- **Staff surfaces**: `TenantContext`, `_csrf` on every `POST`, `expected_version` on transitions, `authorize(...)` on every operation.
- **XSS and CSP**: Askama escaping; JSON `</` escaping; storefront JS writes user text with `textContent` only; no inline scripts (except inert JSON) or `style` attributes; the global CSP and `Permissions-Policy geolocation=()` stay unchanged; the service worker is a same-origin static file.
- **Integrity**: client prices, fees and discounts are ignored; payment confirmation is never accepted from the browser.
- **Logging**: never log names, phones, addresses, notes, cancel tokens or idempotency keys; log internal order ids.

## 20. Privacy requirements

- The establishment is the controller of its consumers' data; QR Cardápio is the processor. Each storefront links a privacy notice in pt-BR, en and es naming the establishment.
- Minimization: pickup needs name and phone; delivery adds the address. Email, CPF, birth date and location are not collected.
- Legal basis: performance of the order. Marketing consent is separate, off by default and unused in the MVP.
- No tracking cookies, third-party analytics, fingerprinting or geolocation on storefronts.
- Device storage of customer data happens only after opt-in and can be cleared from the storefront.
- LGPD export and erasure per Commerce Core section 6, including retention exceptions for de-identified financial order records.
- Events, push payloads and logs carry no PII; push payloads carry order number and state only.

## 21. Performance requirements

| Item | Budget |
|---|---|
| `/s/{slug}` and `catalog.v{n}.json` | Zero DB queries; served by Caddy |
| `index.html` | ≤ 20 KB gzip |
| `catalog.v{n}.json` | ≤ 60 KB gzip for 150 products with options |
| Storefront JS / CSS | ≤ 25 KB / ≤ 15 KB gzip |
| LCP on mid-range Android over 4G | ≤ 2.5 s; Lighthouse mobile performance ≥ 90 |
| Quote | p95 ≤ 50 ms |
| Checkout | p95 ≤ 300 ms at the Commerce Core benchmark load |
| Order center poll | Every 5 s while visible; `304` path uses one indexed query |
| Tracking poll | Every 15 s while visible; stops on terminal states |
| Storefront publication | ≤ 500 ms for 150 products |

## 22. Behavior without other modules

- Requires only QR Core components and Commerce Core. The catalog is edited in the existing dashboard editor; draft editing is a free QR Core operation (`menu.draft.edit`, no capability).
- Without QR Menu: no `/c/{slug}` page is required; the storefront publishes independently.
- Without QR Kitchen: owners and managers use `preparing` manually, or staff use the `accepted → ready` fast path; no `KitchenTicket*` event ever arrives.
- Without QR Fleet: the establishment's couriers deliver; staff (owner, manager, cashier, waiter) mark `ready → completed` after hand-off and confirm the collected payment.
- Without Loyalty, Analytics or Connect: daily summary and CSV export inside the module.

## 23. Behavior integrated with other modules

| Module | Integration |
|---|---|
| QR Menu | The static menu can show an "Order online" link to `/s/{slug}/` on its next publication; `/c/{slug}` and printed QR codes are unchanged. A separate QR code can point to `/s/{slug}/`. Delivery ignores `CatalogPublished` with channel `menu`. |
| QR Kitchen | Kitchen consumes `OrderAccepted` and `OrderCancelled`; Delivery consumes `KitchenTicketCreated`, `KitchenTicketStarted` and `KitchenTicketReady` and moves the order to `preparing` and `ready` as `system`. |
| QR Fleet | Fleet consumes `OrderReady` (fulfillment `delivery`) and `OrderCancelled`; Delivery consumes `DeliveryCreated`, `DeliveryCompleted` and `DeliveryFailed`, correlated by their nullable `order_id`. |
| QR Loyalty | Consumes `OrderCompleted`, `PaymentConfirmed`, `PaymentRefunded`, `OrderCancelled`; points are credited only after completion and reversed through `PaymentRefunded`. |
| QR Analytics | Consumes `CatalogPublished`, `OrderCreated`, `OrderRejected`, `OrderCompleted`, `OrderCancelled`, `PaymentConfirmed`, `PaymentRefunded`. |
| QR Connect | Consumes `CatalogPublished`, `OrderCreated`, `OrderAccepted`, `OrderCompleted`, `OrderCancelled`, `PaymentConfirmed`, `PaymentRefunded` and `DeliveryCompleted` for webhooks; its `external` channel orders use the same order engine. |

## 24. Behavior after suspension or cancellation

| Situation | Storefront | New orders | Existing orders | Data |
|---|---|---|---|---|
| Subscription expired or cancelled | Static files stay; no new or derived publications (`delivery.storefront.publish`, `delivery.settings.update` denied) | Refused with `delivery.ordering_unavailable` (quotes and checkout, `delivery.order.place` denied) | Placed orders can still be accepted or rejected (the acceptance timer keeps running); accepted orders progress and complete under `complete_existing`; staff cancel; consumer cancel while `placed` and refunds (`allow`) | History, CSV and `data.export` available |
| Renewal | Resumes without republishing | Accepted again | Unchanged | Unchanged |
| Administrative or security suspension | Platform admin may unpublish; publication and settings denied | Refused (`deny`) | Unchanged from expiry: `delivery.order.progress` (`complete_existing`), `delivery.order.refund` and `delivery.order.cancel` (`allow`) stay available | Retained; `data.export` available |
| Module `deprecated` | Unchanged for existing contracts | Allowed for renewing contracts; no new sales | Unchanged | Unchanged |
| Module `retired` | Unpublished | Refused | `allow` operations only (refunds, consumer cancellation) | Export available |
| Owner unpublishes storefront | `/s/{slug}` returns `404`; `/c/{slug}` unaffected | Refused | Continue in the order center | Retained |

Suspension follows the top-level `suspension` rule of `authorization-rules.json`, implemented by `OperationalStatus::blocks`: it blocks only operations whose `after_expiry` is `deny`. Time-based expiry emits no event yet (ADR-008; `SubscriptionExpired` is planned), so the API check is authoritative; a later consumer of expiry events will re-render the storefront in "not accepting orders" mode.

## 25. Acceptance criteria

1. Publishing creates `/s/{slug}/` with manifest, service worker, versioned catalog and `CatalogPublished` (`channel: "storefront"`), without changing `/c/{slug}`.
2. Storefront and catalog requests execute zero SQLite queries.
3. A consumer completes a delivery order with name, phone and address, without an account or app install.
4. Server totals equal the documented formulas for every combination of variation, add-ons, coupon, zone and free-delivery threshold.
5. A price or availability change after the cart was built returns `409 commerce.checkout_repriced` and no order until reconfirmation.
6. Retrying with the same `Idempotency-Key` returns the same order; a different body with the same key returns `422`.
7. Two staff accepting the same order produce one `OrderAccepted` and one `409`.
8. Orders are refused when closed, paused, below minimum, in an unserved zone or in a disabled mode.
9. Coupon limits hold under concurrent checkouts; redemptions are released on rejection and cancellation.
10. Unaccepted orders are rejected with `acceptance_timeout` after the configured window.
11. Carts, history and caches of two establishments on the same device never mix.
12. All UI strings exist in pt-BR, en and es with `data-i18n` hooks; no inline scripts or styles.
13. After expiry, quotes, checkout, settings and publication are refused while placed orders can still be accepted or rejected and accepted orders completed, cancelled and refunded.
14. Under administrative or security suspension only the `deny` operations are refused; `delivery.order.progress`, `delivery.order.refund` and `delivery.order.cancel` keep working.
15. With QR Kitchen, the first `KitchenTicketStarted` of an order moves it to `preparing` and emits `OrderPreparing`; the last `KitchenTicketReady` moves it to `ready`.
16. Owners, managers, cashiers and waiters can accept, fast-path to `ready`, complete and confirm pay-on-delivery payments; only owners and managers cancel accepted orders or record refunds.
17. LGPD export and erasure work, keeping de-identified financial records.
18. Every published event matches its registry payload and contains no PII.
19. Commerce Core benchmarks pass with Delivery traffic.

## 26. Test scenarios

| # | Scenario | Expected |
|---|---|---|
| T1 | Order with product id of another establishment | Same error as unknown id; nothing written |
| T2 | Client sends lower unit price | Ignored; server total used |
| T3 | Owner republishes with new price between quote and order | `409` repriced; old snapshot JSON still loadable |
| T4 | Sold-out toggle during checkout | `409` with line unavailable |
| T5 | Network retry of checkout (same key) | One order, same `201` body |
| T6 | Same key, different lines | `422 commerce.idempotency_mismatch` |
| T7 | 100 concurrent checkouts on a coupon with 10 redemptions | Exactly 10 orders with discount |
| T8 | Percent coupon with half-centavo result | Rounded half-up |
| T9 | Subtotal exactly at minimum / one centavo below | Accepted / `422 delivery.below_minimum` |
| T10 | Subtotal at free-delivery threshold | Fee 0 |
| T11 | Order at 23:59 in an interval ending 00:30 | Accepted, numbered in the opening business date |
| T12 | Manual pause active | `422 delivery.closed` |
| T13 | Consumer cancels after acceptance | `409`; order unchanged |
| T14 | Consumer cancels with tracking id but wrong cancel token | `403` |
| T15 | Two concurrent accept clicks | One `303`, one `409` |
| T16 | Cashier or waiter marks an `accepted` order ready (no Kitchen) | Allowed (`accepted → ready`) |
| T17 | Cashier marks a `preparing` order ready | Denied (not an actor of `preparing → ready`) |
| T18 | Waiter accepts a placed order; waiter cancels an accepted order | Allowed; denied (owner/manager only) |
| T19 | `KitchenTicketStarted` for an `accepted` order, then delivered again | One `accepted → preparing` and one `OrderPreparing` |
| T20 | `KitchenTicketStarted`/`KitchenTicketReady` with null `order_id` | Ignored |
| T21 | `DeliveryCompleted` delivered twice | One `OrderCompleted` |
| T22 | `DeliveryCompleted` with null `order_id` | No-op |
| T23 | `KitchenTicketReady` after staff already marked ready | No-op |
| T24 | Order completed with cash payment pending | Payment stays `pending`; flagged |
| T25 | Waiter confirms a cash payment; courier calls the confirm endpoint | Allowed; denied until the open decision in section 11 assigns couriers an operation |
| T26 | Refund above confirmed amount | Refused |
| T27 | Capability expired, consumer requests quote | `403 delivery.ordering_unavailable` |
| T28 | Capability expired, staff accepts a placed order / completes an accepted order | Allowed |
| T29 | Capability expired, consumer cancels a placed order | Allowed |
| T30 | Capability expired, owner republishes storefront or updates settings | Denied |
| T31 | Establishment suspended: checkout, publish; staff completes and refunds an existing order | Denied; allowed |
| T32 | Service worker of `/s/a/` and page `/s/b/` | Not controlled; caches separate |
| T33 | `localStorage` with carts for two establishments | Each storefront reads only its own keys |
| T34 | Erase customer with three orders | PII nulled; totals and items kept |
| T35 | Inspect events for a full order lifecycle | No names, phones or addresses |
| T36 | Cross-site `POST` to orders endpoint | Rejected |
| T37 | Slug change attempt after storefront-only publication | `error.slug_locked` |
| T38 | Storefront published twice with the same version (retry) | One `CatalogPublished` (dedupe channel + establishment id + version) |
| T39 | i18n parity test and CSP scan of storefront templates | Pass |

## 27. Commercial launch requirements

1. All release gates in `delivery.json`: Commerce Core order engine with idempotent checkout; price and availability revalidation against the published snapshot; per-establishment service worker and cart isolation; load test of concurrent order writes; LGPD export and erasure for customers.
2. Commerce Core phase 3 completion criteria met.
3. Open decisions in sections 11 and 16.3 (courier payment confirmation, storefront unpublish operation, remaining endpoints) settled in the contracts, and a JSON Schema in `contracts/events/schemas/` for every Delivery event before it is emitted.
4. Lifecycle advanced per `module-lifecycle.json`: `planned → development → internal_preview` (explicit grants to 5–10 pilot establishments for at least two weeks with zero lost orders) `→ beta` (explicit enablement, first sales) `→ available`. From `beta` on, the version 1 price is hash-locked (ADR-007).
5. SaaS checkout sells the module and QR Start through the Commercial Engine, validated in the Asaas Sandbox.
6. Consumer privacy notice and terms, establishment terms stating controller and processor roles and the absence of commission.
7. Lighthouse and physical-device checks (Android Chrome install, iOS Safari add-to-home-screen and push), WCAG AA contrast and keyboard access.
8. Benchmarks recorded in `docs/performance/PERFORMANCE.md`; runbook entries for storefront republish-all (skipping storefronts with unpublished changes, like the menu `republish-all`), dead-letter handling and order-center outage fallback.
9. `HTTP.md`, `DATABASE.md`, `PUBLISHING.md`, `SECURITY.md`, `SEO.md` (sitemap and canonical for `/s/{slug}/`), registry statuses and `PROJECT_STATE.md` updated.
