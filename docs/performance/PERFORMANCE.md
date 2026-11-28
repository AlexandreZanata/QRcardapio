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

## Commerce Core Increment 3

Run `scripts/bench_commerce.sh checkout`, `mixed`, `outbox`, or `push` on Linux. `COMMERCE_BENCH_CPU` selects an allowed CPU. The script builds locked release binaries before pinning the workload. Each database is temporary and uses SQLite WAL and `synchronous=FULL`.

- Checkout runs for 600 seconds by default with 200 establishments, 64 clients, approximately 10% retries and concurrent staff transitions. It reconciles the order, item and payment ledger amounts and checks identity uniqueness. The mixed run also requests the static menu through the HTTP router and measures against its own pinned baseline. `COMMERCE_BENCH_SECONDS` shortens smoke runs.
- The outbox test drains a 100,000-event backlog through two no-op consumers and measures publication lag under steady-state writes. `COMMERCE_OUTBOX_BACKLOG` selects a smaller smoke backlog. Crash recovery is tested separately.
- Web Push measures encryption, signing, durable claim and acknowledgement with a synthetic provider returning after 100 ms. A separate checkout stream measures p95 with and without active sends. Consumer gateway latency remains a later-phase benchmark because Commerce Core records offline payments.

Rollback smoke test on 2026-11-25 applied migrations `20261125000015` through `20261125000018` on a temporary database initialized by Increment 2. `integrity_check` and `foreign_key_check` passed. Both release binaries returned HTTP 200 for `/readyz` and the public demo. Demo HTML matched after normalizing the build version in static asset URLs.

The full-size outbox run on 2026-11-25 published 43,200 of 100,000 events before it was stopped during local I/O contention. On one pinned logical CPU with SQLite `FULL`, sampled windows recorded 103.3 and 27.5 published events/s. This run used the optimized cursor before the final Commerce write queue change. It missed the 1,000 events/s launch gate.

The final outbox small-backlog run reached 674 events/s across 200 queued events and 136 events/s across 2,000 ongoing events. The steady-state publication lag was 515 ms at p95 and 613 ms at maximum.

The two 10-minute checkout runs used one pinned logical CPU each and shared an ext4 disk with other workloads. Both completed without surfaced SQLite errors, duplicate orders or monetary discrepancies after the Commerce write queue was added.

| Checkout workload | Orders | Replays | Orders/s | Checkout p95 | Checkout p99 |
|---|---:|---:|---:|---:|---:|
| Orders and staff transitions | 2,391 | 320 | 3.90 | 279 ms | 620 ms |
| Orders, staff transitions and static menu reads | 2,308 | 260 | 3.78 | 1,315 ms | 3,862 ms |

The mixed run served 53,209 static menu reads. The menu p95 was 0.271 ms, or 1.74× the 0.155 ms pinned baseline. Both local checkout runs missed the 30 orders/s sustained load gate.

The Web Push benchmark completed all 1,000 synthetic deliveries in 20.747 seconds (2,892/min) after batching outcome writes. Peak concurrency was eight. Checkout p95 was 313 ms in the baseline and 102 ms with sends. The host's changing disk contention prevents attributing that difference to the push workload.

Memory: 14.6 MB RSS idle, 61.4 MB after load. Database scaling with `bench-gen` matched the baseline (100 000 establishments: 768.9 MB, 19.5 s).

Query plans: the entitlement view and the rebuilt `billing_coverage` view force an index-driven join order (`commercial_contracts` or `billing_checkouts` by establishment, then `idx_billing_payments_subscription`). Without it SQLite scanned every payment of the environment on each request, a cost that grows with the customer base. Contract periods and grants are read in one `UNION ALL` round trip. The enforced dashboard still performs one extra pooled query per request; caching the decision per request batch is unnecessary at current traffic. The enforced script lives in the session notes, not in `scripts/`, because it needs Asaas-shaped configuration.

### VPS capacity validation

Run the Commerce Core qualification on the VPS in a dedicated checkout and temporary directory on persistent storage. The fixtures create their own SQLite files. Keep `TMPDIR` on the same persistent filesystem for all scenarios, since a tmpfs run cannot measure durable writes. Run one workload at a time and record the CPU, memory limit, build commit, filesystem and pressure during the run.

Set `COMMERCE_BENCH_CPU` to one allowed CPU and `COMMERCE_BENCH_MEMORY_MAX=1G` to cap the benchmark process in a systemd scope with swap disabled. The scope applies to the executable and leaves the build outside the runtime memory limit. Run `scripts/bench_commerce.sh checkout`, `mixed`, `outbox` and `push` separately, using the default 600-second checkout duration and 100,000-event backlog for qualification.

Checkout reports new placements and replays separately. It reconciles each order against its items and exactly one pending collection record. The mixed workload measures `/c/{slug}` through the application router on the same host. Storefront traffic and public checkout HTTP status are validated when QR Delivery exposes those routes. The outbox benchmark checks the full backlog throughput and the maximum publication delay during the steady phase.

