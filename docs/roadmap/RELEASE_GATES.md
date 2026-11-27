# Release Gates

Checks an offer (module, bundle or add-on) must pass before its lifecycle moves, and the procedure to move it. Status labels follow [PRODUCT_VISION.md](../business/PRODUCT_VISION.md#status-vocabulary). Phase sequencing is in [MASTER_ROADMAP.md](MASTER_ROADMAP.md).

## 1. Lifecycle and what enforces it

| State | Sale | Grants access | Renewals |
|---|---|---|---|
| `planned` | never | never | no |
| `development` | never | never | no |
| `internal_preview` | never | `internal_preview` grants only | no |
| `beta` | explicit enablement | yes | yes |
| `available` | standard | yes | yes |
| `deprecated` | never | yes | yes |
| `retired` | never | never | no |

Legal transitions ([`module-lifecycle.json`](../../contracts/commercial/module-lifecycle.json)): `planned ↔ development ↔ internal_preview ↔ beta → available ↔ deprecated → retired`. There is no transition back from `available` to `beta` and none out of `retired`.

Enforcement (**Implemented and tested**):

- `CommercialCatalog::validate` rejects an invalid catalog at compile-time tests and at start-up (`tests/commercial_contracts.rs`).
- `queries::commercial_catalog::sync_registry`, called by `init_pool` on every start-up, compares the compiled catalog with `commercial_catalog_offers` and `commercial_catalog_versions` and refuses to start when:
  - an offer's lifecycle differs from the last recorded state by anything other than one legal transition;
  - a version of an offer that is or was sellable (`beta`, `available`, `deprecated`, `retired`) changed content (SHA-256 fingerprint);
  - a sellable-ever offer disappeared from the catalog (retire it instead);
  - a contract references a version the catalog no longer defines.
- Consequence: an offer already recorded moves at most one step per deployed release. `planned → available` takes four releases (`development`, `internal_preview`, `beta`, `available`), each started in production so the registry records it.
- A brand-new offer code has no previous record, so the registry accepts whatever state it first ships with. Policy (enforced by review, not by code): every new module, bundle or add-on first ships as `planned`.

Evidence: `tests/commercial_engine.rs::sold_catalog_versions_and_lifecycles_cannot_change_silently`, `tests/commercial_contracts.rs::lifecycle_contract_matches_the_rust_rules`.

## 2. Generic gates

| Gate | Check | Evidence |
|---|---|---|
| G1 Contracts | `modules.json` (or `bundles.json` / `addons.json`), `capabilities.json`, `authorization-rules.json`, `contracts/events/registry.json` and `contracts/modules/<code>.json` describe exactly what is shipped; implemented rows say `implemented` | `cargo test --locked --test commercial_contracts` |
| G2 Catalog registry | The new binary starts against a restored copy of the production database; no frozen version edited; one legal transition per offer | Start-up log of a staging run on a `scripts/restore.sh` copy |
| G3 i18n parity | Every new UI string exists in `i18n/pt-BR.json`, `en.json`, `es.json` and is tagged `data-i18n` | `cargo test --locked --test i18n` |
| G4 Tenant isolation | Every new table and endpoint scoped by `establishment_id`; foreign keys cross-checked to the same establishment; cross-tenant read and write tests | Module integration tests in the style of `tests/auth_and_isolation.rs` |
| G5 Authorization | Every implemented operation exists in `domain::entitlements::Operation` with roles, capability and `after_expiry`; deny, allow and complete-existing paths tested, including suspension | `tests/entitlements.rs` style tests plus G1 parity |
| G6 Financial | Integer centavos only; totals reconcile to the centavo; idempotency on retries and redelivery; refunds and reversals; quote = snapshot = gateway amount for sellable offers | Unit and integration tests; Asaas Sandbox evidence for anything sold |
| G7 Security review | CSRF on every authenticated POST, strict CSP (no inline script or style), rate limits, input bounds, no secrets or PII in logs or events; `cargo audit` when dependencies change | Review notes in the PR; [SECURITY.md](../security/SECURITY.md) updated |
| G8 Performance | Load test of the module's write paths (orders, tickets, sessions) on the 1 vCPU / 1 GB target; public paths stay static and database-free | `scripts/bench_run.sh` or a module script; results in [PERFORMANCE.md](../performance/PERFORMANCE.md) |
| G9 Documentation | Module spec with the 27 required headings naming every contract identifier; [MODULE_CATALOG.md](../business/MODULE_CATALOG.md) row matches the contract; `docs/agents/PROJECT_STATE.md` updated | `module_documentation_matches_module_contracts`, `module_catalog_document_matches_prices` |
| G10 Runbook | Enable, disable, incident and data-repair procedures; operator commands for grants and suspension | `docs/operations/RUNBOOK.md` section for the module |
| G11 Rollback | Migrations are expand-only and idempotent ([ADR-010](../decisions/ADR-010-expand-contract-migrations.md)); binary rollback tested or the forward-fix plan documented (section 6) | PR description; staging rollback drill |
| G12 Sign-off | Product owner approval; no open stop-the-line item in [MASTER_ROADMAP.md](MASTER_ROADMAP.md#stop-the-line-rule) | PR approval |

Always required: `cargo fmt --check`, `cargo clippy --all-targets --all-features -- -D warnings`, `cargo test --locked`, CI checks `branch-policy`, `seo-contracts`, `quality`.

## 3. Gates per transition

| Transition | Required gates | Additional conditions |
|---|---|---|
| `planned → development` | G1 (draft), G9 (spec), G12 | Roadmap phase has started; previous phase complete |
| `development → internal_preview` | G1, G3, G4, G5, G9, G11 | Code merged behind the lifecycle; access only through `internal_preview` grants issued by an operator |
| `internal_preview → beta` | G1 to G11, G12 | Asaas Sandbox homologation passed for the platform; price version final, because it is frozen once `beta` is recorded; most per-module gates closed |
| `beta → available` | All gates, all per-module gates | At least one renewal cycle of beta customers processed without financial discrepancy; landing and pricing pages updated; bundles containing the module re-evaluated |
| `available → deprecated` | G1, G9, G10, G12 | Successor or end-of-sale notice published; existing customers keep access and renewals |
| `deprecated → retired` | G1, G9, G10, G11, G12 | No contract or grant covers the module beyond the retirement date; renewals already stopped or migrated; data export offered; retirement revokes access, so it never cuts a paid period |
| Backward steps (`development → planned`, `internal_preview → development`, `beta → internal_preview`, `deprecated → available`) | G1, G9, G12 | Customer impact assessed: leaving `beta` removes access for beta customers whose grants or contracts relied on it |

Bundles follow the same transitions and are never more sellable than their least available module (validator and quote engine). Add-ons follow the same table without G8 unless they add a write path.

Beta sales: the quote engine supports `SaleChannel::BetaEnablement` (**Implemented and tested**), but no HTTP flow sells a beta offer (**Planned**). Beta access today is given by `qrcardapio operator grant --kind beta` (**Implemented and tested**).

## 4. Per-module launch gates

From the `release_gates` field of each `contracts/modules/<code>.json`, in addition to the generic gates.

| Module | Launch gates | Status |
|---|---|---|
| `menu` | Entitlement parity with legacy coverage | **Implemented and tested** (`tests/commercial_migration.rs`, `tests/entitlements.rs`) |
| | Asaas sandbox homologation | **Blocked** (no Sandbox credentials) |
| | `republish-all` after deploy | Implemented in the deploy agent; not yet observed for the commercial foundation release |
| `delivery` | Commerce Core order engine with idempotent checkout, price and availability revalidation against the published snapshot, per-establishment service worker and cart isolation, load test of concurrent order writes, and LGPD export and erasure for customers | **Internal preview**. [Target-host capacity qualification](../performance/PERFORMANCE.md#qr-delivery-internal-preview-capacity) and physical-device checks remain open. |
| `tables` | Session join codes rotate per session; remote ordering blocked outside an open session; split and transfer keep integer totals exact | **Planned** |
| `kitchen` | Display refresh under 2 s without polling storms; idempotent ticket creation per order; manual tickets without any other module | **Planned** |
| `fleet` | External deliveries without any order; courier PWA isolated per establishment; payout ledger is append-only | **Planned** |
| `loyalty` | Ledger append-only with unique source event per credit; reversal on refund is automatic; expiration is deterministic | **Planned** |
| `analytics` | Incremental aggregation idempotent by `event_id`; public menu access counting without cookies or PII | **Planned** |
| `pos` | Receipts labeled non-fiscal; cash session totals reconcile to the centavo; fiscal issuance stays a separate future domain | **Planned** |
| `connect` | Credentials hashed at rest and revocable; per-credential rate limits; OpenAPI published only for implemented endpoints; existing MCP grants unaffected | **Planned** |

QR Menu is `available` because it is the existing product carried over from the Plano Essencial. Its open gates do not block menu publishing for deployments with no configured SaaS billing gateway. Each provider’s paid flows require its own Sandbox homologation.

## 5. Procedure to move a module one lifecycle step

1. Confirm the roadmap allows it (phase started, previous phase complete, no stop-the-line item).
2. Create a branch following [BRANCH_POLICY.md](../agents/BRANCH_POLICY.md).
3. Edit `lifecycle` of the module in `contracts/commercial/modules.json` by exactly one legal transition. Do not touch `monthly_price_centavos` or `capabilities` of any version that is or was sellable.
4. Update in the same change: `status` in `contracts/modules/<code>.json`; `status` of the module's capabilities in `capabilities.json` and of its operations in `authorization-rules.json`; event statuses in `contracts/events/registry.json`; the `Lifecycle` cell of [MODULE_CATALOG.md](../business/MODULE_CATALOG.md); the module spec; `docs/agents/PROJECT_STATE.md`.
5. Run the required commands and the gates of section 3 for this transition.
6. Start the new binary against a restored copy of production (`scripts/restore.sh`) and confirm the registry accepts the transition.
7. Merge through a green Pull Request. The deploy agent backs up the database, switches the binary, runs smoke checks and `republish-all`.
8. For `internal_preview` or `beta`, issue grants: `qrcardapio operator grant --operator <admin email> --slug <slug> --offer <code> --kind internal_preview|beta --days <n> --reason "<why>"`.
9. Verify on production: start-up succeeded, the billing page shows the expected standing, the outbox recorded the grants.

## 6. Rollback of lifecycle and catalog releases

Most schema rollbacks are a binary symlink switch ([ADR-010](../decisions/ADR-010-expand-contract-migrations.md)). The catalog registry records state at start-up, before the deploy agent's smoke checks, so it is rollback-tolerant (`tests/commercial_engine.rs::registry_tolerates_rollbacks_but_not_new_sellable_offers`):

| Release content | Binary rollback after the new binary started |
|---|---|
| Any single legal lifecycle step (including `beta → available`, `deprecated → retired`) | Safe: the reverse of a legal step is accepted with a warning |
| New offer | Must ship as `planned` or `development` (a new offer recorded directly as sellable is refused); rolling back to a binary without it logs a warning |
| New price version not yet referenced by any contract | Safe |
| New price version already referenced by a contract | Refused: the contract's version must stay defined. Recover by a forward fix; the pre-deploy backup `pre-<sha>-<timestamp>.sqlite` is the last resort |

Still refused in every case: editing a sold version and illegal jumps (for example `planned → available`).

## 7. Procedure to change a price

1. Never edit a version that is or was sellable. Add version `n+1` with a new `effective_from`, the new `monthly_price_centavos` and the same or updated capabilities.
2. Move `current_version` to `n+1`. Bundles pin module versions; publish a new bundle version if a pinned module version changes.
3. New quotes use `n+1`. Existing contracts keep their snapshot; moving them requires the planned policy in [PRICING_POLICY.md](../business/PRICING_POLICY.md#7-price-changes-and-versioning).
4. Update [MODULE_CATALOG.md](../business/MODULE_CATALOG.md) and run G1, G2, G6, G9 and G12.
5. A planned price (offer never sellable) may be edited in place; it is not frozen until the offer reaches `beta`.

## 8. References

[MASTER_ROADMAP.md](MASTER_ROADMAP.md) · [COMMERCIAL_MODEL.md](../business/COMMERCIAL_MODEL.md) · [PRICING_POLICY.md](../business/PRICING_POLICY.md) · [ADR-007](../decisions/ADR-007-versioned-commercial-catalog.md) · [ADR-008](../decisions/ADR-008-entitlement-engine.md) · [ADR-010](../decisions/ADR-010-expand-contract-migrations.md) · `src/domain/commercial/lifecycle.rs` · `src/infrastructure/database/queries/commercial_catalog.rs` · `src/operator_cli.rs`
