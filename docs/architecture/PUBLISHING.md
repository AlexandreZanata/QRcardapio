# Static Menu Publication Architecture

## Why Static Publication?

Scanning a restaurant table QR Code is an overwhelmingly read-heavy event. Generating complete static snapshots at publish time ensures:

- **Zero SQLite queries** during ordinary public visits.
- **Predictable sub-millisecond TTFB** from Caddy or Axum disk delivery.
- **Resilience**: Even if the database or app restarts briefly, Caddy continues serving already-published menus.

## Directory Layout (`DATA_DIR/published`)

```text
data/published/
├── .tmp/                              # Private staging, product asset references, and publication locks
└── {slug}/
    ├── index.html                     # Escaped standalone HTML5 with embedded #qr-menu-i18n JSON
    ├── theme-{hash8}.css              # Content-hashed CSS variables for brand accent & contrast ink
    ├── p/{hash24}.html                 # Product details linked by the current snapshot
    └── media/
        ├── {logo16}_192.png           # Content-addressed establishment logo
        ├── {item16}_400.jpg           # Content-addressed product thumbnails
        └── {item16}_800.jpg           # Larger images for product pages and the cover
```

## Atomic Publication Lifecycle (`src/application/publishing.rs`)

1. Acquire the establishment's async mutex and an exclusive filesystem lock under `published/.tmp/publish-locks/`. Dashboard, MCP and CLI publications sharing the same data directory keep both locks until the publication completes. Other establishments can publish independently.
2. Validate `slug` (`Slug::validate_existing`) and resolve paths via `resolve_safe_subpath` (blocks path traversal).
3. Capture `MenuSnapshot` and the tenant's draft revision in one short SQLite read transaction. Filter to `is_visible = 1` categories and products, then release the DB connection before writing files. `republish-all` checks the publication and dirty flags on this captured state under the same per-establishment lock.
4. Generate `theme-{hash8}.css` (computing WCAG AA foreground ink color from `accent_color`).
5. Render each product page and hash its HTML with the product ID. Render `index.html` with links to those exact files. Askama escapes text, and both views embed `#qr-menu-i18n` for language switching. The public index carries the established canonical URL and Menu JSON-LD. Product pages have `noindex, follow`, and draft views have `noindex, nofollow`.
6. Stage referenced media into `published/{slug}/media/` with `atomic_write_file`. The gallery uses thumbnails, and each product page uses the larger variant of its image when available.
7. Stage `theme-{hash8}.css` and the content-addressed product pages. Store product asset references in the private staging directory for cleanup.
8. Atomically replace `published/{slug}/index.html` via `atomic_write_file`.
9. Record retirement of obsolete assets and prune any whose HTML cache window has elapsed.
10. Set `is_published`, update `published_version` and `last_published_at`, and append a `'published'` row in `publications`. Clear the dirty flag only if the captured draft revision still matches. Each mutation that marks a draft dirty advances `menu_draft_revisions` even when the flag was already set. Edits committed during rendering stay pending for the next owner publication. Because `/sitemap.xml` queries `is_published = 1`, newly published menus enter the sitemap immediately and leave it upon unpublishing.

Already-published menus keep serving their saved snapshot until they are republished. The deployment republish command updates menus with a matching live draft. Menus with pending edits get the new vitrine and product pages when an owner publishes.

An optional Google review invitation is rendered into the static HTML on publication. The direct review link takes precedence over the general Google Maps profile link. Removing both links and publishing removes the invitation. Public scans do not fetch any Google data or open the Google link until the visitor chooses it.

Owners and managers can opt in to public product options in the add-on groups dashboard. Publication captures the preference, available options and draft revision in the same SQLite read transaction. The product detail page shows variation prices and the additional charges for linked add-ons. It uses the same language and currency formatting as the menu. Saving the preference updates the authenticated product preview. The public snapshot reads that preference on the next publication, and automatic republication skips pending drafts.

Owner profile updates use the same per-establishment async mutex within the HTTP process. The update statement also checks the stored publication version so a form opened before publication cannot rename a published path.

## Public demonstration menu

`application/demo_menu` builds a release-owned snapshot for `/c/bistro-da-praca`. The startup seed checks for a conflicting tenant before writing and invokes the same atomic snapshot writer. It keeps catalog data in code, with photos embedded in the binary. This menu has no owner account or database-backed establishment, is excluded from the sitemap, and uses `noindex, follow` without structured business data. It serves the same static HTML, media and theme contract as a published tenant menu.

## Failure Recovery & Rollback

- If any error occurs before step 8's atomic rename (e.g., missing source media file or full disk), temporary files remain isolated in `.tmp/` or unused, a `'failed'` audit row is logged, and **the previous live `index.html` and its assets remain completely intact and serving**.
- Re-running `publish_establishment` is idempotent: content-addressed media files already on disk are skipped cleanly, and the next successful snapshot bumps `published_version`.

## Caching & CDN Propagation Bounds

- `index.html` and `p/{hash24}.html`: `Cache-Control: public, max-age=60, stale-while-revalidate=300` + `ETag`. The QR Code points to `/c/{slug}`, and product URLs are specific to their rendered content.
- `theme-{hash8}.css` & `media/{hash16}_*`: `Cache-Control: public, max-age=31536000, immutable` (new edits emit new content hashes).
- When files leave the live menu, the private retirement marker records when the cache window begins. Later publications prune superseded product pages after the cache window. Their photos and themes stay available while those pages are on disk and get a separate cache window after the pages are removed. Reusing a file clears its marker. Unpublish removes the menu, product pages, and retained assets together.

## Unpublishing

`unpublish_establishment` acquires both locks for the establishment and reloads its current publication version. It deletes `published/{slug}/`, sets `is_published = 0` and `has_unpublished_changes = 1`, and records `'unpublished'` in `publications`. Public requests to `/c/{slug}` immediately return `404`.

## Assinatura e arquivos em preparação

Com Asaas habilitado, a ação de publicar do painel verifica a cobertura da assinatura antes de renderizar o snapshot. A verificação se aplica à gravação de versões novas. A entrega dos snapshots existentes continua estática. No Caddy, `/c/.tmp` e seus descendentes retornam `404` inclusive durante uma publicação.

## QR Delivery storefronts

The ordering storefront lives in `DATA_DIR/storefronts/{slug}`, with its own publication lock and versions separate from the menu on `/c/{slug}`. Public HTML, manifest, worker, images and versioned catalog are served from disk, without SQLite. Consumer quotes and orders use the committed catalog plus current fulfillment rules on the server. Derived publications reuse published product content so changing Delivery settings cannot publish menu drafts.

The committed storefront presentation is retained in SQLite for recovery. `republish-all` restores eligible published storefronts from that version. The service worker uses tenant-scoped caches under `/s/{slug}/`. An offline storefront allows browsing and disables ordering. Transactional API requests and customer tracking data stay outside its cache.
