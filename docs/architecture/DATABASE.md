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
- `establishment_members`: Join table linking `accounts` and `establishments` (`PK (establishment_id, account_id)`, indexed by `account_id`).
- `media_assets`: Content-addressed image variants (`kind`, `content_hash`, dimensions, `filename_large`, `filename_thumb`), indexed by `(establishment_id, kind)`.
- `categories`: Ordered tenant categories (`position`, `is_visible`, optional EN/ES fields), indexed by `(establishment_id, position, id)`.
- `products`: Ordered items (`price_centavos` integer >= 0, `is_available`, `is_visible`, optional EN/ES fields), indexed by `(establishment_id, category_id, position, id)` and `image_asset_id`.
- `publications`: Append-only publication audit log (`version`, `status` in `'published'|'unpublished'|'failed'`, `html_bytes`, `duration_ms`), indexed by `(establishment_id, published_at DESC)`.
- `password_reset_tokens`: Single-use hashed recovery tokens (`expires_at`, `used_at`).

## Backups, Migrations & When to Outgrow SQLite

- **Migrations**: Versioned SQL files under `migrations/` executed automatically at boot via `sqlx::migrate!()` and `qrcardapio db migrate`.
- **Online Backups**: Uses `VACUUM INTO ?` (`backup_sqlite_into`), followed by read-only `PRAGMA integrity_check;`. Never `cp` an active `-wal` database directly.
- **When SQLite stops being the right choice**: When sustained concurrent write throughput across many accounts exceeds ~50–100 write transactions/sec persistently, when horizontal multi-node active-active write instances are required, or when database size outgrows single-node SSD capacity. Migration path: extract a clean repository boundary behind `queries/` targeting PostgreSQL (`sqlx::PgPool`).
