-- Commercial Engine + Entitlement Engine foundation (expand phase, ADR-010).
-- Idempotent: only CREATE ... IF NOT EXISTS, DROP/CREATE of a view and INSERT ... ON CONFLICT DO
-- NOTHING; no ALTER TABLE.
-- Legacy billing tables (billing_checkouts, billing_payments, billing_coverage) stay unchanged and
-- remain the gateway source of truth. No charge, checkout or gateway call is created here.

-- Tenant-consistent parent key for commercial contracts.
CREATE UNIQUE INDEX IF NOT EXISTS idx_billing_checkouts_tenant_reference
    ON billing_checkouts (reference, environment, establishment_id);

-- Payments by gateway subscription (entitlement periods, coverage and affiliate sync).
CREATE INDEX IF NOT EXISTS idx_billing_payments_subscription
    ON billing_payments (environment, subscription_id);

-- Same columns and semantics as 20261123000006, with an indexed join order: the previous plan
-- scanned every payment of the environment before filtering by establishment.
DROP VIEW IF EXISTS billing_coverage;
CREATE VIEW billing_coverage AS
SELECT c.environment, c.establishment_id, c.plan_months, c.price_centavos, p.due_date,
       date(p.due_date, printf('+%d months', c.plan_months), 'floor') AS paid_through
FROM billing_checkouts c
CROSS JOIN billing_payments p INDEXED BY idx_billing_payments_subscription
WHERE p.environment = c.environment AND p.subscription_id = c.subscription_id
  AND p.confirmed_centavos IS NOT NULL AND p.confirmed_centavos = c.price_centavos;

-- Registry of catalog offers seen by running binaries (lifecycle transition guard, ADR-007).
CREATE TABLE IF NOT EXISTS commercial_catalog_offers (
    offer_kind TEXT NOT NULL CHECK (offer_kind IN ('module', 'bundle', 'addon')),
    offer_code TEXT NOT NULL,
    lifecycle TEXT NOT NULL CHECK (lifecycle IN (
        'planned', 'development', 'internal_preview', 'beta', 'available', 'deprecated', 'retired'
    )),
    current_version INTEGER CHECK (current_version IS NULL OR current_version >= 1),
    first_recorded_at TEXT NOT NULL,
    updated_at TEXT NOT NULL,
    PRIMARY KEY (offer_kind, offer_code)
) WITHOUT ROWID;

-- Immutable content hash of every version that is or was sellable.
CREATE TABLE IF NOT EXISTS commercial_catalog_versions (
    offer_kind TEXT NOT NULL,
    offer_code TEXT NOT NULL,
    version INTEGER NOT NULL CHECK (version >= 1),
    content_hash TEXT NOT NULL CHECK (length(content_hash) = 64),
    recorded_at TEXT NOT NULL,
    PRIMARY KEY (offer_kind, offer_code, version),
    FOREIGN KEY (offer_kind, offer_code)
        REFERENCES commercial_catalog_offers (offer_kind, offer_code) ON DELETE RESTRICT
) WITHOUT ROWID;

-- One consolidated, immutable commercial contract per checkout (snapshot of the terms).
CREATE TABLE IF NOT EXISTS commercial_contracts (
    id TEXT PRIMARY KEY NOT NULL,
    environment TEXT NOT NULL CHECK (environment IN ('sandbox', 'production')),
    establishment_id TEXT NOT NULL REFERENCES establishments(id) ON DELETE CASCADE,
    billing_reference TEXT UNIQUE,
    origin TEXT NOT NULL CHECK (origin IN ('checkout', 'legacy_plan')),
    cycle_code TEXT NOT NULL CHECK (cycle_code IN ('monthly', 'semiannual', 'annual')),
    cycle_months INTEGER NOT NULL CHECK (
        (cycle_code = 'monthly' AND cycle_months = 1)
        OR (cycle_code = 'semiannual' AND cycle_months = 6)
        OR (cycle_code = 'annual' AND cycle_months = 12)
    ),
    cycle_discount_bps INTEGER NOT NULL CHECK (cycle_discount_bps >= 0 AND cycle_discount_bps < 10000),
    currency TEXT NOT NULL CHECK (currency = 'BRL'),
    list_total_centavos INTEGER NOT NULL CHECK (list_total_centavos > 0),
    total_centavos INTEGER NOT NULL CHECK (total_centavos > 0 AND total_centavos <= list_total_centavos),
    affiliate_eligible_centavos INTEGER NOT NULL
        CHECK (affiliate_eligible_centavos >= 0 AND affiliate_eligible_centavos <= total_centavos),
    activated_at TEXT,
    created_at TEXT NOT NULL,
    FOREIGN KEY (billing_reference, environment, establishment_id)
        REFERENCES billing_checkouts (reference, environment, establishment_id) ON DELETE CASCADE
);

