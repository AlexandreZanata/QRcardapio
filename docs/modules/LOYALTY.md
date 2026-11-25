# QR Loyalty

> **Status: Planned — not implemented, not sellable.** Nothing in this document is available in production. Every endpoint, table and event below is a design contract.

| Field | Value |
|---|---|
| Module code | `loyalty` |
| Commercial name | QR Loyalty |
| Phase | 8 |
| Lifecycle | `planned` (sale: never; grants: never — see `contracts/commercial/module-lifecycle.json`) |
| Monthly price (catalog version 1) | R$ 19,90 (`monthly_price_centavos: 1990`), effective 2026-11-24, editable until first sale (ADR-007) |
| Standalone | Yes (`standalone: true`, `requires_modules: []`) |
| Machine-readable contract | [`contracts/modules/loyalty.json`](../../contracts/modules/loyalty.json) (`contract_version: 1`) |

The JSON contracts are the single source of truth ([authorization-rules.json](../../contracts/entitlements/authorization-rules.json), [registry.json](../../contracts/events/registry.json), [capabilities.json](../../contracts/entitlements/capabilities.json)); see also [MODULE_CATALOG.md](../business/MODULE_CATALOG.md) and [EVENT_CONTRACTS.md](../architecture/EVENT_CONTRACTS.md). Items marked **Open decision** are not in the contracts and must be added to the JSON (and its parity tests) before implementation.

## 1. Commercial objective

Let small establishments run a points program that brings customers back, at R$ 19,90 per month or inside the QR Business and QR Complete bundles. The program must be trustworthy: an append-only ledger, automatic points only for real completed and paid purchases, automatic reversals on refunds, and no duplicate points. It runs on the shared SQLite database with negligible marginal cost.

## 2. Target audience

- Cafés, bakeries, açaí and ice cream shops, snack bars and restaurants with repeat local customers.
- Owners and managers configuring the program; cashiers (planned `cashier` role) crediting and debiting points and redeeming rewards at the counter.
- Establishments without online ordering that identify customers by phone at the counter.
- Not targeted in the MVP: coalition programs across establishments, cashback wallets, paid membership clubs.

## 3. Problems solved

| Problem | How QR Loyalty addresses it |
|---|---|
| Paper stamp cards are lost or forged | Digital wallet per customer with an immutable statement |
| Spreadsheet balances drift and cannot be audited | Append-only ledger; balance is the sum of entries |
| Points granted for orders later cancelled or refunded | Automatic credit only after completion and payment; refunds generate traceable reversals |
| Duplicate points when systems retry | Unique source per credit; replay-safe consumers |
| Unbounded point liability | Deterministic expiration |
| No simple way to run promotions | Campaigns with multipliers or fixed bonuses, rewards and coupon-style rewards |

## 4. MVP scope

- One loyalty program per establishment: earn rule, minimum eligible amount, expiration period, active or paused.
- Customer lookup and minimal enrollment through the `customers` component, with recorded consent.
- Points wallet per customer and immutable statement (credits, debits, redemptions, reversals, expirations).
- Manual entry: credits, debits and reversals with mandatory reason (`POST /dashboard/loyalty/entries`).
- Automatic points from orders once Commerce Core exists: only when the order is completed and fully paid.
- Automatic, proportional reversal on `PaymentRefunded`.
- Rewards catalog (`product`, `coupon`, `custom`) with point cost; redemptions recorded by staff.
- Coupon-style rewards: single-use redemption codes with an expiry date.
- Campaigns: date-bounded multiplier (basis points) or fixed bonus per qualifying order.
- Deterministic expiration per credit lot.
- Exports of wallets and statements; full `pt-BR`, `en` and `es` UI.

## 5. Future scope

- Cashback (monetary balance), which requires financial liability handling and is out of the MVP.
- Consumer self-service balance page and passwordless lookup.
- Tiers, referral bonuses, birthday campaigns and notification-driven campaigns.
- Coupon validation inside QR Delivery checkout through a dedicated integration contract.
- Multi-establishment (coalition) programs. Points always stay scoped to one establishment.

## 6. Domain entities

