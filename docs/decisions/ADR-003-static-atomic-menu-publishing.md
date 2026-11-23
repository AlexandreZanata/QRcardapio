# ADR-003: Static Atomic Menu Publication

- **Status**: Accepted (2026-11-23)
- **Context**: Restaurant QR scans spike simultaneously at lunch/dinner hours and should never contend on SQLite or pay per-request HTML rendering costs.
- **Decision**: Owner changes remain in SQLite draft state until published. Publishing renders a self-contained static directory (`published/{slug}/index.html`, `theme-{hash8}.css`, content-addressed `media/*`) via temporary files + `fsync` + atomic `rename(2)` on the same filesystem, served directly by Caddy (or Axum disk handler in single-process deployments) with zero DB queries.
- **Consequences**:
  - Public scans execute zero SQLite queries and zero template rendering.
  - Readers never see incomplete HTML or missing assets, even during concurrent republishing.
  - Requires keeping `published/.tmp/` and `published/{slug}/` on the same mount/filesystem.
