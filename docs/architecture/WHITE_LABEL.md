# White-Label Architecture

| Field | Value |
|---|---|
| Status | Architecture strategy. Brand theming of the public menu is implemented. Everything else in this document is **planned**. |
| White-label PWA | Phase 4, together with the QR Delivery storefront (`/s/{slug}`) |
| Native apps | Not started. Kotlin Multiplatform later. Native development is out of scope until phase 12 (`native_apps` add-on, `custom_quote`). |
| Custom domains | Add-on `custom_domain`, capability `branding.custom_domain`, operation `branding.custom_domain.configure` (owner only, `after_expiry: deny`), R$ 14,90 per month (`1490` centavos), `planned`, not affiliate eligible |
| Related contracts | [modules.json](../../contracts/commercial/modules.json), [addons.json](../../contracts/commercial/addons.json), [capabilities.json](../../contracts/entitlements/capabilities.json), [authorization-rules.json](../../contracts/entitlements/authorization-rules.json), [delivery.json](../../contracts/modules/delivery.json), [components.json](../../contracts/core/components.json). See also the [module catalog](../business/MODULE_CATALOG.md). |

White-label means that each establishment's public experience looks like its own brand, while every establishment runs on the same binary, the same database and the same deploy. Personalization is configuration plus snapshot data. It is never a per-establishment codebase.

## 1. Principles

1. **One codebase, one binary, one SQLite database.** There are no per-establishment branches, forks, builds, templates, scripts or databases, and no Node runtime, Redis, Kafka or microservices.
2. **Brand is tenant data.** Every brand attribute is a validated column or row scoped by `establishment_id`, edited through the same tenant-checked handlers as the catalog.
3. **Public is static, transactional is authoritative.** Read-only public content is pre-rendered into snapshots served by Caddy or a CDN with zero database queries. Anything that creates or changes state goes to the Axum backend.
4. **Strict CSP survives personalization.** A brand never injects inline CSS, inline JavaScript, custom HTML or external fonts. Brand values become CSS custom properties in a generated stylesheet.
5. **Capabilities decide features.** Paid brand features, such as a custom domain, are gated by entitlements (`authorize`), never by plan names or per-tenant flags.
6. **Isolation by construction.** Caches, cookies, client storage, service workers and identities are scoped so that two establishments can never share or mix state.

## 2. Personalization model

| Attribute | Status | Source of truth | Where it renders |
|---|---|---|---|
| Business name | Implemented | `establishments.name` | Snapshot HTML, JSON-LD, QR print sheet, (planned) manifest `name` |
| Logo | Implemented | `media_assets` (`kind = 'logo'`, PNG 512/192, EXIF stripped) | Snapshot `media/`, QR sheet, (planned) app icons |
| Brand color | Implemented | `establishments.accent_color` (`#RRGGBB`) | `theme-{hash8}.css`, `/dashboard/theme.css`, (planned) manifest `theme_color` |
| Icon | Planned | Derived from the logo: 192, 512 and maskable 512 PNG with safe-zone padding | Per-establishment manifest, `apple-touch-icon` |
| Cover image | Implemented (automatic) / planned (explicit) | First visible, available product photo (JPEG 800), or an illustrated cover in the brand color. An explicit cover upload is planned. | Snapshot header |
| Visual identity | Planned | Curated presets (corner radius, surface tone, typography scale) chosen from a fixed list | Generated theme stylesheet |
| Public address | Partial | Google Maps profile and review links (validated Google hosts) are implemented. A structured postal address is planned. | Snapshot, JSON-LD `address` |
| Business hours | Planned | QR Core `business_hours` (weekly hours, exceptions, holidays) | Snapshot (display) and backend (enforcement) |
| Service modes | Planned | QR Core `commercial_settings` (dine-in, pickup, delivery) | Storefront snapshot (display) and backend (enforcement) |
| Commercial settings | Planned | `commercial_settings` (minimum order, fees, checkout options) | Storefront snapshot (display) and backend (enforcement) |
| Contracted features | Implemented (engine) | Derived by the Entitlement Engine from `commercial_contracts` and `entitlement_grants` | A public projection only (for example "ordering available"). Never trusted from the client. |
| Languages | Implemented | Owner translations (`*_en`, `*_es`) and the platform catalogs | `data-pt` / `data-en` / `data-es` and embedded `#qr-menu-i18n` |