| Entity | Purpose | Key attributes |
|---|---|---|
| `LoyaltyProgram` | Rules of the establishment program, versioned | `id`, `establishment_id`, `rule_version`, `points_per_real` (integer points per 100 centavos), `min_eligible_centavos`, `expiration_days`, `status` (`active`/`paused`) |
| `LoyaltyWallet` | Points account of one customer | `id`, `establishment_id`, `customer_id`, `balance_points` (cached projection), `opened_at`, `closed_at` |
| `LoyaltyLedgerEntry` | Immutable statement line | `id`, `establishment_id`, `wallet_id`, `seq`, `kind`, `points` (signed integer), `source_event_id`, `idempotency_key`, `source_order_id`, `reverses_entry_id` (for `expiration`, the lot it closes), `redemption_id`, `campaign_id`, `rule_version`, `expires_on`, `reason`, `created_by`, `occurred_at` |
| `Reward` | Redeemable benefit | `id`, `establishment_id`, `kind` (`product`/`coupon`/`custom`), `name` (+ `name_en`, `name_es`), `cost_points`, `product_id` (nullable), `coupon_value_centavos` or `coupon_value_bps`, `is_active` |
| `Redemption` | Use of a reward by a wallet | `id`, `establishment_id`, `wallet_id`, `reward_id`, `entry_id`, `code_hash` (coupon rewards), `code_expires_on`, `fulfilled_at`, `voided_at` |
| `Campaign` | Time-bounded earning boost | `id`, `establishment_id`, `kind` (`multiplier`/`fixed_bonus`), `multiplier_bps` or `bonus_points`, `starts_on`, `ends_on`, `weekdays` mask, `is_active` |

Ledger entry kinds: `manual_credit`, `order_credit`, `manual_debit`, `redemption`, `reversal`, `expiration`, `forfeiture`.

## 7. Responsibilities

| QR Loyalty owns | QR Loyalty does not own |
|---|---|
| Program rules, wallets and the points ledger | Customer personal data (`customers` component) |
| Earning, reversal and expiration logic | Orders, payments and refunds (Commerce Core) |
| Rewards, redemptions and campaigns | Checkout discounts and the `Coupon` entity of QR Delivery |
| Publishing `LoyaltyPointsGranted`, `LoyaltyPointsReversed`, `LoyaltyPointsRedeemed`, `LoyaltyPointsDebited`, `LoyaltyPointsExpired` | Cashback or any monetary balance |
| Loyalty statements and exports | Marketing messages (future, through `notifications`) |

## 8. Invariants

1. Every row carries `establishment_id`; wallet, customer, reward, campaign, product and order references belong to the same establishment.
2. Ledger entries are never updated or deleted; there is no `UPDATE`/`DELETE` path for `loyalty_ledger_entries`.
3. Points are integers; amounts used to compute points are integer centavos; no floating point anywhere.
4. `balance_points` equals the sum of the wallet's entries and is updated in the same transaction as each append.
5. Each automatic credit has a unique `source_event_id`, and at most one `order_credit` exists per `(establishment_id, source_order_id)`.
6. A reversal references the entry it reverses; total reversed points of an entry never exceed its unexpired credited points.
7. A redemption requires `balance_points >= cost_points` after due expirations are applied, and a non-negative balance.
8. Expiration of a credit lot is computed only from its `expires_on` and its recorded consumption, never from the time the sweep runs.
9. One wallet per `(establishment_id, customer_id)`; one program per establishment.

## 9. Business rules

- **Earn rule**: `points = floor(eligible_centavos * points_per_real / 100)`, then campaign effects: `floor(points * multiplier_bps / 10000)` or `+ bonus_points`. The rule version and campaign id are stored on the entry.
- **Eligible amount**: order item subtotal after discounts, excluding delivery fees, capped at the net confirmed payment amount; orders below `min_eligible_centavos` earn nothing.
- **Automatic credit condition**: the order is `completed` **and** fully paid (net confirmed payments cover the order total). The two facts arrive as different aggregates in any order; the consumer re-reads both through Commerce Core and credits when the second one is observed. Anonymous orders and customers without loyalty consent earn nothing.
- **Refunds**: on `PaymentRefunded`, `should_remain = earn(eligible − cumulative_refunded)`; a reversal of `credited − already_reversed − already_expired − should_remain` points is appended when positive. Chargebacks lost arrive as `PaymentRefunded` and follow the same rule.
- **Negative balance**: reversals may make a balance negative when points were already redeemed; redemptions are blocked until it is positive again; fulfilled rewards are never clawed back.
- **Cancellation**: `OrderCancelled` marks the order ineligible, so a late `PaymentConfirmed` never credits it. No reversal is needed: automatic points are credited only after `OrderCompleted` (terminal), so the `order` machine has no loyalty compensation; refunds after completion reverse points through `PaymentRefunded`.
- **Expiration**: each credit lot expires at the end of `occurred_at` local date + `expiration_days` (establishment time zone, default `America/Sao_Paulo`). Debits and redemptions consume lots first-expiring-first-out, ties by `seq`.
- **Manual entries**: reason required (up to 140 characters); per-entry cap of 10 000 points and a per-member daily cap configurable by the owner; each form carries an idempotency key.
- **Redemption codes**: coupon rewards produce an eight-character code, shown once, stored as a hash, valid until `code_expires_on`; staff mark it fulfilled. Voiding an unfulfilled redemption appends a reversal that restores the points.

