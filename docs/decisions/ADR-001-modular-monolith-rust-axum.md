# ADR-001: Modular Monolith in Rust & Axum

- **Status**: Accepted (2026-11-23)
- **Context**: QR Cardápio targets a 1 vCPU / 1 GB RAM VPS with minimal operational complexity, strong multi-tenant isolation, and fast AI-assisted maintenance.
- **Decision**: Build a single Rust 1.100.0 (Edition 2024) modular monolith using Axum 0.8.9, Tokio, Askama 0.16.1, and SQLx 0.9.1, split cleanly into small cohesive files (~50–350 lines each) across `domain/`, `application/`, `infrastructure/`, `http/`, `rendering/`, `i18n/`, and `qr/`.
- **Consequences**:
  - Single binary deployable with ~15–25 MB RSS footprint under load.
  - Zero network hops between modules; compile-time template and type checks.
  - Easy future extraction behind `application/` or `queries/` if write load ever warrants splitting services.
