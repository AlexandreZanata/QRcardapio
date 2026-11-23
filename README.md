# QR Cardápio

**Digital menus and permanent printable QR Codes for Brazilian food businesses (`https://qrcardapio.com.br`).**

> "Crie um cardápio digital bonito, gere seu QR Code e atualize seus produtos e preços quando quiser."

## Key Highlights

- **Zero-DB Public Reads**: Publishing compiles an establishment's visible menu into a self-contained static HTML/CSS/media snapshot via atomic POSIX `rename(2)`. Caddy serves QR scans directly off disk without touching Rust or SQLite.
- **Permanent QR Codes**: QR Codes point to `/c/{slug}`. Owners can edit prices, mark sold-out dishes, reorder items, and republish anytime without reprinting table codes.
- **Full Native Zero-Reload i18n (`PT-BR`, `EN`, `ES`)**: SSR in the visitor's preferred language (`?lang=`, cookie, or `Accept-Language`), plus instant zero-reload switching in the browser—both in the owner dashboard and offline-ready inside static published menus.
- **Inexpensive Single-VPS Footprint**: Rust 1.100.0 + Axum 0.8 + Askama 0.16 + bundled SQLite 3.54.1 (WAL mode) running comfortably within a 1 vCPU / 1 GB RAM container budget.

## Quick Start (Local Development)

Prerequisites: Rust `1.100.0` (auto-selected via `rust-toolchain.toml`) and a C compiler (`cc`) for bundled SQLite compilation.

```bash
cp .env.example .env
./scripts/dev.sh
# Server runs at http://127.0.0.1:8080
```

Seed a demo restaurant (`cafe-da-praca`) or bootstrap an operational admin account:

```bash
cargo run -- seed
cargo run -- admin create --email ops@qrcardapio.com.br --password "ChangeMeStrongPass123"
```

## Production Deployment

```bash
DOMAIN=qrcardapio.com.br PUBLIC_BASE_URL=https://qrcardapio.com.br docker compose up -d --build
```

See [docs/operations/RUNBOOK.md](docs/operations/RUNBOOK.md) for backups, restores, incident recovery, and pilot onboarding, and [AGENTS.md](AGENTS.md) for contributor guidelines.