## 10. State machines

`loyalty.json` declares no state machine (`"state_machines": []`) and `contracts/commerce/state-machines.json` has none for loyalty. The ledger is append-only, so state is expressed by entries rather than transitions. The only set-once facts are `Redemption.fulfilled_at` and `Redemption.voided_at` (mutually exclusive), `LoyaltyWallet.closed_at`, and program or campaign `status` flags, none of which emit events.

| Entry kind | Ledger sign | Event emitted | Operation |
|---|---|---|---|
| `manual_credit`, `order_credit` | positive | `LoyaltyPointsGranted` | `loyalty.points.credit` (automatic credits by the planned `system` role) |
| `reversal` | opposite of the reversed entry | `LoyaltyPointsReversed` | `loyalty.points.reverse` (automatic reversals by `system`) |
| `redemption` | negative | `LoyaltyPointsRedeemed` | `loyalty.reward.redeem` |
| `manual_debit` | negative | `LoyaltyPointsDebited` | `loyalty.points.debit` |
| `expiration` | negative | `LoyaltyPointsExpired` | none; deterministic sweep by `system` |
| `forfeiture` | negative | none in the registry (**Open decision**) | none; LGPD erasure flow |

The ledger stores signed deltas; event payloads carry `points` as a positive magnitude and the event type gives the direction (registry rule).

## 11. Permissions

| Action | Operation | Roles | Capability | After expiry |
|---|---|---|---|---|
| Manual credit | `loyalty.points.credit` | `owner`, `manager`, `cashier` | `loyalty.points.manage` | `deny` |
| Automatic credit from events | `loyalty.points.credit` | planned `system` role (outbox consumer) | `loyalty.points.manage`, re-checked by the consumer as of the triggering event's `occurred_at` | `deny` |
| Manual debit | `loyalty.points.debit` | `owner`, `manager`, `cashier` | `loyalty.points.manage` | `deny` |
| Manual reversal, void of an unfulfilled redemption | `loyalty.points.reverse` | `owner`, `manager` | none | `allow` |
| Automatic reversal on `PaymentRefunded` | `loyalty.points.reverse` | planned `system` role | none | `allow` |
| Redeem a reward, fulfill a code | `loyalty.reward.redeem` | `owner`, `manager`, `cashier` | `loyalty.reward.manage` | `deny` |
| Configure program rules, rewards and campaigns | `loyalty.reward.configure` | `owner`, `manager` | `loyalty.reward.manage` | `deny` |
| View wallets, balances and statements | read only | `owner`, `manager`, `cashier` | none | allowed |
| Export wallets and statements | `data.export` (QR Core operation, not a Loyalty operation) | `owner` | none | `allow` |

Every check runs through `authorize(...)` in the contract order. The planned `system` role (scope `platform`) represents idempotent outbox consumers acting for one establishment: they run only when the triggering event was produced under a valid operation and re-check the consumer module's own capability. The expiration sweep and LGPD forfeiture are `system` actions with no member operation. While the module is `planned`, every operation is denied even with an `entitlement_grants` row.

## 12. Commercial capabilities

| Capability | Summary (from `capabilities.json`) |
|---|---|
| `loyalty.points.manage` | Credit, debit and reverse loyalty points. |
| `loyalty.reward.manage` | Configure rewards, campaigns and redemptions. |

- Sold alone at 1990 centavos per month (catalog version 1), or inside QR Business (16990) and QR Complete (21990). Not part of QR Start.
- Billing cycles: monthly, semiannual (`discount_bps: 500`), annual (`discount_bps: 1000`), rounding `half_up_per_line`. Affiliate eligible.
- Points have no monetary value and are not sold; cashback is future scope.

