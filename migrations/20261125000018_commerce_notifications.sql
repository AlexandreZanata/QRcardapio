CREATE INDEX IF NOT EXISTS idx_orders_placed_deadline
    ON orders (acceptance_deadline_at, id) WHERE state = 'placed';

CREATE TABLE IF NOT EXISTS commerce_push_subscriptions (
    id TEXT PRIMARY KEY NOT NULL,
    establishment_id TEXT NOT NULL,
    account_id TEXT NOT NULL,
    endpoint TEXT NOT NULL UNIQUE,
    p256dh TEXT NOT NULL,
    auth_secret TEXT NOT NULL,
    registered_at INTEGER NOT NULL,
    UNIQUE (establishment_id, id),
    FOREIGN KEY (establishment_id, account_id)
        REFERENCES establishment_members(establishment_id, account_id) ON DELETE CASCADE
) STRICT;

CREATE INDEX IF NOT EXISTS idx_commerce_push_subscriptions_member
    ON commerce_push_subscriptions (establishment_id, account_id);

CREATE UNIQUE INDEX IF NOT EXISTS idx_domain_events_est_event
    ON domain_events (establishment_id, event_id);

CREATE TABLE IF NOT EXISTS commerce_push_deliveries (
    establishment_id TEXT NOT NULL,
    event_id TEXT NOT NULL,
    subscription_id TEXT NOT NULL,
    order_id TEXT NOT NULL,
    available_at INTEGER NOT NULL,
    expires_at INTEGER NOT NULL,
    attempts INTEGER NOT NULL DEFAULT 0 CHECK (attempts >= 0),
    lease_token TEXT,
    finished_at INTEGER,
    outcome TEXT CHECK (outcome IN ('delivered', 'rejected', 'exhausted', 'expired', 'gone')),
    CHECK ((finished_at IS NULL) = (outcome IS NULL)),
    PRIMARY KEY (establishment_id, event_id, subscription_id),
    FOREIGN KEY (establishment_id, event_id)
        REFERENCES domain_events(establishment_id, event_id) ON DELETE CASCADE,
    FOREIGN KEY (establishment_id, subscription_id)
        REFERENCES commerce_push_subscriptions(establishment_id, id) ON DELETE CASCADE,
    FOREIGN KEY (establishment_id, order_id) REFERENCES orders(establishment_id, id) ON DELETE CASCADE
) STRICT;

CREATE INDEX IF NOT EXISTS idx_commerce_push_deliveries_due
    ON commerce_push_deliveries (available_at) WHERE finished_at IS NULL;
CREATE INDEX IF NOT EXISTS idx_commerce_push_deliveries_expiry
    ON commerce_push_deliveries (expires_at);
CREATE INDEX IF NOT EXISTS idx_commerce_push_deliveries_sub
    ON commerce_push_deliveries (establishment_id, subscription_id);
CREATE INDEX IF NOT EXISTS idx_commerce_push_deliveries_order
    ON commerce_push_deliveries (establishment_id, order_id);