CREATE INDEX IF NOT EXISTS idx_commercial_contracts_establishment
    ON commercial_contracts (establishment_id, environment, created_at DESC);

CREATE TABLE IF NOT EXISTS commercial_contract_items (
    contract_id TEXT NOT NULL REFERENCES commercial_contracts(id) ON DELETE CASCADE,
    line_no INTEGER NOT NULL CHECK (line_no >= 1),
    offer_kind TEXT NOT NULL CHECK (offer_kind IN ('module', 'bundle', 'addon')),
    offer_code TEXT NOT NULL,
    offer_version INTEGER NOT NULL CHECK (offer_version >= 1),
    monthly_price_centavos INTEGER NOT NULL CHECK (monthly_price_centavos > 0),
    list_price_centavos INTEGER NOT NULL CHECK (list_price_centavos > 0),
    discount_bps INTEGER NOT NULL CHECK (discount_bps >= 0 AND discount_bps < 10000),
    price_centavos INTEGER NOT NULL CHECK (price_centavos > 0 AND price_centavos <= list_price_centavos),
    affiliate_eligible INTEGER NOT NULL CHECK (affiliate_eligible IN (0, 1)),
    PRIMARY KEY (contract_id, line_no),
    UNIQUE (contract_id, offer_kind, offer_code)
) WITHOUT ROWID;

CREATE TABLE IF NOT EXISTS commercial_contract_capabilities (
    contract_id TEXT NOT NULL,
    line_no INTEGER NOT NULL,
    offer_code TEXT NOT NULL,
    capability TEXT NOT NULL,
    PRIMARY KEY (contract_id, line_no, capability),
    FOREIGN KEY (contract_id, line_no)
        REFERENCES commercial_contract_items (contract_id, line_no) ON DELETE CASCADE
) WITHOUT ROWID;

-- Explicit, audited grants (trial, beta, internal preview, courtesy). Absent rows grant nothing.
CREATE TABLE IF NOT EXISTS entitlement_grants (
    id TEXT PRIMARY KEY NOT NULL,
    establishment_id TEXT NOT NULL REFERENCES establishments(id) ON DELETE CASCADE,
    offer_kind TEXT NOT NULL CHECK (offer_kind IN ('module', 'addon')),
    offer_code TEXT NOT NULL,
    grant_kind TEXT NOT NULL CHECK (grant_kind IN ('trial', 'beta', 'internal_preview', 'courtesy')),
    starts_on TEXT NOT NULL,
    ends_on TEXT NOT NULL,
    reason TEXT NOT NULL CHECK (length(reason) BETWEEN 3 AND 400),
    granted_by_account_id TEXT NOT NULL REFERENCES accounts(id) ON DELETE RESTRICT,
    created_at TEXT NOT NULL,
    revoked_at TEXT,
    revoked_by_account_id TEXT REFERENCES accounts(id) ON DELETE RESTRICT,
    CHECK (ends_on > starts_on),
    CHECK ((revoked_at IS NULL) = (revoked_by_account_id IS NULL))
);

CREATE INDEX IF NOT EXISTS idx_entitlement_grants_establishment
    ON entitlement_grants (establishment_id, ends_on);

-- At most one trial per establishment and offer, ever.
CREATE UNIQUE INDEX IF NOT EXISTS idx_entitlement_grants_one_trial
    ON entitlement_grants (establishment_id, offer_code) WHERE grant_kind = 'trial';

-- Operational state independent of billing. Missing row = active.
CREATE TABLE IF NOT EXISTS establishment_operational_states (
    establishment_id TEXT PRIMARY KEY NOT NULL REFERENCES establishments(id) ON DELETE CASCADE,
    status TEXT NOT NULL CHECK (status IN ('active', 'suspended_admin', 'suspended_security')),
    reason TEXT NOT NULL CHECK (length(reason) BETWEEN 3 AND 400),
    changed_by_account_id TEXT NOT NULL REFERENCES accounts(id) ON DELETE RESTRICT,
    changed_at TEXT NOT NULL
) WITHOUT ROWID;

