# Entitlement Engine

Status: **Implemented and tested** (phase 1). Decision record: [ADR-008](../decisions/ADR-008-entitlement-engine.md).

The engine answers, for one establishment and one capability on one date: does it hold the capability, which source granted it, when it starts and ends, whether a usage limit applies, and whether the access is active, in grace, scheduled, expired or never contracted. `authorize` then combines that answer with four independent checks.

## Five independent dimensions

| # | Dimension | Source | Denial (`Denial`) | i18n key |
|---|---|---|---|---|
| 1 | Module availability | `Lifecycle::grants()` of the offer that owns the capability | `ModuleUnavailable` | `error.module_unavailable` |
| 2 | Establishment operational state for operations that suspension can block | `establishment_operational_states` (`active`, `suspended_admin`, `suspended_security`) | `EstablishmentSuspended` | `error.establishment_suspended` |
| 3 | Member permission | `MemberRole` from `establishment_members` (loaded into `TenantContext.role`) vs `Operation::roles()` | `PermissionDenied` | `error.permission_denied` |
| 4 | Deployment feature flag | `state.billing.enforcement()` derives enforcement and environment from `application::payments::BillingGateways` | — | — |
| 5 | Commercial entitlement | contracts with confirmed coverage + explicit grants | `NotEntitled(decision)` | `error.subscription_required` |

```rust
authorize(&pool, state.billing.enforcement(), &est_id,
          tenant.role, Operation::PublishMenu, billing_now().date_naive()).await?
```

Never branch on plan or bundle names. Handlers translate a `Denial` into a response; the billing page and the MCP read model use `can_publish_menu` for display only. The frontend is never the authority.

Suspension blocks only operations whose `after_expiry` is `deny` (new protected commercial activity). Unpublishing, viewing data and regularizing billing stay available (`OperationalStatus::blocks`).

## Sources

| Source | Table / view | Rule |
|---|---|---|
| Paid contract | `commercial_entitlement_periods` | Contract capability snapshot × payments confirmed at exactly the contracted amount. Period = `[due_date, due_date + cycle months floor)`. `renewing` when the gateway subscription is active. |
| Explicit grant | `entitlement_grants` | `trial` (available offers, ≤ 31 days, one per offer ever), `beta` (beta offers), `internal_preview` (internal preview or beta), `courtesy` (available or deprecated, ≤ 366 days). Expanded to the current capabilities of the granted offer. Revocation is a soft delete with actor and timestamp. |

Lifecycle filter applied on every read: `planned`, `development`, `retired` grant nothing, even if a row exists; `internal_preview` honors only `internal_preview` grants (also when billing is not enforced).

`domain::entitlements::resolve` (pure):

- **Active** when any period covers the date; the reported source is the longest-lasting one (contract preferred on ties). Removing one source never revokes a capability still covered by another.
- **Grace** when the last period ended less than `payment_grace_days` ago and was a renewing contract. The policy is `0`, which preserves the legacy behavior exactly.
- **Scheduled**, **Expired**, **NotContracted** otherwise (all denied).
- `usage_limit` is always `None`: no capability is metered yet.

## Implemented capabilities and operations

| Operation | Roles | Capability | After expiry | Surfaces |
|---|---|---|---|---|
| `menu.publish` | owner, manager | `menu.publish` | deny | dashboard, MCP (owner grant) |
| `menu.unpublish` | owner, manager | — | allow | dashboard, MCP |
| `menu.draft.edit` | owner, manager | — | allow | dashboard, MCP |
| `billing.view` | owner, manager | — | allow | dashboard |
| `billing.manage` | owner | — | allow | dashboard |
| `commerce.hours.manage` | owner, manager | — | allow | dashboard |
| `customers.manage` | owner, manager | — | allow | dashboard |
| `data.export`, `data.customer.erase` | owner | — | allow | dashboard |

All other capabilities belong to planned modules and are listed with their planned operations in `contracts/entitlements/authorization-rules.json`.

Commerce Core workers use the platform `system` actor to expire placed orders, reconcile offline collection intents and enqueue staff alerts. The actor is scoped to the persisted tenant and cannot be assigned to an account. Channel-specific member and consumer operations will be authorized when those entry points are implemented. `complete_existing` is reserved for adapters that verify work began while the capability was active.

## Events

Explicit grants append `EntitlementGranted` / `EntitlementRevoked` to the outbox in the same transaction. Contract-derived entitlements are not copied into events: consumers use `SubscriptionActivated` plus the resolver. Time-based expiry produces no event until a scheduler exists (planned with the outbox dispatcher).

## Operator commands

```bash
qrcardapio operator grant --operator admin@example.com --slug bar-k7m2 --offer menu --kind courtesy --days 30 --reason "pilot"
qrcardapio operator revoke --operator admin@example.com --slug bar-k7m2 --grant-id <id>
qrcardapio operator suspend --operator admin@example.com --slug bar-k7m2 --kind security --reason "phishing report"
qrcardapio operator reinstate --operator admin@example.com --slug bar-k7m2 --reason "resolved"
```

The operator must be an existing platform admin account. See the [runbook](../operations/RUNBOOK.md#commercial-operations).

## Tests

`tests/entitlements.rs` (payments, partial payments, duplicates, out-of-order events, refunds, chargebacks, cancellation, expiry, multi-source, tenant isolation, permission vs capability, suspension, planned modules, subscription page), `tests/commercial_migration.rs` (legacy parity), unit tests in `src/domain/entitlements/`.
