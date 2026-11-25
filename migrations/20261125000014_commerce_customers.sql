CREATE TABLE IF NOT EXISTS customers (
    id TEXT PRIMARY KEY NOT NULL,
    establishment_id TEXT NOT NULL REFERENCES establishments(id) ON DELETE CASCADE,
    name TEXT CHECK (length(name) BETWEEN 1 AND 80),
    phone_e164 TEXT CHECK (length(phone_e164) BETWEEN 9 AND 16 AND substr(phone_e164, 1, 1) = '+'
                           AND substr(phone_e164, 2, 1) BETWEEN '1' AND '9'
                           AND substr(phone_e164, 2) NOT GLOB '*[^0-9]*'),
    email TEXT,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL,
    erased_at TEXT,
    CHECK ((erased_at IS NULL AND name IS NOT NULL AND phone_e164 IS NOT NULL)
        OR (erased_at IS NOT NULL AND name IS NULL AND phone_e164 IS NULL AND email IS NULL)),
    UNIQUE (establishment_id, id)
) STRICT;

CREATE UNIQUE INDEX IF NOT EXISTS idx_customers_active_phone
    ON customers(establishment_id, phone_e164) WHERE erased_at IS NULL;
CREATE INDEX IF NOT EXISTS idx_customers_active_name
    ON customers(establishment_id, name COLLATE NOCASE, id) WHERE erased_at IS NULL;

CREATE TABLE IF NOT EXISTS customer_addresses (
    id TEXT PRIMARY KEY NOT NULL,
    establishment_id TEXT NOT NULL,
    customer_id TEXT NOT NULL,
    street TEXT NOT NULL,
    street_number TEXT NOT NULL,
    complement TEXT NOT NULL DEFAULT '',
    neighborhood TEXT NOT NULL,
    city TEXT NOT NULL,
    uf TEXT NOT NULL CHECK (length(uf) = 2),
    cep TEXT NOT NULL CHECK (length(cep) = 8 AND cep NOT GLOB '*[^0-9]*'),
    reference TEXT NOT NULL DEFAULT '',
    FOREIGN KEY (establishment_id, customer_id) REFERENCES customers(establishment_id, id) ON DELETE CASCADE
) STRICT;

CREATE INDEX IF NOT EXISTS idx_customer_addresses_customer ON customer_addresses(establishment_id, customer_id, id);

-- Keep the customer link separate so erasure can detach retained orders without rewriting them.
CREATE TABLE IF NOT EXISTS order_customers (
    establishment_id TEXT NOT NULL,
    order_id TEXT NOT NULL,
    customer_id TEXT NOT NULL,
    PRIMARY KEY (establishment_id, order_id),
    FOREIGN KEY (establishment_id, order_id) REFERENCES orders(establishment_id, id) ON DELETE CASCADE,
    FOREIGN KEY (establishment_id, customer_id) REFERENCES customers(establishment_id, id)
) STRICT;

CREATE INDEX IF NOT EXISTS idx_order_customers_customer ON order_customers(establishment_id, customer_id, order_id);

CREATE TABLE IF NOT EXISTS order_delivery_details (
    establishment_id TEXT NOT NULL,
    order_id TEXT NOT NULL,
    recipient_name TEXT,
    recipient_phone TEXT,
    street TEXT,
    street_number TEXT,
    complement TEXT,
    neighborhood_label TEXT NOT NULL,
    city TEXT,
    uf TEXT,
    cep TEXT,
    reference TEXT,
    erased_at TEXT,
    CHECK (erased_at IS NULL OR (recipient_name IS NULL AND recipient_phone IS NULL AND street IS NULL
        AND street_number IS NULL AND complement IS NULL AND city IS NULL AND uf IS NULL AND cep IS NULL AND reference IS NULL)),
    PRIMARY KEY (establishment_id, order_id),
    FOREIGN KEY (establishment_id, order_id) REFERENCES orders(establishment_id, id) ON DELETE CASCADE
) STRICT;