Rules:

- New brand attributes extend existing establishment-scoped records or new `establishment_*` tables. They follow expand-contract migrations (ADR-010) and full i18n parity.
- Every attribute has a domain validator in `src/domain/`, like `AccentColor`, `Slug` and `validate_google_maps_link`.
- There is no free-form CSS, HTML, JavaScript, SVG upload or third-party font URL. Visual freedom is limited to tokens and presets the platform can test.

### Configuration lifecycle

| Step | Behavior | Gate |
|---|---|---|
| Edit | The owner or manager changes a brand attribute. The change is saved as draft state and sets `has_unpublished_changes = 1`. | Free QR Core operation (`menu.draft.edit` today) |
| Preview | `/dashboard/preview` and `/dashboard/theme.css` render the draft brand, `noindex`, `no-store`. | Membership only |
| Publish | The brand is frozen into the channel snapshot (theme file, icons, manifest, settings JSON). | `menu.publish` for `/c/{slug}`; `delivery.storefront.publish` for `/s/{slug}` (planned) |
| Expiry | The last published brand keeps serving. New brand edits stay in the draft until a capability returns. | Per channel `after_expiry` |
| Suspension | Same as expiry for brand changes: the `suspension` rule blocks only `deny` operations (`menu.publish`, `delivery.storefront.publish`, `branding.custom_domain.configure`). Draft edits, preview and unpublishing stay available. | `OperationalStatus::blocks` |
| Custom domain | Activated or removed independently of publication. | `branding.custom_domain.configure` (owner only), which requires `branding.custom_domain` |

Because the brand is part of the snapshot, a brand change never reaches consumers before publication. Two channels of the same establishment (menu and storefront) can therefore show different brand versions until both are republished. The dashboard must show this per channel.

## 3. How brand theming works today

1. `AccentColor::parse` accepts only `#RRGGBB` and rejects anything else when the owner saves. If a stored value ever fails to parse, `generate_brand_theme_css` falls back to the default `#D9532F`.
2. `contrast_ink()` computes the WCAG relative luminance of the accent color and returns `#1C1917` above `0.34`, or `#FFFFFF` otherwise. `hover_hex()` darkens each channel to 85%. `soft_tint_rgba()` derives a translucent tint.
3. `generate_brand_theme_css` (`src/rendering/public_menu.rs`) emits a single rule:
   `:root{--brand-accent;--brand-accent-hover;--brand-accent-ink;--brand-soft}`.
   It names the file `theme-{hash8}.css` after the first 8 hex characters of the SHA-256 of its content.
4. Publication stages that file into `published/{slug}/` with `Cache-Control: public, max-age=31536000, immutable`. A color change produces a new file name.
5. The owner dashboard and the draft preview load `/dashboard/theme.css`. It is generated per request from `TenantContext` and sent with `no-store`.
6. Shared stylesheets in `static/css/` only consume these variables. Snapshots reference them as `/static/*?v={ASSET_VERSION}` (`src/rendering/assets.rs`), and the version is written into the HTML when it is rendered. A shared visual refresh therefore reaches existing snapshots only when they are re-rendered, which `republish-all` does after each deploy. `republish-all` re-renders from the current draft, so it skips every establishment with unpublished draft changes; those snapshots keep the previous markup and asset version until the owner republishes. Re-rendering from a persisted per-publication snapshot, so they also receive template upgrades, is the planned follow-up ([QR Menu](../modules/MENU.md), section 24).

This pattern is the template for every future brand attribute: validated value → generated, content-hashed stylesheet or JSON → referenced by the snapshot.

## 4. App strategy

