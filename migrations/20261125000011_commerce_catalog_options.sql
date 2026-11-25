CREATE UNIQUE INDEX IF NOT EXISTS uq_products_establishment_id
    ON products(establishment_id, id);

CREATE TABLE IF NOT EXISTS product_variations (
    id TEXT PRIMARY KEY NOT NULL,
    establishment_id TEXT NOT NULL REFERENCES establishments(id) ON DELETE CASCADE,
    product_id TEXT NOT NULL,
    name TEXT NOT NULL CHECK (length(trim(name)) BETWEEN 1 AND 100),
    name_en TEXT NOT NULL DEFAULT '',
    name_es TEXT NOT NULL DEFAULT '',
    price_centavos INTEGER NOT NULL CHECK (price_centavos >= 0),
    position INTEGER NOT NULL CHECK (position >= 0),
    is_available INTEGER NOT NULL DEFAULT 1 CHECK (is_available IN (0, 1)),
    FOREIGN KEY (establishment_id, product_id) REFERENCES products(establishment_id, id) ON DELETE CASCADE
) STRICT;

CREATE INDEX IF NOT EXISTS idx_product_variations_est_product_position
    ON product_variations(establishment_id, product_id, position, id);

CREATE TABLE IF NOT EXISTS addon_groups (
    id TEXT PRIMARY KEY NOT NULL,
    establishment_id TEXT NOT NULL REFERENCES establishments(id) ON DELETE CASCADE,
    name TEXT NOT NULL CHECK (length(trim(name)) BETWEEN 1 AND 100),
    name_en TEXT NOT NULL DEFAULT '',
    name_es TEXT NOT NULL DEFAULT '',
    min_select INTEGER NOT NULL,
    max_select INTEGER NOT NULL,
    position INTEGER NOT NULL CHECK (position >= 0),
    CHECK (0 <= min_select AND min_select <= max_select AND max_select <= 20),
    UNIQUE (establishment_id, id)
) STRICT;

CREATE TABLE IF NOT EXISTS addon_options (
    id TEXT PRIMARY KEY NOT NULL,
    establishment_id TEXT NOT NULL REFERENCES establishments(id) ON DELETE CASCADE,
    group_id TEXT NOT NULL,
    name TEXT NOT NULL CHECK (length(trim(name)) BETWEEN 1 AND 100),
    name_en TEXT NOT NULL DEFAULT '',
    name_es TEXT NOT NULL DEFAULT '',
    price_centavos INTEGER NOT NULL CHECK (price_centavos >= 0),
    position INTEGER NOT NULL CHECK (position >= 0),
    max_quantity INTEGER NOT NULL CHECK (max_quantity >= 1),
    is_available INTEGER NOT NULL DEFAULT 1 CHECK (is_available IN (0, 1)),
    FOREIGN KEY (establishment_id, group_id) REFERENCES addon_groups(establishment_id, id) ON DELETE CASCADE
) STRICT;

CREATE INDEX IF NOT EXISTS idx_addon_options_est_group_position
    ON addon_options(establishment_id, group_id, position, id);

CREATE TABLE IF NOT EXISTS product_addon_groups (
    establishment_id TEXT NOT NULL REFERENCES establishments(id) ON DELETE CASCADE,
    product_id TEXT NOT NULL,
    group_id TEXT NOT NULL,
    position INTEGER NOT NULL CHECK (position >= 0),
    PRIMARY KEY (product_id, group_id),
    FOREIGN KEY (establishment_id, product_id) REFERENCES products(establishment_id, id) ON DELETE CASCADE,
    FOREIGN KEY (establishment_id, group_id) REFERENCES addon_groups(establishment_id, id) ON DELETE CASCADE
) STRICT;

CREATE INDEX IF NOT EXISTS idx_product_addon_groups_est_product_position
    ON product_addon_groups(establishment_id, product_id, position, group_id);
CREATE INDEX IF NOT EXISTS idx_product_addon_groups_est_group
    ON product_addon_groups(establishment_id, group_id);

CREATE TABLE IF NOT EXISTS catalog_snapshots (
    establishment_id TEXT NOT NULL REFERENCES establishments(id) ON DELETE CASCADE,
    channel TEXT NOT NULL CHECK (length(trim(channel)) BETWEEN 1 AND 64),
    version INTEGER NOT NULL CHECK (version >= 1),
    content_hash TEXT NOT NULL CHECK (length(content_hash) = 64),
    snapshot_json TEXT NOT NULL CHECK (json_valid(snapshot_json) AND json_type(snapshot_json) = 'object'),
    published_at TEXT NOT NULL,
    is_current INTEGER NOT NULL CHECK (is_current IN (0, 1)),
    PRIMARY KEY (establishment_id, channel, version)
) STRICT;

CREATE UNIQUE INDEX IF NOT EXISTS uq_catalog_snapshots_current
    ON catalog_snapshots(establishment_id, channel) WHERE is_current = 1;

CREATE TRIGGER IF NOT EXISTS protect_catalog_snapshot_content
BEFORE UPDATE OF establishment_id, channel, version, content_hash, snapshot_json, published_at ON catalog_snapshots
BEGIN
    SELECT RAISE(ABORT, 'catalog snapshots are immutable');
END;
