# ADR-006: Local Hybrid OCR & PDF Menu Pre-Registration Pipeline

- **Status**: Accepted
- **Date**: 2026-11-23

## Context

Restaurant owners migrating from printed menus, WhatsApp flyers, or Canva PDFs often need to enter 30–120 products before publishing their first digital menu. Typing every item manually creates onboarding friction. At the same time, QR Cardápio targets a 1 vCPU / 1 GB RAM VPS with zero recurring per-page cloud AI API bills, zero external data egress (LGPD), and strict durability guarantees where OCR errors must never reach live customer QR menus automatically.

## Decision

Implement a **local, zero-API-cost, hybrid OCR & PDF ingestion pipeline** with mandatory human staging & review:

1. **Hybrid Extraction (`src/infrastructure/ocr/`)**:
   - **Digital PDFs (fast path)**: Execute `pdftotext -layout -enc UTF-8 -f 1 -l 8` (15 s timeout). When selectable text yields >= 40 alphanumeric characters, bypass rasterization completely (~15 ms, ~100% accuracy).
   - **Scanned PDFs (fallback)**: Rasterize up to 8 pages at 220 DPI via `pdftoppm -png`, run pure-Rust image preprocessing, and run Tesseract per page.
   - **Photos (JPG/PNG/WebP)**: Validate magic bytes & dimensions (`48..8000 px`), preprocess in `spawn_blocking` via `image` (EXIF strip, grayscale, 2nd–98th percentile contrast stretch, mild 3×3 sharpen, optimal rescale), and invoke `tesseract <tmp.png> stdout -l <por+eng+spa> --oem 1 --psm 6` (30 s timeout).
2. **Subprocess Isolation & Bounded Resources**:
   - Use `tokio::process::Command` with direct argument arrays (zero shell invocation), `kill_on_drop(true)`, isolated `TempDir` per job, and a process-wide `Semaphore(1)` concurrency cap so peak memory on a 1 GB VPS never spikes and OS memory is reclaimed immediately upon child exit.
   - Protect extraction endpoints with a per-account `governor` rate limit (`10/min`) and `16 MiB` multipart body cap (`10 MiB` image / `15 MiB` PDF).
3. **Pure-Rust Menu Parser (`src/domain/ocr_parser.rs`)**:
   - Parses Brazilian/EN/ES menu layouts: dotted leaders, `R$`/`$`/`BRL` and common OCR digit substitutions (`RS 1O,5O`), multi-size pipe variants (`300ml R$ 15,00 | 500ml R$ 22,00`), standalone price lines, multi-line descriptions, category heading heuristics, noise filtering (URLs, phones, Wi-Fi), and per-item confidence scoring (`high`, `medium`, `low`).
4. **Staging Persistence & Atomic Draft Commit (`ocr_imports`)**:
   - Store each extraction in `ocr_imports` (`status = 'review'`).
   - Present an interactive review table (`/dashboard/ocr/{id}`) where owners edit categories, names, descriptions, and integer centavos prices, add/remove rows, or discard.
   - Confirming commits all selected items inside a single `BEGIN IMMEDIATE` SQLite transaction, optionally reusing existing categories, setting `establishments.has_unpublished_changes = 1`, and marking the import `applied`. Public static menus remain untouched until explicit publish.

## Consequences

- **Pros**: Zero per-page external AI/cloud costs; works offline on a single 1 GB VPS; instant high-accuracy imports from Canva/Word PDFs; safe memory reclamation; clean extension seam in `OcrService` if optional cloud Vision APIs are ever offered as a future paid tier.
- **Cons**: Runtime container image grows slightly to include `tesseract-ocr`, language packs (`por`, `eng`, `spa`), and `poppler-utils`; complex multi-column decorative photo menus may require quick inline edits in the staging table.
