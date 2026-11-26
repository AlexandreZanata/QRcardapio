# HTTP Routes & Contracts

All routes use Axum 0.8 `{param}` syntax and emit strict security headers via `security_headers_middleware`.

Browser errors without an HTML body use a localized illustrated response with the original status. Health and static asset responses retain their machine-readable behavior. Caddy serves existing public snapshots directly and forwards missing public paths for the localized response. Its own upstream failures use a static illustrated fallback.

## Public Routes

| Method | Path | Auth | Notes |
|---|---|---|---|
| `GET` | `/`, `/en`, `/es` | Optional | Commercial landing pages (`pt-BR`, `en`, `es`) with SSR, canonical/hreflang clusters, JSON-LD `@graph`, FAQ, and zero-reload i18n (`/en/` and `/es/` return `301`). |
| `GET` | `/robots.txt`, `/sitemap.xml`, `/llms.txt`, `/manifest.webmanifest` | None | Technical SEO, AI crawler guidance, and dynamic sitemap (`ETag` + `304`) including live `/c/{slug}` menus. |
| `GET` | `/favicon.ico`, `/apple-touch-icon.png` | None | Root icon assets served from `static/icons/`. |
| `GET` | `/solucoes`, `/en/solutions`, `/es/soluciones` | Optional | Business vertical solutions hub. |
| `GET` | `/cardapio-digital/{slug}`, `/en/digital-menu/{slug}`, `/es/menu-digital/{slug}` | Optional | Eight localized business vertical pages (restaurants, burger shops, cafés, snack bars, pizzerias, açaí shops, bakeries, bars). |
| `GET` | `/ferramentas`, `/en/tools`, `/es/herramientas` | Optional | Free hospitality tools hub. |
| `GET` | `/ferramentas/{slug}`, `/en/tools/{slug}`, `/es/herramientas/{slug}` | Optional | Menu QR generator, CMV food-cost calculator, and printable tabletop QR placard creator. |
| `GET` | `/api/tools/qr.svg`, `/api/tools/qr.png` | None | Rate-limited (60/min/IP) HTTP/HTTPS-validated SVG and 1024px PNG QR code endpoints. |
| `GET` | `/guias`, `/en/guides`, `/es/guias` | Optional | Practical hospitality guides hub. |
| `GET` | `/guias/{slug}`, `/en/guides/{slug}`, `/es/guias/{slug}` | Optional | Four localized deep-dive guides with Answer-First GEO summaries and `Article` JSON-LD. |
| `GET` | `/c/{slug}`, `/c/{slug}/` | None | Static published menu served from disk (**0 SQLite queries**); `ETag` + `304` + branded `404`. |
| `GET` | `/c/{slug}/{*file}` | None | Static published CSS & media (**0 SQLite queries**); `immutable` cache headers. |
| `GET` | `/static/i18n/{file}` | None | Serves embedded `pt-BR.json`, `en.json`, `es.json` with `ETag`, `304`, and `immutable` caching. |
| `GET` | `/static/*` | None | Serves CSS, JS, OG images, and icon assets. |
| `GET` | `/healthz` | None | Liveness probe (`{"status":"ok"}`). |
| `GET` | `/readyz` | None | Readiness probe executing `SELECT sqlite_version()`. Optional `warnings` report delayed/dead-letter outbox events or unavailable diagnostics. |

A visita à home com o fragmento `#demonstracao` é encaminhada pelo navegador para o exemplo fixo `/c/bistro-da-praca`. A placa do QR também é um link direto. O destino usa a entrega estática de `/c/{slug}`.

## Authentication Routes

| Method | Path | Protection | Notes |
|---|---|---|---|
| `GET`/`POST` | `/login` | IP rate limit (10/min) | Rotates session cookie (`qr_session`) on success. |
| `GET`/`POST` | `/register` | IP rate limit (5/min) + honeypot | Creates owner account, issues session cookie, redirects to `/onboarding`. |
| `POST` | `/logout` | `AuthContext` + CSRF | Deletes session hash from SQLite and clears `qr_session`. |
| `GET`/`POST` | `/reset-password` | IP rate limit (5/min) | Redeems single-use CLI token, rotates hash, revokes all active sessions. |

