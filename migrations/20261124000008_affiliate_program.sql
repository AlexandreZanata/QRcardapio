-- Global affiliate program defaults (single-row configuration table).
CREATE TABLE affiliate_program_settings (
    id INTEGER PRIMARY KEY CHECK (id = 1),
    self_signup_enabled INTEGER NOT NULL DEFAULT 1 CHECK (self_signup_enabled IN (0, 1)),
    default_commission_type TEXT NOT NULL DEFAULT 'fixed_monthly_centavos'
        CHECK (default_commission_type IN ('fixed_monthly_centavos', 'percent_bps')),
    default_commission_value INTEGER NOT NULL DEFAULT 500
        CHECK (
            (default_commission_type = 'fixed_monthly_centavos' AND default_commission_value >= 0 AND default_commission_value <= 100000)
            OR (default_commission_type = 'percent_bps' AND default_commission_value >= 0 AND default_commission_value <= 10000)
        ),
    default_hold_days INTEGER NOT NULL DEFAULT 15
        CHECK (default_hold_days >= 0 AND default_hold_days <= 90),
    cookie_ttl_days INTEGER NOT NULL DEFAULT 30
        CHECK (cookie_ttl_days >= 1 AND cookie_ttl_days <= 180),
    updated_at TEXT NOT NULL
);

INSERT INTO affiliate_program_settings (
    id,
    self_signup_enabled,
    default_commission_type,
    default_commission_value,
    default_hold_days,
    cookie_ttl_days,
    updated_at
) VALUES (
    1,
    1,
    'fixed_monthly_centavos',
    500,
    15,
    30,
    '2026-11-24T00:00:00Z'
);

-- Enrolled affiliates (1:1 with accounts; does not require owning an establishment).
CREATE TABLE affiliates (
    id TEXT PRIMARY KEY,
    account_id TEXT NOT NULL UNIQUE REFERENCES accounts(id) ON DELETE CASCADE,
    code TEXT NOT NULL UNIQUE,
    status TEXT NOT NULL DEFAULT 'active'
        CHECK (status IN ('active', 'paused', 'revoked')),
    commission_type TEXT NOT NULL
        CHECK (commission_type IN ('fixed_monthly_centavos', 'percent_bps')),
    commission_value INTEGER NOT NULL
        CHECK (
            (commission_type = 'fixed_monthly_centavos' AND commission_value >= 0 AND commission_value <= 100000)
            OR (commission_type = 'percent_bps' AND commission_value >= 0 AND commission_value <= 10000)
        ),
    hold_days INTEGER NOT NULL DEFAULT 15
        CHECK (hold_days >= 0 AND hold_days <= 90),
    pix_key TEXT NOT NULL DEFAULT '',
    notes TEXT NOT NULL DEFAULT '',
    click_count INTEGER NOT NULL DEFAULT 0 CHECK (click_count >= 0),
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL
);

CREATE INDEX idx_affiliates_status ON affiliates (status, created_at DESC);

-- Immutable referral attribution: an account can only ever be attributed to a single affiliate.
CREATE TABLE affiliate_referrals (
    id TEXT PRIMARY KEY,
    affiliate_id TEXT NOT NULL REFERENCES affiliates(id) ON DELETE RESTRICT,
    referred_account_id TEXT NOT NULL UNIQUE REFERENCES accounts(id) ON DELETE CASCADE,
    referred_establishment_id TEXT UNIQUE REFERENCES establishments(id) ON DELETE SET NULL,
    status TEXT NOT NULL DEFAULT 'signed_up'
        CHECK (status IN ('signed_up', 'active', 'disqualified')),
    disqualified_reason TEXT,
    attributed_at TEXT NOT NULL,
    first_paid_at TEXT,
    updated_at TEXT NOT NULL
);

CREATE INDEX idx_affiliate_referrals_affiliate
    ON affiliate_referrals (affiliate_id, attributed_at DESC);

