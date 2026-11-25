# Security Architecture

## Threat Model & Core Mitigations

| Threat | Mitigation | Implementation |
|---|---|---|
| Weak or leaked passwords | Argon2id (`m=19456, t=2, p=1`) + constant timing on unknown email via dummy verify; bounded `Semaphore(2)` + `spawn_blocking`. | `src/infrastructure/security/mod.rs` |
| Session hijacking / fixation | 32-byte CSPRNG tokens (`getrandom`), SHA-256 hashed in SQLite; rotated on every login/register; `HttpOnly`, `SameSite=Lax`, `Secure` (prod); 7-day expiry + hourly prune. | `src/application/auth.rs`, `src/http/middleware/auth.rs` |
| CSRF on state-changing requests | Per-session random `_csrf` token verified in constant time (`subtle::ConstantTimeEq`) on every authenticated `POST`. | `verify_csrf` in `src/http/middleware/auth.rs` |
| Cross-tenant IDOR | `TenantContext` resolves `establishment_id` strictly from `establishment_members`; all SQL queries filter `AND establishment_id = ?`; `insert/update_product` verifies `category_id` belongs to the same tenant. | `src/infrastructure/database/queries/*.rs` |
| Brute-force & registration spam | IP-keyed `governor` GCRA limiters (`/login` 10/min, `/register` 5/min + hidden honeypot, `/reset-password` 5/min), per-account upload limit (20/min), and per-account OCR limit (10/min). | `src/infrastructure/security/rate_limit.rs` |
| XSS & script injection | Askama auto-escaping (`&#60;`/`&#62;`/`&#34;`/`&#39;`), JSON `</` escaping, strict CSP (`script-src 'self'; style-src 'self'`). | `src/http/middleware/security_headers.rs` |
| Malicious image/PDF uploads / SVG XSS / EXIF leaks | 2 MiB logo / 5 MiB product / 10 MiB OCR photo / 15 MiB OCR PDF caps; magic-byte checks (JPEG/PNG/WebP/%PDF-); dimension bounds; EXIF-stripping decode+re-encode; isolated `TempDir` + `Semaphore(1)` + timeouts + no-shell subprocess invocation for `tesseract`/`pdftotext`/`pdftoppm`. | `src/infrastructure/assets/mod.rs`, `src/infrastructure/ocr/` |
| Path traversal | `resolve_safe_subpath` rejects `..`, `/`, `\`, leading `.`, null bytes; `Slug::validate_existing` restricts slugs to `[a-z0-9-]`. | `src/infrastructure/storage/mod.rs` |

## Account Recovery (Pilot Phase)

No external SMTP dependency is required for the MVP. An operator with server CLI access issues a single-use 1-hour recovery token:
```bash
qrcardapio admin reset-token --email owner@example.com.br
```
Redeeming the token at `/reset-password` updates the password hash and revokes all active sessions atomically.

## LGPD Data Minimization

Only the owner's email, hashed password, preferred locale, and public business menu data are collected. Customers scanning public QR Codes never create accounts, submit personal data, or trigger tracking cookies or database writes.

## Pagamentos e publicação estática

O checkout do plano mensal fica no domínio Asaas. As ações do painel exigem `TenantContext`, papel de proprietário, `_csrf` e limite de seis ações por minuto por estabelecimento. Apenas o webhook autenticado ou uma conciliação feita no servidor associa a assinatura ao checkout. O webhook verifica um segredo separado em tempo constante, limita o corpo a 64 KiB e deduplica por identificador de evento e ambiente. Os dados de cartão e de cadastro do pagador ficam no Asaas. O Caddy bloqueia `/c/.tmp` e seus descendentes para preservar a visibilidade atômica dos arquivos de publicação.

A verificação de 23/11/2026 incluiu o RustSec atualizado, o Gitleaks com saída redigida, a configuração do proxy e 32 testes automatizados. O RustSec não reportou vulnerabilidades nas dependências auditadas e manteve avisos de falta de manutenção em `basic-toml 0.1.10` e `chrono 0.4.45`. Os dois alertas do histórico Git correspondem a senhas fictícias nos testes de cadastro. A suíte cobre autenticação, autorização, CSRF, isolamento multi-tenant, verificação de uploads e publicação, rejeição de webhook sem token e reentrega de eventos financeiros.

## MCP para aplicativos de IA

A autorização delega acesso de proprietário a um estabelecimento por vez. Identidades e callbacks oficiais são pré-cadastrados. O fluxo exige PKCE S256, código de uso único, consentimento com CSRF e tokens vinculados ao recurso. O endpoint aceita bearer no cabeçalho e limita corpo, taxa por origem de autenticação e taxa de ferramentas por estabelecimento. Os dados e as credenciais de cobrança ficam fora das ferramentas. As escritas MCP são limitadas ao rascunho e verificam a versão de cada item. Logs HTTP registram o caminho sem os parâmetros de consulta do OAuth.

## Identidade Google

O fluxo usa PKCE S256, estado de uso único vinculado ao cookie e nonce assinado. O servidor exige assinatura do Google e valida emissor, destinatário e validade antes de abrir uma sessão. Contas existentes só recebem um vínculo depois de autenticação local, verificação de CSRF e conferência do mesmo e-mail na sessão que iniciou o vínculo. A identidade persistida usa o `sub` estável. A recuperação assistida continua exigindo que o suporte verifique o titular.

[Controles do fluxo Google](../architecture/GOOGLE_AUTH.md)

## Provedores de cobrança (Asaas, Stripe, Inter)

- Webhooks: Asaas por token dedicado; Stripe por assinatura HMAC-SHA256 do corpo bruto com tolerância de 300 s e verificação de `livemode`; Inter por segredo no caminho e nova consulta de cada cobrança via mTLS antes de alterar a cobertura (o Inter não assina callbacks). O certificado de cliente do Inter pode ser exigido no Caddy ([runbook](../operations/RUNBOOK.md#banco-inter)).
- Credenciais: chaves e segredos apenas em variáveis de ambiente; certificado e chave do Inter em arquivos `0400` do usuário do serviço, validados na inicialização. Nada disso é registrado em log.
- Traces HTTP registram o padrão de rota do Axum e usam `<unmatched>` para rotas desconhecidas. O caminho e a consulta da requisição ficam fora do trace, preservando o segredo do callback do Inter.
- Valores: o navegador envia apenas oferta, ciclo e forma de pagamento; o total vem do contrato.
- LGPD: CPF/CNPJ e endereço do pagador (Pix/boleto) ficam em `billing_payer_profiles`, somente o proprietário altera, a tela mostra o documento mascarado, nunca entram em eventos ou logs, e o boleto em PDF é buscado sob demanda sem armazenamento.

## Commerce Core

Catalog options, published snapshots and order records are scoped by establishment. Composite foreign keys prevent cross-tenant links. The order engine uses IDs and quantities to resolve product names and prices from the current published snapshot. Order outbox payloads use strict schemas with enumerated reason codes and exclude personal data. Future channel entry points must resolve the tenant server-side and enforce the relevant commercial operation before using these internal services.

Increment 2 dashboard writes verify CSRF and apply the existing entitlement engine with tenant context. Customer and address data are scoped by composite keys and private dashboard paths. Customer export and erasure are owner-only operations, with responses served using `no-store`. Erasure removes address and personal delivery data in one write transaction while preserving amounts, items, states and the retained neighborhood label.

Checkout expects a UUID v4 idempotency key. The attempt table stores its SHA-256 and a keyed digest of the request instead of a second copy of customer input. Offline payment recording checks tenant membership and the payment machine, with immutable audit history and no payment provider credentials or online payment methods.

Web Push endpoints and key material are scoped to the opted-in staff member and excluded from event payloads and diagnostics. The sender restricts destinations to supported HTTPS push service domains, disables redirects and proxies, encrypts payloads, and signs VAPID requests with a separately configured private key file. Removing the membership also removes its subscriptions and pending deliveries. The platform outbox diagnostic route uses `AdminContext` and `Cache-Control: no-store`.