| Stage | Phase | Scope | Status |
|---|---|---|---|
| Responsive web (menu, dashboard) | 1 | Server-rendered, minimal JavaScript, zero-reload i18n | Implemented |
| White-label PWA storefront | 4 | Installable `/s/{slug}` with a per-establishment manifest, icons, theme color and a service worker scoped to that establishment | Planned |
| Merchant operations app | Later | Orders, tables and kitchen for staff. Kotlin Multiplatform over the versioned API. | Not started |
| Courier app | Later | QR Fleet deliveries, proof of delivery | Not started |
| Exclusive restaurant apps | Phase 12 or later | Store-listed app under the establishment's brand. `native_apps` add-on, custom quote, only when commercially justified. | Not started |

There is no native work now. The PWA must reach feature parity for consumers before any native app is considered. Native apps are built only from the same configuration and snapshots, never as per-restaurant forks. One shared native codebase is parameterized at build time by establishment configuration, and only for the exclusive-app add-on.

## 5. Versioned API contracts for native and PWA clients

- Public transactional APIs are versioned in the path: `/api/storefront/v1/...` (Delivery), `/api/tables/v1/...` (Tables). The partner API lives in QR Connect (`/api/v1/openapi.json`). All of them are planned.
- Web, PWA and native clients call the same endpoints, under the same `authorize` evaluation order and tenant checks. Endpoints specific to one app, with weaker rules, are forbidden.
- Within a version, changes are additive only: new optional fields and new endpoints. Removing or renaming a field, or changing its semantics, requires `v{n+1}` and a published deprecation window.
- Data rules: money is integer centavos (`*_centavos`), identifiers are opaque strings, timestamps are RFC 3339, and catalog snapshots carry an explicit version (`/s/{slug}/catalog.v{version}.json`).
- Authentication: staff apps use OAuth 2.1 with PKCE S256 and resource-bound tokens, reusing the MCP authorization server pattern. Consumer flows use tokens bound to a single establishment, or to a single session for Tables. Credentials are never shared across establishments.
- OpenAPI documents list implemented endpoints only (a QR Connect release gate).

## 6. Routing evolution

| Route | Purpose | Status | Rules |
|---|---|---|---|
| `/c/{slug}` | Permanent read-only menu (QR Menu) | Implemented | Printed QR Codes depend on it. It never moves, never redirects to another channel and never changes meaning. The slug is locked after the first publication. |
| `/s/{slug}` | Ordering storefront (QR Delivery) | Planned | A separate snapshot directory and a separate `CatalogPublished` channel (`storefront`). |
| `/t/{table_token}` | Table entry (QR Tables) | Planned | An opaque token, independent of the slug. `noindex`. |
| `{slug}.qrcardapio.com.br` | Establishment subdomain | Planned | Alias of the path routes over the same snapshots. Needs a wildcard certificate (DNS-01). |
| Custom domain | `branding.custom_domain` add-on | Planned | See below. Domain registration is not included. Requires a module with a public menu or storefront. |

### Custom domains

1. **Verification**: the owner adds the domain in the dashboard through `branding.custom_domain.configure` (owner only; denied after expiry and during suspension). The platform issues a random token and requires a DNS TXT record (for example `_qrcardapio.{domain}`) plus a CNAME (or A/AAAA) record pointing to the platform. A domain belongs to at most one establishment. Apex domains of the platform and lookalike names are refused.
2. **TLS with Caddy on-demand certificates**: `on_demand_tls` must use an `ask` endpoint. The endpoint is reachable only on the internal network, never publicly routed, and answers `200` only when all of these hold:
   - the domain is verified;
   - the establishment currently holds `branding.custom_domain`;
   - the establishment is not suspended.
   Issuance and renewal are treated like a `deny` operation. Without this gate, anyone who points DNS at the server can trigger unlimited certificate issuance, exhaust ACME rate limits and fill the disk. The ask endpoint is also rate limited, and issuance per establishment is capped.
