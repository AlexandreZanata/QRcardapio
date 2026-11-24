-- Multi-provider SaaS billing: Asaas (card), Stripe (card) and Banco Inter (Pix/boleto per cycle).
-- Expand-only (ADR-010): new nullable/defaulted columns and new tables. Existing rows keep
-- provider 'asaas' and method 'card'. Older binaries ignore the new columns and tables.

ALTER TABLE billing_checkouts ADD COLUMN provider TEXT NOT NULL DEFAULT 'asaas'
    CHECK (provider IN ('asaas', 'stripe', 'inter'));
ALTER TABLE billing_checkouts ADD COLUMN payment_method TEXT NOT NULL DEFAULT 'card'
    CHECK (payment_method IN ('card', 'pix_boleto'));
-- Hosted checkout URL when the provider does not derive it from the id (Stripe).
ALTER TABLE billing_checkouts ADD COLUMN checkout_url TEXT
    CHECK (checkout_url IS NULL OR checkout_url LIKE 'https://%');

-- Amount originally settled, so a won dispute can restore coverage after a reversal.
ALTER TABLE billing_payments ADD COLUMN settled_centavos INTEGER
    CHECK (settled_centavos IS NULL OR settled_centavos > 0);

-- Payer identification required by bank-issued charges (Inter). Personal data under LGPD:
-- tenant-scoped, owner-managed, never copied into events or logs.
CREATE TABLE IF NOT EXISTS billing_payer_profiles (
    establishment_id TEXT PRIMARY KEY NOT NULL REFERENCES establishments(id) ON DELETE CASCADE,
    person_kind TEXT NOT NULL CHECK (person_kind IN ('FISICA', 'JURIDICA')),
    document TEXT NOT NULL CHECK (
        (person_kind = 'FISICA' AND length(document) = 11)
        OR (person_kind = 'JURIDICA' AND length(document) = 14)
    ),
    legal_name TEXT NOT NULL CHECK (length(legal_name) BETWEEN 3 AND 100),
    email TEXT NOT NULL,
    postal_code TEXT NOT NULL CHECK (length(postal_code) = 8),
    street TEXT NOT NULL,
    number TEXT NOT NULL,
    complement TEXT NOT NULL DEFAULT '',
    district TEXT NOT NULL,
    city TEXT NOT NULL,
    state TEXT NOT NULL CHECK (length(state) = 2),
    updated_by_account_id TEXT NOT NULL REFERENCES accounts(id) ON DELETE RESTRICT,
    updated_at TEXT NOT NULL
) WITHOUT ROWID;

-- One Inter charge (BolePix) per subscription cycle. The row is reserved before the API call so
-- a timeout never issues a second charge for the same cycle.
CREATE TABLE IF NOT EXISTS billing_inter_charges (
    environment TEXT NOT NULL CHECK (environment IN ('sandbox', 'production')),
    our_number TEXT NOT NULL CHECK (length(our_number) BETWEEN 1 AND 15),
    billing_reference TEXT NOT NULL REFERENCES billing_checkouts(reference) ON DELETE CASCADE,
    establishment_id TEXT NOT NULL REFERENCES establishments(id) ON DELETE CASCADE,
    cycle_index INTEGER NOT NULL CHECK (cycle_index >= 0),
    due_date TEXT NOT NULL,
    amount_centavos INTEGER NOT NULL CHECK (amount_centavos > 0),
    request_code TEXT,
    status TEXT NOT NULL DEFAULT 'reserved',
    pix_copy_paste TEXT,
    digitable_line TEXT,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL,
    PRIMARY KEY (environment, our_number),
    UNIQUE (billing_reference, cycle_index)
) WITHOUT ROWID;

CREATE UNIQUE INDEX IF NOT EXISTS idx_billing_inter_charges_request
    ON billing_inter_charges (environment, request_code) WHERE request_code IS NOT NULL;
CREATE INDEX IF NOT EXISTS idx_billing_inter_charges_establishment
    ON billing_inter_charges (establishment_id, due_date DESC);
CREATE INDEX IF NOT EXISTS idx_billing_inter_charges_open
    ON billing_inter_charges (environment, status, due_date);
