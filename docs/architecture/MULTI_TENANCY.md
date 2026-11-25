# Multi-Tenancy, Roles and Isolation

Status: tenant isolation, owner/manager roles, platform admin and operational suspension are **implemented and tested**. Other roles are **planned** contracts.

## Tenant resolution

- `TenantContext` (`src/http/middleware/auth.rs`) loads the signed-in account's establishment **and its member role** from `establishment_members` in one query (`find_membership_for_account`). The browser never supplies a tenant id.
- Every tenant query filters `AND establishment_id = ?` and cross-checks foreign keys (`category_id`, `image_asset_id`) against the same establishment.
- New commercial tables carry `establishment_id` and, where a parent exists, a tenant-consistent composite foreign key: `commercial_contracts (billing_reference, environment, establishment_id) → billing_checkouts (reference, environment, establishment_id)`.
- MCP grants reference `(establishment_id, account_id)` in `establishment_members`; writes require an owner.

### Multiple establishments per account (planned)

Today an account sees its first membership. The evolution keeps the server-side rule: the active establishment is stored in the **server session** (`sessions.active_establishment_id`, validated against `establishment_members` on every request); switching is a CSRF-protected POST that only accepts establishments where the account is a member. URL or form parameters never select a tenant. Every cache key, outbox event and audit record keeps `establishment_id`.

## Roles

| Role | Scope | Status | Notes |
|---|---|---|---|
| Platform admin | platform | Implemented | `accounts.role = 'admin'`; `/admin/*`, operator CLI. |
| Owner | establishment | Implemented | All establishment operations, billing management, MCP write. |
| Manager | establishment | Implemented | Menu editing and publishing, billing view; never billing management. |
| Cashier, waiter, kitchen operator, courier | establishment | Planned | Require an expand migration of the `member_role` CHECK. |
| Consumer | storefront / table session | Planned | Guest by default; never a member. |
| Integration credential | establishment | Planned (QR Connect) | Hashed, scoped, revocable. |
| System | platform | Planned | Outbox consumers acting for one establishment. |

Role → operation mapping: `contracts/entitlements/authorization-rules.json` (implemented rows are enforced by `domain::entitlements::Operation`).

## Isolation guarantees and tests

| Threat | Control | Test |
|---|---|---|
| Cross-tenant reads/writes (IDOR) | `TenantContext`, `establishment_id` filters, composite FKs | `tests/auth_and_isolation.rs`, `tests/entitlements.rs` |
| Entitlement leakage | Coverage and grants filtered by establishment and environment | `tests/entitlements.rs` (`chargebacks_*`, `grants_*`) |
| Client price manipulation | Only offer codes and cycles are accepted; quotes are server-side | `tests/entitlements.rs` (`subscription_page_*`), `tests/commercial_engine.rs` |
| Privilege escalation | `Operation::roles`; manager cannot manage billing; non-admin cannot run operator commands | `tests/entitlements.rs` |
| Unauthenticated webhooks | Separate `asaas-access-token`, constant-time compare | `tests/billing.rs` |
| Financial data exposure | Billing page shows statuses and dates only; card and payer data stay at Asaas; events carry no personal data | `tests/commercial_contracts.rs` |

## LGPD

- Personal data today: account emails, Google subject, affiliate PIX keys, masked emails in the partner portal.
- Planned (Commerce Core): consumer records scoped to one establishment, export (JSON) and erasure that anonymizes personal fields while keeping orders, payments and fiscal-relevant records for their legal retention period.
- Never store raw card data; never store financial credentials in plain text (Asaas keys live in the environment; per-establishment commerce credentials will be encrypted at rest with a key outside the database).
