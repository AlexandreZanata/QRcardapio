# Event Contracts and Transactional Outbox

Status: outbox append **implemented and tested**; dispatcher and consumers **planned** for Commerce Core (phase 3). Decision record: [ADR-009](../decisions/ADR-009-sqlite-transactional-outbox.md). Registry: [`contracts/events/registry.json`](../../contracts/events/registry.json).

## Envelope (`domain_events`)

| Column | Meaning |
|---|---|
| `seq` | Monotonic rowid. Ordering is guaranteed per aggregate only. |
| `event_id` | UUIDv7, globally unique. Consumers deduplicate on it. |
| `event_type`, `event_version` | Registry name and integer version (≥ 1). |
| `establishment_id` | Tenant. Every event belongs to exactly one establishment. |
| `aggregate_type`, `aggregate_id` | The state machine instance that changed. |
| `correlation_id` | Flow identifier (for webhooks: the Asaas event id; otherwise the event's own id). |
| `causation_id` | Event that caused this one, when produced by a consumer. |
| `occurred_at` | RFC 3339 UTC. |
| `payload` | JSON validated by `json_valid`; schema per version under `contracts/events/schemas/`. |
| `dedupe_key` | Optional unique key that makes producers idempotent (`SubscriptionActivated:{contract}`, `CatalogPublished:menu:{establishment}:{version}`, `EntitlementGranted:{grant}`). |

Rules: append in the **same transaction** as the state change (`queries::domain_events::append`); payloads carry identifiers and integer amounts only, never emails, names, phones, addresses, card data, tokens or free text; a new incompatible payload means a new `event_version` and a new schema file, and producers may dual-write during migration.

## Implemented events

| Event | Producer | Emitted when |
|---|---|---|
| `SubscriptionActivated` v1 | `commercial_contracts::activate_for_subscription` | First confirmed payment at the exact contracted amount covers a contract (webhook or manual reconciliation). Once per contract. Legacy contracts backfilled by the migration emit nothing. |
| `EntitlementGranted` v1 / `EntitlementRevoked` v1 | `application::operator` | Explicit grant created / revoked by a platform admin. |
| `EstablishmentSuspended` v1 / `EstablishmentReinstated` v1 | `application::operator` | Operational status changed. Free-text reasons stay in the table, not in the event. |
| `CatalogPublished` v1 | `queries::publications::record_publication_success` | A static menu snapshot became live (`channel: "menu"`). |
| `OrderCreated` v1 | `commerce::order_creation` | An order and its items were persisted from the current channel snapshot. |
| `OrderAccepted` v1 / `OrderRejected` v1 | `commerce::order_transitions` | A placed order was accepted or rejected. |
| `OrderPreparing` v1 / `OrderReady` v1 / `OrderCompleted` v1 | `commerce::order_transitions` | The order entered preparation, became ready or was completed. |
| `OrderCancelled` v1 | `commerce::order_transitions` | An allowed cancellation committed with an enumerated reason and actor kind. |

`tests/commercial_contracts.rs` enforces that every Rust `EventType` is registered as implemented with a strict JSON Schema (no additional properties, no personal-data fields) and that every planned event has a version and payload description.

Order creation and every applied order transition append exactly one order event in the write transaction. All seven schemas require every declared key and reject additional keys. They carry the order UUID and typed fields for channel, amount, count, version, reason, fulfillment and timestamps. `OrderCreated` uses the random public order ID as its dedupe key.

## Planned events

Subscription lifecycle (`SubscriptionChanged`, `SubscriptionExpired`), commerce payments (`PaymentConfirmed`, `PaymentRefunded`), tables, kitchen, fleet, loyalty and POS events are listed in the registry with their payloads. None is emitted today.

## Dispatcher (phase 3 design)

The canonical design is [COMMERCE_CORE §11](COMMERCE_CORE.md#11-planned-dispatcher). Summary:

1. One in-process Tokio task polls `domain_events` by `seq`; no broker and no extra process.
2. Per-consumer checkpoints (`event_consumer_checkpoints`), receipts (`event_consumer_receipts`, PK `(consumer, event_id)`) and failures (`event_consumer_failures`). A handler's writes, its receipt and its checkpoint commit in one `BEGIN IMMEDIATE` transaction, so replays never duplicate effects.
3. Exponential backoff up to 10 attempts, then dead-letter per consumer; later events of the same aggregate are parked for that consumer to keep per-aggregate order, other aggregates continue.
4. Consumers act as the planned `system` role and re-check their own module capability before creating new work; compensations of existing work keep running after expiry.
5. `published_at` (low-water mark) and its partial index arrive with the dispatcher in an expand-only migration.
6. `SubscriptionExpired` is produced by a daily job comparing coverage with the previous run.
7. Retention: events older than 13 months are archived by `VACUUM INTO` snapshot before deletion; financial and audit events follow the legal retention policy.