3. **Host routing without the database**: the application maintains a host directory (for example `published-hosts/{domain}` → `published/{slug}`). It updates that directory atomically when a domain is verified or revoked, so Caddy serves custom hosts from disk with zero queries, exactly like `/c/{slug}`.
4. **Surface**: a custom domain serves only that establishment's public snapshot and its public APIs. Login, dashboard, admin, MCP and webhooks always stay on the platform origin.
5. **Canonical URL**: while a custom domain is active, the snapshot's canonical URL and JSON-LD point to it. `/c/{slug}` keeps serving for printed QR Codes. The canonical URL changes only at the next publication.
6. **Expiry or suspension**: the ask endpoint stops approving renewals, and the host directory entry is removed. The domain then shows a neutral page linking to `/c/{slug}`. `/c/{slug}` itself is never affected.
7. **Abuse**: brand names and domains can be used for impersonation or phishing. Reports lead to a security suspension through `establishment_operational_states` (`qrcardapio operator suspend --kind security`, event `EstablishmentSuspended`). Suspension does not take published snapshots offline; an operator unpublish command does not exist yet (section 15).

## 7. Isolation across establishments

### Cache isolation

- Every cache key includes host and path. A CDN must never ignore the `Host` header, and it must never cache a response that carries `Set-Cookie`, `Cache-Control: private` or `no-store`.
- Snapshot assets are content-addressed inside each establishment's directory (`published/{slug}/media/...`). The same image used by two establishments is stored twice, so deleting one never affects the other.
- Private media (`/dashboard/media/{filename}`) is resolved from `TenantContext` and sent with `private`. Transactional APIs send `no-store`.
- Service worker caches are named `qr:{establishment_id}:{snapshot_version}` and are deleted when a newer version activates.

### Identity and session isolation

- The owner and staff session (`qr_session`) and the CSRF token are host-only cookies on the platform origin. They never carry a `Domain` attribute, so they are never visible to subdomains or custom domains.
- `TenantContext` always resolves the establishment from `establishment_members` on the server. A token or cookie issued for one establishment never authorizes another.
- Consumer identities (planned `customers` component) are scoped to a single establishment. The same person at two restaurants has two unrelated records, and no profile spans establishments.
- If subdomains are adopted, cookies use the `__Host-` prefix, and listing the platform domain on the Public Suffix List is evaluated so that tenant subdomains are treated as separate sites.

### Client storage, service worker and manifest

- **No service worker exists today**, and the only client storage is the platform-wide `qr_lang` language preference.
- **Service worker scope**: `/s/{slug}/` on the platform origin, or `/` on a verified custom domain. It must never be registered at `/` on the platform origin, and `Service-Worker-Allowed` is never used to widen a scope. The worker script is served with `Cache-Control: no-cache`.
- **Manifest per establishment**: `/s/{slug}/manifest.webmanifest` with a stable `id`, plus `start_url` and `scope` under `/s/{slug}/`. It also carries `name` and `short_name` from the business name, `theme_color` from the accent color, `background_color` and icons derived from the logo. The current platform manifest (`/manifest.webmanifest`, `scope: "/"`) must be narrowed or reviewed so an installed platform app never captures tenant storefront links.
- **Client keys** are namespaced by `establishment_id`, never by slug or host alone. Examples: `localStorage` key `qr:{establishment_id}:cart:v1` and IndexedDB database `qr-{establishment_id}`. Namespacing is mandatory on the shared platform origin. Keep it on custom domains as well.
- **Carts never mix restaurants**. A cart stores `establishment_id` and `catalog_version`. On load, a cart whose `establishment_id` differs from the current page is ignored. Checkout sends only identifiers and quantities, and the server rejects items from another establishment.

## 8. Snapshot contents and delivery

| Artifact | Today (QR Menu) | Planned (storefront and PWA) | Cache policy |
|---|---|---|---|
| HTML entry point | `published/{slug}/index.html` | `published-storefront/{slug}/index.html` (or equivalent) | `public, max-age=60, stale-while-revalidate=300`, `ETag` |
| Versioned catalog JSON | Embedded in the HTML | `catalog.v{version}.json` (declared in `delivery.json`) | `immutable` |
| Branding | `theme-{hash8}.css` | Same, plus logo-derived icons | `immutable` |
| Public settings (hours, service modes, minimum order, fee display) | Not applicable | Versioned JSON next to the catalog | `immutable` |
| Manifest | Not applicable | `manifest.webmanifest` per establishment | short `max-age` (stable URL) |
| Shared static files (CSS, JS, icons) | `/static/*?v={hash}` | Same | `immutable` |
| Images | `media/{hash16}_{size}.{jpg|png}` | Same | `immutable` |
| Service worker | Not applicable | `sw.js` per scope | `no-cache` |