The VPS baseline on 2026-11-25 used `b7c0793`, ext4, one pinned CPU and SQLite `FULL`. The 90-second checkout sample placed 3,481 orders and replayed 409 requests. Including staff drain, throughput was 36.36 orders/s and combined request p95/p99 were 37.76/339.29 ms. The outbox baseline had published 39,400 of 100,000 events after 292 seconds, with 99% utilization of the pinned CPU and near-zero disk pressure at the observation. The baseline was stopped after it exceeded the time implied by the 1,000 events/s target. These baseline measurements used the host's available memory. The candidate runs cap memory separately.

Candidate `24a42a1` ran on the same VPS with CPU 3 pinned, a 1 GiB memory cap, swap disabled, and SQLite WAL with `synchronous=FULL`. The workloads ran sequentially against separate synthetic fixtures. Formatting, strict Clippy and 185 tests passed on the VPS, including replays during concurrent writes and rollback across consumers after dispatch interruption.

| 600-second workload | Orders | Replays | Orders/s | Checkout p95 / p99 | Replay p95 / p99 |
|---|---:|---:|---:|---:|---:|
| Orders and staff transitions | 39,290 | 4,416 | 65.32 | 14.615 / 26.230 ms | 0.576 / 1.204 ms |
| Orders, staff transitions and static menu reads | 41,566 | 4,672 | 69.17 | 14.174 / 25.907 ms | 0.553 / 0.942 ms |

Both runs passed the order throughput and latency gates and completed with zero errors, duplicate orders or monetary discrepancies. The mixed run served 52,933 static menu reads with p95 of 0.798 ms, 1.344× the pinned reference of 0.594 ms, within the 2× limit.

The outbox published all 100,000 queued events for two no-op subscribers in 9.610 seconds (10,406.15 events/s). The steady run published 2,000 events at 200/s, with p95 lag of 216 ms and maximum lag of 255 ms.

Web Push delivered 1,000 synthetic notifications in 13.693 seconds (4,381.71/min), with at most eight sends in flight. Observed checkout p95 rose from 6.295 ms in the reference run to 9.063 ms with active sends, within the benchmark's 300 ms latency budget.

### VPS replay validation

The admin replay candidate passed formatting, strict Clippy, 187 functional tests and a locked release build on the VPS on 2026-11-25. In the isolated HTTP check, an admin scheduled an original dead-letter event (`303`), and the duplicate submission received `409`. The dispatcher recorded two receipts for that event and its parked successor. The previous release `b7c0793` started against the database with migration `20261125000019`, served the demo and `/readyz` with HTTP 200, and retained the single audit record. SQLite integrity and foreign key checks passed.

The outbox release benchmark used the same pinned CPU, 1 GiB memory cap without swap and persistent SQLite WAL with `synchronous=FULL`. It drained 100,000 events with two consumers in 9.236 seconds (10,827.35 events/s). At 200 events/s in steady state, the p95 publication delay was 211 ms and the maximum was 220 ms.

### VPS catalog, checkout and expiry validation

The combined candidate passed formatting, strict Clippy, 193 functional tests and a locked release build on the VPS on 2026-11-26. The catalog regressions check immutable history, tenant and channel scoping, and concurrency on repeated availability updates. The checkout regressions check repricing when a fee and discount move by the same amount, idempotent retries and the legacy request digest. The expiry regression injects failures across an entire batch and verifies that a healthy order advances before later-eligible retries. It then restores the operation and checks each deferred order's history and event.

The isolated HTTP test upgraded a database from migration `20261125000019` to `20261126000020`. The candidate and both previous binaries (migration lists through 19 and 18) served the demo and `/readyz` with HTTP 200. SQLite integrity and foreign key checks passed after the rollback test.

The 600-second mixed benchmark exercised full quote confirmation in release on CPU 3, with a 1 GiB memory cap, zero swap and SQLite WAL using `synchronous=FULL` on ext4. With 200 establishments and 64 clients, it placed 50,182 orders and replayed 5,631 requests. Including staff transitions, throughput was 83.51 orders/s. Checkout p95 and p99 were 11.888 ms and 20.632 ms. Replay p95 and p99 were 0.494 ms and 0.779 ms. There were zero errors, duplicate orders or monetary discrepancies.

The mixed workload served 53,113 static menu reads. Menu p95 was 0.718 ms, or 1.864× the pinned baseline of 0.385 ms, within the 2× limit.

## QR Delivery internal preview capacity

Run `COMMERCE_BENCH_CPU=<cpu> COMMERCE_BENCH_MEMORY_MAX=1G ./scripts/bench_commerce.sh delivery` on Linux. Set `TMPDIR` to a directory on persistent storage. The executable creates its own synthetic database and writes with SQLite WAL and `synchronous=FULL`. The scope pins the process to one CPU, caps memory to 1 GiB and disables swap. The release build runs outside that scope.

