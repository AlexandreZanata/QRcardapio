# QR Connect

> **Status: Planned — not implemented, not sellable.** Nothing in this document is available to customers. No Connect endpoint exists, and no OpenAPI document is published. The existing MCP connector (`/mcp`) is **not** part of Connect and stays available as described in [MCP.md](../architecture/MCP.md).

| Field | Value |
|---|---|
| Module code | `connect` (commercial name "QR Connect") |
| Status | Planned — not implemented, not sellable |
| Phase | 11 |
| Lifecycle | `planned` (sale: `never`, grants: `never`, per [module-lifecycle.json](../../contracts/commercial/module-lifecycle.json)) |
| Monthly price | R$ 24,90 (`monthly_price_centavos: 2490`, catalog version 1, effective 2026-11-24) |
| Bundles | `complete` (QR Complete) only |
| Affiliate eligible | Yes |
| Standalone | Yes (`requires_modules: []`) |
| Module contract | [contracts/modules/connect.json](../../contracts/modules/connect.json) |

Source of truth: [modules.json](../../contracts/commercial/modules.json), [capabilities.json](../../contracts/entitlements/capabilities.json), [authorization-rules.json](../../contracts/entitlements/authorization-rules.json), [registry.json](../../contracts/events/registry.json), [MCP tools](../../contracts/mcp/tools.json), [MCP clients](../../contracts/mcp/clients.json). See also [MODULE_CATALOG.md](../business/MODULE_CATALOG.md) and [EVENT_CONTRACTS.md](../architecture/EVENT_CONTRACTS.md). If this document and a contract disagree, the contract wins.

## 1. Commercial objective

Sell **commercial programmatic access** to an establishment's data and operations: a versioned REST API with scoped, revocable credentials, outbound webhooks, import/export, automations and advanced MCP features, documented by OpenAPI. Connect monetizes external integrations (ERPs, accounting, BI, marketing tools, in-house apps). It never monetizes the internal APIs QR Cardápio's own apps use, and never takes away the MCP connector current customers already use.

## 2. Target audience

| Audience | Need |
|---|---|
| Owners with an accountant, ERP or BI tool | Pull orders, payments and catalog automatically. |
| Multi-unit operators and agencies | Sync catalog and receive order events in their own systems. |
| Developers hired by an establishment | Stable, documented, versioned API with test-friendly idempotency. |

Not targeted: consumers, couriers, or owners who only use AI assistants through the existing MCP connector (no Connect needed).

## 3. Problems solved

| Problem | How Connect addresses it |
|---|---|
| Manual re-typing of sales into ERPs or spreadsheets. | Read API and webhooks for orders and payments. |
| Polling for changes. | Signed webhooks for the contracted events. |
| Shared passwords with third parties. | Per-integration credentials with scopes, expiry and instant revocation. |
| Duplicate writes on network retries. | Mandatory `Idempotency-Key` on writes. |
| No visibility into what an integration did. | Per-credential request logs and webhook delivery logs. |

## 4. MVP scope

1. Integration credentials per establishment: issue (`connect.credential.issue`, secret shown once), scope selection, optional expiry, revocation (`connect.credential.revoke`), last-used timestamp.
2. Versioned REST API under `/api/v1/` for implemented resources only (catalog read first; orders and payments read once Commerce Core exists).
3. Credential scopes enforced on every `connect.api.call` in addition to `connect.api.use`.
4. Per-credential and per-establishment rate limits with `429` and `Retry-After`.
5. Request logs (`ApiRequestLog`) visible to the owner.
6. Idempotency for every non-GET call (`IdempotencyRecord`).
7. Outbound webhooks for the eight consumed events (section 15), configured through `connect.webhook.configure`, HMAC-signed, retried, with delivery logs.
8. `GET /api/v1/openapi.json` generated from implemented routes only.
9. Dashboard page `GET /dashboard/integrations/credentials` in `pt-BR`, `en` and `es`.

## 5. Future scope

