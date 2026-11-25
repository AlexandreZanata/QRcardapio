# Database Architecture & SQLite Design

## Engine Verification

SQLite is statically compiled from `libsqlite3-sys` (`sqlite-bundled` feature in `sqlx 0.9.1`). At startup (`init_pool`) and in `/readyz` + automated tests, the application executes `SELECT sqlite_version()` and asserts **SQLite 3.54.1** (rather than relying on older host OS libraries).

## Connection & PRAGMA Configuration

Configured in `src/infrastructure/database/mod.rs`:

| PRAGMA / Setting | Value | Rationale |
|---|---|---|
| `journal_mode` | `WAL` | Concurrent readers never block writers; short write transactions stay fast. |
| `foreign_keys` | `ON` | Enforces referential integrity across tenant cascades. |
| `busy_timeout` | `5000 ms` | Smoothly handles brief write contention without spurious `SQLITE_BUSY`. |
| `synchronous` | `FULL` (default) | Highest crash durability; `fsync` protects against power-loss corruption. |
| `auto_vacuum` | `INCREMENTAL` | Bounds disk bloat across deletions without full blocking `VACUUM`. |
| `cache_size` | `-8000` (8 MiB) | Keeps hot B-tree pages in memory while staying tiny on 1 GB RAM VPS. |
| `temp_store` | `MEMORY` | Fast temporary sort/index passes without temp disk churn. |
| Pool size | `max = 5`, `min = 1` | Matches SQLite's single-writer concurrency model with negligible memory overhead. |
| Write locking | `BEGIN IMMEDIATE` | `begin_immediate()` prevents snapshot upgrade `SQLITE_BUSY` errors under concurrent writers by honoring `busy_timeout`. |

### `synchronous=FULL` vs `synchronous=NORMAL`

In WAL mode, `NORMAL` avoids `fsync` on every transaction commit (flushing mainly on WAL checkpoints), whereas `FULL` calls `fsync` on commit. Because QR Cardápio serves public scans from static files and only writes to SQLite during infrequent owner dashboard edits, **write QPS is low and data integrity is paramount**; therefore `DB_SYNCHRONOUS=FULL` is the default. Operators on battery-backed NVMe may opt into `DB_SYNCHRONOUS=NORMAL` via environment variable.

**Critical Operational Rule:** Never place a WAL SQLite database on a network filesystem (NFS/SMB/EFS); POSIX locking and shared-memory WAL index require local block storage.

## Schema & Indexes (`migrations/20261123000001_initial_schema.sql`)

- `accounts`: Owner and admin credentials (`email` unique `COLLATE NOCASE`, Argon2id hash, `role`, `preferred_locale`).
- `sessions`: Server-side opaque session hashes (`token_hash` unique, `csrf_token`, `expires_at`), indexed by `account_id` and `expires_at`.
- `establishments`: Tenant root (`slug` unique, brand metadata, optional `description_{en,es}`, `accent_color`, `logo_asset_id`, `is_published`, `published_version`, `has_unpublished_changes`).
  Automatic addresses use the existing unique slug index. Initial creation checks membership and inserts the establishment and owner in one `BEGIN IMMEDIATE` transaction. A collision triggers another candidate only for automatic links.
- `20261123000003_google_maps_links.sql` adds nullable `google_maps_url` and `google_review_url` to the tenant root. Saving the profile validates HTTPS links against recognized Google Maps and Business Profile hosts. Omitted fields preserve the stored links and submitting a blank field clears that link.
- `establishment_members`: Join table linking `accounts` and `establishments` (`PK (establishment_id, account_id)`, indexed by `account_id`).
- `media_assets`: Content-addressed image variants (`kind`, `content_hash`, dimensions, `filename_large`, `filename_thumb`), indexed by `(establishment_id, kind)`.
- `categories`: Ordered tenant categories (`position`, `is_visible`, optional EN/ES fields), indexed by `(establishment_id, position, id)`.
- `products`: Ordered items (`price_centavos` integer >= 0, `is_available`, `is_visible`, optional EN/ES fields), indexed by `(establishment_id, category_id, position, id)` and `image_asset_id`.
- `publications`: Append-only publication audit log (`version`, `status` in `'published'|'unpublished'|'failed'`, `html_bytes`, `duration_ms`), indexed by `(establishment_id, published_at DESC)`.
- `password_reset_tokens`: Single-use hashed recovery tokens (`expires_at`, `used_at`).
- `ocr_imports` (`migrations/20261123000002_ocr_imports.sql`): Staged photo/PDF/text menu extractions (`source_kind`, `status` in `'review'|'applied'|'discarded'`, `engine_info`, `page_count`, `raw_text`, `parsed_json`, `warnings_json`, `items_detected`, `items_imported`), indexed by `(establishment_id, created_at DESC)`.

## Backups, Migrations & When to Outgrow SQLite

- **Migrations**: Versioned SQL files under `migrations/` executed automatically at boot via `sqlx::migrate!()` and `qrcardapio db migrate`.
- **Online Backups**: Uses `VACUUM INTO ?` (`backup_sqlite_into`), followed by read-only `PRAGMA integrity_check;`. Never `cp` an active `-wal` database directly.
- **When SQLite stops being the right choice**: When sustained concurrent write throughput across many accounts exceeds ~50–100 write transactions/sec persistently, when horizontal multi-node active-active write instances are required, or when database size outgrows single-node SSD capacity. Migration path: extract a clean repository boundary behind `queries/` targeting PostgreSQL (`sqlx::PgPool`).

## SQLite e assinatura Asaas

