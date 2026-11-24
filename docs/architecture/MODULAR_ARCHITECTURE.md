# Modular Architecture

How QR Cardápio grows from a digital-menu product into a modular commerce platform while staying a single Rust binary on a small VPS. Read with [OVERVIEW](OVERVIEW.md) and [AGENTS.md](../../AGENTS.md).

## Layers

```text
QR Core (free, never sold)
  identity · establishments · members/roles · tenancy · operational state · i18n · catalog · media
  publishing · commercial (catalog, quotes, contracts) · entitlements · SaaS billing · outbox events
        ▲                ▲                 ▲
Commercial modules (sold independently, each standalone)
  QR Menu (available) · Delivery · Tables · Kitchen · Fleet · Loyalty · Analytics · POS · Connect (planned)
        ▲
Commerce Core (phase 3, planned): one Order aggregate for every channel + independent state machines
```

- A module depends only on QR Core components (`contracts/core/components.json`), never on another commercial module (`requires_modules` must be empty; the validator rejects anything else). Optional integrations happen through outbox events.
- Each module owns its capabilities, operations, events and tables; its contract lives in `contracts/modules/<code>.json` and its specification in `docs/modules/<CODE>.md`.

## Code organization (current)

| Concern | Path |
|---|---|
| Commercial domain | `src/domain/commercial/` (`catalog`, `cycle`, `lifecycle`, `quote`, `validation`, `fingerprint`) |
| Entitlement domain | `src/domain/entitlements/` (`capability`, `access`, `authorization`) |
| Domain events | `src/domain/events.rs` |
| Use cases | `src/application/commercial.rs`, `entitlements.rs`, `operator.rs`, `billing.rs` |
| Persistence | `src/infrastructure/database/queries/{commercial_catalog,commercial_contracts,entitlements,domain_events}.rs` |
| Gateway | `src/infrastructure/asaas.rs` (+ `asaas/webhook.rs`) |
| HTTP | `src/http/handlers/billing.rs`, `dashboard.rs`; tenant role in `http/middleware/auth.rs` |
| CLI | `src/operator_cli.rs` (`qrcardapio operator …`) |
| Contracts | `contracts/{commercial,entitlements,events,commerce,core,modules}/` |

Existing files were not moved to fit a target tree. New modules follow this layout when they start:

```text
src/domain/<module>/          pure types, invariants, state machines
src/application/<module>/     use cases calling entitlements::authorize
src/infrastructure/database/queries/<module>_*.rs
src/http/handlers/<module>/   thin handlers; JSON APIs under src/http/api/ (versioned)
templates/<module>/, static/css/<module>.css, i18n keys <module>.*
migrations/<date>_<module>_*.sql (expand-contract)
```

## Sources of truth

| Data | Source | Validated by |
|---|---|---|
| Prices, offers, bundles, cycles, lifecycles | `contracts/commercial/*.json` (compiled) | `CommercialCatalog::load`, start-up registry, `tests/commercial_contracts.rs` |
| Capabilities | `Capability` enum ↔ `capabilities.json` | parity test |
| Operations and roles | `Operation`/`MemberRole` ↔ `authorization-rules.json` | parity test |
| Events | `EventType` ↔ `events/registry.json` + schemas | parity test |
| Module contracts | `contracts/modules/*.json` | cross-reference test (capabilities, operations, events, core components, state machines, planned HTTP) |
| Module documentation | `docs/modules/*.md` | heading and identifier test |
| Catalog table for humans | `docs/business/MODULE_CATALOG.md` | price/lifecycle test |

## Coupling found in the audit and how it was resolved

| Coupling | Resolution |
|---|---|
| `MONTHLY_PRICE_CENTAVOS` constant used by billing, landing, SEO, affiliates | Removed; every price comes from the catalog (`BillingCycle::price`, `entry_monthly_price_centavos`). |
| `BillingPlan` mixed cycle and price | Now an alias of `BillingCycle`; prices come from quotes. |
| `publication_allowed` = "any paid row allows everything" | Replaced by `authorize(Operation::PublishMenu)` and `can_publish_menu`. |
| Owner check by ad hoc query in billing handlers | `TenantContext.role` + `Operation::ManageBilling`. |
| Asaas payload computed its own price | Payload built from the contract quote. |
| `republish-all` re-rendered drafts after every deploy | Skips establishments with unpublished changes. |
| Old binaries rejected newer migrations on rollback | Rollback-tolerant migrator (ADR-010). |

Remaining known couplings: the static menu re-render after asset upgrades needs persisted per-publication snapshots; MCP OAuth scopes are a CHECK-constrained column (new scopes need an expand migration); `establishment_members.member_role` CHECK limits roles to owner/manager.

## Anti-patterns to avoid

- `if plan == "premium"` or any branch on bundle/plan names.
- A second order engine per module.
- Prices or amounts accepted from the browser or from device snapshots.
- Planned features with functional buttons or endpoints announced as available.
- Cross-module synchronous calls for side effects; use the outbox.
