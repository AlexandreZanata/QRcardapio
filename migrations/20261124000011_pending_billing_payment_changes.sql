-- An authenticated refund or dispute can arrive before its invoice payment.
-- Keep the latest fact until the settlement supplies the subscription and billing period.
CREATE TABLE IF NOT EXISTS billing_pending_payment_changes (
    environment TEXT NOT NULL CHECK (environment IN ('sandbox', 'production')),
    payment_id TEXT NOT NULL,
    event_at TEXT NOT NULL,
    restore INTEGER NOT NULL CHECK (restore IN (0, 1)),
    PRIMARY KEY (environment, payment_id)
) WITHOUT ROWID;