## Owner Onboarding & Dashboard Routes (`AuthContext` / `TenantContext`)

All `POST` routes verify `_csrf` against the server session's `csrf_token` in constant time. Dynamic pages set `Cache-Control: no-store`.

| Method | Path | Purpose |
|---|---|---|
| `GET`/`POST` | `/onboarding` | Initial establishment setup. An omitted or blank slug assigns a compact address from the name with a short code on collision. Custom addresses are validated and return `409` on a collision. |
| `GET` | `/dashboard` | Overview: draft/published badge, counts, checklist, publish & unpublish triggers. |
| `GET` | `/dashboard/theme.css` | Tenant-specific brand CSS variables (`no-store`). |
| `GET` | `/dashboard/preview` | Live draft preview (includes unpublished changes and draft preview banner). |
| `GET` | `/dashboard/media/{filename}` | Tenant-isolated private media preview (scoped to `establishment_id`). |
| `POST` | `/dashboard/publish` | Triggers atomic static snapshot build. |
| `POST` | `/dashboard/unpublish` | Removes published static files and reverts to draft. |
| `GET` | `/dashboard/menu?page=N` | Category and product management. Defaults to page 1 and displays up to 12 products or empty categories in menu order. |
| `GET`/`POST` | `/dashboard/categories/new`, `/dashboard/categories` | Create category with optional EN/ES translations. The editor dialog posts the same form with `Accept: application/json` and receives `204` on success or a localized JSON validation error. |
| `GET`/`POST` | `/dashboard/categories/{id}/edit`, `/dashboard/categories/{id}` | Edit category. |
| `POST` | `/dashboard/categories/{id}/delete`, `/move` | Delete or reorder category. |
| `GET` | `/dashboard/products/new` | Opens the product dialog in the editor. Accepts `category_id` and `page`. `standalone=true` renders the same fields as a page for browsers without dialog support. |
| `POST` | `/dashboard/products` | Creates a product (multipart max 6 MiB, upload rate limit 20/min). Dialog requests with `Accept: application/json` receive `204` on success or a localized JSON error. Standalone submissions redirect to the editor on success. |
| `GET`/`POST` | `/dashboard/products/{id}/edit`, `/dashboard/products/{id}` | Edit product, price, availability, photo, translations. |
| `POST` | `/dashboard/products/{id}/toggle`, `/delete`, `/move` | Toggle sold-out status, delete, or reorder within category. |
| `GET` | `/dashboard/ocr` | Menu OCR & PDF upload dropzone, manual text paste fallback, and recent import history. |
| `POST` | `/dashboard/ocr/extract`, `/dashboard/ocr/text` | Extract menu draft from JPG/PNG/WebP/PDF (multipart max 16 MiB, rate limit 10/min) or pasted text. |
| `GET`/`POST` | `/dashboard/ocr/{id}`, `/apply`, `/discard` | Staging review table, atomic bulk pre-registration into draft menu, or discard staging draft. |
| `GET`/`POST` | `/dashboard/settings` | Update profile, brand color, EN/ES description, and optional Google Maps and review links. The slug locks after publication. |
| `POST` | `/dashboard/settings/logo`, `/logo/remove` | Upload (max 2 MiB) or remove establishment logo. |
| `GET` | `/dashboard/qr`, `/qr/print`, `/qr.svg`, `/qr.png` | Preview, printable card sheet, SVG download, 1024px PNG download. |

## Operational Administration