## 13. Technical dependencies

| Core component | Status (`components.json`) | Use |
|---|---|---|
| `identity`, `establishments`, `members`, `tenancy`, `i18n` | implemented | Sessions, CSRF, `TenantContext` |
| `commercial`, `entitlements` | implemented | `authorize(...)` |
| `events` | partial | Outbox append exists; consumption needs the dispatcher (Commerce Core, phase 3) |
| `customers` | planned | Customer identity, consent, LGPD export and erasure |
| `notifications` | planned | Optional balance and expiration notices (future campaigns) |
| `orders`, `order_payments` | planned | Read port for order state, eligible subtotal, net confirmed and refunded centavos (automatic points) |

Optional integrations: `delivery`, `tables`, `pos`, `analytics`, `connect`. Expiration needs an in-process periodic task, like the existing session prune. The planned `cashier` role needs an expand migration for member roles, and the planned `system` role needs the outbox dispatcher.

## 14. Published events

All events are appended to `domain_events` in the same transaction as the ledger entry (`aggregate_type = loyalty_wallet`, ordered by `seq` per wallet).

| Event | Trigger | Payload (registry) | Design `dedupe_key` |
|---|---|---|---|
| `LoyaltyPointsGranted` | `manual_credit` or `order_credit` appended | `wallet_id`, `entry_id`, `points`, `source_event_id` (nullable string) | `loyalty_credit:{source_event_id}` or `loyalty_credit:manual:{idempotency_key}` |
| `LoyaltyPointsReversed` | `reversal` appended | `wallet_id`, `entry_id`, `reverses_entry_id`, `points` | `loyalty_reversal:{source_event_id}` or `loyalty_reversal:manual:{idempotency_key}` |
| `LoyaltyPointsRedeemed` | `redemption` appended | `wallet_id`, `entry_id`, `reward_id`, `points` | `loyalty_redemption:{redemption_id}` |
| `LoyaltyPointsDebited` | `manual_debit` appended | `wallet_id`, `entry_id`, `points` | `loyalty_debit:{idempotency_key}` |
| `LoyaltyPointsExpired` | `expiration` appended for one credit lot | `wallet_id`, `entry_id`, `points`, `expired_on` | `loyalty_expiration:{credit_entry_id}` |

`points` is always a positive magnitude; the event type gives the direction. `source_event_id` is null for manual credits. All five events are `planned` in the registry and have no JSON Schema file yet. Payloads never contain customer names, phones or emails. Envelope and dispatch rules: [EVENT_CONTRACTS.md](../architecture/EVENT_CONTRACTS.md).

## 15. Consumed events

| Event | Producer | Effect | Idempotency |
|---|---|---|---|
| `OrderCompleted` | Commerce Core | Evaluate the credit condition for the order | Dispatcher record by `event_id`; unique `source_event_id`; unique `order_credit` per order |
| `PaymentConfirmed` | Commerce Core payments | Evaluate the credit condition for the order | Same as above |
| `PaymentRefunded` | Commerce Core payments | Append the proportional reversal, if any (refunds after completion and lost chargebacks) | Unique `source_event_id` on the reversal |
| `OrderCancelled` | Commerce Core | Mark the order ineligible so a late `PaymentConfirmed` never credits it; no reversal, because nothing was credited before completion | Ineligibility is an upsert |

All consumers run as the planned `system` role and re-check the Loyalty capability before appending a credit; reversals need no capability.

## 16. HTTP contracts

Contract endpoints (all **planned**, none available):

| Method | Path | Audience | Purpose |
|---|---|---|---|
| `GET` | `/dashboard/loyalty` | member | Program overview, customer lookup and recent entries |
| `POST` | `/dashboard/loyalty/entries` | member | Manual credit, debit or reversal (`kind`, `wallet_id` or customer, `points`, `reason`, `idempotency_key`, `_csrf`) |

`POST /dashboard/loyalty/entries` maps `kind` to `loyalty.points.credit`, `loyalty.points.debit` or `loyalty.points.reverse`.

**Open decision**: endpoints not yet in `loyalty.json`: `GET /dashboard/loyalty/wallets/{id}` (statement, paginated), `GET`/`POST /dashboard/loyalty/program` and `/dashboard/loyalty/rewards` (`/{id}`), `GET`/`POST /dashboard/loyalty/campaigns` (all `loyalty.reward.configure`), `POST /dashboard/loyalty/redemptions` and `/dashboard/loyalty/redemptions/{id}/fulfill` (`loyalty.reward.redeem`), `/dashboard/loyalty/redemptions/{id}/void` (`loyalty.points.reverse`). Exports use the QR Core `data.export` surface.

