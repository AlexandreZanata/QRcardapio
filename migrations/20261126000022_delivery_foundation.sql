-- Expand-only QR Delivery foundation (ADR-010: additive, idempotent, rollback-tolerant).

CREATE TABLE IF NOT EXISTS establishment_member_roles (
    establishment_id TEXT NOT NULL REFERENCES establishments(id) ON DELETE CASCADE,
    account_id TEXT NOT NULL REFERENCES accounts(id) ON DELETE CASCADE,
    member_role TEXT NOT NULL CHECK (member_role IN ('cashier', 'waiter', 'kitchen_operator', 'courier')),
    created_at TEXT NOT NULL,
    PRIMARY KEY (establishment_id, account_id)
) STRICT;

CREATE INDEX IF NOT EXISTS idx_establishment_member_roles_account
    ON establishment_member_roles(account_id);

CREATE TABLE IF NOT EXISTS storefronts (
    establishment_id TEXT PRIMARY KEY NOT NULL REFERENCES establishments(id) ON DELETE CASCADE,
    status TEXT NOT NULL DEFAULT 'draft' CHECK (status IN ('draft', 'published', 'unpublished')),
    published_version INTEGER NOT NULL DEFAULT 0 CHECK (published_version >= 0),
    has_unpublished_changes INTEGER NOT NULL DEFAULT 1 CHECK (has_unpublished_changes IN (0, 1)),
    last_published_at TEXT,
    accept_cash INTEGER NOT NULL DEFAULT 1 CHECK (accept_cash IN (0, 1)),
    accept_card_on_delivery INTEGER NOT NULL DEFAULT 1 CHECK (accept_card_on_delivery IN (0, 1)),
    accept_pix_on_delivery INTEGER NOT NULL DEFAULT 0 CHECK (accept_pix_on_delivery IN (0, 1)),
    pix_key_display TEXT NOT NULL DEFAULT '',
    contact_whatsapp TEXT NOT NULL DEFAULT '',
    pickup_instructions TEXT NOT NULL DEFAULT '',
    default_prep_minutes INTEGER NOT NULL DEFAULT 25 CHECK (default_prep_minutes BETWEEN 5 AND 180),
    updated_at TEXT NOT NULL
) STRICT;

CREATE TABLE IF NOT EXISTS storefront_publications (
    id TEXT PRIMARY KEY NOT NULL,
    establishment_id TEXT NOT NULL REFERENCES establishments(id) ON DELETE CASCADE,
    version INTEGER NOT NULL CHECK (version >= 1),
    kind TEXT NOT NULL CHECK (kind IN ('manual', 'derived')),
    status TEXT NOT NULL CHECK (status IN ('published', 'failed', 'unpublished')),
    html_bytes INTEGER NOT NULL DEFAULT 0,
    catalog_bytes INTEGER NOT NULL DEFAULT 0,
    duration_ms INTEGER NOT NULL DEFAULT 0,
    error_message TEXT,
    published_at TEXT NOT NULL
) STRICT;

CREATE INDEX IF NOT EXISTS idx_storefront_publications_est_time
    ON storefront_publications(establishment_id, published_at DESC);

CREATE TABLE IF NOT EXISTS delivery_zones (
    id TEXT PRIMARY KEY NOT NULL,
    establishment_id TEXT NOT NULL REFERENCES establishments(id) ON DELETE CASCADE,
    neighborhood TEXT NOT NULL,
    city TEXT NOT NULL,
    normalized_key TEXT NOT NULL,
    fee_centavos INTEGER NOT NULL CHECK (fee_centavos BETWEEN 0 AND 1000000),
    min_order_centavos INTEGER CHECK (min_order_centavos IS NULL OR min_order_centavos BETWEEN 0 AND 1000000),
    free_delivery_from_centavos INTEGER CHECK (
        free_delivery_from_centavos IS NULL OR free_delivery_from_centavos BETWEEN 1 AND 1000000
    ),
    eta_min_minutes INTEGER NOT NULL DEFAULT 20 CHECK (eta_min_minutes BETWEEN 1 AND 240),
    eta_max_minutes INTEGER NOT NULL DEFAULT 45 CHECK (eta_max_minutes BETWEEN eta_min_minutes AND 360),
    is_active INTEGER NOT NULL DEFAULT 1 CHECK (is_active IN (0, 1)),
    position INTEGER NOT NULL DEFAULT 0,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL,
    UNIQUE (establishment_id, normalized_key),
    UNIQUE (establishment_id, id)
) STRICT;

CREATE INDEX IF NOT EXISTS idx_delivery_zones_est_pos
    ON delivery_zones(establishment_id, is_active, position, id);

