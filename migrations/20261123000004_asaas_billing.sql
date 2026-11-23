CREATE TABLE billing_checkouts (
    reference TEXT PRIMARY KEY,
    environment TEXT NOT NULL CHECK (environment IN ('sandbox', 'production')),
    establishment_id TEXT NOT NULL REFERENCES establishments(id) ON DELETE CASCADE,
    checkout_id TEXT,
    checkout_status TEXT NOT NULL DEFAULT 'creating'
        CHECK (checkout_status IN ('creating', 'open', 'paid', 'closed')),
    subscription_id TEXT,
    subscription_status TEXT NOT NULL DEFAULT 'pending'
        CHECK (subscription_status IN ('pending', 'active', 'inactive', 'deleted')),
    subscription_event_at TEXT,
    created_at TEXT NOT NULL,
    UNIQUE (environment, checkout_id),
    UNIQUE (environment, subscription_id)
);

CREATE INDEX idx_billing_checkout_history
    ON billing_checkouts (establishment_id, environment, created_at DESC, reference DESC);

CREATE UNIQUE INDEX idx_billing_one_current_checkout
    ON billing_checkouts (establishment_id, environment)
    WHERE checkout_status <> 'closed' AND subscription_status <> 'deleted';

CREATE TABLE billing_payments (
    environment TEXT NOT NULL CHECK (environment IN ('sandbox', 'production')),
    payment_id TEXT NOT NULL,
    subscription_id TEXT NOT NULL,
    due_date TEXT NOT NULL,
    paid_through TEXT,
    event_at TEXT NOT NULL,
    event_priority INTEGER NOT NULL CHECK (event_priority IN (1, 2)),
    PRIMARY KEY (environment, payment_id)
) WITHOUT ROWID;

CREATE INDEX idx_billing_paid_period ON billing_payments (environment, subscription_id, paid_through DESC)
    WHERE paid_through IS NOT NULL;

CREATE TABLE billing_webhook_events (
    environment TEXT NOT NULL CHECK (environment IN ('sandbox', 'production')),
    event_id TEXT NOT NULL,
    PRIMARY KEY (environment, event_id)
) WITHOUT ROWID;