- Write APIs: catalog write, orders with channel `external` through Commerce Core.
- Bulk import/export jobs over the API (catalog, orders history).
- Automations: declarative rules "on event X, call webhook / perform contracted operation Y".
- Advanced MCP features: new MCP tools for orders, analytics or POS gated by Connect, with new OAuth scopes beyond `menu.read` / `menu.write`.
- Third-party developer apps (OAuth authorization code for partners) and named external integrations; integrations Connect does not cover remain the `exclusive_integrations` add-on (custom quote).

## 6. Domain entities

| Entity | Purpose | Key fields (conceptual) |
|---|---|---|
| `IntegrationCredential` | Secret that authenticates one integration for one establishment. | `id`, `establishment_id`, label, `secret_hash`, `last4`, `expires_at`, `revoked_at`, `created_by` |
| `CredentialScope` | Scope granted to a credential. | `credential_id`, `establishment_id`, `scope` |
| `WebhookEndpoint` | HTTPS destination and subscribed events. | `id`, `establishment_id`, URL, event types, secret version, state, consecutive failures |
| `WebhookDelivery` | One attempt sequence for one event to one endpoint. | `endpoint_id`, `event_id`, attempt count, next attempt, last status code, outcome |
| `ApiRequestLog` | Metadata of one API call. | `credential_id`, method, route template, status, latency, request id, timestamp |
| `IdempotencyRecord` | Stored outcome of a write. | `(establishment_id, credential_id, key)`, request hash, response status and body, `expires_at` |

## 7. Responsibilities

Connect **owns**: credentials, scopes, authentication of `/api/v1/*`, rate limiting, request logs, idempotency records, webhook endpoints and deliveries, OpenAPI generation, and (future) automations and advanced MCP tools.

Connect **delegates**: every business operation to the owning module's application layer, which runs its own `authorize(...)`. Connect never bypasses another module's lifecycle, roles or capability (for example, publishing a menu through the API still requires `menu.publish`).

Connect **does not own**: the existing MCP connector, OAuth for AI clients, internal app APIs, or any business data.

### Internal APIs are not Connect

| Surface | Audience | Owner | Connect? |
|---|---|---|---|
| `/dashboard/*` (session + `_csrf`) | Members | Each module | No |
| `/mcp` and `/mcp/oauth/*` (`menu.read`, `menu.write`) | AI clients authorized by an owner | QR Menu / QR Core | No |
| `/api/storefront/v1/*` | Consumers of the Delivery storefront | QR Delivery | No |
| `/api/tables/v1/*` | Consumers in a table session | QR Tables | No |
| `/api/tools/qr.svg`, `/api/tools/qr.png` | Public free tools | QR Core | No |
| `/api/v1/*` | `integration_credential` | QR Connect | Yes |

Internal APIs may change with app releases. Only `/api/v1/*` carries a public stability promise: additive changes within `v1`; breaking changes ship as `v2` with a published deprecation period.

### Existing MCP connector (preserved)

The six tools `get_account_settings`, `update_account_settings`, `get_menu_draft`, `apply_menu_changes`, `publish_menu` and `unpublish_menu`, with OAuth scopes `menu.read` and `menu.write`, remain available to current customers **without Connect**. Connect's lifecycle, purchase, expiry or cancellation never changes them. `publish_menu` continues to be gated only by `menu.publish`. Advanced MCP features are additive tools and scopes; they never move the six existing tools behind Connect.

## 8. Invariants

1. A credential belongs to exactly one `establishment_id`; it can never read or write another tenant.
2. Secrets are stored only as SHA-256 hashes of high-entropy random values (consistent with session and MCP tokens); the plaintext is shown once.
3. A revoked or expired credential fails authentication on the next request; there is no cache that outlives revocation.
4. Every call requires `connect.api.use`, the needed scope, and the target operation's own authorization; failing any one denies the call.
5. A non-GET call without `Idempotency-Key` is rejected; the same key with a different request hash is rejected; the same key with the same hash replays the stored response.
6. Webhook payloads contain only the event envelope; no PII, no secrets, no enrichment beyond the registry payload.
7. OpenAPI lists only implemented routes (release gate).
8. Existing MCP grants keep `menu.read` / `menu.write` regardless of Connect state (release gate).
9. Money in API payloads is integer centavos (`*_centavos`); no decimals or floats.

