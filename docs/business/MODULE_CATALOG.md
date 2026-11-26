# Module Catalog

Human-readable view of the commercial catalog. The source of truth is `contracts/commercial/*.json`, compiled into the binary (ADR-007). `tests/commercial_contracts.rs` fails if a price, code or lifecycle below differs from the contracts.

Prices are monthly list prices in integer centavos. Cycle discounts (`billing-cycles.json`): monthly 0 bps, semiannual 500 bps, annual 1000 bps, applied per line with round-half-up.

## Modules

| Code | Module | Monthly (centavos) | Lifecycle | Phase | Sellable now | Spec |
|---|---|---|---|---|---|---|
| `menu` | QR Menu | 2990 | `available` | 1 | Yes | [MENU](../modules/MENU.md) |
| `delivery` | QR Delivery | 6990 | `development` | 4 | No | [DELIVERY](../modules/DELIVERY.md) |
| `tables` | QR Tables | 3990 | `planned` | 5 | No | [TABLES](../modules/TABLES.md) |
| `kitchen` | QR Kitchen | 2490 | `planned` | 6 | No | [KITCHEN](../modules/KITCHEN.md) |
| `fleet` | QR Fleet | 2990 | `planned` | 7 | No | [FLEET](../modules/FLEET.md) |
| `loyalty` | QR Loyalty | 1990 | `planned` | 8 | No | [LOYALTY](../modules/LOYALTY.md) |
| `analytics` | QR Analytics | 1990 | `planned` | 9 | No | [ANALYTICS](../modules/ANALYTICS.md) |
| `pos` | QR POS | 6990 | `planned` | 10 | No | [POS](../modules/POS.md) |
| `connect` | QR Connect | 2490 | `planned` | 11 | No | [CONNECT](../modules/CONNECT.md) |

Sum of individual modules: 32910 centavos (R$ 329,10) per month. [Commerce Core](../architecture/COMMERCE_CORE.md) is the free QR Core foundation. Shared checkout, offline payments and event processing are internal services. QR Delivery is in development and its sales and grants are disabled.

QR Menu offers an opt-in read-only display of catalog variations and add-ons in the published menu. [QR Menu scope](../modules/MENU.md#4-mvp-scope)

## Bundles

| Code | Bundle | Monthly (centavos) | Modules | Lifecycle | Sellable now |
|---|---|---|---|---|---|
| `start` | QR Start | 8490 | menu, delivery | `planned` | No |
| `business` | QR Business | 16990 | menu, delivery, tables, kitchen, fleet, loyalty, analytics | `planned` | No |
| `complete` | QR Complete | 21990 | all nine modules | `planned` | No |

A bundle version pins module versions, so its composition never changes after sale. A bundle is never more sellable than its least available module: the catalog validator rejects a sellable bundle that contains an unsellable module, and the quote engine re-checks at runtime.

## Add-ons and services

| Code | Name | Pricing | Monthly (centavos) | Lifecycle | Commission-eligible |
|---|---|---|---|---|---|
| `custom_domain` | Custom domain (registration not included) | recurring | 1490 | `planned` | No |
| `assisted_onboarding` | Assisted onboarding | one-off quote | — | `planned` | No |
| `native_apps` | Exclusive native apps | custom contract | — | `planned` | No |
| `exclusive_integrations` | Exclusive integrations | custom contract | — | `planned` | No |
| `external_usage` | Resources with external costs | usage-based pass-through | — | `planned` | No |

Quoted, custom and usage-based services never enter the self-service checkout (`QuoteError::NotSelfService`).

## Cycle prices of sellable offers

| Offer | Monthly | Semiannual (6 × list, −5%) | Annual (12 × list, −10%) |
|---|---|---|---|
| QR Menu | 2990 | 17940 → 17043 | 35880 → 32292 |

These are exactly the legacy "Plano Essencial" amounts, so existing customers see no price change.

## Capabilities by offer

| Offer | Capabilities |
|---|---|
| menu | `menu.publish` |
| delivery | `delivery.storefront.publish`, `delivery.order.create`, `delivery.order.manage`, `delivery.settings.manage` |
| tables | `tables.session.manage`, `tables.order.create`, `tables.bill.manage` |
| kitchen | `kitchen.ticket.create`, `kitchen.ticket.manage`, `kitchen.station.manage` |
| fleet | `fleet.delivery.create`, `fleet.delivery.assign`, `fleet.driver.manage` |
| loyalty | `loyalty.points.manage`, `loyalty.reward.manage` |
| analytics | `analytics.reports.advanced`, `analytics.import` |
| pos | `pos.sale.create`, `pos.cash_register.manage` |
| connect | `connect.api.use`, `connect.webhook.manage` |
| custom_domain | `branding.custom_domain` |

`menu.update` was merged into `menu.publish` and `menu.qr.generate` is a free QR Core operation (see `reclassified` in `contracts/entitlements/capabilities.json`).

## Changing the catalog

1. Never edit a version that was sold (`beta`, `available`, `deprecated`, `retired`): the start-up registry refuses it. Add `versions[n+1]` and move `current_version`; existing contracts keep their snapshot.
2. Move lifecycles only through legal transitions (`module-lifecycle.json`); `planned → available` in one release is refused.
3. Planned prices may still be refined before launch.
4. Run `cargo test --test commercial_contracts` and update this table.
