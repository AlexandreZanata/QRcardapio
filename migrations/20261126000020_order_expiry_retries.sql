CREATE TABLE IF NOT EXISTS order_expiry_retries (
    establishment_id TEXT NOT NULL,
    order_id TEXT NOT NULL,
    retry_at INTEGER NOT NULL,
    PRIMARY KEY (establishment_id, order_id),
    FOREIGN KEY (establishment_id, order_id)
        REFERENCES orders (establishment_id, id) ON DELETE CASCADE
) STRICT, WITHOUT ROWID;

CREATE INDEX IF NOT EXISTS idx_order_expiry_retries_due
    ON order_expiry_retries (retry_at, order_id);

CREATE TRIGGER IF NOT EXISTS clear_order_expiry_retry
AFTER UPDATE OF state ON orders
WHEN OLD.state = 'placed' AND NEW.state <> 'placed'
BEGIN
    DELETE FROM order_expiry_retries
    WHERE order_id = NEW.id AND establishment_id = NEW.establishment_id;
END;