## 9. Business rules

| Topic | Rule |
|---|---|
| Credential issue | `connect.credential.issue`: owner only, requires `connect.api.use`; label required; at least one scope; optional expiry. |
| Credential limit | Bounded number of active credentials per establishment (design default 10, not contracted). |
| Scopes | Naming follows the MCP convention `<resource>.read` / `<resource>.write`. **Open decision**: the scope list must be contracted (for example `contracts/connect/scopes.json`) before implementation. A scope never grants more than the role-equivalent of `manager`. |
| Revocation | `connect.credential.revoke`: owner only, no capability, `allow`; immediate; available after expiry and during suspension. **Open decision**: automatic revocation when the creating member is removed. |
| Webhook management | `connect.webhook.configure`: owner only, requires `connect.webhook.manage`. |
| Rate limits | GCRA (`governor`) per credential and per establishment; `429` with `Retry-After`; counters reported to `usage_metering`. |
| Idempotency | `Idempotency-Key` up to 128 ASCII chars; records kept 7 days, aligned with the MCP `operation_key` window. |
| Webhooks | HTTPS only; at-least-once delivery; receivers dedupe by `event_id`; ordering guaranteed only per aggregate; exponential backoff for up to 24 hours, then dead letter; endpoint disabled after repeated consecutive failures and shown as such in the dashboard. |
| Webhook signature | HMAC-SHA256 over timestamp and body with a per-endpoint secret; receivers reject timestamps older than 5 minutes; secret rotation keeps the previous secret valid for 24 hours. |
| Logs | Request logs and delivery logs retained 30 days (design default, not contracted); bodies never stored. |
| Errors | JSON problem objects with a stable `code` mapped from the i18n error keys; `401` (auth), `403` (scope, permission or capability, distinguishable), `404` (other tenant or missing), `409`, `422`, `429`. |

## 10. State machines

`state_machines` is empty in the module contract. Connect drives no machine of [state-machines.json](../../contracts/commerce/state-machines.json) by itself; future write APIs call Commerce Core, which enforces the `order` and `order_payment` machines. Credential validity (active, expired, revoked) and webhook endpoint health (enabled, disabled) are internal statuses, not contracted machines, and emit no events in contract version 1.

## 11. Permissions

| Operation | Roles | Capability | After expiry | Source |
|---|---|---|---|---|
| `connect.api.call` | `integration_credential` | `connect.api.use` | `deny` | authorization-rules.json |
| `connect.credential.issue` | `owner` | `connect.api.use` | `deny` | authorization-rules.json |
| `connect.credential.revoke` | `owner` | none | `allow` | authorization-rules.json |
| `connect.webhook.configure` | `owner` | `connect.webhook.manage` | `deny` | authorization-rules.json |

**Open decisions**: which roles may view `/dashboard/integrations/credentials` and the logs (no read operation is contracted), and whether disabling or deleting a webhook endpoint after expiry needs an `allow` operation separate from `connect.webhook.configure`.

`integration_credential` is a `planned` role. Webhook delivery runs as the planned `system` role, which re-checks `connect.webhook.manage` for the establishment. `authorize(...)` evaluates module lifecycle, establishment operational state, member permission (credential scopes take the place of member roles), deployment enforcement and commercial entitlement, in that order.

## 12. Commercial capabilities

| Capability | Summary (capabilities.json) |
|---|---|
| `connect.api.use` | Use the versioned commercial REST API with scoped credentials. |
| `connect.webhook.manage` | Manage outbound webhooks to external systems. |

Granted by `connect` version 1 (R$ 24,90, `2490` centavos) and the `complete` bundle. Cycle discounts come only from [billing-cycles.json](../../contracts/commercial/billing-cycles.json) through the Commercial Engine. Contracted after-expiry behavior: "Credentials stop authenticating and webhooks stop. Existing MCP connections granted before this module keep their menu.read and menu.write scopes."

