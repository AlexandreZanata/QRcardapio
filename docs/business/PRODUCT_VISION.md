# Product Vision

QR Cardápio started as a mobile-first digital menu with permanent printable QR Codes. It is evolving into a modular commerce, service, delivery and management platform for food establishments, sold module by module on top of a free shared foundation (QR Core).

This document states the direction and the constraints that every phase must respect. Prices and subscription mechanics live in [COMMERCIAL_MODEL.md](COMMERCIAL_MODEL.md), [PRICING_POLICY.md](PRICING_POLICY.md) and [SUBSCRIPTION_RULES.md](SUBSCRIPTION_RULES.md). Sequencing lives in [MASTER_ROADMAP.md](../roadmap/MASTER_ROADMAP.md).

## Status vocabulary

Every business and roadmap document uses the same four labels.

| Label | Meaning |
|---|---|
| **Implemented and tested** | Code exists in this repository and automated tests exercise it. |
| **Implemented, not homologated** | Code exists and local tests pass, but it has not been validated against the real external system (Asaas Sandbox or production) or on production hardware. |
| **Planned** | Design contract only (JSON contract, ADR or module spec). No production code. Nothing is sold or granted. |
| **Blocked** | Cannot progress until an external dependency is resolved. The blocker is named. |

When a document and a contract under `contracts/` disagree, the contract wins and the document is fixed.

## 1. Purpose

Give small and medium food businesses the operational software that large chains have, at a price and complexity a single-location owner can accept, without forcing them to buy a suite they do not need.

## 2. Who it serves

| Segment | Typical first module | Why |
|---|---|---|
| Restaurants | QR Menu, then Tables and Kitchen | In-venue menu, table ordering, production flow |
| Burger joints, pizzerias | QR Menu + QR Delivery | Own ordering channel without marketplace commission |
| Cafés, bakeries | QR Menu, then POS | Counter sales and a printable menu |
| Snack bars, açaí and ice-cream shops | QR Menu or QR Delivery | Fast catalog changes, pickup and delivery |
| Bars | QR Menu + QR Tables | Tabs, table sessions, splitting bills |
| Food trucks | QR Menu, then POS | Mobile counter, changing locations |
| Delivery-only kitchens | QR Delivery + QR Kitchen + QR Fleet | No dining room; orders, production and couriers |
| Chains and franchises | Bundles, then phase 12 multi-unit | Consistent brand across units |

Only the QR Menu column is sellable today. Every other module is **Planned** (see [MODULE_CATALOG.md](MODULE_CATALOG.md)).

## 3. Product principles

1. **Modular by contract.** Each module solves one job and can be bought alone. No commercial module may be required to use another module's main function. Technical prerequisites belong to the free QR Core.
2. **Cheap to operate.** One small VPS (target 1 vCPU / 1 GB), one Rust binary, SQLite in WAL mode with `synchronous=FULL`, Caddy in front. No broker, no Kubernetes, no per-tenant infrastructure.
3. **Fast public paths.** Public menus are atomic static snapshots served with zero database queries ([ADR-003](../decisions/ADR-003-static-atomic-menu-publishing.md)). Future storefronts reuse the same publishing pipeline.
4. **Secure by default.** Tenant isolation on every query, strict CSP, CSRF on every authenticated form, bounded Argon2id, authenticated idempotent webhooks ([SECURITY.md](../security/SECURITY.md)).
5. **Financially exact.** Money is integer centavos. Discounts are integer basis points with one deterministic rounding rule used by every surface.
6. **Honest commercial state.** A module is sold only when it is `available`. Planned modules are shown as "in development" without prices and never grant access.
7. **Continuity over lock-out.** Expiry blocks new protected work; it never deletes data, never takes a published menu offline and never strands an order already accepted.
8. **Easy.** Server-rendered pages, minimal JavaScript, native `pt-BR` / `en` / `es`, guided onboarding.

## 4. Platform shape

```text
                 +-----------------------------------------------+
  Sold modules   | Menu | Delivery | Tables | Kitchen | Fleet |   |
                 | Loyalty | Analytics | POS | Connect            |
                 +-----------------------------------------------+
  Free QR Core   | identity, establishments, members, permissions,|
  (never sold)   | tenancy, i18n, catalog, media, publishing,      |
                 | commercial engine, entitlements, SaaS billing,  |
                 | events (outbox), audit, and planned Commerce    |
                 | Core: orders, customers, addresses, payments    |
                 +-----------------------------------------------+
```

- **QR Core** is listed in [`contracts/core/components.json`](../../contracts/core/components.json). It is free, never sold and never a module. Free core infrastructure does not mean free commercial operations: publishing a menu or accepting an order still requires the capability of the module that sells it.
- **Modules** grant capabilities (for example `menu.publish`). Code checks capabilities, never plan names ([ADR-008](../decisions/ADR-008-entitlement-engine.md)).
- **Bundles** are priced packages that pin module versions. **Add-ons and services** are optional (custom domain, assisted onboarding, exclusive apps and integrations, external-cost resources).

## 5. Module independence

