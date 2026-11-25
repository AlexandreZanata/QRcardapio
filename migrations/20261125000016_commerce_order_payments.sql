-- Offline collection is tenant-scoped and never uses the SaaS billing ledger.
CREATE TABLE IF NOT EXISTS order_payments (
    id TEXT PRIMARY KEY NOT NULL,
    public_id TEXT NOT NULL UNIQUE,
    establishment_id TEXT NOT NULL,
    order_id TEXT NOT NULL,
    requested_method TEXT NOT NULL CHECK (requested_method IN ('cash', 'card_on_delivery', 'pix_on_delivery')),
    method TEXT NOT NULL CHECK (method IN ('cash', 'card_on_delivery', 'pix_on_delivery')),
    state TEXT NOT NULL DEFAULT 'pending' CHECK (state IN ('pending', 'confirmed', 'cancelled', 'partially_refunded', 'refunded')),
    version INTEGER NOT NULL DEFAULT 1 CHECK (version >= 1),
    amount_centavos INTEGER NOT NULL CHECK (amount_centavos BETWEEN 0 AND 1000000),
    refunded_centavos INTEGER NOT NULL DEFAULT 0 CHECK (refunded_centavos BETWEEN 0 AND amount_centavos),
    change_for_centavos INTEGER,
    refund_due_centavos INTEGER NOT NULL DEFAULT 0 CHECK (refund_due_centavos BETWEEN 0 AND amount_centavos - refunded_centavos),
    refund_due_at TEXT,
    refund_due_event_id TEXT,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL,
    UNIQUE (establishment_id, id),
    FOREIGN KEY (establishment_id, order_id) REFERENCES orders(establishment_id, id) ON DELETE CASCADE,
    CHECK (change_for_centavos IS NULL OR (requested_method = 'cash' AND change_for_centavos >= amount_centavos)),
    CHECK (state NOT IN ('pending', 'cancelled', 'confirmed') OR refunded_centavos = 0),
    CHECK (state <> 'partially_refunded' OR refunded_centavos BETWEEN 1 AND amount_centavos - 1),
    CHECK (state <> 'refunded' OR (amount_centavos > 0 AND refunded_centavos = amount_centavos)),
    CHECK (refund_due_centavos = 0 OR (state IN ('confirmed', 'partially_refunded')
        AND refund_due_centavos = amount_centavos - refunded_centavos
        AND refund_due_at IS NOT NULL AND refund_due_event_id IS NOT NULL))
) STRICT;

-- Replacing an uncollected, cancelled intent keeps the old audit trail.
CREATE UNIQUE INDEX IF NOT EXISTS uq_order_live_offline_payment
    ON order_payments (establishment_id, order_id) WHERE state <> 'cancelled';
CREATE INDEX IF NOT EXISTS idx_order_payments_order ON order_payments (establishment_id, order_id, id);

CREATE TRIGGER IF NOT EXISTS order_payment_terms_immutable
BEFORE UPDATE OF id, public_id, establishment_id, order_id, requested_method,
    amount_centavos, change_for_centavos, created_at ON order_payments
BEGIN SELECT RAISE(ABORT, 'order payment terms are immutable'); END;

CREATE TRIGGER IF NOT EXISTS order_payment_preserve_history
BEFORE DELETE ON order_payments
WHEN EXISTS (SELECT 1 FROM orders WHERE id = OLD.order_id AND establishment_id = OLD.establishment_id)
BEGIN SELECT RAISE(ABORT, 'order payment history is retained'); END;

CREATE TABLE IF NOT EXISTS payment_refunds (
    id TEXT PRIMARY KEY NOT NULL,
    establishment_id TEXT NOT NULL,
    payment_id TEXT NOT NULL,
    amount_centavos INTEGER NOT NULL CHECK (amount_centavos BETWEEN 1 AND 1000000),
    reason_code TEXT NOT NULL CHECK (reason_code IN ('order_cancelled', 'customer_request', 'payment_error', 'other')),
    recorded_at TEXT NOT NULL,
    FOREIGN KEY (establishment_id, payment_id) REFERENCES order_payments(establishment_id, id) ON DELETE CASCADE
) STRICT;

CREATE INDEX IF NOT EXISTS idx_payment_refunds_payment ON payment_refunds (establishment_id, payment_id, id);

-- The aggregate balance is updated before the corresponding ledger entry in the same transaction.
CREATE TRIGGER IF NOT EXISTS payment_refund_matches_balance
BEFORE INSERT ON payment_refunds
WHEN NEW.amount_centavos + COALESCE((
    SELECT SUM(amount_centavos) FROM payment_refunds
    WHERE payment_id = NEW.payment_id AND establishment_id = NEW.establishment_id
), 0) <> (
    SELECT refunded_centavos FROM order_payments
    WHERE id = NEW.payment_id AND establishment_id = NEW.establishment_id
)
BEGIN SELECT RAISE(ABORT, 'payment refund differs from aggregate refund balance'); END;

CREATE TRIGGER IF NOT EXISTS payment_refunds_append_only_update
BEFORE UPDATE ON payment_refunds
BEGIN SELECT RAISE(ABORT, 'payment refunds are append-only'); END;

CREATE TRIGGER IF NOT EXISTS payment_refunds_append_only_delete
BEFORE DELETE ON payment_refunds
WHEN EXISTS (SELECT 1 FROM order_payments WHERE id = OLD.payment_id AND establishment_id = OLD.establishment_id)
BEGIN SELECT RAISE(ABORT, 'payment refunds are append-only'); END;

CREATE TABLE IF NOT EXISTS order_payment_transitions (
    establishment_id TEXT NOT NULL,
    payment_id TEXT NOT NULL,
    version INTEGER NOT NULL CHECK (version >= 2),
    from_state TEXT NOT NULL,
    to_state TEXT NOT NULL,
    actor_kind TEXT NOT NULL,
    actor_account_id TEXT,
    causation_id TEXT,
    occurred_at TEXT NOT NULL,
    PRIMARY KEY (establishment_id, payment_id, version),
    FOREIGN KEY (establishment_id, payment_id) REFERENCES order_payments(establishment_id, id) ON DELETE CASCADE
) STRICT;

CREATE TRIGGER IF NOT EXISTS order_payment_transitions_append_only_update
BEFORE UPDATE ON order_payment_transitions
BEGIN SELECT RAISE(ABORT, 'order payment transitions are append-only'); END;

CREATE TRIGGER IF NOT EXISTS order_payment_transitions_append_only_delete
BEFORE DELETE ON order_payment_transitions
WHEN EXISTS (SELECT 1 FROM order_payments WHERE id = OLD.payment_id AND establishment_id = OLD.establishment_id)
BEGIN SELECT RAISE(ABORT, 'order payment transitions are append-only'); END;