Rules: `TenantContext` on every route; ids resolved with `AND establishment_id = ?`, foreign ids return `404`; `_csrf` on every `POST`; a repeated `idempotency_key` returns the original result; insufficient balance returns `409` with the current balance; errors map to i18n keys.

## 17. Integration contracts

- **Commerce Core read port**: `loyalty_order_view(establishment_id, order_id)` returns state, customer id, eligible subtotal, total, net confirmed and refunded centavos.
- **Customers port**: lookup by normalized phone or email, minimal creation, consent flag, erasure callback. Loyalty stores only `customer_id`.
- **Order cancellation**: the `order` machine has no loyalty compensation, because automatic points are credited only after `completed`, which is terminal. Refunds after completion reverse points through `PaymentRefunded`.
- **QR Analytics** consumes `LoyaltyPointsGranted` and `LoyaltyPointsReversed`; redemptions, debits and expirations are not consumed.
- **QR Delivery coupons**: QR Delivery owns the checkout `Coupon` entity. Until a dedicated contract exists, coupon rewards are validated by staff in the Loyalty panel.

## 18. Conceptual persistence model

| Table | Notes |
|---|---|
| `loyalty_programs` | `UNIQUE (establishment_id)`; rule history in `loyalty_program_versions` (append-only) |
| `loyalty_wallets` | `UNIQUE (establishment_id, customer_id)` |
| `loyalty_ledger_entries` | `UNIQUE (source_event_id)` where not null; partial unique `(establishment_id, source_order_id) WHERE kind = 'order_credit'`; `UNIQUE (establishment_id, idempotency_key)`; partial unique `(reverses_entry_id) WHERE kind = 'expiration'`; index `(establishment_id, wallet_id, seq)` |
| `loyalty_lot_allocations` | Append-only `(debit_entry_id, credit_entry_id, points)` for first-expiring-first-out consumption |
| `loyalty_order_eligibility` | `(establishment_id, order_id, ineligible_at)` for cancelled orders |
| `loyalty_rewards`, `loyalty_redemptions`, `loyalty_campaigns` | Tenant-scoped; `code_hash` unique per establishment |

Ids are `TEXT`; writes use `BEGIN IMMEDIATE`; triggers reject `UPDATE` and `DELETE` on the ledger and allocations; migrations are expand-only (ADR-010); SQL lives in `src/infrastructure/database/queries/loyalty_*.rs`.

## 19. Security requirements

- Tenant isolation on every query and cross-check of wallet, customer, reward, campaign and order ownership.
- CSRF on every `POST`; idempotency keys on manual entries and redemptions.
- Manual credit caps and per-member rate limits (30 entries per minute) to contain insider abuse; every manual entry records the member and reason.
- Redemption codes are random, hashed at rest, compared in constant time, single-use and attempt-limited.
- Strict CSP: no inline scripts or styles; statements rendered server-side with Askama escaping.

## 20. Privacy requirements

- Loyalty stores no names, phones or emails; it references `customer_id` from the `customers` component.
- Enrollment requires recorded consent; automatic points accrue only for consenting customers. Marketing messages need separate opt-in (future).
- LGPD export includes the wallet statement; erasure anonymizes the customer record, appends a `forfeiture` entry for any remaining balance and closes the wallet, keeping pseudonymous ledger rows for accounting integrity.
- Events carry identifiers and integer points only.

## 21. Performance requirements

- Balance reads are O(1) from `balance_points`; statements are paginated by `seq`.
- An append (entry, allocations, wallet projection, outbox row) is one short `BEGIN IMMEDIATE` transaction; p95 under 20 ms on 1 vCPU.
- The expiration sweep runs hourly in batches of at most 500 lots per transaction, ordered by `expires_on`, and also runs for a single wallet before any redemption or debit.
- Event consumers read one order view per event; no N+1 queries over wallets.

## 22. Behavior without other modules

Fully usable with QR Core plus the `customers` component: enroll customers at the counter, credit points manually after a sale, redeem rewards, run campaigns on manual credits and let points expire. `LoyaltyPointsGranted` carries a null `source_event_id` for manual credits.

## 23. Behavior integrated with other modules