Every module contract in `contracts/modules/*.json` declares `"standalone": true` and lists only QR Core components under `core_dependencies`. Integrations with other modules are optional and travel through outbox events. Examples the design must keep working:

| Scenario | What the customer buys | Behavior |
|---|---|---|
| Delivery alone | QR Delivery | Storefront, cart, checkout and order center work without QR Menu; the catalog lives in QR Core. |
| Kitchen alone | QR Kitchen | Staff create manual tickets; no order module is needed. Orders from Delivery, Tables or POS become tickets only when those modules are also contracted. |
| Fleet alone | QR Fleet | Deliveries originating outside QR Cardápio (phone, marketplace) are registered and assigned to couriers. |
| Menu alone | QR Menu | Today's product: static menu, QR Codes, MCP editing. |

The catalog validator rejects any module that lists another commercial module in `requires_modules` (`src/domain/commercial/validation.rs`). Status: **Implemented and tested** (`tests/commercial_contracts.rs::invalid_catalogs_are_rejected`).

## 6. Two separate money flows

| Flow | Who pays whom | Status |
|---|---|---|
| SaaS Billing | Establishment pays the platform for modules, bundles and add-ons | **Implemented, not homologated** (Asaas hosted checkout) |
| Commerce Payments | Consumer pays the establishment for orders, counter sales, tabs | **Planned** (phase 3 contracts, phase 12 online payments) |

They never share credentials, beneficiaries, state or ledgers. The platform's central Asaas account must never collect an establishment's sales. The platform charges no commission on food orders. Details: [COMMERCIAL_MODEL.md](COMMERCIAL_MODEL.md#6-saas-billing-vs-commerce-payments).

## 7. Operating constraints every phase inherits

- Modular monolith, Rust 2024, Axum 0.8, SQLx with bundled SQLite ([ADR-001](../decisions/ADR-001-modular-monolith-rust-axum.md), [ADR-002](../decisions/ADR-002-sqlite-wal-synchronous-full.md)).
- Cross-module reactions use the SQLite transactional outbox, not a broker ([ADR-009](../decisions/ADR-009-sqlite-transactional-outbox.md)).
- Schema evolution is expand-contract so binary rollback stays a symlink switch ([ADR-010](../decisions/ADR-010-expand-contract-migrations.md)).
- The commercial catalog is a compiled, versioned contract ([ADR-007](../decisions/ADR-007-versioned-commercial-catalog.md)).
- No module may add a second order engine, price calculator or payment ledger; those live in Commerce Core ([COMMERCE_CORE.md](../architecture/COMMERCE_CORE.md)).
- White-label is configuration plus snapshot data on one binary, never a per-tenant codebase ([WHITE_LABEL.md](../architecture/WHITE_LABEL.md)).

## 8. Scope discipline

`AGENTS.md` section 2 lists features that are out of scope for the MVP (online ordering, carts, payments, KDS, POS integrations and others). The roadmap schedules some of them as future modules. That does not lift the restriction: each item becomes in scope only when its roadmap phase starts, the previous phase has passed its completion criteria, and the product owner explicitly requests it.

Permanent non-goals:

- Commission on consumers' food orders.
- One gateway subscription per module.
- Silent price changes on existing contracts.
- Forced card re-registration or forced migration of legacy customers.
- Consumer accounts required to view a public menu.

## 9. Where the product stands (2026-11-24)

| Area | Status |
|---|---|
| QR Menu (menu, static publishing, QR Codes, OCR import, MCP, i18n) | **Implemented and tested** |
| Commercial Engine (versioned catalog, quotes, contract snapshots, registry) | **Implemented and tested** |
| Entitlement Engine (capabilities, grants, five-dimension authorization, grace, suspension) | **Implemented and tested** |
| Legacy "Plano Essencial" migrated to QR Menu v1 contracts | **Implemented and tested** (`tests/commercial_migration.rs`) |
| Asaas recurring checkout and webhooks | **Implemented, not homologated** |
| Asaas Sandbox homologation | **Blocked** (no Sandbox credentials in this environment) |
| Subscription change flows (upgrade, downgrade, cycle change, proration) | **Planned** (rules in [SUBSCRIPTION_RULES.md](SUBSCRIPTION_RULES.md)) |
| Commerce Core and modules Delivery through Connect | **Planned** |
| Bundles and add-ons | **Planned** (priced, not sellable) |

## 10. Related documents

- [COMMERCIAL_MODEL.md](COMMERCIAL_MODEL.md), [PRICING_POLICY.md](PRICING_POLICY.md), [SUBSCRIPTION_RULES.md](SUBSCRIPTION_RULES.md), [MODULE_CATALOG.md](MODULE_CATALOG.md)
- [MASTER_ROADMAP.md](../roadmap/MASTER_ROADMAP.md), [RELEASE_GATES.md](../roadmap/RELEASE_GATES.md)
- Module specifications: `docs/modules/*.md` (for example [DELIVERY.md](../modules/DELIVERY.md))
- [PROJECT_STATE.md](../agents/PROJECT_STATE.md)