| Method | Path | Protection | Purpose |
|---|---|---|---|
| `GET` | `/admin` | `AdminContext` (`role = 'admin'`) | Operational counts, SQLite version, recent establishments, recent publication metrics. |
| `GET` | `/admin/outbox` | `AdminContext` (`role = 'admin'`) | `no-store` JSON with pending age, per-consumer lag and dead letters, including `replay_generation`. Event payloads are omitted. |
| `GET` | `/admin/outbox/recovery` | `AdminContext` | `no-store` dead-letter recovery page with event and consumer identifiers. |
| `POST` | `/admin/outbox/replay` | `AdminContext` and CSRF | Form fields `consumer`, `event_id`, `generation` and `_csrf`. `303` after scheduling, `409` if the observed dead letter can no longer be scheduled. |

The [Commerce Core OpenAPI contract v1](../../contracts/openapi/commerce.v1.json) specifies the session cookie, replay form, diagnostics fields and response headers for these three outbox routes.

## Assinatura Asaas

- Com o Asaas configurado, o modal do plano encaminha pelo cadastro, login ou configuração do estabelecimento. `GET` e `POST` de `/register`, `/login` e `/onboarding` preservam `?plan=monthly`, `?plan=semiannual` ou `?plan=annual` nos formulários e encaminham a `/dashboard/billing` com o período escolhido após a configuração. Links antigos com `?plan=1` seguem para o mensal. Os destinos são rotas fixas do sistema.
- `GET /dashboard/billing` exibe o plano, o estado do checkout ou da assinatura, a cobertura paga e o catálogo de módulos (contratado, disponível, expirado, suspenso, em desenvolvimento). Módulos planejados aparecem sem preço e sem botão. O retorno do navegador para essa rota é somente leitura.
- `POST /dashboard/billing/checkout`, `POST /dashboard/billing/reconcile` e `POST /dashboard/billing/cancel` verificam sessão, vínculo de proprietário e `_csrf`. A criação do checkout recebe `plan` (ciclo) e `offer` (código do catálogo, padrão `menu`; `bundle:<code>` e `addon:<code>` são aceitos sintaticamente) e calcula o valor no servidor pelo Commercial Engine. Ofertas não comercializáveis retornam `422` antes de qualquer chamada ao Asaas; valores enviados pelo navegador são ignorados. O gerenciamento exige a permissão `billing.manage` (proprietário). As três ações retornam ao painel. O cancelamento exige confirmação e verifica o ID informado contra a assinatura corrente do estabelecimento.
- `POST /webhooks/asaas` aceita JSON até 64 KiB e autentica `asaas-access-token` antes de interpretar o corpo. Retorna `200` após aplicar o evento de forma idempotente, `401` com token ausente ou inválido, `413` acima do limite e `404` quando a cobrança está desabilitada.
- Com Asaas habilitado, `POST /dashboard/publish` direciona o usuário ao painel de assinatura quando faltar cobertura paga. Nenhuma requisição pública ao cardápio consulta a cobrança.

## MCP e autenticação de aplicativos de IA

- `POST /mcp` transporta JSON-RPC com autenticação bearer, limite de 128 KiB e seis ferramentas: consulta/edição de conta e estabelecimento (`get_account_settings`, `update_account_settings`), consulta/edição/reordenação/exclusão no rascunho (`get_menu_draft`, `apply_menu_changes`) e publicação/despublicação (`publish_menu`, `unpublish_menu`, respeitando assinatura). `GET` e `DELETE /mcp` autenticados retornam `405 Allow: POST`; `OPTIONS` e CORS são restritos às origens oficiais dos conectores.
- Os documentos OAuth de descoberta estão em `/.well-known/oauth-protected-resource/mcp`, `/.well-known/oauth-protected-resource` e `/.well-known/oauth-authorization-server`.
- `GET /mcp/oauth/authorize` inicia a solicitação. `GET` e `POST /mcp/oauth/consent` apresentam e decidem o consentimento. `POST /mcp/oauth/login` autentica quando necessário. `POST /mcp/oauth/token` troca códigos PKCE ou renova tokens.
- `GET /dashboard/integrations` lista os vínculos do proprietário. `POST /dashboard/integrations/revoke` exige sessão, proprietário e `_csrf` para revogar um vínculo. Veja [Arquitetura MCP](MCP.md).

