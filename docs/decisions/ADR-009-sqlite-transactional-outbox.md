# ADR-009: Transactional Outbox in SQLite

- **Status**: Accepted
- **Date**: 2026-11-24

Commerce Core Increment 3 implements the dispatcher and stores publication timestamps in an additive companion table. See [Event Contracts](../architecture/EVENT_CONTRACTS.md#dispatcher) for the current design.

## Context

Future modules (Delivery, Kitchen, Fleet, Loyalty, Analytics) must react to each other's state changes without a broker, while the platform keeps running on a single small VPS. A message broker (Kafka, RabbitMQ, Redis streams) would add cost and an extra failure domain. Commercial changes already happen inside `BEGIN IMMEDIATE` SQLite transactions.

## Decision

1. Domain events are appended to `domain_events` **in the same transaction** as the state change that caused them (`infrastructure::database::queries::domain_events::append`).
2. The envelope is fixed: `event_id`, `event_type`, `event_version`, `establishment_id`, `aggregate_type`, `aggregate_id`, `correlation_id`, `causation_id`, `occurred_at`, `payload`.
3. A nullable unique `dedupe_key` makes producers idempotent across webhook redelivery and retries.
4. Ordering is guaranteed per aggregate by the monotonic `seq` rowid; consumers must not assume global ordering across aggregates.
5. Payloads carry identifiers and amounts only. Emails, names, phone numbers and addresses are never written to events.
6. `contracts/events/registry.json` lists every event (implemented or planned) with its version and schema; tests enforce that every implemented Rust event type is registered with a JSON Schema.
7. Delivery to consumers is **not implemented in this phase**: no module consumes events yet. The dispatcher (per-consumer checkpoints on `seq`, retries with backoff and a dead-letter table) is specified in `docs/architecture/EVENT_CONTRACTS.md` and ships with Commerce Core (phase 3). No dispatcher-only columns are created before then.

## Consequences

- Event emission cannot diverge from committed state, and costs one extra insert per state change.
- Until the dispatcher exists, `domain_events` doubles as an append-only audit trail for commercial and operational actions.
- Moving to an external broker later only requires a relay that reads the same table.