Caddy (or a CDN in front of it) serves every artifact above from disk, with zero SQLite queries. Transactional operations always reach the authoritative backend, are never cached and never run from the service worker cache: quotes, checkout, orders, order status, table joins, waiter calls and payments.

## 9. Device snapshots are hints, never authority

A snapshot on a device, in a CDN or in a service worker cache may be stale or tampered with. The backend never trusts it for these things:

| Concern | Authoritative check |
|---|---|
| Prices and totals | Recomputed on the server in integer centavos from the catalog version and the establishment's current settings. The request carries only `product_id`, options and quantities. |
| Availability | Revalidated when the order is created. A sold-out or hidden item returns `409` with current data. |
| Catalog version | The client sends `catalog_version`, and the server answers `409` with a fresh quote when it is stale (`OrderCreated` records `catalog_version`). |
| Permissions and entitlements | `authorize` runs on every write, so a lapsed module refuses orders even if the cached storefront still shows a button. |
| Business hours, service modes, minimum order, fees, coupons | Re-evaluated by the backend for every quote and order. |
| Order and payment state | Read from the server state machines. The client only displays them. Payments are confirmed only by authenticated gateway webhooks or by staff. |

## 10. Publication atomicity and partial failures

### Current pipeline (QR Menu)

1. Serialize per establishment (`PublishLocks`), then read the draft snapshot in a single short query.
2. Stage the media and `theme-{hash8}.css` through temporary file, `fsync` and rename. Replace `index.html` last, atomically.
3. Record the publication (`establishments`, `publications`) and append `CatalogPublished` in the same SQLite transaction.
4. Remove stale theme and media files after the new HTML is live.

| Failure point | Result |
|---|---|
| Before the `index.html` rename (missing media, disk full) | The previous snapshot keeps serving untouched. A `failed` publication row is recorded and no event is emitted. |
| After the rename, before the database record commits | The new HTML is live, but `published_version` is unchanged, `has_unpublished_changes` stays `1` and no `CatalogPublished` exists. The next publication converges, because content-addressed files are skipped and the version moves forward. |
| Cleanup of stale files fails | Orphan files remain. They are harmless and are removed by the next publication. |
| The outbox append fails | The whole recording transaction rolls back. An event never exists without its publication row. |

**Known limitation**: cleanup deletes the previous `theme-*.css` and media files right after the HTML switch. A browser or CDN may still hold the previous HTML for up to `max-age + stale-while-revalidate` (360 s), so it can request a deleted asset and get a `404`. Multi-artifact snapshots must keep the previous version's immutable files for at least that window.

### Multi-artifact snapshots (planned)

- Write order: immutable, versioned artifacts first (catalog JSON, settings JSON, theme, media, icons), then the mutable entry points (`manifest.webmanifest`, `index.html`), each through an atomic rename.
- The HTML references exact versioned artifact names. A reader therefore sees either the complete old set or the complete new set, never a mix.
- Each channel (`menu`, `storefront`) publishes into its own directory under its own lock. A failure in one channel never changes the other. `core.publishing` appends one `CatalogPublished` per channel publication, deduplicated by channel + establishment id + publication version (implemented for the menu as `CatalogPublished:menu:{establishment_id}:{version}`).
- Changing a custom domain mapping is independent of publication and never rewrites snapshots.
- A CDN purge after publication is best effort. If it fails, staleness only extends to the cache bounds above.

## 11. PWA caching and offline behavior (planned)

| Request class | Service worker strategy | Offline behavior |
|---|---|---|
| Immutable artifacts (versioned catalog and settings JSON, theme, media, `/static/*?v=`) | Cache first, keyed by full URL in the establishment's cache | Served from cache |
| HTML entry point and manifest | Network first, with a short timeout, falling back to the cached copy | Shows the last snapshot with an "offline, prices may have changed" banner |
| Transactional APIs (`/api/storefront/v1/...`, `/api/tables/v1/...`) | Never cached and never intercepted, except to fail fast | The action is disabled. No offline order queue. |
| Anything outside the scope | Not intercepted | Browser default |

