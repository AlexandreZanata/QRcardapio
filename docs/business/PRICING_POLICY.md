# Pricing Policy

How prices are defined, discounted, rounded, versioned and changed. Status labels follow [PRODUCT_VISION.md](PRODUCT_VISION.md#status-vocabulary).

Sources of truth: [`modules.json`](../../contracts/commercial/modules.json), [`bundles.json`](../../contracts/commercial/bundles.json), [`addons.json`](../../contracts/commercial/addons.json), [`billing-cycles.json`](../../contracts/commercial/billing-cycles.json). The human price table is [MODULE_CATALOG.md](MODULE_CATALOG.md); `tests/commercial_contracts.rs` fails when it drifts from the contracts.

## 1. Principles

1. Money is integer centavos (`i64`) everywhere: domain, SQLite, templates, gateway payload. Floats are never used for money.
2. Discounts are integer basis points (`500` = 5%).
3. One rounding rule, one function: `domain::commercial::apply_discount`. Backend, database snapshot, gateway payload and UI all use its output; the UI never recomputes prices.
4. Only the backend prices a subscription. The browser submits offer codes and a cycle.
5. A sold price is never changed. Prices change only by adding a new catalog version.
6. Prices for planned and internal-preview offers are internal targets. The product displays prices and accepts checkout for eligible offers.

Status: principles 1 to 5 **Implemented and tested**; principle 6 **Implemented and tested** for the dashboard (`tests/entitlements.rs::subscription_page_lists_modules_without_selling_planned_ones`).

## 2. Price list (monthly list prices, centavos)

| Offer | Code | Monthly | Lifecycle | Affiliate-eligible |
|---|---|---|---|---|
| QR Menu | `menu` | 2990 | `available` | yes |
| QR Delivery | `delivery` | 6990 | `internal_preview` | yes |
| QR Tables | `tables` | 3990 | `planned` | yes |
| QR Kitchen | `kitchen` | 2490 | `planned` | yes |
| QR Fleet | `fleet` | 2990 | `planned` | yes |
| QR Loyalty | `loyalty` | 1990 | `planned` | yes |
| QR Analytics | `analytics` | 1990 | `planned` | yes |
| QR POS | `pos` | 6990 | `planned` | yes |
| QR Connect | `connect` | 2490 | `planned` | yes |
| QR Start (Menu + Delivery) | `bundle:start` | 8490 | `planned` | yes |
| QR Business (7 modules, no POS, no Connect) | `bundle:business` | 16990 | `planned` | yes |
| QR Complete (all 9 modules) | `bundle:complete` | 21990 | `planned` | yes |
| Custom domain (registration not included) | `addon:custom_domain` | 1490 | `planned` | no |
| Assisted onboarding | `addon:assisted_onboarding` | one-off quote | `planned` | no |
| Exclusive native apps | `addon:native_apps` | custom contract | `planned` | no |
| Exclusive integrations | `addon:exclusive_integrations` | custom contract | `planned` | no |
| Resources with external costs | `addon:external_usage` | usage pass-through | `planned` | no |

Sum of the nine modules: 32910 centavos (R$ 329,10) per month.

## 3. Billing cycles

| Cycle | Months | Discount (bps) | Asaas cycle |
|---|---|---|---|
| `monthly` | 1 | 0 | `MONTHLY` |
| `semiannual` | 6 | 500 | `SEMIANNUALLY` |
| `annual` | 12 | 1000 | `YEARLY` |

The validator accepts only these three cycles and requires `rounding = "half_up_per_line"` (`src/domain/commercial/validation.rs`). **Implemented and tested.**

## 4. The rounding rule

For each quote line:

```text
list_price  = monthly_price_centavos × cycle_months
line_price  = (list_price × (10000 − discount_bps) + 5000) / 10000     (integer division)
total       = Σ line_price                                              (sum of rounded lines)
gateway     = total, sent as reais with two decimals: total / 100 "." total % 100
```

- Rounding is half-up on the half centavo, computed in `i128` to avoid overflow.
- Rounding happens per line, never on the sum. The contract total is the sum of already rounded lines, so every line in the snapshot adds up exactly to what the gateway charges.
- The confirmed payment must equal the contract total to the centavo; any other amount grants nothing (see [SUBSCRIPTION_RULES.md](SUBSCRIPTION_RULES.md#5-financial-events)).

Status: **Implemented and tested** (`src/domain/commercial/cycle.rs` unit tests, `tests/commercial_engine.rs::discounts_use_deterministic_integer_rounding`, Asaas payload test in `src/infrastructure/asaas.rs`).

## 5. Worked examples

All values in centavos.

**QR Menu (sellable today, identical to the legacy Plano Essencial):**

| Cycle | List | Computation | Charged |
|---|---|---|---|
| Monthly | 2990 × 1 = 2990 | (2990 × 10000 + 5000) / 10000 | 2990 |
| Semiannual | 2990 × 6 = 17940 | (17940 × 9500 + 5000) / 10000 = 170435000 / 10000 | 17043 |
| Annual | 2990 × 12 = 35880 | (35880 × 9000 + 5000) / 10000 = 322925000 / 10000 | 32292 |

**QR Start annual (planned):** 8490 × 12 = 101880 → (101880 × 9000 + 5000) / 10000 = 916925000 / 10000 = 91692.

**Half-centavo case (synthetic future version, not in the catalog):** monthly 2995, semiannual. List 17970; exact 95% is 17071.5; (17970 × 9500 + 5000) / 10000 = 170720000 / 10000 = 17072. Half rounds up.

**Per-line vs sum (synthetic):** two lines with list price 15 each at 500 bps. Per line: (15 × 9500 + 5000) / 10000 = 14 (exact 14.25). Total = 14 + 14 = 28. Discounting the sum would give (30 × 9500 + 5000) / 10000 = 29. The contract total and the gateway amount are 28.

**Custom combination (planned):** Menu + Kitchen + custom domain, annual. Lines 32292 + 26892 + 16092 = 75276 charged. Affiliate-eligible amount: 32292 + 26892 = 59184 (the add-on is not eligible).

### Cycle prices of every catalog offer

| Offer | Monthly | Semiannual list → charged | Annual list → charged |
|---|---|---|---|
| menu, fleet | 2990 | 17940 → 17043 | 35880 → 32292 |
| delivery, pos | 6990 | 41940 → 39843 | 83880 → 75492 |
| tables | 3990 | 23940 → 22743 | 47880 → 43092 |
| kitchen, connect | 2490 | 14940 → 14193 | 29880 → 26892 |
| loyalty, analytics | 1990 | 11940 → 11343 | 23880 → 21492 |
| bundle:start | 8490 | 50940 → 48393 | 101880 → 91692 |
| bundle:business | 16990 | 101940 → 96843 | 203880 → 183492 |
| bundle:complete | 21990 | 131940 → 125343 | 263880 → 237492 |
| addon:custom_domain | 1490 | 8940 → 8493 | 17880 → 16092 |

Only the QR Menu row can be bought today.

## 6. Bundle pricing

| Bundle | Sum of modules (monthly) | Bundle price | Monthly saving |
|---|---|---|---|
| QR Start | 2990 + 6990 = 9980 | 8490 | 1490 |
| QR Business | 23430 | 16990 | 6440 |
| QR Complete | 32910 | 21990 | 10920 |

Rules (**Implemented and tested** by the validator and the quote engine):

1. A bundle has at least two modules and lists each module once.
2. A bundle version pins module versions; its composition never changes after it is sold.
3. A bundle price must be positive and strictly below the sum of its modules' monthly prices.
4. A bundle is sellable only if every module in it is sellable; the quote engine takes the least available lifecycle of its modules.
5. A quote cannot contain a bundle and a module that the bundle already covers.

## 7. Price changes and versioning

| Rule | Status |
|---|---|
| A price change adds a new version (`versions[]`, `effective_from`) and moves `current_version`; old versions stay defined | **Implemented and tested** |
| At start-up the binary records a SHA-256 hash of every version whose offer is or was sellable (`beta`, `available`, `deprecated`, `retired`) in `commercial_catalog_versions`; a binary that changes a recorded version refuses to start | **Implemented and tested** (`tests/commercial_engine.rs::sold_catalog_versions_and_lifecycles_cannot_change_silently`) |
| A version referenced by any contract must remain defined; a sold offer cannot be deleted, only retired | **Implemented and tested** |
| Planned versions remain editable until the offer first becomes sellable | **Implemented and tested** |
| Existing contracts keep their snapshot price; renewals charge the snapshot amount stored at the gateway | **Implemented, not homologated** (gateway side) |
| Moving existing contracts to a new price version | **Planned**. Requires a published rule, advance notice to the owner, effect only from a future renewal, never retroactive, and a new contract snapshot. Until it exists, existing contracts keep their price indefinitely. |

Never change `monthly_price_centavos` of a sold version, never reuse a version number, and never make a price change by editing the database. See [RELEASE_GATES.md](../roadmap/RELEASE_GATES.md#7-procedure-to-change-a-price).

## 8. Add-ons, services and external costs

- **Custom domain**: recurring 1490 per month, cycle discount applies. Domain registration and renewal are paid by the establishment to its registrar. Requires a module with a public menu or storefront. **Planned.**
- **Assisted onboarding**: one-off quote per establishment, never part of the recurring total, never self-service. **Planned.**
- **Exclusive native apps, exclusive integrations**: custom contracts outside the self-service checkout. **Planned** (native apps not before phase 12).
- **External-cost resources** (SMS, maps, cloud OCR and similar): pass-through at cost plus any published handling fee, under a usage policy published before the first charge, with per-establishment quotas and metering (`usage_metering`, planned). **Planned.** Local OCR stays free under the free-tier limits.

None of these is affiliate-eligible by default.

## 9. What is never charged by the platform

- Commission on consumers' food orders, counter sales or tabs.
- QR Core: accounts, members, draft editing, QR Code generation, unpublishing, billing management, data export.
- Payment gateway fees, third-party fees and logistics fees. They exist, but the establishment pays them directly to its providers; they are never added to the subscription.

## 10. Discounts other than cycle discounts

| Discount | Status |
|---|---|
| Cycle discounts (0, 500, 1000 bps) | **Implemented and tested** |
| Bundle pricing | **Implemented and tested** (engine); bundles **Planned** for sale |
| Promotional or negotiated discounts | **Planned**. Must be stored as basis points per contract line in the snapshot, computed with `apply_discount` after the cycle discount is chosen, approved by an operator with an audit event, and bounded (never below zero, never retroactive). No coupon codes for SaaS billing until defined here. |
| Credits from downgrades | **Planned**. See [SUBSCRIPTION_RULES.md](SUBSCRIPTION_RULES.md#9-proration-formula). Credits are never paid out in cash. |

## 11. References

- Code: `src/domain/commercial/{cycle,quote,validation,catalog,fingerprint}.rs`, `src/infrastructure/database/queries/commercial_catalog.rs`, `src/infrastructure/asaas.rs`
- [ADR-007](../decisions/ADR-007-versioned-commercial-catalog.md), [COMMERCIAL_MODEL.md](COMMERCIAL_MODEL.md), [SUBSCRIPTION_RULES.md](SUBSCRIPTION_RULES.md), [MODULE_CATALOG.md](MODULE_CATALOG.md)