-- Manual payouts recorded by administrators.
CREATE TABLE affiliate_payouts (
    id TEXT PRIMARY KEY,
    affiliate_id TEXT NOT NULL REFERENCES affiliates(id) ON DELETE RESTRICT,
    environment TEXT NOT NULL CHECK (environment IN ('sandbox', 'production')),
    total_centavos INTEGER NOT NULL CHECK (total_centavos > 0),
    entries_count INTEGER NOT NULL CHECK (entries_count > 0),
    pix_key_snapshot TEXT NOT NULL DEFAULT '',
    reference TEXT NOT NULL DEFAULT '',
    notes TEXT NOT NULL DEFAULT '',
    paid_by_account_id TEXT NOT NULL REFERENCES accounts(id) ON DELETE RESTRICT,
    paid_at TEXT NOT NULL,
    created_at TEXT NOT NULL
);

CREATE INDEX idx_affiliate_payouts_affiliate
    ON affiliate_payouts (affiliate_id, environment, paid_at DESC);

-- Immutable financial commission ledger (monthly installments + negative clawbacks).
CREATE TABLE affiliate_commissions (
    id TEXT PRIMARY KEY,
    environment TEXT NOT NULL CHECK (environment IN ('sandbox', 'production')),
    affiliate_id TEXT NOT NULL REFERENCES affiliates(id) ON DELETE RESTRICT,
    referral_id TEXT NOT NULL REFERENCES affiliate_referrals(id) ON DELETE RESTRICT,
    establishment_id TEXT NOT NULL REFERENCES establishments(id) ON DELETE RESTRICT,
    payment_id TEXT NOT NULL,
    subscription_id TEXT NOT NULL,
    entry_kind TEXT NOT NULL CHECK (entry_kind IN ('commission', 'clawback')),
    installment_index INTEGER NOT NULL CHECK (installment_index >= 1 AND installment_index <= 12),
    billing_cycle_months INTEGER NOT NULL CHECK (billing_cycle_months IN (1, 6, 12)),
    gross_payment_centavos INTEGER NOT NULL CHECK (gross_payment_centavos > 0),
    commission_centavos INTEGER NOT NULL CHECK (
        (entry_kind = 'commission' AND commission_centavos > 0)
        OR (entry_kind = 'clawback' AND commission_centavos < 0)
    ),
    snapshot_commission_type TEXT NOT NULL
        CHECK (snapshot_commission_type IN ('fixed_monthly_centavos', 'percent_bps')),
    snapshot_commission_value INTEGER NOT NULL CHECK (snapshot_commission_value >= 0),
    status TEXT NOT NULL CHECK (status IN ('pending', 'available', 'paid', 'voided')),
    available_on TEXT NOT NULL,
    payout_id TEXT REFERENCES affiliate_payouts(id) ON DELETE RESTRICT,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL,
    UNIQUE (environment, payment_id, installment_index, entry_kind)
);

CREATE INDEX idx_affiliate_commissions_affiliate
    ON affiliate_commissions (affiliate_id, environment, status, available_on);

CREATE INDEX idx_affiliate_commissions_payment
    ON affiliate_commissions (environment, payment_id);

CREATE INDEX idx_affiliate_commissions_payout
    ON affiliate_commissions (payout_id) WHERE payout_id IS NOT NULL;

-- Tracks linked, validly-confirmed payments already evaluated for affiliate accrual
-- so later Asaas settlement events (e.g. PAYMENT_RECEIVED 30 days after PAYMENT_CONFIRMED)
-- never retroactively grant commissions for a payment that was ineligible on Day 0.
CREATE TABLE affiliate_evaluated_payments (
    environment TEXT NOT NULL CHECK (environment IN ('sandbox', 'production')),
    payment_id TEXT NOT NULL,
    evaluated_at TEXT NOT NULL,
    PRIMARY KEY (environment, payment_id)
) WITHOUT ROWID;
