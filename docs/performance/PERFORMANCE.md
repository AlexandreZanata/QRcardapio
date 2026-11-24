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

## Verificação local após a integração Asaas — 23/11/2026

Release otimizado em Linux, dados sintéticos, HTTP em localhost e gerador `oha`. O host dispõe de 16 threads de CPU e 31 GiB de RAM. Os cenários abaixo tiveram HTTP 200 em todas as respostas concluídas.

### Cardápio, painel e publicação

- Regressão via `scripts/bench_run.sh`, com cobrança desabilitada. O cardápio no Axum chegou a **30.086 req/s**, p95 **1,94 ms**, com 32 conexões por 5 s. O painel autenticado chegou a **3.248 req/s**, p95 **6,36 ms**, com 16 conexões por 5 s. O cardápio durante cinco republicações chegou a **26.435 req/s**, p95 **2,24 ms**, com 32 conexões por 4 s. O `oha` encerrou 80 requisições em voo pelo prazo de duração, sem contabilizá-las como respostas HTTP.
- O Caddy, em contêiner com limite de 1 vCPU e sem o processo Axum rodando, serviu o cardápio de 35 itens a **6.469 req/s**, p95 **10,65 ms**, com 32 conexões por 5 s. A regra de controle de arquivos temporários retornou `404`.
- A geração sintética de **100.000 estabelecimentos, 300.000 categorias e 1,2 milhão de produtos**, com três publicações de amostra, levou **19,86 s**. O arquivo SQLite mediu aproximadamente **733 MiB**.

### Cobrança habilitada, aplicação limitada a 1 CPU

Execute `BENCH_CPU=0 ./scripts/bench_billing.sh` ou escolha um núcleo permitido no seu sistema. O script usa 1.000 estabelecimentos, ambiente temporário e credenciais fictícias, sem requisições ao Asaas. Cada cenário usa 16 conexões por 5 s, com espera pelas requisições já iniciadas.

- O painel autenticado chegou a **1.987 req/s**, p95 **10,69 ms**.
- A página de assinatura chegou a **3.020 req/s**, p95 **6,98 ms**.
- O webhook processou a **reentrega do mesmo evento** a **39.832 req/s**, p95 **0,57 ms**. A medição cobre leitura de idempotência.
- Em uma segunda execução, **1.000 cobranças inéditas** foram enviadas por 16 conexões persistentes com um gerador Python. As 1.000 retornaram `200`, a **201 req/s** e p95 **164,99 ms**. Esse cenário inclui a transação SQLite de deduplicação e persistência de cada cobrança, com `synchronous=FULL`. A escrita é a etapa mais sensível à concorrência nesta bateria.
- Na primeira execução, o RSS da aplicação passou de **13.652 KiB** para **18.176 KiB**. Na execução com eventos inéditos, encerrou em **18.500 KiB**.

As amostras medem latência de resposta completa e não incluem TLS público, tráfego externo, chamadas ao Asaas nem carga sustentada na VPS de produção. Varie a quantidade de conexões e a mistura de rotas para dimensionar a implantação.

## Commercial foundation verification — 24/11/2026

Same host as the baseline (16 threads, 31 GiB, release build, `scripts/bench_run.sh`). These numbers cover menu and dashboard paths only; they are **not** evidence of order-processing capacity (see [SCALING_STRATEGY](../architecture/SCALING_STRATEGY.md)).

| Path | Billing | RPS | p50 | p95 | p99 | Baseline RPS |
|---|---|---|---|---|---|---|
| `GET /c/{slug}` (static, 32 conn.) | — | 25 467 | 1.15 ms | 2.19 ms | 3.05 ms | 28 795 (HTML now 44.7 KB vs 33.3 KB) |
| `GET /c/cafe-da-praca` during 5 publishes | — | 27 098 | 1.08 ms | 2.06 ms | 2.94 ms | 26 881 |
| `GET /dashboard` (16 conn.) | not enforced | 3 466 | 4.54 ms | 5.51 ms | 6.46 ms | 3 334 |
| `GET /dashboard` (16 conn.) | enforced (entitlement query) | 2 544 | 6.25 ms | 7.13 ms | 7.60 ms | — |
| `GET /dashboard/billing` (16 conn.) | enforced (module catalog) | 3 062 | 5.10 ms | 6.42 ms | 7.42 ms | — |
| `POST /dashboard/publish`, 20 sequential | enforced (authorize + render + outbox) | — | avg 9 ms end-to-end | — | — | — |

Memory: 14.6 MB RSS idle, 61.4 MB after load. Database scaling with `bench-gen` matched the baseline (100 000 establishments: 768.9 MB, 19.5 s).

Query plans: the entitlement view and the rebuilt `billing_coverage` view force an index-driven join order (`commercial_contracts` or `billing_checkouts` by establishment, then `idx_billing_payments_subscription`). Without it SQLite scanned every payment of the environment on each request, a cost that grows with the customer base. Contract periods and grants are read in one `UNION ALL` round trip. The enforced dashboard still performs one extra pooled query per request; caching the decision per request batch is unnecessary at current traffic. The enforced script lives in the session notes, not in `scripts/`, because it needs Asaas-shaped configuration.
