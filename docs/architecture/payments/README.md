# SaaS Billing Providers

Status: Asaas, Stripe and Banco Inter adapters are **implemented and tested against simulated webhooks and loopback API mocks**. **None is homologated** in its provider sandbox yet (no credentials in the development environment). Decision record: [ADR-011](../../decisions/ADR-011-multi-provider-saas-billing.md).

This covers SaaS billing only (the platform charging establishments). Commerce Payments (consumers paying establishments) are a separate, planned domain ([COMMERCE_CORE](../COMMERCE_CORE.md#9-saas-billing-vs-commerce-payments)).

## Routing

| Payment method | Provider | Flow | Recurrence |
|---|---|---|---|
| `card` | Asaas or Stripe (`BILLING_CARD_PROVIDER` when both are configured) | Hosted checkout; the provider stores the card | Provider subscription |
| `pix_boleto` | Banco Inter | Payer data form, then one BolePix charge (boleto + Pix QR) per cycle | Our scheduler, 10 days before each due date |

The billing page lists only the methods whose provider is configured. A subscription never changes provider; switching means cancelling renewal and subscribing again.

## Common model

```text
provider webhook / poll ──▶ adapter ──▶ GatewayEvent ──▶ application::billing_events::apply_events
                                                        (billing_checkouts, billing_payments,
                                                         affiliate ledger, SubscriptionActivated)
```

| GatewayEvent | Asaas | Stripe | Inter |
|---|---|---|---|
| `Checkout` | `CHECKOUT_*` | `checkout.session.completed/expired` | local (charge 0 paid or dead) |
| `Subscription` | `SUBSCRIPTION_*` | `customer.subscription.*` | local (`inter:<reference>`) |
| `Payment` (settled) | `PAYMENT_CONFIRMED/RECEIVED` | `invoice.paid` | charge `RECEBIDO`/`MARCADO_RECEBIDO` (re-fetched) |
| `Payment` (reversal) | refund, chargeback, deletion | — | — |
| `PaymentReversed` / `PaymentRestored` | — | full `charge.refunded`, `charge.dispute.created`/`closed` (resolved to the invoice via `/v1/invoice_payments`) | manual (see INTER.md) |

Rules shared by all providers: amounts are integer centavos; coverage starts at the cycle's due date and lasts the contracted months (`floor` month arithmetic); only a payment equal to the contract price grants coverage; reversals win over settlements at the same timestamp; webhook deliveries are deduplicated by provider-qualified event id (Inter is state-based and idempotent); identifiers of new providers are stored as `stripe:<id>` / `inter:<id>`.

## Data

| Table / column | Purpose |
|---|---|
| `billing_checkouts.provider`, `.payment_method`, `.checkout_url` | Which provider holds the subscription; Stripe's hosted URL |
| `billing_payments.settled_centavos` | Amount restored when a dispute is won |
| `billing_pending_payment_changes` | Newest normalized refund or dispute fact received before its payment, scoped by environment and payment ID |
| `billing_payer_profiles` | CPF/CNPJ, name, e-mail and address required by Inter (personal data; owner-managed) |
| `billing_inter_charges` | One row per Inter cycle: `seuNumero`, request code, status, Pix copy-and-paste, digitable line |

## Security summary

| Provider | Webhook authentication | Outbound authentication |
|---|---|---|
| Asaas | `asaas-access-token` header, constant time | `access_token` header |
| Stripe | `Stripe-Signature` HMAC-SHA256 (raw body, 300 s tolerance, multiple `v1` during rotation) + `livemode` check | Bearer secret or restricted key, pinned `Stripe-Version`, `Idempotency-Key` on POST |
| Inter | Secret path segment + re-fetch of every charge over mTLS (callbacks are hints). Optional: verify Inter's client certificate at Caddy | mTLS with the integration certificate + OAuth2 client credentials (token cached ~55 min; endpoint allows 5 calls/min) |

Secrets live only in the environment; certificates in files readable by the service user. Payloads, keys and payer documents are never logged.

Provider details: [STRIPE.md](STRIPE.md) · [INTER.md](INTER.md) · [Asaas (BILLING.md)](../BILLING.md). Operations: [RUNBOOK](../../operations/RUNBOOK.md#payment-providers-stripe-and-banco-inter).
