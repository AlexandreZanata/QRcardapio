# ADR-011: Multi-Provider SaaS Billing (Asaas, Stripe, Banco Inter)

- **Status**: Accepted
- **Date**: 2026-11-24

## Context

SaaS billing (the platform charging establishments) was hard-wired to Asaas: `AppState.asaas`, an Asaas-shaped webhook `Change`, Asaas id formats and the Asaas environment as the only enforcement switch. The business wants Stripe and Banco Inter available as well, at the same time. Stripe offers hosted recurring card checkout. Inter offers no card or hosted checkout API; it issues boleto-with-Pix charges (Cobrança v3) and Pix charges, with recurrence left to the integrator. Product decisions (2026-11-24):

1. The three providers can be active simultaneously; the customer chooses the payment method and each subscription stays with the provider it was contracted on.
2. Inter subscriptions use one charge per cycle issued by our scheduler, which requires collecting the payer's CPF/CNPJ and address.

## Decision

1. **One provider-neutral event model.** Every provider adapter translates its webhooks or polled state into `domain::payments::GatewayEvent` (`Checkout`, `Subscription`, `Payment`, `PaymentReversed`, `PaymentRestored`). `application::billing_events::apply_events` applies them in one `BEGIN IMMEDIATE` transaction with the existing coverage, affiliate and outbox logic. Coverage still requires a payment confirmed at exactly the contracted amount.
2. **Same tables, qualified identifiers.** `billing_checkouts` gains `provider`, `payment_method` and `checkout_url`; `billing_payments` gains `settled_centavos`. Stripe and Inter identifiers are stored as `stripe:<id>` / `inter:<id>` so the legacy unique keys stay globally unique (Asaas and Stripe both issue `sub_…`/`evt_…` ids). Asaas rows keep their historical unqualified ids.
3. **Routing.** `application::payments::BillingGateways` maps `card` to Asaas or Stripe (`BILLING_CARD_PROVIDER` is required when both are configured) and `pix_boleto` to Inter. All configured providers must share one environment so the Entitlement Engine keeps a single `environment`.
4. **Authentication per provider.** Asaas: dedicated header token. Stripe: `Stripe-Signature` HMAC-SHA256 over the raw body, 300 s tolerance, endpoint pinned to API version `2026-11-18.endive`. Inter: callbacks carry no signature, so the callback path contains a 32+ character secret and every announced charge is **re-fetched over mTLS** before it changes coverage; Inter's client certificate can additionally be verified at the TLS edge.
5. **Inter recurrence.** `billing_inter_charges` reserves one row per cycle (deterministic 15-character `seuNumero`) before calling Inter; failures are recovered by looking the charge up by `seuNumero` (Inter refuses identical requests for 30 minutes). An hourly pass issues charges 10 days before each due date and reconciles open charges. An unpaid charge that expires or is cancelled ends the subscription.
6. **Out of scope.** Pix Automático (Inter, Stripe invite-only), Stripe Boleto/Pix, refunds initiated by us, and Commerce Payments (consumer → establishment) remain planned.

## Consequences

- Entitlements, affiliates and contracts did not change: providers only produce payments.
- Inter adds personal data (`billing_payer_profiles`) and an operational scheduler; refunds of Inter charges are manual (boleto has no refund API; Pix refunds need the end-to-end id the callback does not carry).
- Provider homologation is per provider; none of the three is declared homologated until its sandbox checklist in the runbook passes.
