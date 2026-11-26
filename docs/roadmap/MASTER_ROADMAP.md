# Master Roadmap

Sequencing of the evolution from QR Menu to a modular commerce platform. Each phase has an objective, deliverables and objective completion criteria. Status labels follow [PRODUCT_VISION.md](../business/PRODUCT_VISION.md#status-vocabulary): **Implemented and tested**, **Implemented, not homologated**, **Planned**, **Blocked**.

Module lifecycle moves are governed by [RELEASE_GATES.md](RELEASE_GATES.md). Commercial terms are in [COMMERCIAL_MODEL.md](../business/COMMERCIAL_MODEL.md).

## Stop-the-line rule

A new operational phase never starts while the previous phase has an open failure in any of these categories:

| Category | Examples that block |
|---|---|
| Security | Cross-tenant read or write, missing CSRF or authorization check, secret in logs, CSP regression |
| Financial consistency | Amount mismatch between quote, snapshot and gateway; access granted without exact confirmed payment; duplicate charge; commission on ineligible revenue |
| Data integrity | Lost or corrupted rows, non-idempotent webhook or outbox consumer, migration that breaks rollback |
| Operations | Failing backup or restore, failing deploy rollback, failing `republish-all`, no runbook for a new failure mode |

"Start" means writing production code for the phase. Specs and contracts for later phases may be drafted at any time.

## Overview

| Phase | Name | Status |
|---|---|---|
| 0 | Audit and contracts | **Implemented and tested** (module specs under reconciliation) |
| 1 | Commercial foundation | **Implemented and tested**; Asaas side **Implemented, not homologated** |
| 2 | QR Menu consolidation | Technical part **Implemented and tested**; homologation **Blocked**; live checks pending |
| 3 | Commerce Core | **In progress** (capacity validated on the VPS candidate) |
| 4 | QR Delivery | **Development** |
| 5 | QR Tables | **Planned** |
| 6 | QR Kitchen | **Planned** |
| 7 | QR Fleet | **Planned** |
| 8 | QR Loyalty | **Planned** |
| 9 | QR Analytics | **Planned** |
| 10 | QR POS | **Planned** |
| 11 | QR Connect | **Planned** |
| 12 | Advanced expansion | **Planned** |

```text
0 -> 1 -> 2 -> 3 (Commerce Core) -> 4 Delivery -> 5 Tables -> 6 Kitchen -> 7 Fleet
                                  -> 8 Loyalty -> 9 Analytics -> 10 POS -> 11 Connect -> 12
```

Phases 4 to 11 run one at a time. Each module reaches `available` and is consolidated before the next module's production code starts.

## Phase 0 — Audit and contracts

**Objective.** Freeze the commercial and technical contracts before writing commercial code.

**Deliverables.** `contracts/commercial/*.json`, `contracts/entitlements/*.json`, `contracts/core/components.json`, `contracts/modules/*.json`, `contracts/events/registry.json`, `contracts/commerce/state-machines.json`; ADR-007 to ADR-010; module specs `docs/modules/*.md`; [COMMERCE_CORE.md](../architecture/COMMERCE_CORE.md), [WHITE_LABEL.md](../architecture/WHITE_LABEL.md); business documents under `docs/business/`.

**Completion criteria.**

- `tests/commercial_contracts.rs` passes: catalog prices and lifecycles, invalid catalogs rejected, capability and lifecycle parity with Rust, authorization rules parity, event registry parity, module contracts reference existing contracts, state machines consistent.
- `module_documentation_matches_module_contracts` passes: every module spec has the 27 required headings and names every capability, operation and event of its contract.
- `module_catalog_document_matches_prices` passes for [MODULE_CATALOG.md](../business/MODULE_CATALOG.md).

**Status.** **Implemented and tested** for the contracts. Module specs are being reconciled with their contracts; the two documentation tests above are the exit check.

## Phase 1 — Commercial foundation

**Objective.** Replace the hard-coded "Plano Essencial" with a versioned catalog, contract snapshots and capability-based authorization, without changing what existing customers pay or see.

**Deliverables (all present).**

- Commercial Engine: `src/domain/commercial/` (catalog, cycles with `apply_discount`, quotes, validation, lifecycle, fingerprints), `src/application/commercial.rs`, catalog version registry (`queries/commercial_catalog.rs`).
- Entitlement Engine: `src/domain/entitlements/` (capabilities, resolver with grace, roles, operations, suspension), `src/application/entitlements.rs`.
- Operator CLI: `qrcardapio operator grant|revoke|suspend|reinstate` (`src/application/operator.rs`, `src/operator_cli.rs`).
- Migration `20261124000009_commercial_foundation.sql` (expand only) with the legacy backfill to QR Menu v1 contracts.
- Asaas checkout priced from quotes; contract snapshot written with the reservation; activation on confirmed payment.
- Affiliate commissions based on `affiliate_eligible_centavos`.
- Transactional outbox append (`domain_events`).
- Initial commercial UI: the billing page lists modules with their standing (contracted, available, expired, suspended, in development) and never sells planned modules.

**Completion criteria.** `tests/commercial_engine.rs`, `tests/entitlements.rs`, `tests/commercial_migration.rs`, `tests/billing.rs`, `tests/affiliates.rs` and `tests/commercial_contracts.rs` pass under `cargo test --locked`, together with `cargo fmt --check` and `cargo clippy --all-targets --all-features -- -D warnings`.

**Status.** **Implemented and tested**. Gateway interaction is **Implemented, not homologated**.

## Phase 2 — QR Menu consolidation

**Objective.** Prove that QR Menu on the new foundation is at least as correct, fast and compatible as before, and homologate subscription billing.

| Item | Status |
|---|---|
| Regression suite of the MVP (auth, isolation, menu CRUD, publishing, QR, OCR, SEO, MCP, i18n) green on the new foundation | **Implemented and tested** |
| Legacy compatibility (prices, cycles, coverage, Asaas ids, affiliates, MCP, menus, URLs, QR Codes) | **Implemented and tested** (`tests/commercial_migration.rs`) |
| Static publishing gated by `authorize(Operation::PublishMenu)`; unpublish always allowed | **Implemented and tested** |
| Permissions: owner and manager roles separated from capabilities; billing management owner-only | **Implemented and tested** |
| i18n parity of all new keys in `pt-BR`, `en`, `es` | **Implemented and tested** (`tests/i18n.rs`) |
| Documentation (ADRs, business, roadmap, module specs) | **Implemented** (module specs under reconciliation) |
| Asaas Sandbox homologation of checkout, renewal, refund, chargeback, cancellation and reconciliation | **Blocked**: no Asaas Sandbox credentials in this environment |
| Live smoke test of this release on a running instance (deploy, migration, registry start-up, `republish-all`, billing page, rollback procedure) | **Planned**: not executed in this mission |
| Benchmarks on production hardware (1 vCPU / 1 GB VPS) after the commercial foundation | **Planned**: only local benchmarks exist ([PERFORMANCE.md](../performance/PERFORMANCE.md)) |

**Completion criteria.** All rows above green; the Asaas enablement and homologation steps of the runbook ("Habilitação do Asaas") executed against a Sandbox account with evidence; one production renewal of a migrated legacy subscription observed activating its QR Menu v1 contract at the contracted price; benchmark results on production hardware recorded and within the "Target Production Hardware Budget" of PERFORMANCE.md.

**Decision rule.** Phase 3 production code does not start until phase 2 is complete, unless the product owner explicitly accepts the open homologation item in writing. In every case, no module other than QR Menu may reach `beta` or `available` before Asaas homologation passes.

## Phase 3 — Commerce Core

**Status.** In progress. Increment 1 delivers the internal foundation for catalog options and snapshots, pricing and orders with outbox append. Increment 2 adds dashboard option management, service hours and settings, and customer records with address and privacy operations. Increment 3 implements internal checkout, offline payments, the dispatcher, acceptance expiry and staff Web Push. The consolidation candidate combines the VPS capacity work, audited dead-letter replay, outbox OpenAPI contract, checkout and deadline hardening, and opt-in display of variations and add-ons on static product pages. The product owner accepted pending Asaas Sandbox homologation in writing on 2026-11-25. See [Project State](../agents/PROJECT_STATE.md#commerce-core--phase-3-2026-11-25) and [VPS evidence](../performance/PERFORMANCE.md#vps-capacity-validation).

**Objective.** Build the shared, free order engine that every selling module uses ([COMMERCE_CORE.md](../architecture/COMMERCE_CORE.md)).

**Deliverables.** Catalog variations, add-on groups and options; customers and addresses (establishment-scoped, LGPD export and erasure); the `Order` aggregate and state machines from `contracts/commerce/state-machines.json`; idempotent order creation; offline `order_payments`; outbox dispatcher with per-consumer checkpoints, retries with backoff and a dead-letter table; business hours and commercial settings; versioned REST contracts.

The [Commerce Core OpenAPI contract v1](../../contracts/openapi/commerce.v1.json) describes the implemented administrative outbox interface. Channel-facing ordering contracts are listed under their respective module phases.

The readiness increment extends published availability revisions to options, checks the quoted breakdown in checkout, and keeps the acceptance deadline queue moving after an individual order failure.

**Completion criteria.** State-machine transitions tested exhaustively, including illegal transitions; idempotency keys tested against retries and concurrent submissions; dispatcher at-least-once delivery with idempotent consumers tested; tenant isolation tests on every new table; integer totals reconcile to the centavo; load test of concurrent order writes on the 1 vCPU target; no sellable module change.

## Phase 4 — QR Delivery

**Status.** Development candidate on top of the Commerce Core consolidation. The compiled catalog keeps sales and grants disabled. The local verification suite uses an injected preview catalog and explicit grants. Production capacity, physical device installation, pilot and launch gates remain open.

**Objective.** Own ordering channel for delivery and pickup without marketplace commission.

**Deliverables.** White-label PWA storefront, cart, checkout, orders, neighborhood-based delivery zones and fees, pickup, pay on delivery (recorded, not processed), order center, order tracking, notifications.

**Completion criteria.** Generic gates of [RELEASE_GATES.md](RELEASE_GATES.md) plus the `delivery.json` gates. QR Delivery is fully consolidated (at `available`, stable for one renewal cycle of its first paying customers, no open stop-the-line item) before any other module's production code starts. QR Start becomes sellable only when QR Delivery is `available`.

## Phases 5 to 11 — Modules

| Phase | Module | Objective | Key exit evidence (beyond generic gates) |
|---|---|---|---|
| 5 | QR Tables | In-venue ordering through controlled table sessions and bills | Join codes rotate per session; remote ordering blocked outside an open session; split and transfer keep integer totals exact |
| 6 | QR Kitchen | Production tickets and kitchen display | Manual tickets with no other module; idempotent ticket creation per order; display refresh under 2 s without polling storms |
| 7 | QR Fleet | Courier management, including external deliveries | External deliveries without any order; courier PWA isolated per establishment; append-only payout ledger |
| 8 | QR Loyalty | Points and rewards | Append-only ledger with unique source event per credit; automatic reversal on refund; deterministic expiration |
| 9 | QR Analytics | Advanced reports and imports | Idempotent incremental aggregation by `event_id`; public menu counting without cookies or PII |
| 10 | QR POS | Counter sales and cash sessions | Receipts labeled non-fiscal; cash session totals reconcile to the centavo; fiscal issuance kept as a separate future domain |
| 11 | QR Connect | Public API and outbound webhooks | Credentials hashed and revocable; per-credential rate limits; OpenAPI only for implemented endpoints; existing MCP grants unaffected |

QR Business becomes sellable only after phase 9 (its last module, Analytics) is `available`; QR Complete only after phase 11.

## Phase 12 — Advanced expansion

**Objective.** Capabilities that need external accounts, heavier infrastructure or regulatory work.

**Scope.** Online consumer payments with per-establishment financial accounts (dynamic Pix, card, reconciliation); exclusive native apps; custom domains at scale; courier GPS; logistics integrations; fiscal issuance; franchises and multi-unit management; horizontal scaling beyond one VPS.

**Entry criteria.** Each item gets its own ADR, contract and release gates before code. Online payments require legal review of the money flow and must keep the central Asaas account out of establishments' sales.

## Cross-phase rules

1. Every phase updates the affected contracts, `docs/agents/PROJECT_STATE.md` and the corresponding docs.
2. Every schema change is expand-contract ([ADR-010](../decisions/ADR-010-expand-contract-migrations.md)).
3. Every new money path uses integer centavos and `apply_discount` or an equally tested integer rule.
4. Every module keeps `standalone: true` and depends only on QR Core components.
5. Required checks before merging: `cargo fmt --check`, `cargo clippy --all-targets --all-features -- -D warnings`, `cargo test --locked`, and the CI checks `branch-policy`, `seo-contracts`, `quality` ([BRANCH_POLICY.md](../agents/BRANCH_POLICY.md)).

## References

[PRODUCT_VISION.md](../business/PRODUCT_VISION.md) · [RELEASE_GATES.md](RELEASE_GATES.md) · [SUBSCRIPTION_RULES.md](../business/SUBSCRIPTION_RULES.md) · [PROJECT_STATE.md](../agents/PROJECT_STATE.md) · `docs/modules/*.md`
