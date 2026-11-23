-- QR Cardapio initial schema (SQLite 3.54.1, WAL mode, UTC timestamps, centavos integer pricing)

CREATE TABLE IF NOT EXISTS accounts (
    id TEXT PRIMARY KEY NOT NULL,
    email TEXT NOT NULL UNIQUE COLLATE NOCASE,
    password_hash TEXT NOT NULL,
    role TEXT NOT NULL DEFAULT 'owner' CHECK (role IN ('owner', 'admin')),
    preferred_locale TEXT NOT NULL DEFAULT 'pt-BR' CHECK (preferred_locale IN ('pt-BR', 'en', 'es')),
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS sessions (
    id TEXT PRIMARY KEY NOT NULL,
    account_id TEXT NOT NULL REFERENCES accounts(id) ON DELETE CASCADE,
    token_hash TEXT NOT NULL UNIQUE,
    csrf_token TEXT NOT NULL,
    expires_at TEXT NOT NULL,
    created_at TEXT NOT NULL,
    last_seen_at TEXT NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_sessions_account_id ON sessions(account_id);
CREATE INDEX IF NOT EXISTS idx_sessions_expires_at ON sessions(expires_at);

CREATE TABLE IF NOT EXISTS establishments (
    id TEXT PRIMARY KEY NOT NULL,
    slug TEXT NOT NULL UNIQUE,
    name TEXT NOT NULL,
    description TEXT NOT NULL DEFAULT '',
    description_en TEXT NOT NULL DEFAULT '',
    description_es TEXT NOT NULL DEFAULT '',
    accent_color TEXT NOT NULL DEFAULT '#D9532F',
    logo_asset_id TEXT,
    is_published INTEGER NOT NULL DEFAULT 0 CHECK (is_published IN (0, 1)),
    published_version INTEGER NOT NULL DEFAULT 0,
    has_unpublished_changes INTEGER NOT NULL DEFAULT 0 CHECK (has_unpublished_changes IN (0, 1)),
    last_published_at TEXT,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_establishments_published ON establishments(is_published, last_published_at);

CREATE TABLE IF NOT EXISTS establishment_members (
    establishment_id TEXT NOT NULL REFERENCES establishments(id) ON DELETE CASCADE,
    account_id TEXT NOT NULL REFERENCES accounts(id) ON DELETE CASCADE,
    member_role TEXT NOT NULL DEFAULT 'owner' CHECK (member_role IN ('owner', 'manager')),
    created_at TEXT NOT NULL,
    PRIMARY KEY (establishment_id, account_id)
);

CREATE INDEX IF NOT EXISTS idx_establishment_members_account ON establishment_members(account_id);

CREATE TABLE IF NOT EXISTS media_assets (
    id TEXT PRIMARY KEY NOT NULL,
    establishment_id TEXT NOT NULL REFERENCES establishments(id) ON DELETE CASCADE,
    kind TEXT NOT NULL CHECK (kind IN ('logo', 'product_image')),
    content_hash TEXT NOT NULL,
    mime_type TEXT NOT NULL,
    width INTEGER NOT NULL,
    height INTEGER NOT NULL,
    byte_size INTEGER NOT NULL,
    filename_large TEXT NOT NULL,
    filename_thumb TEXT NOT NULL,
    created_at TEXT NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_media_assets_establishment ON media_assets(establishment_id, kind);

CREATE TABLE IF NOT EXISTS categories (
    id TEXT PRIMARY KEY NOT NULL,
    establishment_id TEXT NOT NULL REFERENCES establishments(id) ON DELETE CASCADE,
    name TEXT NOT NULL,
    name_en TEXT NOT NULL DEFAULT '',
    name_es TEXT NOT NULL DEFAULT '',
    description TEXT NOT NULL DEFAULT '',
    description_en TEXT NOT NULL DEFAULT '',
    description_es TEXT NOT NULL DEFAULT '',
    position INTEGER NOT NULL,
    is_visible INTEGER NOT NULL DEFAULT 1 CHECK (is_visible IN (0, 1)),
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_categories_est_pos ON categories(establishment_id, position, id);

CREATE TABLE IF NOT EXISTS products (
    id TEXT PRIMARY KEY NOT NULL,
    establishment_id TEXT NOT NULL REFERENCES establishments(id) ON DELETE CASCADE,
    category_id TEXT NOT NULL REFERENCES categories(id) ON DELETE CASCADE,
    name TEXT NOT NULL,
    name_en TEXT NOT NULL DEFAULT '',
    name_es TEXT NOT NULL DEFAULT '',
    description TEXT NOT NULL DEFAULT '',
    description_en TEXT NOT NULL DEFAULT '',
    description_es TEXT NOT NULL DEFAULT '',
    price_centavos INTEGER NOT NULL CHECK (price_centavos >= 0),
    image_asset_id TEXT REFERENCES media_assets(id) ON DELETE SET NULL,
    position INTEGER NOT NULL,
    is_available INTEGER NOT NULL DEFAULT 1 CHECK (is_available IN (0, 1)),
    is_visible INTEGER NOT NULL DEFAULT 1 CHECK (is_visible IN (0, 1)),
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_products_est_cat_pos ON products(establishment_id, category_id, position, id);
CREATE INDEX IF NOT EXISTS idx_products_image_asset ON products(image_asset_id);

CREATE TABLE IF NOT EXISTS publications (
    id TEXT PRIMARY KEY NOT NULL,
    establishment_id TEXT NOT NULL REFERENCES establishments(id) ON DELETE CASCADE,
    version INTEGER NOT NULL,
    status TEXT NOT NULL CHECK (status IN ('published', 'unpublished', 'failed')),
    html_bytes INTEGER NOT NULL DEFAULT 0,
    asset_count INTEGER NOT NULL DEFAULT 0,
    duration_ms INTEGER NOT NULL DEFAULT 0,
    error_message TEXT,
    published_at TEXT NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_publications_est_time ON publications(establishment_id, published_at DESC);

CREATE TABLE IF NOT EXISTS password_reset_tokens (
    id TEXT PRIMARY KEY NOT NULL,
    account_id TEXT NOT NULL REFERENCES accounts(id) ON DELETE CASCADE,
    token_hash TEXT NOT NULL UNIQUE,
    expires_at TEXT NOT NULL,
    used_at TEXT,
    created_at TEXT NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_reset_tokens_account ON password_reset_tokens(account_id);
