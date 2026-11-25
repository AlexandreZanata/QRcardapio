CREATE TABLE IF NOT EXISTS checkout_attempts (
    establishment_id TEXT NOT NULL REFERENCES establishments(id) ON DELETE CASCADE,
    idempotency_key_hash BLOB NOT NULL CHECK (length(idempotency_key_hash) = 32),
    request_hash BLOB NOT NULL CHECK (length(request_hash) = 32),
    order_id TEXT NOT NULL,
    created_at INTEGER NOT NULL,
    expires_at INTEGER NOT NULL CHECK (expires_at > created_at),
    PRIMARY KEY (establishment_id, idempotency_key_hash),
    FOREIGN KEY (establishment_id, order_id) REFERENCES orders(establishment_id, id) ON DELETE CASCADE
) STRICT;

CREATE INDEX IF NOT EXISTS idx_checkout_attempts_expiration
    ON checkout_attempts(expires_at, establishment_id, idempotency_key_hash);

-- An address entered at checkout is private. Retained labels belong to server-owned zones.
CREATE TABLE IF NOT EXISTS order_delivery_neighborhoods (
    establishment_id TEXT NOT NULL,
    order_id TEXT NOT NULL,
    neighborhood TEXT NOT NULL CHECK (length(trim(neighborhood)) BETWEEN 1 AND 80),
    PRIMARY KEY (establishment_id, order_id),
    FOREIGN KEY (establishment_id, order_id)
        REFERENCES order_delivery_details(establishment_id, order_id) ON DELETE CASCADE
) STRICT;

-- Older binaries already set erased_at; retain erasure semantics during rollback.
CREATE TRIGGER IF NOT EXISTS erase_private_delivery_neighborhood
AFTER UPDATE OF erased_at ON order_delivery_details
WHEN NEW.erased_at IS NOT NULL
BEGIN
    DELETE FROM order_delivery_neighborhoods
    WHERE order_id = NEW.order_id AND establishment_id = NEW.establishment_id;
END;
