# Commercial Model

How QR Cardápio makes money, what it sells, what it never charges for, and how commercial terms reach code. Status labels follow [PRODUCT_VISION.md](PRODUCT_VISION.md#status-vocabulary): **Implemented and tested**, **Implemented, not homologated**, **Planned**, **Blocked**.

Sources of truth: [`contracts/commercial/`](../../contracts/commercial/), [`contracts/entitlements/`](../../contracts/entitlements/), [`contracts/core/components.json`](../../contracts/core/components.json). Human price table: [MODULE_CATALOG.md](MODULE_CATALOG.md).

## 1. Revenue model in one paragraph

The platform earns recurring SaaS revenue from establishments: one consolidated subscription per establishment whose total is the sum of the contracted modules, bundles and recurring add-ons, discounted by billing cycle. One-off services (assisted onboarding) and custom contracts (exclusive apps, exclusive integrations) are quoted separately. Third-party costs are passed through under a published usage policy. The platform takes no commission on consumers' food orders.

## 2. What is sold

| Offer kind | Examples | Pricing | Status |
|---|---|---|---|
| Module | `menu`, `delivery`, `tables`, `kitchen`, `fleet`, `loyalty`, `analytics`, `pos`, `connect` | Recurring, monthly list price × cycle months, cycle discount | `menu` **Implemented and tested**; others **Planned** |
| Bundle | `start`, `business`, `complete` | Recurring, one bundle price, cycle discount | **Planned** (priced, not sellable) |
| Recurring add-on | `custom_domain` | Recurring | **Planned** |
| Quoted service | `assisted_onboarding` | One-off quote, never in the recurring total | **Planned** |
| Custom contract | `native_apps`, `exclusive_integrations` | Custom quote | **Planned** |
| Usage pass-through | `external_usage` | Usage-based, published policy | **Planned** |

Monthly list prices (centavos): menu 2990, delivery 6990, tables 3990, kitchen 2490, fleet 2990, loyalty 1990, analytics 1990, pos 6990, connect 2490 (sum 32910). Bundles: QR Start 8490, QR Business 16990, QR Complete 21990. Custom domain 1490. Arithmetic and examples: [PRICING_POLICY.md](PRICING_POLICY.md).

Quoted, custom and usage-based add-ons are rejected by the self-service quote engine (`QuoteError::NotSelfService` in `src/domain/commercial/quote.rs`). **Implemented and tested** (`src/application/commercial.rs` test `only_available_modules_can_be_quoted`).

## 3. QR Core is free and never sold

QR Core is the shared foundation listed in [`components.json`](../../contracts/core/components.json): identity, establishments, members, permissions, tenancy, operational state, i18n, catalog, media, commercial, entitlements, SaaS billing, events, audit and publishing are implemented or partial; business hours, commercial settings, customers, addresses, orders, order payments, notifications and usage metering are planned.

Rules:

1. QR Core is never an offer in the catalog and never appears on an invoice.
2. Any technical prerequisite shared by two modules belongs to QR Core, so no module has to buy another.
3. Free core infrastructure is not free commercial operation. Operations that create new protected commercial activity (publish a menu, accept an order, open a table session) require the selling module's capability.
4. Free operations stay free forever: draft editing, QR Code generation, unpublishing, billing view and management, LGPD export (planned operation `data.export`). Reclassification decisions are recorded in [`capabilities.json`](../../contracts/entitlements/capabilities.json) (`reclassified`).

Abuse limits on the free tier (`free_tier_limits`):

| Limit | Status |
|---|---|
| Rate limits per IP and per account on auth, uploads, OCR and billing | **Implemented and tested** |
| Upload caps (2 MiB logos, 5 MiB photos, 16 MiB OCR, 64 KiB webhooks) | **Implemented and tested** |
| OCR concurrency `Semaphore(1)` with subprocess timeouts; mandatory review before OCR reaches the draft | **Implemented and tested** |
| Per-establishment storage quota | **Planned** |
| Monthly OCR page quota | **Planned** |
| Notification and external-cost quotas | **Planned** |

## 4. Composition rules

A customer may buy one module, any combination of modules, one bundle, the complete package, and optional add-ons. The quote engine (`CommercialCatalog::quote`) enforces:

| Rule | Status |
|---|---|
| Only offers whose lifecycle sale policy allows it can be quoted (`available` = standard; `beta` = explicit enablement only) | **Implemented and tested** |
| An offer cannot appear twice in one quote | **Implemented and tested** |
| Two offers cannot cover the same module (for example QR Start plus QR Delivery) | **Implemented and tested** |
| A bundle is never more sellable than its least available module | **Implemented and tested** |
| No commercial module requires another commercial module | **Implemented and tested** (validator) |
| Bundle price must be strictly below the sum of its modules | **Implemented and tested** (validator) |
| The browser sends offer codes and a cycle, never amounts | **Implemented and tested** (`tests/commercial_engine.rs::submitted_offer_codes_cannot_carry_prices`) |
| Combining a bundle with modules outside it in one subscription | Allowed by the engine; **Planned** for the UI (only QR Menu is sellable) |

## 5. One consolidated subscription per establishment

- Each establishment has at most one current gateway subscription per environment, whose amount is the contract total. Modules never get separate gateway subscriptions.
- The commercial contract (`commercial_contracts` + `commercial_contract_items` + `commercial_contract_capabilities`) is an immutable snapshot of the terms: offer kind, code and version, monthly price, list price, discount in basis points, final line price, currency, cycle, capabilities, bundle code and affiliate-eligible amount. Start and end dates come from confirmed payments.
- The legacy unique partial index `idx_billing_one_current_checkout` guarantees one open checkout or live subscription per establishment and environment.

Status: **Implemented and tested** for single-offer contracts (QR Menu). Multi-line and bundle quotes are supported by the schema and the quote engine and are exercised in `tests/commercial_engine.rs` against a test catalog in which QR Delivery and QR Start are made sellable, but no multi-line offer is sellable in the real catalog yet. Asaas side: **Implemented, not homologated**.

## 6. SaaS Billing vs Commerce Payments

| Aspect | SaaS Billing | Commerce Payments |
|---|---|---|
| Payer → payee | Establishment → platform | Consumer → establishment |
| What | Subscriptions, modules, bundles, add-ons, adjustments | Orders, counter sales, tabs, refunds |
| Gateway account | Platform's central Asaas account | The establishment's own financial account (planned) |
| Tables | `billing_*`, `commercial_contract*`, affiliate ledger | `order_payments`, `payment_refunds`, `financial_accounts` (planned) |
| Status | **Implemented, not homologated** | **Planned** (contracts only) |

Hard rules:

1. Never share credentials, beneficiaries, state machines or ledgers between the two domains.
2. The platform's central Asaas account must never collect an establishment's sales, not even temporarily.
3. Offline methods (cash, card machine, pay on delivery) are recorded, not processed, in phase 4.
4. Online consumer payments (dynamic Pix, card) require per-establishment financial accounts and daily reconciliation; they are contracts only until phase 12. See [COMMERCE_CORE.md](../architecture/COMMERCE_CORE.md#9-saas-billing-vs-commerce-payments).
5. No platform commission on food orders. Gateway fees, third-party fees and logistics fees are contracted and paid by the establishment directly to those providers and are never part of the subscription total.

## 7. Capabilities, lifecycle and access

Code authorizes capabilities, never plan names. An establishment holds a capability on a date when a paid contract or an explicit grant covers it ([ADR-008](../decisions/ADR-008-entitlement-engine.md)).

| Lifecycle | Sale | Grants access | Renewals |
|---|---|---|---|
| `planned` | never | never | no |
| `development` | never | never | no |
| `internal_preview` | never | explicit `internal_preview` grant only | no |
| `beta` | explicit enablement | yes | yes |
| `available` | standard self-service | yes | yes |
| `deprecated` | never (no new sales) | yes | yes |
| `retired` | never | never | no |

Source: [`module-lifecycle.json`](../../contracts/commercial/module-lifecycle.json), enforced by `src/domain/commercial/lifecycle.rs`. **Implemented and tested** (`tests/commercial_contracts.rs::lifecycle_contract_matches_the_rust_rules`, `tests/entitlements.rs::planned_modules_never_grant_operations`).

### Explicit grants

Issued only by a platform admin through `qrcardapio operator grant`, audited with a reason and an outbox event (`src/application/operator.rs`).

| Grant kind | Allowed lifecycle | Max days | Extra rule |
|---|---|---|---|
| `trial` | `available` | 31 | At most one per establishment and offer, ever (unique index) |
| `beta` | `beta` | 366 | — |
| `internal_preview` | `internal_preview`, `beta` | 366 | — |
| `courtesy` | `available`, `deprecated` | 366 | — |

Status: **Implemented and tested** (`tests/entitlements.rs::grants_are_an_independent_audited_source`). A self-service trial flow is **Planned**.

## 8. Legacy customers

Every "Plano Essencial" checkout is represented as a QR Menu v1 contract (`origin = 'legacy_plan'`) by the idempotent backfill in `migrations/20261124000009_commercial_foundation.sql`. Price, cycle, coverage, Asaas identifiers, history, affiliate commissions, permissions, MCP integrations, published menus, branding, URLs and printed QR Codes are preserved. No new charge is created and no card re-registration is requested. Legacy tables stay in place (expand-contract, [ADR-010](../decisions/ADR-010-expand-contract-migrations.md)). **Implemented and tested** (`tests/commercial_migration.rs`). Details: [SUBSCRIPTION_RULES.md](SUBSCRIPTION_RULES.md#10-legacy-compatibility).

## 9. Affiliates within the commercial model

The existing program ([AFFILIATES.md](../architecture/AFFILIATES.md)) is preserved: fixed monthly or percentage commission, month-by-month installments for 6 and 12 month cycles, hold period, clawbacks, self-referral protection and email masking.

Implemented rules (**Implemented and tested**, `tests/affiliates.rs`, `tests/commercial_engine.rs::affiliate_commissions_use_eligible_saas_revenue_once_per_establishment`):

1. Commissions are computed only on confirmed, eligible SaaS revenue: `commercial_contracts.affiliate_eligible_centavos`, capped by the confirmed payment. Legacy contracts are fully eligible.
2. Recurring add-ons are not eligible by default (`affiliate_eligible: false` in `addons.json`). Modules and bundles are eligible.
3. Excluded always: food sales, delivery fees, amounts passed to establishments, refunded or charged-back amounts, unpaid credits, any ineligible line.
4. A fixed commission is per establishment per covered month (`value × cycle months`, capped by eligible revenue). It is never multiplied by the number of modules, because there is one consolidated subscription.
5. The affiliate's commission terms are snapshotted per payment. Changing an affiliate's quota affects only future payments.

Rules for future change flows (**Planned**, apply when [subscription changes](SUBSCRIPTION_RULES.md#8-subscription-changes) ship):

| Change | Affiliate effect |
|---|---|
| Upgrade (add module, bigger bundle) | Percentage commission applies to the eligible part of each confirmed charge, including a confirmed prorated adjustment. A fixed commission does not grow; prorated adjustments earn no extra fixed commission. |
| Downgrade or module removal | Already earned installments of paid periods are kept. Future payments use the new eligible amount. No clawback unless money is refunded. |
| Discounts (cycle or promotional) | Commission base is the discounted amount actually confirmed. |
| Cycle change | Takes effect at renewal; installments follow the new cycle (1, 6 or 12). |
| Bundle ↔ custom selection | Base is the eligible total of the new contract snapshot. |
| Credits applied at renewal | Credits are not revenue; eligible base is the cash confirmed. |
| Refund or chargeback | Existing void and clawback rules apply to the affected payment. |

Existing affiliate ledger entries are never recalculated retroactively.

## 10. Status summary

| Item | Status |
|---|---|
| Versioned catalog, validator, version registry | **Implemented and tested** |
| Quotes, contract snapshots, consolidated subscription for QR Menu | **Implemented and tested** |
| Asaas checkout, webhooks, cancellation, reconciliation | **Implemented, not homologated** |
| Asaas Sandbox homologation | **Blocked** (credentials unavailable in this environment) |
| Bundles, add-ons, modules other than QR Menu | **Planned** |
| Change flows, proration, credits, promotional discounts | **Planned** |
| Commerce Payments | **Planned** |
| Usage metering and pass-through billing | **Planned** |

## 11. References

- [ADR-007 Versioned catalog](../decisions/ADR-007-versioned-commercial-catalog.md), [ADR-008 Entitlements](../decisions/ADR-008-entitlement-engine.md), [ADR-009 Outbox](../decisions/ADR-009-sqlite-transactional-outbox.md), [ADR-010 Expand-contract](../decisions/ADR-010-expand-contract-migrations.md)
- Code: `src/domain/commercial/`, `src/domain/entitlements/`, `src/application/{commercial,entitlements,operator,billing}.rs`
- [BILLING.md](../architecture/BILLING.md), [AFFILIATES.md](../architecture/AFFILIATES.md), [MODULE_CATALOG.md](MODULE_CATALOG.md)
