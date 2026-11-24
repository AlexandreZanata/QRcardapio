# Banco Inter (Pix/boleto per cycle)

Status: implemented; unit tests for payload and callback parsing; integration tests with a loopback mock of the Inter API (`tests/payment_providers.rs`). **Not homologated** (needs an Inter sandbox integration).

Code: `src/infrastructure/inter.rs` (+ `inter/parse.rs`), `src/application/billing_inter.rs`, `src/domain/{billing_cycle,payer}.rs`, `src/http/handlers/{billing_webhooks,billing_payer}.rs`.

## Product used

**API Cobrança v3 ("boleto com Pix")**: each charge carries a boleto and a Pix QR. Inter has no card, hosted checkout or subscription object, so the platform issues one charge per cycle. Pix Automático (BCB recurring Pix) exists in Inter's API but is restricted to CNPJs with 6+ months of activity and is not used yet.

| | Production | Sandbox |
|---|---|---|
| Base | `https://cdpj.partners.bancointer.com.br` | `https://cdpj-sandbox.partners.uatinter.co` |
| Token | `POST /oauth/v2/token` (form: `client_id`, `client_secret`, `grant_type=client_credentials`, `scope=boleto-cobranca.read boleto-cobranca.write`) | same |
| Calls | `/cobranca/v3/cobrancas…` with `Authorization: Bearer`, optional `x-conta-corrente` | same; Mon–Fri 08:00–20:00 |

Every call uses **mTLS** with the integration certificate (`.crt` + `.key`, loaded once from `INTER_CERT_PATH`/`INTER_KEY_PATH`). The API hosts negotiate TLS 1.2. Tokens last 60 minutes and the token endpoint allows 5 calls per minute, so the client caches the token and refreshes it 5 minutes early.

## Flow

1. The owner saves payer data (`/dashboard/billing/payer`): CPF or CNPJ (check digits validated), name, e-mail, CEP, street, number, complement, district, city, UF. Owner-only.
2. Subscribing with `method=pix_boleto` reserves the checkout (`provider=inter`, `inter:<reference>` as both checkout and subscription id) and cycle 0 in `billing_inter_charges` (`seuNumero` = deterministic 15 characters), then calls `POST /cobranca/v3/cobrancas` (`valorNominal` from integer centavos, `dataVencimento` today — tomorrow after 19:00 —, `numDiasAgenda=5`, `formasRecebimento=["BOLETO","PIX"]`). Emission is asynchronous; only `codigoSolicitacao` is returned.
3. If the call fails or times out, the charge is looked up by `seuNumero` (Inter refuses identical requests for 30 minutes); otherwise the hourly pass retries.
4. Callbacks and the hourly pass fetch `GET /cobranca/v3/cobrancas/{codigo}`: status, Pix copy-and-paste, digitable line. The billing page shows them, renders the Pix QR locally (`/dashboard/billing/pix.svg`) and proxies the PDF (`/dashboard/billing/boleto.pdf`, fetched on demand, never stored).
5. `RECEBIDO`/`MARCADO_RECEBIDO` with `valorTotalRecebido` equal to the contract price → payment `inter:<codigo>` covering `[due date, due date + months)`. A different amount grants nothing.
6. Renewal: 10 days before the next due date (always computed from the first due date) the hourly pass issues the next cycle once.
7. `EXPIRADO`/`CANCELADO`/`FALHA_EMISSAO` without payment → subscription ends (and the checkout closes on cycle 0) so the owner can subscribe again.
8. Cancel renewal: pending charges are cancelled at Inter (`POST …/cancelar`, asynchronous); paid periods stay covered.

## Webhook

Register with `qrcardapio billing register-inter-webhook` → `PUT /cobranca/v3/cobrancas/webhook` with `<PUBLIC_BASE_URL>/webhooks/inter/<INTER_WEBHOOK_TOKEN>`. Inter sends a JSON array of charges, retries up to 4 times (20/30/60/120 minutes) and does not notify `ATRASADO`. Callbacks have no signature: the path token must match (constant time; otherwise 404) and each `codigoSolicitacao` is re-fetched over mTLS before anything changes; unknown codes are ignored. Inter also presents a client certificate issued by its CA (`ca.crt`, "Certificado Webhook" in the Internet Banking) that Caddy can verify on a dedicated host (runbook).

## Refunds and disputes

Boleto has no refund API or chargeback. Pix refunds need the end-to-end id, which the Cobrança callback does not carry. Refunds of Inter charges are therefore manual (Internet Banking) and are **not** reflected automatically; if coverage must be revoked, the operator records it (planned operator command). Pix MED returns are not distinguishable through Inter's API.

## Open items (not verified in Inter's documentation)

Error returned by the 30-minute duplicate guard; allowed characters of `seuNumero` (we use `[0-9A-Z]`); exact meaning of `MARCADO_RECEBIDO` and `EXPIRADO`; boleto settlement timing for payments at other banks; whether Inter validates our server certificate against public CAs; webhook timeout and batching.

References: [Cobrança v3](https://developers.inter.co/references/cobranca-bolepix) · [Token](https://developers.inter.co/references/token) · [mTLS](https://developers.inter.co/docs/introducao/autenticacao-mtls) · [Webhooks](https://developers.inter.co/docs/webhooks/como-config-webhooks) · [Sandbox](https://developers.inter.co/docs/sandbox) · [Pix Automático](https://developers.inter.co/references/pix-automatico)
