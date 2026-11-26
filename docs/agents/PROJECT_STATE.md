# Project State

Last updated: 2026-11-25

## Current Status — MVP Complete

All 5 implementation phases of the **QR Cardápio MVP** (including full native zero-reload `pt-BR` / `en` / `es` i18n across both owner panels and static published menus) are implemented, tested, benchmarked, and documented:

- **Phase 1 — Technical Foundation**: Complete & verified (`Cargo.toml`, `rust-toolchain.toml` pinned to `1.100.0`, Edition 2024, bundled SQLite `3.54.1` via `sqlx 0.9.1`, WAL + `synchronous=FULL`, migrations, structured logging, health/readiness endpoints).
- **Phase 2 — Domain, Security & Multi-Tenancy**: Complete & verified (Argon2id with bounded semaphore, CSPRNG SHA-256 hashed sessions with rotation, constant-time CSRF, IP & account rate limiting, `TenantContext` isolation, `BEGIN IMMEDIATE` write transactions, standardized per-entity submodules under `src/infrastructure/database/queries/`, EXIF-stripping image resizing).
- **Phase 3 — Premium Interface & Zero-Reload i18n**: Complete & verified (design tokens, responsive Askama templates, strict CSP with zero inline scripts/styles, `/static/i18n/{locale}.json` immutable endpoints, `static/js/i18n.js` + `static/js/menu.js` zero-reload switching).
- **Phase 4 — Static Publishing & QR Codes**: Complete & verified (atomic temp+fsync+rename publishing to `published/{slug}/`, content-addressed assets, failure rollback preserving previous live menu, unpublishing, zero-DB disk delivery, high-contrast SVG & >=1024px PNG QR generation, printable sheet).
- **Phase 5 — Operations, Benchmarks & AI-First Docs**: Complete & verified (`Dockerfile`, `compose.yaml`, `Caddyfile`, `scripts/{dev,backup,restore,bench_run}.sh`, full `AGENTS.md` + `docs/` + 6 ADRs, real benchmarks at 100..100k establishments).
- **Phase 6 — Local Hybrid OCR & PDF Menu Pre-Registration**: Complete & verified (`migrations/20261123000002_ocr_imports.sql`, pure-Rust menu parser under `src/domain/ocr_parser/`, hybrid `pdftotext` -> `pdftoppm` + pure-Rust image preprocessing + `tesseract` engine under `src/infrastructure/ocr/` with `Semaphore(1)` and timeouts, `ocr_imports` staging persistence, `/dashboard/ocr` upload/paste/review/bulk-apply/discard flow with full `pt-BR`/`en`/`es` i18n and strict multi-tenant + CSRF + rate-limit protection, ADR-006).

## Modular Platform Foundation — Phases 0, 1 and technical part of 2 (2026-11-24)

QR Cardápio is now a free **QR Core** plus independently sold modules. Only **QR Menu** is implemented and sellable; Delivery, Tables, Kitchen, Fleet, Loyalty, Analytics, POS and Connect are `planned` with full contracts and specifications. Delivery is the next module (after Commerce Core).

