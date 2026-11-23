# ADR-002: Bundled SQLite 3.54.1 in WAL Mode with `synchronous=FULL`

- **Status**: Accepted (2026-11-23)
- **Context**: Menu edits are infrequent compared to public reads, and target VPS budgets cannot afford dedicated managed Postgres clusters during MVP validation.
- **Decision**: Bundle SQLite 3.54.1 statically via `sqlx`'s `sqlite-bundled` feature (`libsqlite3-sys 0.40.0`). Enable `WAL` mode, `foreign_keys=ON`, `busy_timeout=5000`, conservative pool (`max_connections=5`), and default `synchronous=FULL`. Backups use online `VACUUM INTO` + `PRAGMA integrity_check`.
- **Consequences**:
  - Guarantees consistent SQLite 3.54.1 regardless of older Linux distro packages.
  - Zero external DB daemon RAM/cost; maximum crash durability for owner menu edits.
  - Single concurrent writer per file; requires local block storage (never NFS/SMB) and eventual Postgres migration only if sustained concurrent writes exceed single-node limits.
