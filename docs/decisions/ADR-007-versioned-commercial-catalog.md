# ADR-007: Versioned Commercial Catalog as a Compiled Contract

- **Status**: Accepted
- **Date**: 2026-11-24

## Context

Until this decision the only commercial offer was the "Plano Essencial": a `MONTHLY_PRICE_CENTAVOS = 2990` constant in `src/domain/billing.rs` and a `BillingPlan` enum that also hard-coded the cycle discounts. The landing page, the dashboard, the Asaas payload, the JSON-LD offers and the affiliate commission cap all read that constant. The platform is evolving into nine independently sold modules, three bundles and several add-ons, most of which do not exist yet. We need one unambiguous source of truth for identifiers, prices, lifecycle states and bundle composition, and a guarantee that a published price can never be changed under an existing contract.

## Decision

1. **Source of truth.** The commercial catalog lives in `contracts/commercial/{modules,bundles,addons,billing-cycles}.json`. The files are embedded at compile time (`include_str!`) and parsed into typed Rust structures in `src/domain/commercial/catalog.rs`. No price, module code or bundle composition is written anywhere else in Rust, templates or SQL.
2. **Validation.** `CommercialCatalog::load` (`domain::commercial::validation`) rejects duplicated codes, unknown capabilities, bundle components that do not exist, bundles or modules marked sellable while depending on unsellable modules, inter-module requirements (no commercial module may require another commercial module), dependency cycles, non-positive recurring prices and bundles not priced strictly below the sum of their parts. An invalid catalog panics at first access; `tests/commercial_contracts.rs` and process start-up (`init_pool`) both exercise it, so an invalid catalog never ships.
3. **Versioning.** Every module, bundle and add-on has numbered versions. A version pins its monthly price, its capabilities (modules), or its pinned module versions (bundles). Prices change only by adding a new version and moving `current_version`.
4. **Immutability once sellable.** At start-up, `init_pool` calls `queries::commercial_catalog::sync_registry`, which records a SHA-256 content hash of every version whose offer is, or has ever been, sellable (`beta`, `available`, `deprecated`, `retired`) in `commercial_catalog_versions`. A later binary that changes a recorded version's content refuses to start. Planned prices remain editable until launch.
5. **Lifecycle guard.** The same registry stores the last lifecycle state per offer (`commercial_catalog_offers`). A start-up with an illegal transition (for example `available -> planned`) is refused, which prevents a release from silently revoking paid access. After the first start-up, a new offer must appear as `planned` or `development`. Because the registry is written before deploy smoke checks, the reverse of a legal transition and a missing offer are accepted with a warning so a rolled-back binary can start (ADR-010); contracts referencing a version the binary does not define are always refused.
6. **Snapshots.** Contracts never reference the live catalog for money. `commercial_contract_items` copies the version, monthly price, discount and final price; `commercial_contract_capabilities` copies the granted capabilities.

## Consequences

- Changing prices or launching a module is a reviewed code change with automated validation, not a database edit.
- The JSON contracts double as documentation for humans and coding agents, and `docs/business/MODULE_CATALOG.md` is checked against them by tests.
- Catalog changes require a deploy. This is acceptable at the current scale; an admin-editable catalog can be added later behind the same registry without changing contract snapshots.
