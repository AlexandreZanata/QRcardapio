# Static Menu Publication Architecture

## Why Static Publication?

Scanning a restaurant table QR Code is an overwhelmingly read-heavy event. Generating complete static snapshots at publish time ensures:

- **Zero SQLite queries** during ordinary public visits.
- **Predictable sub-millisecond TTFB** from Caddy or Axum disk delivery.
- **Resilience**: Even if the database or app restarts briefly, Caddy continues serving already-published menus.

## Directory Layout (`DATA_DIR/published`)

```text
data/published/
├── .tmp/                              # Temp files on the SAME filesystem for atomic rename(2)
└── {slug}/
    ├── index.html                     # Escaped standalone HTML5 with embedded #qr-menu-i18n JSON
    ├── theme-{hash8}.css              # Content-hashed CSS variables for brand accent & contrast ink
    └── media/
        ├── {logo16}_192.png           # Content-addressed establishment logo
        ├── {item16}_400.jpg           # Content-addressed product thumbnails
        └── {cover16}_800.jpg          # Optional cover from the first visible, available product with a photo
```

## Atomic Publication Lifecycle (`src/application/publishing.rs`)

1. Acquire per-establishment async mutex (`PublishLocks`) to serialize concurrent publishes for the same tenant.
2. Validate `slug` (`Slug::validate_existing`) and resolve paths via `resolve_safe_subpath` (blocks path traversal).
3. Load `MenuSnapshot` in a single short SQLite read (`load_menu_snapshot(pool, est_id, false)`), filtering to `is_visible = 1` categories and products, then immediately release the DB connection.
4. Generate `theme-{hash8}.css` (computing WCAG AA foreground ink color from `accent_color`).
5. Render `index.html` using `PublicMenuTemplate` (Askama auto-escapes all user text; embeds ~1.6 KB `#qr-menu-i18n` JSON for zero-reload PT/EN/ES switching; includes canonical URL, Open Graph metadata, and XSS-safe `FoodEstablishment` + `Menu` JSON-LD for published snapshots while keeping `/dashboard/preview` `noindex, nofollow`).
6. Stage all referenced media files into `published/{slug}/media/` via `atomic_write_file` (`NamedTempFile::new_in(tmp_dir)` -> `write_all` -> `sync_all` -> `persist`). Stage the large variant of the selected cover alongside the product thumbnails.
7. Stage `theme-{hash8}.css` via `atomic_write_file`.
8. Atomically replace `published/{slug}/index.html` via `atomic_write_file`.
9. Update `establishments` (`is_published = 1`, `published_version += 1`, `has_unpublished_changes = 0`, `last_published_at`) and append a `'published'` row in `publications`. Because `/sitemap.xml` queries `is_published = 1`, newly published menus enter the sitemap immediately and leave it upon unpublishing.
10. Clean up obsolete `theme-*.css` and unreferenced `media/*` files **only after** `index.html` is live.

Already-published menus pick up the visual refresh through the shared stylesheet. They receive a photo cover during their next normal publication.

An optional Google review invitation is rendered into the static HTML on publication. The direct review link takes precedence over the general Google Maps profile link. Removing both links and publishing removes the invitation. Public scans do not fetch any Google data or open the Google link until the visitor chooses it.

Owner profile updates use the same per-establishment lock as publishing. The update statement also checks the stored publication version so a form opened before publication cannot rename a published path.

## Public demonstration menu

`application/demo_menu` builds a release-owned snapshot for `/c/bistro-da-praca`. The startup seed checks for a conflicting tenant before writing and invokes the same atomic snapshot writer. It keeps catalog data in code, with photos embedded in the binary. This menu has no owner account or database-backed establishment, is excluded from the sitemap, and uses `noindex, follow` without structured business data. It serves the same static HTML, media and theme contract as a published tenant menu.

## Failure Recovery & Rollback

- If any error occurs before step 8's atomic rename (e.g., missing source media file or full disk), temporary files remain isolated in `.tmp/` or unused, a `'failed'` audit row is logged, and **the previous live `index.html` and its assets remain completely intact and serving**.
- Re-running `publish_establishment` is idempotent: content-addressed media files already on disk are skipped cleanly, and the next successful snapshot bumps `published_version`.

## Caching & CDN Propagation Bounds

- `index.html`: `Cache-Control: public, max-age=60, stale-while-revalidate=300` + strong `ETag`. When placed behind an optional CDN, maximum staleness after an owner republishes is **60 seconds** (or immediate if purged on publish in a future extension).
- `theme-{hash8}.css` & `media/{hash16}_*`: `Cache-Control: public, max-age=31536000, immutable` (new edits emit new content hashes).

## Unpublishing

`unpublish_establishment` acquires the per-slug mutex, deletes `published/{slug}/`, sets `is_published = 0` and `has_unpublished_changes = 1`, and records `'unpublished'` in `publications`. Public requests to `/c/{slug}` immediately return `404`.

## Assinatura e arquivos em preparação

Com Asaas habilitado, a ação de publicar do painel verifica a cobertura da assinatura antes de renderizar o snapshot. A verificação se aplica à gravação de versões novas. A entrega dos snapshots existentes continua estática. No Caddy, `/c/.tmp` e seus descendentes retornam `404` inclusive durante uma publicação.
