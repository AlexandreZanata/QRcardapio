# Commercial Engine

Status: **Implemented and tested** (phase 1). Asaas sandbox homologation of the new contract path is **pending** (no credentials in this environment). Decision record: [ADR-007](../decisions/ADR-007-versioned-commercial-catalog.md).

The Commercial Engine turns a list of offer codes and a billing cycle into a priced, versioned, immutable contract. It is decoupled from operational modules: no module code reads prices, and no access check reads plan names.

## Concepts

| Concept | Where | Notes |
|---|---|---|
| Commercial product / module | `contracts/commercial/modules.json`, `ModuleDef` | Nine modules; only `menu` is `available`. |
| Capability | `domain::entitlements::Capability` | What a module grants. See [ENTITLEMENT_ENGINE](ENTITLEMENT_ENGINE.md). |
| Add-on / service | `addons.json`, `AddonDef` | `recurring`, `one_time_quote`, `custom_quote`, `usage_based`. Only recurring add-ons can be quoted. |
| Price version | `CapabilityVersion`, `BundleVersion` | Monthly price + capabilities (modules/add-ons) or pinned module versions (bundles). |
| Bundle | `bundles.json`, `BundleDef` | Composition pinned per version. |
| Billing cycle | `BillingCycle`, `billing-cycles.json` | Months are structural; discounts (bps) and gateway codes come from the contract. |
| Lifecycle | `Lifecycle` | `planned` … `retired`; sale and grant policies in `module-lifecycle.json`. |
| Quote | `domain::commercial::Quote` | Backend-only pricing of a request. |
| Contract (subscription version) | `commercial_contracts` | One per checkout; immutable snapshot of the quote. |
| Contract item | `commercial_contract_items` | One line per offer: version, monthly price, list, discount, final price, commission eligibility. |
| Contracted capabilities | `commercial_contract_capabilities` | Snapshot of what each line grants. |
| Gateway subscription and payments | `billing_checkouts`, `billing_payments` | Unchanged legacy tables; still the gateway source of truth ([BILLING](BILLING.md)). |
| Credits, adjustments, invoices, usage | — | **Planned.** See [SUBSCRIPTION_RULES](../business/SUBSCRIPTION_RULES.md). |

## Flow

```text
browser: offer=menu&plan=annual  ──▶  http::handlers::billing::checkout
                                          │ parse_offer / BillingCycle::from_key (codes only, never amounts)
                                          ▼
                     application::commercial::quote_checkout ──▶ CommercialCatalog::quote(Standard)
                                          │   validates: known, sellable, no duplicates, no overlap
                                          ▼
          application::billing::start_checkout
             ├─ queries::billing::reserve_contract_checkout   (BEGIN IMMEDIATE)
             │     ├─ INSERT billing_checkouts (price_centavos = quote.total)
             │     └─ commercial_contracts::insert_contract (contract + items + capabilities)
             └─ AsaasClient::create_checkout(quote)  → one consolidated recurring item, cycle from catalog
webhooks ──▶ process_webhook (same transaction): link, record payment, affiliate sync,
             commercial_contracts::activate_for_subscription → SubscriptionActivated (outbox, deduplicated)
```

The reservation happens before the gateway call and is protected by the existing unique partial index (`idx_billing_one_current_checkout`) and the "no current coverage" guard, so timeouts, retries and double clicks never create a second charge.

## Pricing rule

```text
line.list  = monthly_price × cycle.months
line.price = (line.list × (10000 − cycle.discount_bps) + 5000) / 10000   // integer, round half up
total      = Σ line.price
```

`apply_discount` (`src/domain/commercial/cycle.rs`) is the only implementation. The gateway receives `total` formatted from integer centavos; the database stores every line; the UI formats the same integers. Legacy amounts are reproduced exactly (2990, 17043, 32292).

## Catalog integrity

- `CommercialCatalog::load` + `validation.rs` reject duplicated codes, unknown or foreign capabilities, inter-module requirements, dependency cycles, invalid versions, non-positive prices, bundles priced at or above their parts, sellable bundles with unsellable modules, orphan capabilities, invalid cycles, and an unsellable entry offer.
- `queries::commercial_catalog::sync_registry` runs inside `init_pool` and refuses start-up when a sold version changed, a sold offer disappeared, a lifecycle moved illegally, or a contract references a version the catalog no longer defines.

## Legacy mapping

Migration `20261124000009_commercial_foundation.sql` creates one `legacy_plan` contract per existing `billing_checkouts` row (id = reference): QR Menu v1, original cycle, original price, list = 2990 × months, Asaas identifiers reachable through `billing_reference`, `activated_at` set when a valid payment already exists. It creates no checkout, payment, webhook or event row. Proven by `tests/commercial_migration.rs`.

## Not implemented (by design in this phase)

- Subscription changes (add/remove modules, bundle swaps, cycle changes, reactivation with new terms, credits, proration): no second sellable module exists and Asaas subscription updates are not homologated. Contracts and formulas: [SUBSCRIPTION_RULES](../business/SUBSCRIPTION_RULES.md). Planned shape: a new contract row that supersedes the current one from the next renewal date.
- Invoices and invoice items beyond Asaas charges; credits and financial adjustments; usage metering.
- Beta sales (`SaleChannel::BetaEnablement` exists in the domain but no HTTP surface uses it).
