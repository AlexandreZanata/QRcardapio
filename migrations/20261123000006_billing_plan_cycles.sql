ALTER TABLE billing_checkouts ADD COLUMN plan_months INTEGER NOT NULL DEFAULT 1
    CHECK (plan_months IN (1, 6, 12));
ALTER TABLE billing_checkouts ADD COLUMN price_centavos INTEGER NOT NULL DEFAULT 2990
    CHECK (price_centavos > 0);

ALTER TABLE billing_payments ADD COLUMN confirmed_centavos INTEGER
    CHECK (confirmed_centavos IS NULL OR confirmed_centavos > 0);
UPDATE billing_payments SET confirmed_centavos = 2990 WHERE paid_through IS NOT NULL;
DROP INDEX idx_billing_paid_period;
ALTER TABLE billing_payments DROP COLUMN paid_through;
CREATE INDEX idx_billing_paid_period ON billing_payments (environment, subscription_id, due_date DESC)
    WHERE confirmed_centavos IS NOT NULL;

CREATE VIEW billing_coverage AS
SELECT c.environment, c.establishment_id, c.plan_months, c.price_centavos, p.due_date,
       date(p.due_date, printf('+%d months', c.plan_months), 'floor') AS paid_through
FROM billing_checkouts c
JOIN billing_payments p ON p.environment = c.environment AND p.subscription_id = c.subscription_id
WHERE p.confirmed_centavos IS NOT NULL AND p.confirmed_centavos = c.price_centavos;
