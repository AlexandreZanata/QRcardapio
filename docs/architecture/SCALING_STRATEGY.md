# Scaling Strategy

Status: strategy document. Current measurements: [PERFORMANCE](../performance/PERFORMANCE.md).

## Principles

1. **Static first.** Public, read-mostly surfaces (menus today, storefront catalogs later) are atomic static snapshots served by Caddy and later a CDN, with zero database queries.
2. **Authoritative writes.** Prices, availability, permissions and order state are always resolved by the backend against SQLite, never from device snapshots.
3. **One process, one database** until measurements say otherwise. No Redis, broker, microservices or per-tenant databases.
4. **Measure the path you change.** Static-menu benchmarks say nothing about order throughput.

## Current shape

| Path | Cost per request | Notes |
|---|---|---|
| `/c/{slug}` | file read | Caddy or Axum `ServeDir`; unaffected by the Commercial Engine. |
| Dashboard pages | a few indexed reads | The billing page adds one coverage query and one grants query per establishment. |
| Publish | one entitlement resolution + render + atomic rename + 1 outbox insert | Entitlement resolution reads `commercial_entitlement_periods` filtered by establishment and environment (indexed by `idx_commercial_contracts_establishment` and the billing indexes). |
| Webhook | one `BEGIN IMMEDIATE` transaction | Adds contract activation and at most one outbox row. |

## Growth path

| Trigger | Action |
|---|---|
| Order writes > ~50 sustained writes/s or p99 write latency > 50 ms on the VPS | Batch outbox dispatch, shorten transactions, move read models (analytics) to a second SQLite file attached read-only, then evaluate PostgreSQL behind the `queries/` boundary. |
| Public traffic spikes | CDN in front of Caddy for `/c/*`, `/s/*` static trees and `/static/*` (already immutable-versioned). |
| Storage of media | Object storage behind `infrastructure::storage` with the same content-addressed names. |
| Notifications volume | Dedicated outbox consumer with rate limits per provider. |
| Horizontal app servers | Requires PostgreSQL (or LiteFS-style replication) first; sessions and rate limits move to the database; the publish lock becomes a database advisory lock. Phase 12. |

## Benchmarks required before each operational phase

- Commerce Core: concurrent checkout writes (100 establishments × 5 concurrent orders), idempotent retries, outbox append+dispatch throughput, p99 under 1 vCPU.
- Delivery: storefront snapshot publication time and size, quote endpoint latency, order center polling cost.
- Payments: webhook bursts with duplicates and out-of-order delivery.
- Notifications: queue drain under provider throttling.

Use `scripts/bench_run.sh` as the template and record results in `docs/performance/`.
