# Performance Budgets, Benchmark Methodology & Baseline Results

## Target Production Hardware Budget

- **Compute**: 1 vCPU, 1 GB RAM Linux VPS (`compose.yaml` caps `app` at `512m` and `caddy` at `128m`)
- **Storage**: Local SSD (`SQLite` + `media/` + `published/`)
- **Runtime footprint**: Single stripped release binary (`target/release/qrcardapio` = **9.5 MiB**)

## Measured Local Benchmark Environment

- **Host**: Linux `7.2.15-76070215-generic`, 16 CPU threads, 31 GiB RAM
- **Toolchain**: `rustc 1.100.0` (`--release --locked`, `lto = "thin"`, `codegen-units = 1`, `opt-level = 3`)
- **Load Generator**: `oha`
- **Important Caveat**: Throughput numbers below were measured on the development workstation above against localhost loopback; single-vCPU VPS deployments will scale proportionally lower for CPU-bound SSR routes while static Caddy delivery remains I/O and memory-cache bound.

## Reproducible Benchmark Script

Run `./scripts/bench_run.sh` (creates isolated temp dirs in `/tmp`, leaves workspace clean).

## 1. Multi-Tenant SQLite Scaling (`bench-gen`, 3 categories + 12 products per establishment)

| Establishments | Accounts | Categories | Products | SQLite File Size | Bytes / Est. | Generation + Sample Publish Time |
|---:|---:|---:|---:|---:|---:|---:|
| 100 | 100 | 300 | 1,200 | 933,888 B (~912 KiB) | ~9,339 B | 5,162 ms (pre-PASSIVE checkpoint fix) |
| 1,000 | 1,000 | 3,000 | 12,000 | 7,700,480 B (~7.34 MiB) | ~7,700 B | 302 ms |
| 10,000 | 10,000 | 30,000 | 120,000 | 76,460,032 B (~72.9 MiB) | ~7,646 B | 2,329 ms |
| 100,000 | 100,000 | 300,000 | 1,200,000 | 768,606,208 B (~733 MiB) | ~7,686 B | 21,370 ms |

## 2. Static Snapshot Size & Publication Latency (by Menu Size)

Each published `index.html` includes full Askama SSR markup, `data-pt`/`data-en`/`data-es` attributes, and the embedded `#qr-menu-i18n` zero-reload JSON bundle.

| Menu Size | Raw HTML Size | Gzipped HTML Size | CLI `republish-all` Wall Time (Startup + DB + Atomic Write) |
|---:|---:|---:|---:|
| Small (10 items) | 14,824 B | 2,309 B | 18.0 ms |
| Medium (35 items) | 33,319 B | 2,813 B | 11.1 ms |
| Large (80 items) | 66,664 B | 3,728 B | 77.2 ms |

## 3. HTTP Throughput, Latency & Memory (`1,000` Establishments DB, 35-item Target Menu)

- **Server RSS idle** (`/proc/<pid>/status` `VmRSS`): **10,780 kB (~10.5 MiB)**
- **Server RSS after sustained HTTP load**: **53,040 kB (~51.8 MiB)**

| Workload | Concurrency | Duration | Requests/sec | Total OK (200) | Success Rate | p50 | p95 | p99 |
|---|---:|---:|---:|---:|---:|---:|---:|---:|
| Public static menu `GET /c/{slug}` (0 DB queries) | 32 | 5s | 28,794.5 RPS | 144,418 | 100.00% | 0.97 ms | 2.15 ms | 3.07 ms |
| Authenticated owner `GET /dashboard` (SQLite + SSR) | 16 | 5s | 3,333.9 RPS | 16,656 | 100.00% | 4.70 ms | 5.83 ms | 6.95 ms |
| Public `GET /c/cafe-da-praca` during 5 active publishes | 32 | 4s | 26,880.9 RPS | 107,561 | 100.00% | 1.03 ms | 2.35 ms | 3.35 ms |

## 4. Concurrent Owner Write Contention

- **Test**: 8 distinct authenticated owners simultaneously creating 3 categories + publishing + verifying `GET /c/{slug}` against the live `--release` server (`BEGIN IMMEDIATE` write transactions + `busy_timeout = 5000`).
- **Result**: **8 / 8 succeeded (100% HTTP 200, 0 `SQLITE_BUSY` errors)**, total wall time **617.6 ms**, average per-owner 5-request flow **362.1 ms**.
