# Project State

Last updated: 2026-11-23

## Current Status — MVP Complete

All 5 implementation phases of the **QR Cardápio MVP** (including full native zero-reload `pt-BR` / `en` / `es` i18n across both owner panels and static published menus) are implemented, tested, benchmarked, and documented:

- **Phase 1 — Technical Foundation**: Complete & verified (`Cargo.toml`, `rust-toolchain.toml` pinned to `1.100.0`, Edition 2024, bundled SQLite `3.54.1` via `sqlx 0.9.1`, WAL + `synchronous=FULL`, migrations, structured logging, health/readiness endpoints).
- **Phase 2 — Domain, Security & Multi-Tenancy**: Complete & verified (Argon2id with bounded semaphore, CSPRNG SHA-256 hashed sessions with rotation, constant-time CSRF, IP & account rate limiting, `TenantContext` isolation, `BEGIN IMMEDIATE` write transactions, standardized per-entity submodules under `src/infrastructure/database/queries/`, EXIF-stripping image resizing).
- **Phase 3 — Premium Interface & Zero-Reload i18n**: Complete & verified (design tokens, responsive Askama templates, strict CSP with zero inline scripts/styles, `/static/i18n/{locale}.json` immutable endpoints, `static/js/i18n.js` + `static/js/menu.js` zero-reload switching).
- **Phase 4 — Static Publishing & QR Codes**: Complete & verified (atomic temp+fsync+rename publishing to `published/{slug}/`, content-addressed assets, failure rollback preserving previous live menu, unpublishing, zero-DB disk delivery, high-contrast SVG & >=1024px PNG QR generation, printable sheet).
- **Phase 5 — Operations, Benchmarks & AI-First Docs**: Complete & verified (`Dockerfile`, `compose.yaml`, `Caddyfile`, `scripts/{dev,backup,restore,bench_run}.sh`, full `AGENTS.md` + `docs/` + 6 ADRs, real benchmarks at 100..100k establishments).
- **Phase 6 — Local Hybrid OCR & PDF Menu Pre-Registration**: Complete & verified (`migrations/20261123000002_ocr_imports.sql`, pure-Rust menu parser under `src/domain/ocr_parser/`, hybrid `pdftotext` -> `pdftoppm` + pure-Rust image preprocessing + `tesseract` engine under `src/infrastructure/ocr/` with `Semaphore(1)` and timeouts, `ocr_imports` staging persistence, `/dashboard/ocr` upload/paste/review/bulk-apply/discard flow with full `pt-BR`/`en`/`es` i18n and strict multi-tenant + CSRF + rate-limit protection, ADR-006).

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

The category creation dialog shares its fields with the standalone category form and shows validation without leaving the editor. The overview uses an illustrated, keyboard-accessible confirmation for taking a menu offline.

The product dialog opens from the editor, a category row, or the product creation URL. It shares the standard fields and photo picker with the standalone form and edit page. Localized validation stays in the dialog, and a successful save returns to the same editor page.

The menu editor uses tenant-scoped pagination with 12 product or empty-category rows per page. The editor retains the active page through inline actions and edit forms. Mobile layouts preserve the landing Maps mockup composition, adapt dashboard tables into labeled rows, and scale the QR print preview to the screen.

O cadastro atribui links compactos a partir do nome do estabelecimento. Um código de quatro caracteres distingue nomes repetidos. Endereços personalizados continuam disponíveis no rascunho e o caminho publicado é protegido contra renomeação concorrente.

## Verification Snapshot

- `cargo fmt --check`: Passed
- `cargo clippy --all-targets --all-features -- -D warnings`: Passed (0 warnings)
- `cargo test --locked`: Passed (19 unit tests + 13 integration tests across 9 integration suites, including real `pdftotext` + `tesseract` E2E OCR tests)
- `cargo audit`: Passed (0 vulnerabilities; 2 unmaintained advisory notices: `basic-toml 0.1.10` transitive, `chrono 0.4.45`)
- `cargo build --release --locked`: Passed
- Live smoke test (`admin create`, `seed`, `backup.sh`, `restore.sh`, HTTP SSR + static `/c/cafe-da-praca`, `/dashboard/ocr`): Passed

## Next Recommended Actions (Pilot Validation)

1. Provision a 1 vCPU / 1 GB Linux VPS with DNS pointing `qrcardapio.com.br` and run `compose.yaml` + `Caddyfile` over HTTPS.
2. Run Lighthouse audits on a live HTTPS deployment (`/` and `/c/{slug}`) and verify physical smartphone QR scanning on iOS and Android cameras.
3. Onboard the first 5–10 pilot restaurants and collect real feedback before evaluating out-of-scope features (e.g., WhatsApp ordering or custom domains).

## Assinatura Asaas

O Plano Essencial usa o preço da integração na landing. O modal mantém a escolha do visitante ao criar uma conta ou entrar e leva o proprietário à assinatura após a configuração do estabelecimento. A prévia local exibe o cartão e o modal quando o Asaas está desabilitado.

A cobrança pode ser habilitada por ambiente. O modal compara os ciclos mensal, semestral com 5% e anual com 10%. O checkout e a cobertura usam o ciclo e o preço contratados. O proprietário contrata pelo checkout hospedado do Asaas, acompanha a cobrança no painel e pode cancelar a renovação. Webhooks autenticados e idempotentes sincronizam o acesso à publicação. Cardápios já publicados mantêm a entrega estática. A configuração e a homologação estão no [runbook](../operations/RUNBOOK.md#habilitação-do-asaas). A suíte local validou o ciclo financeiro com webhooks simulados. A validação na conta Sandbox do Asaas está pendente. Os [resultados de carga](../performance/PERFORMANCE.md#verificação-local-após-a-integração-asaas--23112026) incluem o proxy Caddy e a reentrega de webhooks a 1 vCPU.

## Integrações de IA

O endpoint MCP remoto usa OAuth com PKCE e vincula ChatGPT, Claude e Antigravity ao estabelecimento autorizado pelo proprietário. As ferramentas consultam e editam o rascunho em lotes atômicos, com idempotência e verificação da versão dos itens. A publicação continua no painel. A integração é configurada com `MCP_ENABLED=true` e HTTPS. Veja [Arquitetura MCP](../architecture/MCP.md) e [configuração dos clientes](../operations/RUNBOOK.md#integrar-chatgpt-claude-e-antigravity).
