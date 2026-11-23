# HTTP Routes & Contracts

All routes use Axum 0.8 `{param}` syntax and emit strict security headers via `security_headers_middleware`.

Browser errors without an HTML body use a localized illustrated response with the original status. Health and static asset responses retain their machine-readable behavior. Caddy serves existing public snapshots directly and forwards missing public paths for the localized response. Its own upstream failures use a static illustrated fallback.

## Public Routes

| Method | Path | Auth | Notes |
|---|---|---|---|
| `GET` | `/` | Optional | Commercial landing page (`landing.page_title`) with SSR + zero-reload i18n. |
| `GET` | `/c/{slug}`, `/c/{slug}/` | None | Static published menu served from disk (**0 SQLite queries**); `ETag` + `304` + branded `404`. |
| `GET` | `/c/{slug}/{*file}` | None | Static published CSS & media (**0 SQLite queries**); `immutable` cache headers. |
| `GET` | `/static/i18n/{file}` | None | Serves embedded `pt-BR.json`, `en.json`, `es.json` with `ETag`, `304`, and `immutable` caching. |
| `GET` | `/static/*` | None | Serves CSS, JS, and favicon assets. |
| `GET` | `/healthz` | None | Liveness probe (`{"status":"ok"}`). |
| `GET` | `/readyz` | None | Readiness probe executing `SELECT sqlite_version()`. |

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

## Assinatura Asaas

- Com o Asaas configurado, o modal do plano encaminha pelo cadastro, login ou configuração do estabelecimento. `GET` e `POST` de `/register`, `/login` e `/onboarding` preservam `?plan=1` nos formulários e encaminham a `/dashboard/billing` após a configuração. Os destinos são rotas fixas do sistema.
- `GET /dashboard/billing` exibe o plano, o estado do checkout ou da assinatura e a cobertura paga. O retorno do navegador para essa rota é somente leitura.
- `POST /dashboard/billing/checkout`, `POST /dashboard/billing/reconcile` e `POST /dashboard/billing/cancel` verificam sessão, vínculo de proprietário e `_csrf`. As três ações retornam ao painel. O cancelamento exige confirmação e verifica o ID informado contra a assinatura corrente do estabelecimento.
- `POST /webhooks/asaas` aceita JSON até 64 KiB e autentica `asaas-access-token` antes de interpretar o corpo. Retorna `200` após aplicar o evento de forma idempotente, `401` com token ausente ou inválido, `413` acima do limite e `404` quando a cobrança está desabilitada.
- Com Asaas habilitado, `POST /dashboard/publish` direciona o usuário ao painel de assinatura quando faltar cobertura paga. Nenhuma requisição pública ao cardápio consulta a cobrança.

## MCP e autenticação de aplicativos de IA

- `POST /mcp` transporta JSON-RPC com autenticação bearer, limite de 128 KiB e ferramentas de consulta e edição de rascunho.
- Os documentos OAuth de descoberta estão em `/.well-known/oauth-protected-resource/mcp`, `/.well-known/oauth-protected-resource` e `/.well-known/oauth-authorization-server`.
- `GET /mcp/oauth/authorize` inicia a solicitação. `GET` e `POST /mcp/oauth/consent` apresentam e decidem o consentimento. `POST /mcp/oauth/login` autentica quando necessário. `POST /mcp/oauth/token` troca códigos PKCE ou renova tokens.
- `GET /dashboard/integrations` lista os vínculos do proprietário. `POST /dashboard/integrations/revoke` exige sessão, proprietário e `_csrf` para revogar um vínculo. Veja [Arquitetura MCP](MCP.md).