- With QR Delivery, QR Tables or QR POS, completed and fully paid orders credit points automatically; refunds reverse them proportionally.
- QR Analytics receives `LoyaltyPointsGranted` and `LoyaltyPointsReversed`; QR Connect consumes no loyalty events in v1.
- Manual entry stays available for purchases made outside QR Cardápio.

## 24. Behavior after suspension or cancellation

- **Establishment suspended** (administrative or security): the contract `suspension` rule blocks only `after_expiry: deny` operations (`OperationalStatus::blocks`). Credits (manual and automatic), `loyalty.points.debit`, `loyalty.reward.redeem` and `loyalty.reward.configure` are refused; `loyalty.points.reverse` (including automatic reversals on refunds), balance reads and `data.export` stay available. Expiration continues deterministically because it does not depend on the sweep time.
- **Subscription expired or cancelled** (contract `after_expiry`): "No new credits or redemptions. Reversals, balances and exports stay available." Automatic credits for events whose `occurred_at` falls after expiry are skipped; refunds still reverse points.
- **Module `retired`**: no operations are granted; balances and exports stay readable.
- **Open decision**: fulfilling a coupon code redeemed before expiry or suspension falls under `loyalty.reward.redeem` (`deny`); the contract has no `complete_existing` path for it.

## 25. Acceptance criteria

1. With QR Core and `customers` only, a cashier enrolls a consenting customer, credits points manually and the owner sees the statement.
2. A completed and paid order credits points exactly once, whatever the delivery order or repetition of `OrderCompleted` and `PaymentConfirmed`.
3. A partial refund appends a proportional reversal; a full refund leaves the order contribution at zero.
4. No code path updates or deletes a ledger entry; the cached balance always equals the sum of entries.
5. Expiration produces the same entries whether the sweep runs on time or days later.
6. A redemption with insufficient balance returns `409` and appends nothing.
7. After expiry or during suspension, credits, debits, redemptions and configuration are refused with distinct i18n errors, while reversals and `data.export` still work.
8. Every ledger append emits its registry event (`LoyaltyPointsGranted`, `LoyaltyPointsReversed`, `LoyaltyPointsRedeemed`, `LoyaltyPointsDebited` or `LoyaltyPointsExpired`) in the same transaction, with a positive `points` magnitude.
9. A cancelled order never earns points and needs no loyalty compensation.
10. All UI strings exist in `pt-BR`, `en` and `es`; templates have no inline scripts or styles.

## 26. Test scenarios

| Scenario | Type |
|---|---|
| Earn rule, campaign multiplier and bonus with integer floor rounding | unit (domain) |
| First-expiring-first-out allocation and expiration remainder | unit (domain) |
| `PaymentConfirmed` before `OrderCompleted`, and the reverse order, credit once | integration |
| Replaying all consumed events produces no new entries | integration |
| Partial refunds, full refund and refund after expiration never over-reverse | integration |
| Refund after redemption yields a negative balance and blocks redemption | integration |
| Cross-tenant wallet id returns `404` | integration |
| Repeated manual entry with the same idempotency key appends once | integration |
| `OrderCancelled` then late `PaymentConfirmed` credits nothing | integration |
| Expiration sweep emits one `LoyaltyPointsExpired` per lot, even when rerun | integration |
| `cashier` may credit, debit and redeem but not run `loyalty.reward.configure` or `loyalty.points.reverse` | integration |
| Suspended establishment: credit denied, reversal and `data.export` allowed | integration |
| `planned` lifecycle denies every operation even with a grant | integration |
| i18n key parity for every `loyalty.*` key | contract |
| Expiration sweep of 100 000 lots on 1 vCPU | benchmark |

## 27. Commercial launch requirements

- Release gates from the contract: "ledger is append-only with unique source event per credit", "reversal on refund is automatic", "expiration is deterministic".
- Open decisions resolved in the contracts before launch: JSON Schemas for the five loyalty events, the extra HTTP endpoints of section 16, an event for `forfeiture`, and fulfilment of codes redeemed before expiry.
- Prerequisites shipped: event dispatcher, `orders` and `order_payments`, `customers` with consent and LGPD flows, the `cashier` and `system` roles.
- Program terms template in three languages stating that points have no cash value and expire.
- Lifecycle progression `planned → development → internal_preview → beta → available`; price version hashed once sellable (ADR-007); docs and `docs/agents/PROJECT_STATE.md` updated.