-- Transactional outbox (ADR-009). Appended in the same transaction as the state change.
CREATE TABLE IF NOT EXISTS domain_events (
    seq INTEGER PRIMARY KEY AUTOINCREMENT,
    event_id TEXT NOT NULL UNIQUE,
    event_type TEXT NOT NULL,
    event_version INTEGER NOT NULL CHECK (event_version >= 1),
    establishment_id TEXT NOT NULL REFERENCES establishments(id) ON DELETE CASCADE,
    aggregate_type TEXT NOT NULL,
    aggregate_id TEXT NOT NULL,
    correlation_id TEXT NOT NULL,
    causation_id TEXT,
    occurred_at TEXT NOT NULL,
    payload TEXT NOT NULL CHECK (json_valid(payload)),
    dedupe_key TEXT UNIQUE
);

CREATE INDEX IF NOT EXISTS idx_domain_events_aggregate
    ON domain_events (aggregate_type, aggregate_id, seq);
CREATE INDEX IF NOT EXISTS idx_domain_events_establishment
    ON domain_events (establishment_id, seq);

-- Paid coverage per contract capability. Same period rule as billing_coverage, plus the
-- contract snapshot must match the gateway amount exactly. CROSS JOIN fixes the join order so
-- lookups by establishment or contract start from commercial_contracts (indexed) instead of
-- scanning every payment of the environment.
CREATE VIEW IF NOT EXISTS commercial_entitlement_periods AS
SELECT k.environment, k.establishment_id, k.id AS contract_id, cap.offer_code, cap.capability,
       p.payment_id, p.due_date AS starts_on,
       date(p.due_date, printf('+%d months', c.plan_months), 'floor') AS ends_on,
       CASE WHEN c.subscription_status = 'active' THEN 1 ELSE 0 END AS renewing
FROM commercial_contracts k
CROSS JOIN billing_checkouts c
CROSS JOIN billing_payments p INDEXED BY idx_billing_payments_subscription
CROSS JOIN commercial_contract_capabilities cap
WHERE c.reference = k.billing_reference
  AND c.environment = k.environment
  AND c.establishment_id = k.establishment_id
  AND p.environment = c.environment
  AND p.subscription_id = c.subscription_id
  AND cap.contract_id = k.id
  AND p.confirmed_centavos IS NOT NULL
  AND p.confirmed_centavos = c.price_centavos
  AND c.price_centavos = k.total_centavos
  AND c.plan_months = k.cycle_months;

-- Backfill: every legacy "Plano Essencial" checkout becomes a QR Menu v1 contract that keeps the
-- contracted price, cycle, Asaas identifiers (via billing_reference) and paid periods.
-- The legacy list price was R$ 29,90 per month with 0/5/10% cycle discounts.
INSERT INTO commercial_contracts (
    id, environment, establishment_id, billing_reference, origin, cycle_code, cycle_months,
    cycle_discount_bps, currency, list_total_centavos, total_centavos, affiliate_eligible_centavos,
    activated_at, created_at
)
SELECT c.reference, c.environment, c.establishment_id, c.reference, 'legacy_plan',
       CASE c.plan_months WHEN 1 THEN 'monthly' WHEN 6 THEN 'semiannual' ELSE 'annual' END,
       c.plan_months,
       CASE c.plan_months WHEN 1 THEN 0 WHEN 6 THEN 500 ELSE 1000 END,
       'BRL',
       MAX(2990 * c.plan_months, c.price_centavos),
       c.price_centavos,
       c.price_centavos,
       CASE WHEN EXISTS (
           SELECT 1 FROM billing_payments p
           WHERE p.environment = c.environment AND p.subscription_id = c.subscription_id
             AND p.confirmed_centavos = c.price_centavos
       ) THEN c.created_at END,
       c.created_at
FROM billing_checkouts c
WHERE NOT EXISTS (SELECT 1 FROM commercial_contracts k WHERE k.billing_reference = c.reference)
ON CONFLICT DO NOTHING;

INSERT INTO commercial_contract_items (
    contract_id, line_no, offer_kind, offer_code, offer_version, monthly_price_centavos,
    list_price_centavos, discount_bps, price_centavos, affiliate_eligible
)
SELECT k.id, 1, 'module', 'menu', 1, 2990, k.list_total_centavos, k.cycle_discount_bps,
       k.total_centavos, 1
FROM commercial_contracts k
WHERE k.origin = 'legacy_plan'
ON CONFLICT DO NOTHING;

INSERT INTO commercial_contract_capabilities (contract_id, line_no, offer_code, capability)
SELECT k.id, 1, 'menu', 'menu.publish'
FROM commercial_contracts k
WHERE k.origin = 'legacy_plan'
ON CONFLICT DO NOTHING;