| Area | Status | Where |
|---|---|---|
| Versioned commercial catalog (9 modules, 3 bundles, 5 add-ons, 3 cycles) compiled from JSON | Implemented and tested | `contracts/commercial/`, `src/domain/commercial/`, [MODULE_CATALOG](../business/MODULE_CATALOG.md) |
| Catalog validation + start-up registry (sold versions immutable, lifecycle transitions guarded) | Implemented and tested | `validation.rs`, `queries/commercial_catalog.rs`, ADR-007 |
| Backend-only quotes, integer round-half-up per line, immutable contract snapshots | Implemented and tested | `quote.rs`, `cycle.rs`, `commercial_contracts`, [COMMERCIAL_ENGINE](../architecture/COMMERCIAL_ENGINE.md) |
| Entitlement Engine: capabilities from paid contracts and audited grants; five-dimension `authorize` | Implemented and tested | `src/domain/entitlements/`, `src/application/entitlements.rs`, ADR-008 |
| QR Menu integrated (dashboard + MCP publish through `authorize`; billing page module catalog) | Implemented and tested | `handlers/{dashboard,billing}.rs`, `application/mcp/account.rs` |
| Legacy "Plano Essencial" → QR Menu v1 contracts (no new charges, same prices/periods/ids) | Implemented and tested | migration `20261124000009`, `tests/commercial_migration.rs` |
| Asaas payload from the contract quote; webhook activation event | Implemented, tested with simulated webhooks; **sandbox homologation pending** (no credentials) | `infrastructure/asaas.rs`, `application/billing.rs` |
| Affiliate commissions on eligible SaaS revenue | Implemented and tested | `affiliate_ledger.rs`, [AFFILIATES](../architecture/AFFILIATES.md) |
| Transactional outbox (commercial, publication, orders, offline payments) | Append, dispatcher, receipts, retries, per-aggregate parking and platform-admin diagnostics | `domain_events`, ADR-009, [EVENT_CONTRACTS](../architecture/EVENT_CONTRACTS.md) |
| Operator CLI (grants, suspension) | Implemented and tested | `qrcardapio operator …`, [RUNBOOK](../operations/RUNBOOK.md#commercial-operations) |
| Rollback-tolerant migrator, idempotent expand migration | Implemented and tested | ADR-010, [RUNBOOK](../operations/RUNBOOK.md#rollback-of-the-commercial-foundation-release) |
| `republish-all` no longer publishes pending drafts after deploys | Fixed | `application/publishing.rs` |
| Subscription changes (add/remove modules, cycle change, proration, credits) | Planned — blocked until a second module is sellable and Asaas updates are homologated | [SUBSCRIPTION_RULES](../business/SUBSCRIPTION_RULES.md) |
| Commerce Core, Delivery and later modules | Commerce Core in progress. Delivery and later modules planned | [Commerce Core](../architecture/COMMERCE_CORE.md), [MASTER_ROADMAP](../roadmap/MASTER_ROADMAP.md) |

Contract drift is enforced by `tests/commercial_contracts.rs` (catalog, capabilities, lifecycle, authorization rules, events and schemas, module contracts, state machines, module docs, catalog doc).

### Verification (2026-11-24)

- `cargo fmt --check`, `cargo clippy --all-targets --all-features -- -D warnings`: passed.
- `cargo test --locked`: passed — 112 tests (49 unit, 63 integration), including the new `commercial_contracts` (10), `commercial_engine` (8), `entitlements` (6) and `commercial_migration` (2).
- `cargo audit`: 0 vulnerabilities (same 2 unmaintained notices); no dependency changes.
- `cargo build --release --locked`: passed. Benchmarks: [PERFORMANCE](../performance/PERFORMANCE.md#commercial-foundation-verification--24112026).
- Not run: Asaas sandbox homologation, live HTTPS smoke test, physical QR scans.

### Known risks and pending items

1. **Asaas sandbox homologation** of the new contract path (checkout payload description now names the offers; values unchanged) — required before declaring the flow homologated.
2. **Rollback of this specific release** to a pre-existing binary requires the manual runbook procedure (older binaries reject the new migration).
3. Menus with pending drafts no longer receive template upgrades from `republish-all`; persisted per-publication snapshots are the follow-up.
4. Retired theme and media files now survive the 360 s HTML cache window. Expired files are pruned by subsequent publications. Explicit unpublish still removes the entire public snapshot immediately.
5. The landing page still names the offer "Plano Essencial" (marketing copy); billing and contracts call it QR Menu.
6. `establishment_members.member_role` and MCP `scope` CHECK constraints need expand migrations before new roles/scopes.

## SaaS Billing Providers: Asaas, Stripe and Banco Inter (2026-11-24)

The three providers can be active at the same time; the owner chooses the payment method and each subscription stays with its provider ([ADR-011](../decisions/ADR-011-multi-provider-saas-billing.md), [providers](../architecture/payments/README.md)).

| Area | Status |
|---|---|
| Provider-neutral `GatewayEvent` pipeline shared by all providers | Implemented and tested |
| Stripe card subscriptions (hosted Checkout, API `2026-11-18.endive`, HMAC webhooks, refunds and disputes via `invoice_payments`, cancel at period end) | Implemented, tested with signed events and an API mock; **not homologated** |
| Banco Inter Pix/boleto (Cobrança v3 per cycle, mTLS + cached OAuth, payer data, secret-path webhook with re-fetch, hourly renewal and reconciliation, cancel) | Implemented, tested with an API mock; **not homologated** |
| Asaas | Unchanged behavior; sandbox homologation still pending |
| Refunds of Inter charges, Pix Automático, Stripe Pix/Boleto | Planned |

Verification on this branch: `cargo fmt --check`, `cargo clippy -D warnings`, `cargo test --locked` (all suites, including `tests/payment_providers.rs`).

## Commerce Core — Phase 3 (2026-11-25)

The product owner authorized Phase 3 production work in writing on 2026-11-25 while Phase 2 Asaas Sandbox homologation remains pending, under the [Master Roadmap decision rule](../roadmap/MASTER_ROADMAP.md#phase-2--qr-menu-consolidation). The authorization preserves the homologation gate for `beta` and `available` for all modules other than QR Menu. QR Delivery remains `planned`.

- Baseline stop-the-line fixes cover Inter callback token redaction, an early Stripe reversal recorded before settlement, atomic Inter settlement and first-charge reservation, concurrent edits during QR Menu publication, cross-process serialization of publish and unpublish, and asset retention for cached HTML.
- Increment 1 delivers all six typed machine definitions, checked integer pricing, catalog option persistence, canonical published channel snapshots, and orders with immutable items, per-day numbering and atomic transitions and events.
- [ADR-012](../decisions/ADR-012-commerce-core-increment-1.md) records the price cap and commercial date, the internal publication boundary, and the interpretation of option quantities.
- Increment 2 adds dashboard editing for variations and add-ons, hours and service settings, and tenant-scoped customers, addresses and privacy operations. The landing highlights the demo in a wide card directly above the three-step section. The public demo displays removable ingredients and optional extras.
- Increment 3 implements idempotent checkout against the published snapshot, exact offline payment and refund records, dispatcher, timeout rejection and compensation, and optional staff Web Push. Public checkout and notification subscription surfaces belong to QR Delivery. See [Commerce Core §15](../architecture/COMMERCE_CORE.md#15-ordered-task-list-to-start-commerce-core) and the [validation evidence](../performance/PERFORMANCE.md#commerce-core-increment-3).
- The functional suite passed 184 tests. Both Increment 2 and Increment 3 release binaries started on the migrated SQLite database. The local durable checkout and full-size outbox benchmarks missed the throughput gates, so Phase 3 remains in progress.

## Landing and Panel Refresh

The homepage includes native 4K browser captures of the owner dashboard and public mobile menu, with responsive WebP variants on the landing. The owner dashboard and operational administration share responsive side navigation. Operational tables include translated statuses and empty states.

The three how-it-works cards use custom storefront, menu book, and QR printer illustrations in the same style as the feature icons.

The public menu uses a branded cover, sticky search and categories, and responsive product cards. The shared CSS updates published snapshots in place. A photo cover is staged during publication when the visible menu has an available product photo.

The landing, dashboard, and public menu share an illustrated disclosure that shows the active language and opens the PT, EN, and ES choices. Previously-published menus gain the picker through their existing menu script.

The dashboard uses original icons and a contextual tutorial on the owner and platform workflows. Empty collections and browser error pages use coordinated illustrations. Owners can save a Google Maps profile or direct review link and publish an optional review invitation in their menu. The landing comparison uses public Maps profile fields and labeled synthetic review data for the fictional Brasa 128 hamburger restaurant. The landing style has a transferable implementation guide under `docs/design/`.

The Google Maps landing section combines the fictional business profile and ratings in one panel, with the mobile review invitation and a food photograph in the overlapping phone. Its example link navigates to the responsive demonstration. The simulated comparison uses light gray for the before card and the landing's cobalt blue for the after card.

The Google Maps phone uses responsive 4K frame artwork with translated HTML content. The complete Portuguese mockup is available as a transparent 2160 × 3840 WebP. The homepage hero displays its responsive device images and their demo caption.

Settings groups business details, Google Maps links, and the current logo. Dashboard forms share translated Askama field components for settings, onboarding, categories, products, and menu import and review. Logo, product photo, and menu imports share an illustrated file picker with image preview, local format and size checks, and a native input fallback.

Menu import separates file upload, pasted text, and illustrated tips. The menu editor and overview share the blue publication action, with the preview and live menu links grouped in the overview.

A visão geral usa selos ilustrados de publicação e cartões em azul, rosa e amarelo para categorias, produtos e versão publicada. Os selos distinguem o cardápio no ar, o estado atualizado, as alterações pendentes e o rascunho. Os cartões se empilham em telas estreitas.

The category creation dialog shares its fields with the standalone category form and shows validation without leaving the editor. The overview uses an illustrated, keyboard-accessible confirmation for taking a menu offline.

The product dialog opens from the editor, a category row, or the product creation URL. It shares the standard fields and photo picker with the standalone form and edit page. Localized validation stays in the dialog, and a successful save returns to the same editor page.

The menu editor uses tenant-scoped pagination with 12 product or empty-category rows per page. The editor retains the active page through inline actions and edit forms. Mobile layouts preserve the landing Maps mockup composition, adapt dashboard tables into labeled rows, and scale the QR print preview to the screen.

O cadastro atribui links compactos a partir do nome do estabelecimento. Um código de quatro caracteres distingue nomes repetidos. Endereços personalizados continuam disponíveis no rascunho e o caminho publicado é protegido contra renomeação concorrente.

## Verification Snapshot

- `cargo fmt --check`: Passed
- `cargo clippy --all-targets --all-features -- -D warnings`: Passed (0 warnings)
- `cargo test --locked`: Passed (20 unit tests + 22 integration tests across 13 integration suites, including `tests/mcp.rs`, `tests/seo.rs`, and `tests/seo_contracts.rs`)
- `cargo audit`: Passed (0 vulnerabilities; 2 unmaintained advisory notices: `basic-toml 0.1.10` transitive, `chrono 0.4.45`)
- `cargo build --release --locked`: Passed
- Live smoke test (`admin create`, `seed`, `backup.sh`, `restore.sh`, HTTP SSR + static `/c/cafe-da-praca`, `/dashboard/ocr`): Passed

## Arquitetura de SEO e Crescimento Orgânico (GEO)

Implementada em `src/seo/`, `src/http/handlers/seo/` e detalhada em [SEO Architecture](../architecture/SEO.md):
- `robots.txt` (incluindo crawlers de busca por IA), `sitemap.xml` dinâmico com `hreflang` bidirecional (`pt-BR`, `en`, `es`, `x-default`) e cardápios publicados (`/c/{slug}`), `llms.txt`, `manifest.webmanifest`, `favicon.ico`, `apple-touch-icon.png` e cards sociais `og-{pt-BR,en,es}.png` 1200×630.
- Rotas multilíngues `/`, `/en`, `/es` (com `301` de `/en/` e `/es/`), `JSON-LD @graph` (`Organization`, `WebSite`, `SoftwareApplication`, `HowTo`, `FAQPage`, `WebApplication`, `Article`, `BreadcrumbList`, além de `FoodEstablishment` + `Menu` nos cardápios publicados e `noindex` em rotas privadas/prévia).
- Oito páginas programáticas por segmento gastronômico, três ferramentas gratuitas sem cadastro (Gerador QR SVG/PNG, Calculadora de CMV com aritmética em centavos e Criador de Placa QR imprimível) e quatro guias práticos com blocos Answer-First para motores de resposta e IA.
- Contratos automatizados de SEO em `tests/seo_contracts.rs` validando reciprocidade `hreflang`, unicidade/tamanho de `title` e `meta description`, `JSON-LD`, integridade de links (`llms.txt` e rodapé), dimensões dos ícones/OG e `noindex` em rotas privadas.
- Política de nomeação de branches em [BRANCH_POLICY.md](BRANCH_POLICY.md) e gates de PR no GitHub Actions (`branch-policy`, `seo-contracts`, `quality`) protegendo `main`.

## Next Recommended Actions (Pilot Validation)

1. Provision a 1 vCPU / 1 GB Linux VPS with DNS pointing `qrcardapio.com.br` and run `compose.yaml` + `Caddyfile` over HTTPS.
2. Run Lighthouse audits on a live HTTPS deployment (`/` and `/c/{slug}`) and verify physical smartphone QR scanning on iOS and Android cameras.
3. Onboard the first 5–10 pilot restaurants and collect real feedback before evaluating out-of-scope features (e.g., WhatsApp ordering or custom domains).

## Assinatura Asaas

O Plano Essencial usa o preço da integração na landing. O modal mantém a escolha do visitante ao criar uma conta ou entrar e leva o proprietário à assinatura após a configuração do estabelecimento. A prévia local exibe o cartão e o modal quando o Asaas está desabilitado.

Os selos semestral e anual reservam espaço no canto dos cartões e exibem os percentuais centralizados.

O seletor de planos pode ser fechado pelo fundo da página, pelo botão de fechar ou por Escape.

A cobrança pode ser habilitada por ambiente. O modal compara os ciclos mensal, semestral com 5% e anual com 10%. O checkout e a cobertura usam o ciclo e o preço contratados. O proprietário contrata pelo checkout hospedado do Asaas, acompanha a cobrança no painel e pode cancelar a renovação. Webhooks autenticados e idempotentes sincronizam o acesso à publicação. Cardápios já publicados mantêm a entrega estática. A configuração e a homologação estão no [runbook](../operations/RUNBOOK.md#habilitação-do-asaas). A suíte local validou o ciclo financeiro com webhooks simulados. A validação na conta Sandbox do Asaas está pendente. Os [resultados de carga](../performance/PERFORMANCE.md#verificação-local-após-a-integração-asaas--23112026) incluem o proxy Caddy e a reentrega de webhooks a 1 vCPU.

## Integrações de IA

A landing apresenta o conector MCP, as marcas dos três clientes e a etapa de revisão e publicação. A seção mostra o endereço de conexão e aponta para o guia de ativação e a implementação.

O bloco de ativação separa pré-requisitos, autorização e endereço. A faixa de tipos de comércio tem nove ícones, animação contínua com pausa ao passar o mouse e uma versão estática para a preferência por movimento reduzido.

O endpoint MCP remoto usa OAuth 2.1 com PKCE, descoberta RFC 9728/8414, `WWW-Authenticate` RFC 6750 e CORS restrito às origens confiáveis para vincular ChatGPT, Claude e Google Antigravity ao estabelecimento autorizado pelo proprietário. As seis ferramentas (`get_account_settings`, `update_account_settings`, `get_menu_draft`, `apply_menu_changes`, `publish_menu` e `unpublish_menu`) permitem consultar e editar configurações da conta/estabelecimento (respeitando trava de slug pós-publicação), gerenciar categorias e produtos no rascunho (criar, editar, traduzir, mover, reordenar e excluir com limpeza de mídia órfã, idempotência e trava otimista) e publicar/despublicar o cardápio estático respeitando as regras de assinatura. A integração é configurada com `MCP_ENABLED=true` e HTTPS. Veja [Arquitetura MCP](../architecture/MCP.md) e [configuração dos clientes](../operations/RUNBOOK.md#integrar-chatgpt-claude-e-antigravity).

## Navegação da landing

A landing agrupa links secundários no menu Explorar, ao lado das ações da conta no desktop, e mantém o seletor de idioma na mesma linha da marca. O menu tem um ícone próprio de três traços e reúne as ações de conta em telas estreitas. Os oito segmentos têm cards ilustrados, e cada pergunta frequente tem uma arte própria. O sprite inclui a seta dos segmentos, os controles da FAQ e o selo da arte do Google Maps. O rodapé compartilhado organiza os links em Produto, Para seu negócio, Ferramentas e Guias.

## Login com o Google e contratos MCP

O cadastro, o login e o consentimento MCP aceitam Google quando as credenciais OAuth estão configuradas. Contas antigas vinculam a identidade na área de segurança. Os testes de contrato cobrem o fluxo OpenID Connect com tokens assinados, cadastro, sessão, vínculo, escolha de plano e retomada do MCP. Os contratos MCP exercitam descoberta, OAuth e ferramentas para ChatGPT, Claude e Google Antigravity.

[Ativação e homologação](../operations/RUNBOOK.md#habilitação-do-login-com-o-google) · [Contrato Google](../architecture/GOOGLE_AUTH.md) · [Contratos MCP](../architecture/MCP.md#contratos-e-homologação)

## Páginas de segmentos

As oito páginas de segmentos combinam uma arte exclusiva, cores por tema, benefícios em cards, um exemplo de cardápio, dicas operacionais, perguntas frequentes e links relacionados. Soluções, ferramentas e guias compartilham o breadcrumb com destaque para a página atual. O layout serve os mesmos textos e caminhos em português, inglês e espanhol.

A galeria de ferramentas gratuitas destaca o gerador de QR Code, a calculadora de CMV e o criador de placas com uma arte vetorial por ferramenta e cartões responsivos em português, inglês e espanhol.

As páginas das ferramentas reutilizam as artes da galeria e têm uma área de trabalho com formulário, prévia ou resultado, instruções e perguntas frequentes. O gerador oferece SVG e PNG de pelo menos 1024 pixels, a calculadora destaca o preço sugerido e o criador de placas imprime o resultado em uma folha A5.

A galeria de Soluções reutiliza as oito ilustrações e paletas das páginas individuais. Cada card abre o modelo do segmento no idioma selecionado.

## Guias práticos

A galeria e os quatro guias têm artes vetoriais e paletas próprias. Nas páginas individuais, a capa oferece um atalho ao primeiro capítulo e separa o resumo do texto da introdução. As seções de leitura têm índice, perguntas frequentes e links para guias, soluções e ferramentas no idioma selecionado.

## Cardápio público de demonstração

O QR original da home encaminha para o Bistrô da Praça em `/c/bistro-da-praca`. O seed executado antes da prontidão do servidor publica cinco categorias e nove produtos com fotos, preços e conteúdo em português, inglês e espanhol. O catálogo é fictício, tem endereço reservado e usa os arquivos estáticos de publicação.

## Autodeploy na VPS (Pull + Sigstore)

Implementado em `.github/workflows/deploy-bundle.yml` e `deploy/vps/` sem armazenar chaves SSH, IPs ou segredos de servidor no GitHub:
- Todo merge em `main` valida fmt, clippy, suíte completa Rust, testes unitários Python do agente e `cargo audit`, empacota `qrcardapio-<sha12>-linux-amd64.tar.gz`, gera atestação criptográfica Sigstore (`actions/attest-build-provenance`) e publica na release `deploy-main`.
- Na VPS, `qrcardapio-deploy.timer` executa `deploy/vps/qrcardapio-deploy-agent.py` a cada 60s (com `ETag`, `flock`, verificação de `refs/heads/main`, SHA-256, `gh attestation verify --bundle`, backup online SQLite via `VACUUM INTO`, cutover atômico via symlink `/opt/qrcardapio/current`, smoke checks de saúde/SEO/MCP, `republish-all` e rollback automático em caso de falha). Veja [Runbook](../operations/RUNBOOK.md#autodeploy-seguro-na-vps-pull-com-atestação-sigstore).

## Programa de Afiliados

Implementado de forma modular em `migrations/20261124000008_affiliate_program.sql`, `src/domain/{affiliate,affiliate_math}.rs`, `src/infrastructure/database/queries/{affiliates,affiliate_referrals,affiliate_ledger,affiliate_payouts}.rs`, `src/application/{affiliates,affiliate_operations}.rs`, `src/http/handlers/{affiliates,admin_affiliates,admin_affiliate_detail}.rs` e `src/rendering/affiliates.rs`:
- Autoativação aberta para qualquer conta autenticada (mesmo sem restaurante) em `/dashboard/affiliate` usando cota padrão configurável pelo admin, além de cadastro e personalização individual de cota mensal (`R$/mês` ou `%` da assinatura), carência (padrão 15 dias) e status (`active`/`paused`/`revoked`) em `/admin/affiliates`.
- Captura de indicação via `/r/{code}` e `/?ref={code}` (com redirecionamento `302 no-store` para URL canônica limpa e `Disallow: /r/`), atribuição imutável no cadastro (`/register` e primeiro login via Google OAuth), bloqueio de autoindicação (`account_id`, e-mail canônico `+alias`/Gmail e `establishment_members`) e mascaramento LGPD de e-mails no portal do parceiro.
- Sincronização atômica na mesma transação `BEGIN IMMEDIATE` dos webhooks Asaas e reconciliação: parcelamento mês a mês (`1x`, `6x`, `12x`) em centavos inteiros com snapshot imutável da cota, estorno automático de parcelas não pagas, geração de clawback negativo para parcelas já repassadas com compensação em repasses futuros, proteção contra eventos D+30 retroativos e baixa manual de PIX atômica isolada por ambiente (`sandbox`/`production`). Validado com 7 suítes de testes de integração em `tests/affiliates.rs` + 6 testes unitários de domínio (70 testes totais passando). Veja [Programa de Afiliados](../architecture/AFFILIATES.md).
