# Security Architecture

## Threat Model & Core Mitigations

| Threat | Mitigation | Implementation |
|---|---|---|
| Weak or leaked passwords | Argon2id (`m=19456, t=2, p=1`) + constant timing on unknown email via dummy verify; bounded `Semaphore(2)` + `spawn_blocking`. | `src/infrastructure/security/mod.rs` |
| Session hijacking / fixation | 32-byte CSPRNG tokens (`getrandom`), SHA-256 hashed in SQLite; rotated on every login/register; `HttpOnly`, `SameSite=Lax`, `Secure` (prod); 7-day expiry + hourly prune. | `src/application/auth.rs`, `src/http/middleware/auth.rs` |
| CSRF on state-changing requests | Per-session random `_csrf` token verified in constant time (`subtle::ConstantTimeEq`) on every authenticated `POST`. | `verify_csrf` in `src/http/middleware/auth.rs` |
| Cross-tenant IDOR | `TenantContext` resolves `establishment_id` strictly from `establishment_members`; all SQL queries filter `AND establishment_id = ?`; `insert/update_product` verifies `category_id` belongs to the same tenant. | `src/infrastructure/database/queries/*.rs` |
| Brute-force & registration spam | IP-keyed `governor` GCRA limiters (`/login` 10/min, `/register` 5/min + hidden honeypot, `/reset-password` 5/min), per-account upload limit (20/min), and per-account OCR limit (10/min). | `src/infrastructure/security/rate_limit.rs` |
| XSS & script injection | Askama auto-escaping (`&#60;`/`&#62;`/`&#34;`/`&#39;`), JSON `</` escaping, strict CSP (`script-src 'self'; style-src 'self'`). | `src/http/middleware/security_headers.rs` |
| Malicious image/PDF uploads / SVG XSS / EXIF leaks | 2 MiB logo / 5 MiB product / 10 MiB OCR photo / 15 MiB OCR PDF caps; magic-byte checks (JPEG/PNG/WebP/%PDF-); dimension bounds; EXIF-stripping decode+re-encode; isolated `TempDir` + `Semaphore(1)` + timeouts + no-shell subprocess invocation for `tesseract`/`pdftotext`/`pdftoppm`. | `src/infrastructure/assets/mod.rs`, `src/infrastructure/ocr/` |
| Path traversal | `resolve_safe_subpath` rejects `..`, `/`, `\`, leading `.`, null bytes; `Slug::validate_existing` restricts slugs to `[a-z0-9-]`. | `src/infrastructure/storage/mod.rs` |

## Account Recovery (Pilot Phase)

No external SMTP dependency is required for the MVP. An operator with server CLI access issues a single-use 1-hour recovery token:
```bash
qrcardapio admin reset-token --email owner@example.com.br
```
Redeeming the token at `/reset-password` updates the password hash and revokes all active sessions atomically.

## LGPD Data Minimization

Only the owner's email, hashed password, preferred locale, and public business menu data are collected. Customers scanning public QR Codes never create accounts, submit personal data, or trigger tracking cookies or database writes.