## Cadastro e login com o Google

- `GET /auth/google/start` inicia autorização Google. O parâmetro `plan` preserva a seleção de assinatura quando disponível.
- `GET /auth/google/callback` valida e consome a solicitação do navegador e redireciona com uma sessão local ou mensagem traduzida.
- `GET /account/security` exibe o vínculo da conta autenticada. `POST /auth/google/link` e `POST /account/google/unlink` exigem CSRF e sessão local.
- `GET /mcp/oauth/google/start?flow=…` exige a solicitação MCP válida do mesmo navegador. A configuração inicial pode continuar por `/onboarding?mcp=…`, com nova validação da solicitação no consentimento.
- As rotas de identidade usam `Cache-Control: no-store` e `Referrer-Policy: no-referrer`.

[Fluxo e contratos de login](GOOGLE_AUTH.md)

## Programa de Afiliados

Todas as rotas abaixo aplicam `Cache-Control: no-store` e limite de corpo de 16 KiB; todas as mutações `POST` validam `_csrf`.

| Method | Path | Protection | Purpose |
|---|---|---|---|
| `GET` | `/r/{code}` | IP rate limit (60/min) | Valida afiliado ativo, incrementa cliques (sem inflar com cookie repetido), grava cookie `qr_ref` (`HttpOnly`, `SameSite=Lax`) e redireciona (`302`) para rota interna limpa (`Disallow: /r/` em `robots.txt`). |
| `GET` | `/`, `/en`, `/es`, `/register` (com `?ref=`) | IP rate limit | Captura o código de indicação e redireciona (`302 no-store`) para a URL canônica limpa sem `?ref=`. |
| `GET` | `/dashboard/affiliate` | `AuthContext` | Portal do afiliado (funciona com ou sem estabelecimento): autoativação, link de indicação, cota mensal, chave PIX, indicados (e-mail mascarado LGPD), extrato de comissões e histórico de repasses. |
| `POST` | `/dashboard/affiliate/activate`, `/pix` | `AuthContext` + CSRF | Autoativação com cota padrão global e atualização da chave PIX. |
| `GET` | `/admin/affiliates`, `/admin/affiliates/{id}` | `AdminContext` | Visão geral por ambiente (`production`/`sandbox`), regras padrão, cadastro manual, edição de cota mensal/status/carência, baixa manual PIX e auditoria de indicados/extrato. |
| `POST` | `/admin/affiliates/settings`, `/enroll`, `/{id}/update`, `/{id}/payout`, `/{id}/referrals/{referral_id}/disqualify` | `AdminContext` + CSRF | Mutação das regras padrão, cadastro/edição de afiliado, liquidação atômica de repasse PIX (exige `confirm=yes`) e desqualificação de indicação com estorno das parcelas em aberto. |

[Arquitetura do Programa de Afiliados](AFFILIATES.md)

## Commerce Core

The catalog publication, order transitions, checkout, offline payment ledger and Web Push subscription management have internal Rust interfaces. Dispatcher diagnostics and replay scheduling are exposed only on the platform admin routes. A future authorized channel adapter can submit the full quoted breakdown for comparison. It will map `CheckoutOutcome::Repriced` to `409 commerce.checkout_repriced` with the current quote and `commerce.idempotency_mismatch` to `422`. See the [QR Delivery API specification](../modules/DELIVERY.md#16-http-contracts).

### Commerce Core dashboard (Increment 2)

`GET /dashboard/products/{id}/options` and `GET /dashboard/menu/addons` let owners and managers manage variations and reusable add-on groups. Tenant-scoped forms save, remove and link entities through POST actions. `GET /dashboard/hours` configures modalities, minimum, acceptance, pauses, weekly windows and date exceptions. `GET /dashboard/customers` and customer detail pages support customer and address management. Owner-only POST actions at `/dashboard/customers/{id}/export` and `/dashboard/customers/{id}/erase` require CSRF. The export is a JSON attachment with `Cache-Control: no-store`. Erasure requires explicit form confirmation.