The default 600-second scenario uses 200 establishments and 64 checkout clients. It alternates delivery and pickup, quotes current prices, exercises idempotent retries and progresses each order through three real staff requests. In-process HTTP requests use the Axum router. The two static readers run as runtime tasks in both the read-only baseline and the mixed scenario. The report includes staff queue time, processing time and completions by minute. After timing ends, it reconciles orders, payments and checkout attempts, waits for outbox publication and verifies notification consumer receipts.

The thresholds are those of Commerce Core §13 and QR Delivery §21. The run requires at least 30 new orders/s, checkout p95 at most 300 ms and p99 at most 1 s, quote p95 at most 50 ms, static p95 at most twice its own baseline, and zero failed or duplicate writes or monetary discrepancies. The request timings cover application routing and storage, including the static file path used by the native VPS app. Caddy, network and CDN timings require a separate measurement.

Use `COMMERCE_BENCH_SECONDS=15` for a short smoke run. It exercises the routes and reconciliation and reports measurements. Use the full duration and target host for qualification.

### Local comparison on 2026-11-27

Two 30-second release smoke runs used 200 synthetic establishments, 64 clients, logical CPU 4, a verified 1 GiB cgroup limit with swap disabled, and ext4 with SQLite WAL/FULL. The candidate reads the schedule and published catalog once per checkout transaction and reuses them for price verification and order creation.

| Sample | Baseline p95 / p99 | Candidate p95 / p99 |
|---|---:|---:|
| Quote | 9.602 / 16.841 ms | 1.482 / 17.641 ms |
| Checkout | 461.633 / 473.115 ms | 101.713 / 137.452 ms |
| Idempotent replay | 0.484 / 0.796 ms | 0.515 / 1.139 ms |
| Static menu under load | 0.362 / 0.585 ms (1.401× p95) | 0.319 / 0.570 ms (1.368× p95) |
| Static storefront under load | 0.244 / 0.401 ms (1.672× p95) | 0.229 / 0.350 ms (1.614× p95) |

The baseline reconciled 1,248 orders, split equally between delivery and pickup. The candidate reconciled 1,514 orders, including 756 deliveries and 758 pickups, 192 replays, 1,514 pending offline payments, 4,542 staff transitions and 6,056 order events. Including the staff queue drain, throughput rose from 40.54 to 45.92 completed orders/s. Peak memory was 77.5 MiB in the baseline and 80.7 MiB in the candidate. Both samples reconciled without discrepancies.

### Sustained local run on 2026-11-27

The 600-second candidate run used the same CPU, memory limit and persistent SQLite settings. It reconciled 9,795 orders (4,897 deliveries and 4,898 pickups), 1,091 replays, 9,795 pending offline payments, 29,385 staff transitions and 39,180 order events without duplicate or inconsistent records.

| Gate | Sustained result | Outcome |
|---|---:|---|
| Completed orders/s ≥ 30 | 16.02 over 611.24 s including drain | Below threshold |
| Quote p95 ≤ 50 ms | 3.747 ms | Passed |
| Checkout p95 ≤ 300 ms and p99 ≤ 1 s | 97.047 ms p95 and 408.501 ms p99 | Passed |
| Static menu p95 ≤ 2× baseline | 0.765 ms, 3.033× baseline | Above threshold |
| Static storefront p95 ≤ 2× baseline | 0.516 ms, 3.353× baseline | Above threshold |

The benchmark failed the sustained capacity gate. Qualification on the target host remains open.

### Sustained local follow-up on 2026-11-28

The optimized candidate ran for 600 seconds on a shared local machine with CPU 4 pinned, a 1 GiB cgroup limit, swap disabled and ext4 with SQLite WAL/FULL. The adjusted runner scheduled static readers the same way in both phases. The host experienced I/O pressure during the run.

| Gate | Sustained result | Outcome |
|---|---:|---|
| Completed orders/s ≥ 30 | 13.17 over 605.12 s including drain | Below threshold |
| Quote p95 ≤ 50 ms | 3.915 ms | Passed |
| Checkout p95 ≤ 300 ms and p99 ≤ 1 s | 108.456 ms p95 and 814.764 ms p99 | Passed |
| Static menu p95 ≤ 2× baseline | 0.573 ms, 2.254× baseline | Above threshold |
| Static storefront p95 ≤ 2× baseline | 0.539 ms, 2.186× baseline | Above threshold |

The run reconciled 7,972 orders (3,986 delivery and 3,986 pickup), 896 replays, 7,972 pending offline payments and 23,916 staff transitions without duplicates or monetary discrepancies. The outbox published 31,888 order events and recorded 7,972 notification consumer receipts. Staff queue wait p95 was 19.12 s and staff processing p95 was 163.196 ms per order. Staff completions ranged from 411 in the first minute to 1,100 in the last full minute.

The sustained capacity gate remains open. Qualification requires a run on the target VPS, including its actual static routing path.
