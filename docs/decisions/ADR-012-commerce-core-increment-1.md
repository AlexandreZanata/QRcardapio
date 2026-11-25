# ADR-012 — Commerce Core Increment 1 boundaries

- **Status** Accepted
- **Date** 2026-11-25

## Context

Increment 1 adds internal catalog and order persistence. Ordering channels will share the pricing rules and daily numbering. The Commerce Core specification leaves the per-establishment timezone and the permissions for manual orders open.

## Decision

1. Calculate the order number’s date in the IANA zone `America/Sao_Paulo` at placement. Number orders from 1 within each establishment and local date. The business-hours increment will define overnight intervals and per-establishment timezones. Keep dates and numbers already issued when the scheduling policy expands.
2. Cap the final order total at 1,000,000 centavos. Use checked integer arithmetic and round a percentage discount half-up on the items subtotal. Apply the delivery fee after the discount. Count distinct options toward group selection limits and apply quantity limits separately to each option.
3. Promote visible, tenant-scoped catalog data in a dedicated internal publication transaction. Read only immutable snapshot content for order creation. Derive availability revisions from the current snapshot, so a sold-out toggle preserves any unrelated draft changes. QR Menu keeps its own static publication path.
4. Store all six channel values. Keep `manual` callable only through the trusted internal foundation until a creating operation is defined. Use the rejection and cancellation codes specified by QR Delivery for order events. Persist fulfillment state independently of payment.

## Consequences

Jiff embeds the São Paulo rules used at order placement. A time-zone data refresh requires rebuilding the binary. The future checkout must add idempotency and commercial authorization at its entry point. Courier payment permission, kitchen compensation after readiness, customer retention and the other open cross-module decisions remain in [Commerce Core §16](../architecture/COMMERCE_CORE.md#16-decisions-still-open).
