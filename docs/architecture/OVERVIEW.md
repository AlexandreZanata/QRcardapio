# Architecture Overview

## System Context

QR Cardápio is built as a **single-binary modular monolith** in Rust paired with Caddy for static delivery in production.

```text
  [Customer Smartphone] ──GET /c/{slug}──▶ [Caddy] ──▶ /srv/published/{slug}/index.html (0 DB queries)
                                              │
  [Restaurant Owner] ────/dashboard/*─────────┴──▶ [qrcardapio (Axum + Tokio)] ──▶ [SQLite 3.54.1 WAL]
                                                         │
                                                         └── publish() ──▶ atomic rename ──▶ /data/published/{slug}/
```

## Core Design Principles

1. **Read/Write Asymmetry**: Public customer traffic outnumbers owner menu edits by orders of magnitude. Moving rendering work to publication time eliminates database reads, connection contention, and template allocation from public QR scans.
2. **Modular Structure (Zero King Files)**: Every module is organized around a single cohesive responsibility (~50–350 lines per file), preventing oversized files that slow down human review and AI context windows.
3. **Zero Node.js Runtime**: HTML templates compile at Rust build time via Askama (`templates/`), CSS uses native custom properties (`static/css/`), and progressive enhancement uses tiny vanilla JS modules (`static/js/`).
4. **Strict CSP & Input Hygiene**: No inline executable JavaScript and no inline `style="..."` attributes anywhere. Dynamic brand accent themes live in generated CSS files.

## Module Responsibilities

| Layer | Directory | Responsibility |
|---|---|---|
| Entry & Composition | `src/main.rs`, `src/app.rs`, `src/config.rs`, `src/error.rs` | CLI parsing, graceful shutdown, router assembly, typed configuration, unified error mapping. |
| Domain | `src/domain/` | Pure business types and validation (`Email`, `Slug`, `AccentColor`, `parse_price_centavos`, `Establishment`, `Category`, `Product`, `MenuSnapshot`). |
| Application | `src/application/` | Orchestrated use cases (`auth`, `establishments`, `menus`, `publishing`, `admin`, `seed`, `bench_gen`). |
| Infrastructure | `src/infrastructure/` | Bundled SQLite pool & migrations (`database/`), domain-split SQL queries (`database/queries/`), Argon2id & rate limiters (`security/`), EXIF-stripping image resizing (`assets/`), safe atomic filesystem writes (`storage/`). |
| HTTP | `src/http/` | Thin feature handlers (`handlers/`), auth/locale/security-header extractors (`middleware/`), flash & ETag helpers (`responses/`). |
| Presentation & QR | `src/rendering/`, `src/qr/`, `src/i18n/` | Askama view models, static HTML/CSS generator, SVG/PNG QR Code generation, compile-time i18n catalogs. |