## 13. Technical dependencies

| Dependency | Status | Why |
|---|---|---|
| `identity`, `establishments`, `members`, `tenancy` | Implemented | Owners manage credentials; tenant scoping. |
| `commercial`, `entitlements` | Implemented | Capabilities and `authorize(...)`. |
| `events` | Partial | Webhooks need the outbox dispatcher (Commerce Core, phase 3). |
| `usage_metering` | Planned | Per-establishment API and webhook counters; Connect cannot launch without it. |
| Optional modules (`menu`, `delivery`, `tables`, `kitchen`, `fleet`, `loyalty`, `analytics`, `pos`) | Only `menu` implemented | Resources exposed by the API grow with them. |
| `reqwest` (existing) | Implemented | Outbound webhook calls; no new HTTP client. |

No API gateway, no Redis, no message broker, no Node runtime. Webhooks are sent by an in-process worker reading the outbox.

## 14. Published events

None. `events.publishes` is empty in [connect.json](../../contracts/modules/connect.json). Writes made through the API emit the owning module's events (for example `CatalogPublished`, written by `core.publishing` for QR Menu), never Connect events.

## 15. Consumed events

These are the only event types available as webhooks in contract version 1.

| Event | Producer | Webhook use |
|---|---|---|
| `CatalogPublished` | `core.publishing` | Catalog changed (`channel`, `publication_version`); integrator re-reads the catalog. |
| `OrderCreated` | `commerce` | New order in any channel. |
| `OrderAccepted` | `commerce` | Order accepted with `promised_at`. |
| `OrderCompleted` | `commerce` | Order completed. |
| `OrderCancelled` | `commerce` | Order cancelled with `reason_code`. |
| `PaymentConfirmed` | `commerce.payments` | Payment confirmed (`amount_centavos`, `method`). |
| `PaymentRefunded` | `commerce.payments` | Refund registered. |
| `DeliveryCompleted` | `fleet` | Delivery finished; `order_id` is null for deliveries registered outside an order. |

The consumer is idempotent by `event_id`; each `(endpoint_id, event_id)` pair produces at most one `WebhookDelivery`. Events are delivered only to endpoints of the same `establishment_id` that subscribed to that type.

## 16. HTTP contracts

All routes are **planned and not available**. The OpenAPI document itself is planned and, once live, lists only implemented endpoints.

| Method | Path | Audience | Status | Behavior |
|---|---|---|---|---|
| `GET` | `/api/v1/openapi.json` | `integration_credential` | Planned | OpenAPI 3.1 of implemented `/api/v1/*` routes only; `Authorization: Bearer` required. |
| `GET` | `/dashboard/integrations/credentials` | `member` | Planned | Askama SSR list of credentials, scopes, last use; `TenantContext`; `no-store`. |

**Open decision**: the routes for `connect.credential.issue`, `connect.credential.revoke` and `connect.webhook.configure`, and every resource route, must be added to `contracts/modules/connect.json` (Axum 0.8 `{id}` syntax, for example `/dashboard/integrations/credentials/{id}/revoke`) before implementation. Conventions for `/api/v1/*`: JSON only, `Authorization: Bearer <credential>` header only (never query strings), cursor pagination, `X-Request-Id` on every response, CORS disabled (server-to-server API).

## 17. Integration contracts

- **Inbound API.** Handler extracts the credential, resolves `establishment_id` from it, calls `authorize(connect.api.call)`, checks scope, applies rate limit and idempotency, then calls the owning module's application function, which runs its own authorization.
- **Outbound webhooks.** An outbox consumer running as the planned `system` role, idempotent by `event_id`. Headers carry delivery id, event type, timestamp and signature; body is the registry envelope (`event_id`, `event_type`, `event_version`, `establishment_id`, `aggregate_type`, `aggregate_id`, `correlation_id`, `causation_id`, `occurred_at`, `payload`). Receivers must answer `2xx` within 10 seconds.
- **MCP.** The existing connector keeps its own OAuth (`mcp_grants`, `menu.read` / `menu.write`). The `scope` columns of the MCP tables only accept `menu.read` and `menu.read menu.write`, so advanced MCP scopes require a new expand-only table (ADR-010), not an altered CHECK constraint.
- **Analytics.** Future read scope over aggregates only.
- **Usage metering.** Connect reports call and delivery counts; external costs, if any, follow the `external_usage` add-on policy.

