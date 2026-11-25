# ADR-008: Derived Entitlements and Five-Dimension Authorization

- **Status**: Accepted
- **Date**: 2026-11-24

## Context

The only commercial gate was `application::billing::publication_allowed`, which returned `true` when Asaas was not configured and otherwise looked for any paid coverage row. Plan names were not compared directly, but the rule "a paid row means everything is allowed" cannot scale to nine modules, bundles, trials, beta enablement and module retirement. Access also mixed several concerns: what the establishment bought, what the signed-in member may do, whether the module exists, whether billing is enforced in this deployment, and whether the establishment is suspended.

## Decision

1. **Strongly typed capabilities.** `domain::entitlements::Capability` enumerates every commercial capability. `contracts/entitlements/capabilities.json` is the machine-readable registry; tests enforce exact parity.
2. **Entitlements are derived, not copied.** An establishment holds a capability on a date when at least one source covers it:
   - a commercial contract whose capability snapshot includes it and whose confirmed Asaas payment (exact contract price) covers the date (`commercial_entitlement_periods` view);
   - an explicit, audited `entitlement_grants` row (`trial`, `beta`, `internal_preview`, `courtesy`) that is not revoked.
   Removing one source never revokes a capability still covered by another, because the resolver evaluates all sources.
3. **Five independent dimensions.** `application::entitlements::authorize` approves an `Operation` only when every applicable check passes, in this order:
   1. module lifecycle permits operations (`domain::commercial::Lifecycle`);
   2. the establishment is operational (`establishment_operational_states`);
   3. the member role grants the permission (`domain::entitlements::MemberRole`);
   4. the deployment enforces billing (`state.billing.enforcement()` via `application::payments::BillingGateways`, after [ADR-011](ADR-011-multi-provider-saas-billing.md)). Available modules have no commercial gate in deployments with zero configured gateways;
   5. the establishment holds the capability (resolver above).
   Each denial maps to its own i18n error key so the UI and logs never confuse "you may not" with "you did not buy".
4. **Continuity.** Operations are classified in `contracts/entitlements/authorization-rules.json`. Only operations that create new protected commercial activity require an active capability; completing or reverting existing work (for example unpublishing a menu) remains available after expiry.
5. **Grace.** The policy `payment_grace_days` (currently `0`, matching the legacy behavior) is evaluated by the pure resolver, separate from administrative and security suspension.

## Consequences

- Handlers and MCP tools call `authorize(Operation::PublishMenu)` instead of reasoning about plans.
- Derived entitlements cannot drift from payments, but time-based expiry emits no event until a scheduler exists; explicit grants emit `EntitlementGranted`/`EntitlementRevoked` through the outbox.
- Adding a module means adding capabilities, a catalog entry and operations; no authorization code changes in unrelated modules.
