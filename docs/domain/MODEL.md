# Domain Model & Invariants

## Entities

1. **Account (`src/domain/account.rs`)**
   - `email`: Normalized trimmed lowercase ASCII, unique `COLLATE NOCASE`, 5–254 chars.
   - `password`: 8–128 Unicode characters, stored strictly as Argon2id PHC string.
   - `role`: `'owner'` | `'admin'`.
   - `preferred_locale`: `'pt-BR'` | `'en'` | `'es'`.

2. **Establishment (`src/domain/establishment.rs`)**
   - `slug`: Normalized lowercase ASCII `^[a-z0-9]+(?:-[a-z0-9]+)*$` (length 3–64); diacritics stripped (`Açaí do João` -> `acai-do-joao`). Reserved system prefixes (`admin`, `api`, `c`, `dashboard`, `login`, `register`, `static`, etc.) are rejected.
   - **Automatic link**: A blank address in onboarding derives a base of at most 32 ASCII characters from the display name. A name shorter than three characters or a reserved word uses a `-loja` ending. A name with no usable characters uses `loja`. If the base is taken, a hyphen and four base-32 characters distinguish the address (e.g. `bar-k7m2`). The automatic slug is at most 37 characters. Owners can choose a custom address up to 64 characters before publication.
   - **Slug Immutability Rule**: Once `published_version > 0`, `slug` cannot be modified via profile updates so printed table QR Codes remain valid forever.
   - `accent_color`: Validated `#RRGGBB` hex string; computes WCAG relative luminance to select `#FFFFFF` or `#1C1917` foreground ink (`contrast_ink()`).
   - `description`, `description_en`, `description_es`: Up to 280 chars; EN/ES fall back cleanly to primary `description` when empty.

3. **Category (`src/domain/category.rs`)**
   - Belongs to one `establishment_id`.
   - Ordered explicitly via integer `position` within the establishment (`move_category_for_est` swaps positions atomically).
   - `is_visible`: Controls inclusion in published static menus (draft preview still displays categories so owners can inspect them).
   - Optional `name_en`, `name_es`, `description_en`, `description_es`.

4. **Product (`src/domain/product.rs`)**
   - Belongs to one `establishment_id` AND one `category_id` (verified to belong to the SAME `establishment_id`).
   - `price_centavos`: Non-negative integer (`0..=99_999_999`). Parsed purely via integer string parsing (`parse_price_centavos`) accepting `29,90`, `29.90`, `1.250,00`, `15`—zero floating-point arithmetic.
   - `is_available`: Marks an item as sold out (`Indisponível no momento`) without hiding it from the menu.
   - `is_visible`: Hides an item completely from public published snapshots.
   - Optional `image_asset_id` referencing `media_assets` in the same tenant.

5. **MediaAsset & PublicationRecord (`src/domain/publication.rs`)**
   - Content-addressed image variants (`logo` -> PNG 512/192; `product_image` -> JPEG 800/400).
   - Every mutation to establishment profile, logo, category, or product sets `establishments.has_unpublished_changes = 1`.
