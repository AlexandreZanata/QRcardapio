-- Expand-only order storage. The price cap mirrors domain::commerce::pricing.
CREATE TABLE IF NOT EXISTS order_number_counters (
    establishment_id TEXT NOT NULL REFERENCES establishments(id) ON DELETE CASCADE,
    business_date TEXT NOT NULL,
    last_number INTEGER NOT NULL CHECK (last_number >= 1),
    PRIMARY KEY (establishment_id, business_date)
) STRICT;

CREATE TABLE IF NOT EXISTS orders (
    id TEXT PRIMARY KEY NOT NULL,
    establishment_id TEXT NOT NULL REFERENCES establishments(id) ON DELETE CASCADE,
    public_id TEXT NOT NULL UNIQUE CHECK (length(public_id) = 32 AND public_id NOT GLOB '*[^0-9a-f]*'),
    order_number INTEGER NOT NULL CHECK (order_number >= 1),
    business_date TEXT NOT NULL,
    channel TEXT NOT NULL CHECK (channel IN ('delivery', 'pickup', 'table', 'counter', 'manual', 'external')),
    fulfillment TEXT NOT NULL CHECK (fulfillment IN ('delivery', 'pickup', 'in_venue')),
    state TEXT NOT NULL DEFAULT 'placed' CHECK (state IN ('placed', 'accepted', 'rejected', 'preparing', 'ready', 'completed', 'cancelled')),
    version INTEGER NOT NULL DEFAULT 1 CHECK (version >= 1),
    catalog_channel TEXT NOT NULL,
    catalog_version INTEGER NOT NULL CHECK (catalog_version >= 1),
    items_subtotal_centavos INTEGER NOT NULL CHECK (items_subtotal_centavos >= 0),
    discount_centavos INTEGER NOT NULL CHECK (discount_centavos BETWEEN 0 AND items_subtotal_centavos),
    delivery_fee_centavos INTEGER NOT NULL CHECK (delivery_fee_centavos >= 0),
    total_centavos INTEGER NOT NULL CHECK (
        total_centavos BETWEEN 0 AND 1000000
        AND total_centavos >= delivery_fee_centavos
        AND total_centavos - delivery_fee_centavos = items_subtotal_centavos - discount_centavos
    ),
    placed_at TEXT NOT NULL,
    acceptance_deadline_at TEXT NOT NULL,
    updated_at TEXT NOT NULL,
    accepted_at TEXT,
    promised_at TEXT,
    preparing_at TEXT,
    ready_at TEXT,
    completed_at TEXT,
    rejected_at TEXT,
    cancelled_at TEXT,
    reason_code TEXT,
    UNIQUE (establishment_id, id),
    UNIQUE (establishment_id, business_date, order_number),
    FOREIGN KEY (establishment_id, catalog_channel, catalog_version)
        REFERENCES catalog_snapshots(establishment_id, channel, version)
) STRICT;

CREATE INDEX IF NOT EXISTS idx_orders_est_state_at ON orders (establishment_id, state, placed_at DESC);
CREATE INDEX IF NOT EXISTS idx_orders_catalog_snapshot ON orders (establishment_id, catalog_channel, catalog_version);

-- Catalog IDs are historical labels. Removing a draft product, variation or option never
-- changes an order. The composite foreign keys below only link within the order aggregate.
CREATE TABLE IF NOT EXISTS order_items (
    id TEXT PRIMARY KEY NOT NULL,
    establishment_id TEXT NOT NULL,
    order_id TEXT NOT NULL,
    position INTEGER NOT NULL CHECK (position >= 0),
    product_id TEXT NOT NULL,
    name TEXT NOT NULL,
    name_en TEXT NOT NULL,
    name_es TEXT NOT NULL,
    variation_id TEXT,
    variation_name TEXT,
    variation_name_en TEXT,
    variation_name_es TEXT,
    base_price_centavos INTEGER NOT NULL CHECK (base_price_centavos >= 0),
    unit_price_centavos INTEGER NOT NULL CHECK (unit_price_centavos >= base_price_centavos),
    quantity INTEGER NOT NULL CHECK (quantity BETWEEN 1 AND 99),
    line_total_centavos INTEGER NOT NULL CHECK (
        line_total_centavos >= 0 AND line_total_centavos / quantity = unit_price_centavos
        AND line_total_centavos % quantity = 0
    ),
    CHECK ((variation_id IS NULL AND variation_name IS NULL AND variation_name_en IS NULL AND variation_name_es IS NULL)
        OR (variation_id IS NOT NULL AND variation_name IS NOT NULL AND variation_name_en IS NOT NULL AND variation_name_es IS NOT NULL)),
    UNIQUE (establishment_id, id),
    UNIQUE (establishment_id, order_id, position),
    FOREIGN KEY (establishment_id, order_id) REFERENCES orders(establishment_id, id) ON DELETE CASCADE
) STRICT;