CREATE TABLE IF NOT EXISTS coupons (
    id TEXT PRIMARY KEY NOT NULL,
    establishment_id TEXT NOT NULL REFERENCES establishments(id) ON DELETE CASCADE,
    code TEXT NOT NULL,
    kind TEXT NOT NULL CHECK (kind IN ('percent_bps', 'amount_centavos')),
    percent_bps INTEGER CHECK (percent_bps IS NULL OR percent_bps BETWEEN 1 AND 10000),
    amount_centavos INTEGER CHECK (amount_centavos IS NULL OR amount_centavos BETWEEN 1 AND 1000000),
    min_subtotal_centavos INTEGER NOT NULL DEFAULT 0 CHECK (min_subtotal_centavos BETWEEN 0 AND 1000000),
    valid_from TEXT,
    valid_until TEXT,
    max_redemptions INTEGER CHECK (max_redemptions IS NULL OR max_redemptions >= 1),
    max_per_customer INTEGER CHECK (max_per_customer IS NULL OR max_per_customer >= 1),
    redemption_count INTEGER NOT NULL DEFAULT 0 CHECK (
        redemption_count >= 0 AND (max_redemptions IS NULL OR redemption_count <= max_redemptions)
    ),
    allow_delivery INTEGER NOT NULL DEFAULT 1 CHECK (allow_delivery IN (0, 1)),
    allow_pickup INTEGER NOT NULL DEFAULT 1 CHECK (allow_pickup IN (0, 1)),
    is_active INTEGER NOT NULL DEFAULT 1 CHECK (is_active IN (0, 1)),
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL,
    CHECK (
        (kind = 'percent_bps' AND percent_bps IS NOT NULL AND amount_centavos IS NULL)
        OR (kind = 'amount_centavos' AND amount_centavos IS NOT NULL AND percent_bps IS NULL)
    ),
    UNIQUE (establishment_id, code),
    UNIQUE (establishment_id, id)
) STRICT;

CREATE TABLE IF NOT EXISTS coupon_redemptions (
    id TEXT PRIMARY KEY NOT NULL,
    establishment_id TEXT NOT NULL,
    coupon_id TEXT NOT NULL,
    order_id TEXT NOT NULL UNIQUE,
    customer_id TEXT NOT NULL,
    status TEXT NOT NULL DEFAULT 'consumed' CHECK (status IN ('consumed', 'released')),
    created_at TEXT NOT NULL,
    released_at TEXT,
    FOREIGN KEY (establishment_id, coupon_id) REFERENCES coupons(establishment_id, id) ON DELETE CASCADE,
    FOREIGN KEY (establishment_id, order_id) REFERENCES orders(establishment_id, id) ON DELETE CASCADE
) STRICT;

CREATE INDEX IF NOT EXISTS idx_coupon_redemptions_customer
    ON coupon_redemptions(establishment_id, coupon_id, customer_id, status);

CREATE TABLE IF NOT EXISTS delivery_order_metadata (
    establishment_id TEXT NOT NULL,
    order_id TEXT NOT NULL,
    cancel_token_hash TEXT NOT NULL,
    zone_id TEXT,
    neighborhood_label TEXT NOT NULL DEFAULT '',
    coupon_id TEXT,
    coupon_code TEXT,
    order_note TEXT NOT NULL DEFAULT '',
    pix_receipt_key TEXT,
    created_at TEXT NOT NULL,
    PRIMARY KEY (establishment_id, order_id),
    FOREIGN KEY (establishment_id, order_id) REFERENCES orders(establishment_id, id) ON DELETE CASCADE
) STRICT;

CREATE TABLE IF NOT EXISTS delivery_order_item_notes (
    establishment_id TEXT NOT NULL,
    order_item_id TEXT NOT NULL,
    note TEXT NOT NULL CHECK (length(note) BETWEEN 1 AND 140),
    PRIMARY KEY (establishment_id, order_item_id),
    FOREIGN KEY (establishment_id, order_item_id) REFERENCES order_items(establishment_id, id) ON DELETE CASCADE
) STRICT;

-- Customer erasure has not yet removed the order links when erased_at changes.
CREATE TRIGGER IF NOT EXISTS delivery_erase_order_notes
AFTER UPDATE OF erased_at ON customers
WHEN NEW.erased_at IS NOT NULL AND OLD.erased_at IS NULL
BEGIN
    UPDATE delivery_order_metadata SET order_note = ''
    WHERE establishment_id = OLD.establishment_id AND order_id IN (
        SELECT order_id FROM order_customers
        WHERE establishment_id = OLD.establishment_id AND customer_id = OLD.id
    );
    DELETE FROM delivery_order_item_notes
    WHERE establishment_id = OLD.establishment_id AND order_item_id IN (
        SELECT i.id FROM order_items i
        INNER JOIN order_customers c ON c.establishment_id = i.establishment_id AND c.order_id = i.order_id
        WHERE c.establishment_id = OLD.establishment_id AND c.customer_id = OLD.id
    );
END;

CREATE TABLE IF NOT EXISTS delivery_customer_blocks (
    establishment_id TEXT NOT NULL REFERENCES establishments(id) ON DELETE CASCADE,
    phone_e164 TEXT NOT NULL,
    reason TEXT NOT NULL DEFAULT '',
    blocked_by_account_id TEXT REFERENCES accounts(id) ON DELETE SET NULL,
    blocked_at TEXT NOT NULL,
    PRIMARY KEY (establishment_id, phone_e164)
) STRICT;


-- Latest committed public presentation, including the exact media references.
CREATE TABLE IF NOT EXISTS storefront_presentations (
    establishment_id TEXT PRIMARY KEY NOT NULL REFERENCES establishments(id) ON DELETE CASCADE,
    publication_version INTEGER NOT NULL CHECK (publication_version >= 1),
    payload_json TEXT NOT NULL
) STRICT;
