# Subscription Rules

Rules for the SaaS subscription an establishment pays to the platform: purchase, activation, coverage, financial events, renewal, cancellation, expiry, suspension, future changes and legacy compatibility. Consumer payments to establishments are a different domain ([COMMERCIAL_MODEL.md](COMMERCIAL_MODEL.md#6-saas-billing-vs-commerce-payments)). Status labels follow [PRODUCT_VISION.md](PRODUCT_VISION.md#status-vocabulary).

Gateway: Asaas hosted recurring checkout. All Asaas flows are **Implemented, not homologated**: the local suite validates them with simulated webhooks; validation in an Asaas Sandbox account is **Blocked** (no Sandbox credentials in this environment). Operational procedure: [BILLING.md](../architecture/BILLING.md) and `docs/operations/RUNBOOK.md` (section "Habilitação do Asaas").

## 1. Core rules

| # | Rule | Status |
|---|---|---|
| 1 | One consolidated subscription per establishment and environment; one gateway subscription whose amount is the contract total, never one per module | **Implemented and tested** |
| 2 | Only offers whose lifecycle permits sale can be bought (`available`; `beta` only by explicit enablement) | **Implemented and tested** |
| 3 | Amounts are computed only by the backend from the compiled catalog | **Implemented and tested** |
| 4 | Every contract is an immutable snapshot of its terms | **Implemented and tested** |
| 5 | Access starts only after a confirmed payment equal to the contract total | **Implemented and tested** |
| 6 | Webhooks are authenticated, idempotent and safe out of order | **Implemented and tested** (locally); **Implemented, not homologated** (Asaas) |
| 7 | Partial or divergent payments never grant access | **Implemented and tested** |
| 8 | Refunds and chargebacks revoke the affected period only | **Implemented and tested** |
| 9 | Cancelling renewal preserves the paid period | **Implemented and tested** |
| 10 | Sandbox and production never share rows, events or ledgers (`environment` column everywhere) | **Implemented and tested** |
| 11 | Full history is kept: checkouts, payments, contracts and outbox events are never deleted by billing flows | **Implemented and tested** |
| 12 | Change flows (add, remove, switch bundle, change cycle, proration, credits) | **Planned** (section 8) |

## 2. Contract snapshot

Created in the same `BEGIN IMMEDIATE` transaction as the checkout reservation (`queries::billing::reserve_contract_checkout`):

- `commercial_contracts`: environment, establishment, `billing_reference` (the checkout reservation), `origin` (`checkout` or `legacy_plan`), cycle code, months and discount bps, currency, list total, total, affiliate-eligible amount, `activated_at`.
- `commercial_contract_items`: per line offer kind, code and version, monthly price, list price, discount bps, final price, affiliate eligibility. A bundle is one line; its pinned modules are expanded into capabilities.
- `commercial_contract_capabilities`: the capabilities granted by each line.

Coverage dates are not stored on the contract. They are derived from confirmed payments by the view `commercial_entitlement_periods`, so they cannot drift from the gateway. Catalog edits never alter a snapshot ([PRICING_POLICY.md](PRICING_POLICY.md#7-price-changes-and-versioning)).

## 3. Purchase flow

1. The owner selects an offer and cycle. The form posts offer codes (`menu`, `bundle:start`, `addon:custom_domain`) and a cycle; amounts are never accepted from the browser.
2. `application::commercial::quote_checkout` prices the request with `SaleChannel::Standard`.
3. `reserve_contract_checkout` inserts the `billing_checkouts` reservation and the contract snapshot before any gateway call. It inserts nothing when the establishment already has an open checkout or live subscription (unique partial index `idx_billing_one_current_checkout`) or paid coverage for today.
4. `AsaasClient::create_checkout` sends one item whose value is the contract total and the gateway cycle (`MONTHLY`, `SEMIANNUALLY`, `YEARLY`). The client uses HTTPS, fixed hosts per environment, a timeout and no automatic retries.
5. The checkout id is persisted and the owner is redirected to the Asaas-hosted page. Card data never reaches our servers.
6. Webhooks link the subscription and record payments. `commercial_contracts::activate_for_subscription` sets `activated_at` and appends `SubscriptionActivated` to the outbox only when a confirmed payment covers the contract.

Only owners may manage billing; owners and managers may view it (`billing.manage`, `billing.view` in [`authorization-rules.json`](../../contracts/entitlements/authorization-rules.json)).

## 4. Coverage period

- A confirmed payment covers `[due_date, add_months_floor(due_date, cycle_months))`. The end is exclusive. Month arithmetic clamps to the last day of the target month (SQLite `'floor'`, `domain::affiliate_math::add_months_floor`). Example: due 2026-01-31, monthly, covers through 2026-02-27; access ends on 2026-02-28.
- The billing calendar is UTC−03:00 (`billing_now`).
- Several sources may cover one capability (contract payments, explicit grants). Access holds while any source covers the date; removing one source never revokes a capability still covered by another (`domain::entitlements::resolve`).

Status: **Implemented and tested** (`tests/entitlements.rs::access_is_granted_by_confirmed_payment_and_ends_with_the_paid_period`, unit tests in `src/domain/entitlements/access.rs`).

## 5. Financial events

| Situation | Behavior | Status |
|---|---|---|
| Redelivered webhook | `(environment, event_id)` deduplicates; the second delivery is a no-op | **Implemented and tested** |
| Events out of order | Financial events for the same payment are ordered by the Asaas event date; in the same second a reversal wins over a confirmation. Payment events that arrive before the subscription link are applied once the link exists | **Implemented and tested** |
| Partial or divergent amount | `confirmed_centavos` must equal the reserved price and the contract total; otherwise no coverage, no activation and no commission | **Implemented and tested** |
| Full refund or chargeback | The affected payment stops covering its period; other paid periods remain; affiliate installments are voided or clawed back | **Implemented and tested** |
| Chargeback dispute won later | A newer confirmation for the same payment is applied by event-date ordering and restores eligible commissions | **Implemented and tested** for commissions (`tests/affiliates.rs`); coverage restoration **Implemented, not homologated** (no dedicated test) |
| `CHECKOUT_PAID` without subscription link | Owner-triggered reconciliation queries the gateway by checkout session and links the subscription | **Implemented and tested** |
| Scheduled daily reconciliation against the gateway | Compare gateway payments with local rows and report mismatches without silent adjustment | **Planned** |
| Expiry event when coverage lapses | `SubscriptionExpired` is registered as planned; time-based expiry emits no event until a scheduler exists | **Planned** |

## 6. Renewal and voluntary cancellation

- Renewal is automatic at the gateway for the snapshot amount. Each confirmed renewal extends coverage by one cycle from its due date.
- Cancelling renewal (owner, dashboard) deletes the gateway subscription. The current paid period stays active until its exclusive end; nothing is refunded automatically; data and the published menu are kept. **Implemented and tested** (`tests/entitlements.rs::chargebacks_revoke_and_cancellation_keeps_the_paid_period`), gateway call **Implemented, not homologated**.
- The cancel request carries the expected subscription id; if it changed meanwhile, the request is refused (`error.billing_subscription_changed`).

## 7. Expiry, grace and suspension

Four separate concepts. None deletes data.

| Concept | Trigger | Effect | Status |
|---|---|---|---|
| Payment grace | Renewal of a still-renewing contract is late | Capability stays `Grace` (allowed) for `payment_grace_days` after the period end. Applies only to renewing contracts, never to grants or cancelled contracts. Policy value is `0` (legacy behavior); the validator allows 0 to 30 | **Implemented and tested** (resolver); value `0` means no grace in production |
| Administrative suspension | `qrcardapio operator suspend --kind admin` | Blocks only operations whose `after_expiry` is `deny`; lifted by `operator reinstate` | **Implemented and tested** |
| Security suspension | `qrcardapio operator suspend --kind security` | Same blocking rule, separate status and audit event | **Implemented and tested** |
| Voluntary cancellation | Owner cancels renewal | Paid period preserved, then normal expiry | **Implemented and tested** |

Suspensions require a platform admin account, a reason of 3 to 400 characters, and append `EstablishmentSuspended` or `EstablishmentReinstated` to the outbox (`src/application/operator.rs`, `src/operator_cli.rs`).

### What still works after expiry or during suspension

Operations are classified in [`authorization-rules.json`](../../contracts/entitlements/authorization-rules.json):

- `deny`: creates new protected commercial activity (publish a menu, accept a new order, open a table session, register a sale). Blocked.
- `allow`: free QR Core operation or reversal of existing work (edit drafts, unpublish, view and manage billing, refunds, cancellations, LGPD export). Always available.
- `complete_existing`: finishing work started while the capability was active (progress accepted orders, close open tabs, finish assigned deliveries, close a cash session). Allowed, never creates new work. **Planned** (no implemented operation uses it yet).

Regularization (paying again) is always possible: `billing.manage` needs no capability and is not blocked by suspension. Exceptions never allow new commercial operations.

QR Menu specifics (**Implemented and tested**): a lapsed customer's published menu stays online at the same URL and QR Code under the legacy policy until a new explicit policy is defined. Drafts, preview, QR Codes and unpublishing remain available; publishing and republishing require `menu.publish`.

## 8. Subscription changes

None of these flows exists today. QR Menu is the only sellable module and updating an Asaas subscription value has not been homologated. This section defines the contracts that the implementation must follow.

| Change | Effective | Money | Status |
|---|---|---|---|
| Add a module or add-on | Next renewal (first release); immediate with proration only after full financial homologation | Scheduled: new total at renewal. Immediate: prorated charge (section 9) | **Planned** |
| Remove a module or add-on | Next renewal | No refund; paid access kept until period end | **Planned** |
| Change bundle (up) | Same as add | Prorated difference if immediate | **Planned** |
| Change bundle (down) | Next renewal | No refund | **Planned** |
| Bundle ↔ custom selection | Up if the new total is higher, otherwise down | As above | **Planned** |
| Change cycle | Next renewal only | New cycle discount applies from that renewal | **Planned** |
| Reactivate after cancellation | New checkout and new contract at the current catalog version | Full cycle price | **Implemented, not homologated**: the normal checkout accepts a new reservation once no live subscription or paid coverage exists; the refusal while a cancelled period is still covered is tested in `tests/billing.rs` |
| Cancel renewal | Immediate stop of renewal | Paid period kept | **Implemented and tested** |
| Credits | Applied to the next renewal of the same establishment and environment | Never paid out in cash; expire with the subscription | **Planned** |
| Promotional discounts | Snapshotted per line | See [PRICING_POLICY.md](PRICING_POLICY.md#10-discounts-other-than-cycle-discounts) | **Planned** |

Rules for every change:

1. A change creates a new contract snapshot; the previous contract is closed, never edited. This requires an expand migration: the current `origin` check admits only `checkout` and `legacy_plan`.
2. Only one pending change per establishment and environment.
3. Scheduled changes at next renewal ship first. Immediate prorated upgrades are explicitly not enabled before full financial homologation (Asaas Sandbox, refunds, reconciliation).
4. Downgrades never trigger refunds; access already paid stays until the period end.
5. Affiliate effects follow [COMMERCIAL_MODEL.md](COMMERCIAL_MODEL.md#9-affiliates-within-the-commercial-model).

## 9. Proration formula

Inputs, all integers:

- `S`, `E`: start and exclusive end of the current paid period, `E = add_months_floor(S, cycle_months)`.
- `D = E − S` in days: the real coverage length (28 to 31 for monthly, 181 to 184 for semiannual, 365 or 366 for annual). Never a fixed 30 days per month.
- `T`: change date in the billing calendar, `S ≤ T < E`. `R = E − T` remaining days.
- `line_price`: the line's price for the contract cycle after the applicable discounts, computed with `apply_discount` exactly as in a new quote.

```text
prorate(amount, R, D) = (2 × amount × R + D) / (2 × D)        (integer division = round half up)
charge = Σ prorate(new or upgraded lines) − Σ prorate(replaced lines)
if charge < 0: charge = 0 and credit = −charge (applied at next renewal)
```

Rounding is per line, like quotes. The adjustment is a separate one-off gateway charge; the recurring amount changes only at the next renewal.

**Worked example (hypothetical: QR Delivery and QR Start are not sellable yet).** QR Menu annual paid with due date 2026-01-10. `E` = 2027-01-10, `D` = 365. Change on 2026-07-10, `R` = 184.

- Add QR Delivery: annual line 75492. `prorate(75492, 184, 365)` = (2 × 75492 × 184 + 365) / 730 = 27781421 / 730 = 38056. Charge 38056.
- Switch to QR Start instead: new line 91692 → (2 × 91692 × 184 + 365) / 730 = 33743021 / 730 = 46223. Replaced QR Menu line 32292 → (2 × 32292 × 184 + 365) / 730 = 11883821 / 730 = 16279. Charge 46223 − 16279 = 29944.

### No duplicate charges

Implemented for checkout (**Implemented and tested**; gateway side **Implemented, not homologated**):

- The reservation row and contract are committed before the gateway call; the unique partial index prevents a second concurrent checkout; paid coverage blocks a new checkout.
- On gateway timeout the reservation stays `creating`; `CHECKOUT_CREATED` with `externalReference` recovers it; a reservation that never exposed a link can be redone after 60 minutes.
- Webhook redelivery is deduplicated by event id; affiliate rows by `(environment, payment_id, installment_index, entry_kind)`; outbox events by `dedupe_key`.

Required for change flows (**Planned**): an idempotency key (UUIDv7) stored with the pending change before any gateway call and sent as the gateway external reference; retries after timeout first reconcile by that reference; a one-off adjustment charge is created at most once per change; a failed gateway call leaves the change pending, never half-applied.

## 10. Legacy compatibility

Every "Plano Essencial" checkout became a QR Menu v1 contract (`origin = 'legacy_plan'`) through the idempotent backfill in `migrations/20261124000009_commercial_foundation.sql`:

- Contracted price, cycle, discount, coverage, Asaas identifiers (through `billing_reference`) and history are preserved; `activated_at` is set when a matching confirmed payment exists.
- No new charge, no new checkout, no card re-registration.
- Affiliate commissions, member permissions, MCP grants, published menus, branding, URLs and printed QR Codes are unchanged.
- Legacy tables `billing_checkouts`, `billing_payments`, `billing_coverage` stay and remain the gateway source of truth ([ADR-010](../decisions/ADR-010-expand-contract-migrations.md)).
- A renewal of a migrated subscription activates and keeps the contracted price.

Status: **Implemented and tested** (`tests/commercial_migration.rs::legacy_subscribers_become_menu_contracts_without_new_charges`, `::migrated_subscriber_renewal_activates_and_keeps_contracted_price`).

## 11. References

- Code: `src/application/{billing,commercial,entitlements,operator}.rs`, `src/domain/entitlements/{access,authorization}.rs`, `src/infrastructure/database/queries/{billing,commercial_contracts,entitlements,affiliate_ledger}.rs`, `src/infrastructure/asaas.rs`
- Tests: `tests/billing.rs`, `tests/entitlements.rs`, `tests/commercial_engine.rs`, `tests/commercial_migration.rs`, `tests/affiliates.rs`
- [ADR-008](../decisions/ADR-008-entitlement-engine.md), [ADR-009](../decisions/ADR-009-sqlite-transactional-outbox.md), [PRICING_POLICY.md](PRICING_POLICY.md), [RELEASE_GATES.md](../roadmap/RELEASE_GATES.md)