## 18. Conceptual persistence model

| Table (proposed) | Key / constraints | Notes |
|---|---|---|
| `connect_credentials` | `id`; unique `secret_hash` | `establishment_id`, label, `last4`, `expires_at`, `revoked_at`, `last_used_at`. |
| `connect_credential_scopes` | `(credential_id, scope)` | `establishment_id` repeated and cross-verified. |
| `connect_webhook_endpoints` | `id`; `(establishment_id)` | URL, subscribed types, `secret_version`, enabled flag, failure counter. |
| `connect_webhook_deliveries` | unique `(endpoint_id, event_id)` | Attempts, `next_attempt_at`, last status, outcome. |
| `connect_request_logs` | `id`; index `(establishment_id, created_at)` | Metadata only; pruned after retention. |
| `connect_idempotency_records` | `(establishment_id, credential_id, idempotency_key)` | Request hash, stored response, `expires_at`. |

Webhook secrets are derived (HMAC of a server master key from `Config` with endpoint id and `secret_version`) so no plaintext secret is stored. Queries live in `src/infrastructure/database/queries/connect_*.rs`.

## 19. Security requirements

- Credentials: 32-byte CSPRNG secrets with a non-secret prefix for lookup; hash comparison in constant time (`subtle`); never logged; `Authorization` treated as a sensitive header by `tower-http`.
- Credentials hashed at rest and revocable; per-credential rate limits (release gates).
- SSRF defense for webhooks: HTTPS only, resolve DNS and reject loopback, private, link-local, CGNAT and metadata ranges (IPv4 and IPv6), pin the resolved address, no redirects, connect and total timeouts, response body discarded beyond 1 KiB, bounded concurrency.
- Request body size limits; strict JSON schemas; unknown fields rejected on writes.
- Dashboard pages: `TenantContext`, `_csrf` on every `POST`, strict CSP.
- Logs record route templates, never query strings, bodies or secrets.
- Every denial is logged with credential id and reason code for the owner's audit view.

## 20. Privacy requirements

- Webhooks carry only registry payloads, which exclude PII by contract.
- Future API resources that include personal data (customers, addresses) require explicit scopes, LGPD review and minimization; they are not in MVP.
- Request logs store no IP beyond rate-limit memory and no bodies.
- Deleting an establishment deletes credentials, endpoints, deliveries, logs and idempotency records.

## 21. Performance requirements

- Authentication + authorization overhead p95 under 5 ms per call on the reference VPS.
- Catalog read p95 under 50 ms for 1,000 products.
- Webhook worker: bounded concurrency, never blocks request handlers or the SQLite writer; backlog drained in `seq` order per aggregate.
- Request logging batched to avoid one extra write transaction per call.
- No effect on `/c/{slug}` static delivery or on `/mcp` latency.

## 22. Behavior without other modules

Connect works standalone on QR Core: credentials, scopes, revocation, logs, rate limits and OpenAPI work with no other module. Exposed resources are limited to QR Core data (establishment profile, draft catalog read). Webhooks are configurable but receive only events produced by modules the establishment actually uses; with none, the dashboard says so explicitly.

## 23. Behavior integrated with other modules

| Module | Effect |
|---|---|
| QR Menu | `CatalogPublished` webhook; catalog read API; publishing through API still requires `menu.publish`. |
| QR Delivery / QR Tables / QR POS | Order and payment webhooks and read APIs through Commerce Core. |
| QR Fleet | `DeliveryCompleted` webhook. |
| QR Kitchen / QR Loyalty | No consumed events in contract version 1. |
| QR Analytics | Future read scope over aggregates. |