- Orders are never queued for background sync. A replayed order could arrive after prices, availability or hours changed, or be submitted twice. The user retries explicitly, with the same `idempotency_key`.
- On activation, the worker deletes caches of older snapshot versions of the same establishment only. It never enumerates or touches another establishment's caches.
- If the snapshot returns `404` (unpublished) or the custom domain is revoked, the worker unregisters itself and clears its caches.

## 12. Request flows

```text
Public read (zero database)
  phone ── GET /c/{slug} | /s/{slug} | custom host ──▶ CDN? ──▶ Caddy ──▶ published/... on disk

Transactional write (authoritative)
  phone ── POST /api/{module}/v1/... ──▶ Caddy ──▶ Axum
           ──▶ tenant resolution ──▶ authorize(operation)
           ──▶ BEGIN IMMEDIATE: state change + domain_events append ──▶ COMMIT
           ──▶ JSON with integer centavos (no-store)

Custom domain certificate (on demand, planned)
  TLS ClientHello(host) ──▶ Caddy ──▶ internal ask?domain=host ──▶ Axum
           verified + branding.custom_domain + not suspended ? 200 : 403
           ──▶ ACME issuance (rate limited) ──▶ serve from published-hosts/{host}
```

The certificate check reads SQLite only at issuance or renewal, never per request. Public delivery stays database free on every host.

## 13. Security requirements specific to white-label

- Every brand input is validated by a domain type: colors are `#RRGGBB`, images are checked by magic bytes and re-encoded (no SVG uploads), owner text is escaped by Askama, and links are restricted to validated hosts.
- The strict CSP is identical on platform, subdomain and custom-domain responses: `script-src 'self'; style-src 'self'; frame-ancestors 'none'`.
- Name, slug and domain reservation lists block platform names. Impersonation reports go to security suspension.
- The custom-domain `ask` endpoint and host directory changes are audited through the outbox and are never publicly reachable.

## 14. Acceptance criteria and tests

| Criterion | Verification |
|---|---|
| Any `#RRGGBB` gets a contrasting ink color, and invalid input is rejected | Unit test `accent_color_contrast_selection` in `src/domain/establishment.rs` (implemented) |
| The public snapshot contains no inline style or script except inert JSON | `tests/seo_contracts.rs`, `tests/publishing_and_qr.rs` (implemented) |
| Two establishments with the same image and color share no file path or cache entry | Planned isolation test |
| A cart saved for establishment A is ignored on establishment B's storefront, on the same origin | Planned browser test |
| The service worker for `/s/{slug}/` intercepts nothing outside its scope and never caches `/api/*` | Planned browser test |
| The custom-domain `ask` endpoint refuses unverified, unentitled or suspended domains, and is unreachable from the internet | Planned integration test and Caddy configuration review |
| A tampered client price, availability or `catalog_version` is rejected or re-quoted by the backend | Planned contract tests for Delivery and Tables |
| A failure before the HTML rename leaves the previous snapshot serving | `tests/publishing_and_qr.rs` (implemented for QR Menu) |
| Previous immutable artifacts remain fetchable for at least 360 s after republication | Planned (current cleanup deletes them immediately) |
| `republish-all` leaves snapshots with pending draft changes untouched | Implemented in `republish_all`; regression test required |
| `branding.custom_domain.configure` is refused for managers, after expiry and during suspension | Planned authorization test |

## 15. Open decisions

- Subdomain adoption, and whether to list the platform domain on the Public Suffix List.
- The canonical URL policy when a custom domain lapses (revert at the next publication, or republish automatically).
- The retention window for previous snapshot artifacts, and whether to keep the previous HTML for instant rollback.
- Whether an administrative (non-security) suspension also removes the custom-domain host entry, given that the `suspension` rule only blocks `deny` operations and `/c/{slug}` stays online.
- An audited operator command to unpublish an abusive snapshot (today only members can unpublish).
- Whether exclusive native apps are store listings under the establishment's own developer account or the platform's.
