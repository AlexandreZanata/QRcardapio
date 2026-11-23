# Module Dependencies & Boundaries

## Allowed Dependency Direction

```text
          ┌──────────────┐
          │   http/      │  Handlers, Extractors, Middleware, Flash/ETag
          └──────┬───────┘
                 │ calls
                 ▼
          ┌──────────────┐     renders via      ┌──────────────┐
          │ application/ │ ───────────────────▶ │ rendering/   │
          └──────┬───────┘                      └──────┬───────┘
                 │ uses                                │
        ┌────────┴────────┐                            │
        ▼                 ▼                            ▼
┌────────────────┐ ┌──────────────┐             ┌──────────────┐
│ infrastructure/│ │   domain/    │ ◀────────── │    i18n/     │
└────────────────┘ └──────────────┘             └──────────────┘
```

## Rules Enforced Across Modules

1. **`domain/` has zero framework dependencies**: Domain modules must not import `axum`, `sqlx`, `tokio`, or `askama`.
2. **`http/handlers/` contain no SQL or image codecs**: Handlers extract validated request context (`AuthContext`, `TenantContext`, `AdminContext`, `RequestLocale`), delegate to `application/`, and pass view models to `rendering/`.
3. **Database queries live exclusively in `infrastructure/database/queries/`**: Split into entity-scoped files (`accounts.rs`, `sessions.rs`, `establishments.rs`, `categories.rs`, `products.rs`, `media.rs`, `publications.rs`, `admin.rs`, `mappers.rs`). Exception: `application/bench_gen.rs` executes batched synthetic inserts for benchmark generation.
4. **Templates contain no business logic**: Askama templates receive pre-formatted strings/flags from `src/rendering/`.
5. **No circular imports**: `qr/` depends only on `error`, `qrcode`, and `image`. `i18n/` depends only on `serde_json` and `sha2`.

## Verified Production Crate Baseline (`Cargo.lock`)

- Rust toolchain: `1.100.0` (Edition `2024`)
- Web & async: `axum 0.8.9`, `axum-extra 0.12.6`, `tokio 1.54.1`, `tower 0.5.3`, `tower-http 0.7.1`
- Database: `sqlx 0.9.1` (`sqlite-bundled` -> `libsqlite3-sys 0.40.0` -> **SQLite 3.54.1**)
- Templates & serialization: `askama 0.16.1`, `serde 1.0.229`, `serde_json 1.0.154`
- Security & rate limiting: `argon2 0.6.0`, `governor 0.10.4`, `subtle 2.6.1`, `getrandom 0.4.3`, `sha2 0.11.1`, `base64 0.23.1`, `hex 0.4.3`
- Media & QR: `image 0.25.10` (`jpeg`, `png`, `webp` only), `qrcode 0.15.0` (`svg`, `image` only)
- Utilities: `clap 4.6.9`, `chrono 0.4.45`, `uuid 1.29.3`, `thiserror 2.0.21`, `tempfile 3.27.0`, `dotenvy 0.15.7`, `tracing 0.1.44`, `tracing-subscriber 0.3.23`