CREATE TABLE IF NOT EXISTS order_item_options (
    establishment_id TEXT NOT NULL,
    order_item_id TEXT NOT NULL,
    option_id TEXT NOT NULL,
    group_id TEXT NOT NULL,
    group_name TEXT NOT NULL,
    group_name_en TEXT NOT NULL,
    group_name_es TEXT NOT NULL,
    name TEXT NOT NULL,
    name_en TEXT NOT NULL,
    name_es TEXT NOT NULL,
    unit_price_centavos INTEGER NOT NULL CHECK (unit_price_centavos >= 0),
    quantity INTEGER NOT NULL CHECK (quantity >= 1),
    PRIMARY KEY (establishment_id, order_item_id, option_id),
    FOREIGN KEY (establishment_id, order_item_id) REFERENCES order_items(establishment_id, id) ON DELETE CASCADE
) STRICT;

CREATE TRIGGER IF NOT EXISTS order_items_immutable_update
BEFORE UPDATE ON order_items
BEGIN SELECT RAISE(ABORT, 'order item snapshots are immutable'); END;

CREATE TRIGGER IF NOT EXISTS order_items_immutable_delete
BEFORE DELETE ON order_items
WHEN EXISTS (SELECT 1 FROM orders WHERE id = OLD.order_id AND establishment_id = OLD.establishment_id)
BEGIN SELECT RAISE(ABORT, 'order item snapshots are immutable'); END;

CREATE TRIGGER IF NOT EXISTS order_item_options_immutable_update
BEFORE UPDATE ON order_item_options
BEGIN SELECT RAISE(ABORT, 'order item option snapshots are immutable'); END;

CREATE TRIGGER IF NOT EXISTS order_item_options_immutable_delete
BEFORE DELETE ON order_item_options
WHEN EXISTS (SELECT 1 FROM order_items WHERE id = OLD.order_item_id AND establishment_id = OLD.establishment_id)
BEGIN SELECT RAISE(ABORT, 'order item option snapshots are immutable'); END;

CREATE TABLE IF NOT EXISTS order_transitions (
    establishment_id TEXT NOT NULL,
    order_id TEXT NOT NULL,
    version INTEGER NOT NULL CHECK (version >= 2),
    from_state TEXT NOT NULL,
    to_state TEXT NOT NULL,
    actor_kind TEXT NOT NULL,
    actor_account_id TEXT,
    reason_code TEXT,
    at TEXT NOT NULL,
    PRIMARY KEY (establishment_id, order_id, version),
    FOREIGN KEY (establishment_id, order_id) REFERENCES orders(establishment_id, id) ON DELETE CASCADE
) STRICT;

CREATE TRIGGER IF NOT EXISTS order_transitions_append_only_update
BEFORE UPDATE ON order_transitions
BEGIN SELECT RAISE(ABORT, 'order transitions are append-only'); END;

CREATE TRIGGER IF NOT EXISTS order_transitions_append_only_delete
BEFORE DELETE ON order_transitions
WHEN EXISTS (SELECT 1 FROM orders WHERE id = OLD.order_id AND establishment_id = OLD.establishment_id)
BEGIN SELECT RAISE(ABORT, 'order transitions are append-only'); END;
