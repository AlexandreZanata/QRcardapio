# Stripe (card subscriptions)

Status: implemented; unit tests for the checkout payload, signature and event parsing; integration test with signed events and a loopback API mock (`tests/payment_providers.rs`). **Not homologated.**

Code: `src/infrastructure/stripe/{mod,signature,webhook}.rs`, `src/application/billing_events.rs` (`process_stripe_event`), `src/http/handlers/billing_webhooks.rs`.

## API version

Requests send `Stripe-Version: 2026-11-18.endive` (`stripe::API_VERSION`). The webhook endpoint must be created with the same API version and **snapshot** payloads; Stripe renders event payloads with the endpoint's version, and it cannot be changed after creation (create a new endpoint to upgrade). Since the 2025-03-31 "basil" release the integration relies on:

- `invoice.parent.subscription_details.subscription` (not `invoice.subscription`);
- invoice line `period.start` as the service period (not `invoice.period_start`);
- `/v1/invoice_payments?payment[type]=payment_intent&payment[payment_intent]=…` to map refunds and disputes to the invoice (charges no longer carry the invoice);
- `allowed_payment_method_types[]=card` (the old `payment_method_types` returns 400 since 2026-09-30).

## Flow

1. `POST /v1/checkout/sessions` (`mode=subscription`, one line with inline `price_data`: `currency=brl`, `unit_amount` = contract total in centavos, `recurring[interval]=month`, `recurring[interval_count]` = 1/6/12), `client_reference_id` and metadata = our reference, `expires_at` = now + 35 min, `locale=pt-BR`, `Idempotency-Key: checkout-<reference>`. The session id (`stripe:cs_…`) and `url` are stored on `billing_checkouts`.
2. `checkout.session.completed` → checkout paid + subscription linked (`stripe:sub_…`).
3. `invoice.paid` (status `paid`, currency `brl`, `amount_remaining = 0`) → payment `stripe:in_…` with due date = line period start in UTC−03:00.
4. `customer.subscription.updated/deleted`: `active`/`trialing` → active; `past_due`/`unpaid`/`paused` → inactive; `canceled`, `incomplete_expired` or `cancel_at_period_end=true` → renewal cancelled.
5. Full `charge.refunded` and `charge.dispute.created` revoke the invoice's coverage; `charge.dispute.closed` with `won`/`warning_closed` restores it; partial refunds keep coverage (same policy as Asaas).

If a refund or dispute arrives before `invoice.paid`, the webhook transaction saves the newest fact with its deduplication record. Settlement applies that fact before granting coverage, calculating affiliate commissions, or emitting `SubscriptionActivated`. Reversals take precedence when timestamps tie.
6. Cancel renewal: `POST /v1/subscriptions/{id}` with `cancel_at_period_end=true` (paid period kept).
7. Reconcile: `GET /v1/checkout/sessions/{id}` returns the subscription when its event was missed.

## Webhook

Endpoint `POST /webhooks/stripe`. Verification: split `Stripe-Signature` on commas, take `t` and every `v1`, compute HMAC-SHA256 of `"{t}.{raw body}"` with the endpoint secret, compare in constant time, reject when `|now − t| > 300 s`. Events whose `livemode` differs from the configured key are rejected. Deduplication key: `stripe:<evt_id>`. Subscribe to: `checkout.session.completed`, `checkout.session.expired`, `checkout.session.async_payment_succeeded`, `customer.subscription.created`, `customer.subscription.updated`, `customer.subscription.deleted`, `invoice.paid`, `charge.refunded`, `charge.dispute.created`, `charge.dispute.closed`.

## Brazil notes

BRL has two decimals and a minimum charge of R$ 0,50. Brazilian accounts accept Visa and Mastercard cards; payouts are automatic and daily. Stripe Pix (Pix Automático for subscriptions) is invite-only for Brazilian accounts and Boleto has no refunds or chargebacks; neither is enabled here.

## Open items (not verified in Stripe's documentation)

- Whether the signing key is the full `whsec_…` string (official SDKs use it as-is; we do too).
- HTTP status returned with `idempotency_error`.

References: [API versioning](https://docs.stripe.com/api/versioning) · [Checkout Sessions](https://docs.stripe.com/api/checkout/sessions/create) · [Subscriptions](https://docs.stripe.com/api/subscriptions/object) · [Invoices](https://docs.stripe.com/api/invoices/object) · [Invoice payments](https://docs.stripe.com/api/invoice-payment/list) · [Webhook signatures](https://docs.stripe.com/webhooks#verify-manually) · [Event types](https://docs.stripe.com/api/events/types) · [Billing webhooks](https://docs.stripe.com/billing/subscriptions/webhooks)