## 24. Behavior after suspension or cancellation

- **Expiry or cancellation of Connect**: credentials stop authenticating and webhooks stop (contracted `after_expiry`). Events that occur while expired are not queued for later delivery; integrators reconcile through the API after renewal. `connect.credential.revoke` (`allow`) and log viewing remain available; `connect.credential.issue` and `connect.webhook.configure` are denied.
- **Existing MCP**: connections granted before this module keep their `menu.read` and `menu.write` scopes; MCP is unaffected by Connect state.
- **Payment grace**: `payment_grace_days` is `0`.
- **Administrative or security suspension**: the contract `suspension` rule blocks only `after_expiry: deny` operations (`OperationalStatus::blocks`): `connect.api.call`, `connect.credential.issue` and `connect.webhook.configure` are refused; `connect.credential.revoke` stays available. Webhook deliveries pause while suspended (design choice, not contracted).
- **Module retirement**: announced deprecation period for `/api/v1/*`; credentials stop at `retired`.

## 25. Acceptance criteria

1. The six existing MCP tools and their scopes work identically with and without Connect (release gate "existing MCP grants unaffected").
2. Credentials are hashed at rest and revocable; a revoked credential fails on the next request.
3. Per-credential rate limits return `429` with `Retry-After`.
4. OpenAPI lists exactly the implemented routes; a test diffs it against the router.
5. Replayed writes with the same `Idempotency-Key` return the original response and create nothing.
6. Webhook signatures verify with the documented algorithm; private-network URLs are rejected.
7. No cross-tenant data via API or webhooks.
8. Only the owner can issue credentials, revoke them and configure webhooks; revocation works after expiry and during suspension.
9. i18n parity; `cargo fmt`, `clippy -D warnings`, `cargo test --locked` pass.

## 26. Test scenarios

| ID | Scenario | Expected |
|---|---|---|
| CN-01 | MCP `publish_menu` for an establishment without Connect | Works; gated only by `menu.publish`. |
| CN-02 | Credential of A requests resource of B | `404`. |
| CN-03 | Credential without the needed scope | `403` with scope code. |
| CN-04 | Valid credential, Connect expired | `403` with "not purchased" code. |
| CN-05 | Revoke then call | `401`. |
| CN-06 | Same `Idempotency-Key`, different body | Rejected; no side effect. |
| CN-07 | Webhook URL resolving to `127.0.0.1` or `169.254.169.254` | Rejected at save and at send. |
| CN-08 | `OrderCreated` dispatched twice | One delivery per endpoint. |
| CN-09 | Receiver returns `500` repeatedly | Backoff, dead letter, endpoint disabled and visible. |
| CN-10 | Burst above credential limit | `429`; other credentials unaffected. |
| CN-11 | Module still `planned` | Every `/api/v1/*` call denied by the lifecycle check. |
| CN-12 | `manager` tries `connect.credential.issue` or `connect.webhook.configure` | `403` "not permitted". |
| CN-13 | Suspended establishment | API calls, issue and webhook configuration refused; owner revocation succeeds. |
| CN-14 | Connect expired, owner revokes a credential | Allowed; credential gone. |
| CN-15 | `DeliveryCompleted` with null `order_id` | Webhook delivered with the registry payload unchanged. |

## 27. Commercial launch requirements

1. Contracted release gates: "credentials hashed at rest and revocable", "per-credential rate limits", "OpenAPI published only for implemented endpoints", "existing MCP grants unaffected".
2. Commerce Core dispatcher and `usage_metering` in production.
3. Open decisions closed in the contracts: a contracted scope list; management and resource routes in `connect.json`; read access roles for the credentials page; webhook endpoint removal after expiry.
4. Published API versioning and deprecation policy; developer documentation in `pt-BR`, `en`, `es`.
5. Security review of SSRF defenses and secret derivation; `cargo audit` if any dependency is added.
6. Lifecycle `planned → development → internal_preview → beta → available`; immutable version content from `beta` (ADR-007); landing copy states that the AI connector does not require Connect.