A migração `20261123000004_asaas_billing.sql` cria `billing_checkouts` por estabelecimento, `billing_payments` por cobrança do provedor e `billing_webhook_events` para deduplicação. As chaves do provedor incluem o ambiente para isolar Sandbox e produção. Os eventos e as alterações de cobrança são confirmados na mesma transação de escrita. A publicação lê apenas a cobertura vigente vinculada ao estabelecimento. O snapshot público continua independente do SQLite.

A migração `20261123000006_billing_plan_cycles.sql` registra os meses e o preço em centavos de cada checkout. As cobranças confirmadas guardam o valor em centavos. A visão `billing_coverage` calcula o fim do período pelo vencimento, pelo ciclo armazenado e pela correspondência entre o valor confirmado e o preço do checkout. A migração mantém as cobranças anteriores no plano mensal.

## OAuth e operações MCP

A migração `20261123000005_mcp_oauth.sql` mantém solicitações de autorização com expiração, códigos de uso único, vínculos com hashes de acesso e renovação, e chaves de idempotência por conta e estabelecimento. Os vínculos referenciam o membro do estabelecimento. A transação de escrita valida o vínculo e aplica as mudanças em um único lote. A manutenção remove registros expirados por índice.

## Identidade Google

A migração `20261123000007_google_oidc.sql` cria `google_identities` e `google_oauth_flows`. A primeira tabela tem um `sub` único do Google por conta local. Cadastro e vínculo são transacionais. A segunda tabela registra o estado temporário por hash e o consome com `DELETE … RETURNING`, atrelado ao navegador, cliente e URI de retorno. A limpeza expira esses registros e as referências encerram fluxos ligados a sessões ou autorizações MCP excluídas.

Uma conta criada via Google tem senha local vazia até cadastrar uma pelo processo de recuperação. O login por senha faz a verificação Argon2 de referência para essas contas. Tokens Google de identidade, acesso e renovação são descartados após o callback.

## Programa de Afiliados

A migração `20261124000008_affiliate_program.sql` adiciona seis tabelas com integridade referencial estrita:
- `affiliate_program_settings` (linha única `id = 1`): controle de autoativação (`self_signup_enabled`), tipo/valor padrão de cota mensal (`fixed_monthly_centavos` ou `percent_bps`), carência padrão (`default_hold_days`) e janela do cookie (`cookie_ttl_days`).
- `affiliates`: vínculo `1:1` com `accounts(id)` (independente de possuir estabelecimento), `code` único, `status` (`active`|`paused`|`revoked`), cota individual, `hold_days`, `pix_key`, `notes` e `click_count`.
- `affiliate_referrals`: atribuição imutável (`UNIQUE(referred_account_id)` e `UNIQUE(referred_establishment_id)`), status (`signed_up`|`active`|`disqualified`) e carimbo do primeiro pagamento.
- `affiliate_commissions`: razão financeiro imutável isolado por `environment` (`sandbox`|`production`), com `UNIQUE(environment, payment_id, installment_index, entry_kind)` cobrindo parcelas mensais (`1..=12`) e estornos negativos (`clawback`), snapshot da cota aplicada e data determinística `available_on`.
- `affiliate_payouts`: registro de repasses manuais via PIX com `pix_key_snapshot`, comprovante (`reference`), `entries_count`, `paid_by_account_id` e liquidação atômica em `BEGIN IMMEDIATE`.
- `affiliate_evaluated_payments`: registro idempotente de pagamentos já avaliados na primeira confirmação válida para impedir concessão retroativa em eventos `PAYMENT_RECEIVED` tardios. Veja [Programa de Afiliados](AFFILIATES.md).

## Commercial Engine e Entitlement Engine

A migração `20261124000009_commercial_foundation.sql` é uma migração *expand* idempotente (somente `CREATE … IF NOT EXISTS` e `INSERT … ON CONFLICT DO NOTHING`, sem `ALTER TABLE`):

- `commercial_catalog_offers` / `commercial_catalog_versions`: registro do catálogo visto pelos binários; hash SHA-256 imutável de cada versão vendável e guarda de transição de ciclo de vida.
- `commercial_contracts`, `commercial_contract_items`, `commercial_contract_capabilities`: snapshot imutável de cada contratação (ciclo, desconto, preço, versão, capacidades, receita elegível a comissão). Chave estrangeira composta `(billing_reference, environment, establishment_id)` garante o mesmo tenant do checkout.
- `entitlement_grants`: concessões explícitas auditadas (experimentação, beta, prévia interna, cortesia), com no máximo uma experimentação por oferta.
- `establishment_operational_states`: suspensão administrativa ou de segurança (ausência de linha = ativo).
- `domain_events`: outbox transacional ([EVENT_CONTRACTS](EVENT_CONTRACTS.md)).
- Visão `commercial_entitlement_periods`: cobertura paga por capacidade, com a mesma regra de `billing_coverage`.
- Backfill: cada linha existente de `billing_checkouts` vira um contrato `legacy_plan` do QR Menu v1 preservando preço, ciclo e identificadores do Asaas.

`init_pool` executa as migrações com `ignore_missing = true` (binário antigo aceita banco migrado por versão mais nova; ver [ADR-010](../decisions/ADR-010-expand-contract-migrations.md)) e em seguida sincroniza o registro do catálogo, recusando a inicialização se uma versão vendida tiver mudado.

## Cobrança com vários provedores

A migração `20261124000010_multi_provider_billing.sql` (expand) adiciona `provider`, `payment_method` e `checkout_url` a `billing_checkouts`, `settled_centavos` a `billing_payments`, e cria `billing_payer_profiles` (CPF/CNPJ e endereço do pagador, dado pessoal) e `billing_inter_charges` (uma cobrança Inter por ciclo, reservada antes da chamada à API, com `UNIQUE (billing_reference, cycle_index)`). Identificadores da Stripe e do Inter são gravados como `stripe:<id>`/`inter:<id>` para manter as chaves únicas existentes globais.
