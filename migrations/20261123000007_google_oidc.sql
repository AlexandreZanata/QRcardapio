CREATE TABLE google_identities (
    subject TEXT PRIMARY KEY NOT NULL,
    account_id TEXT NOT NULL UNIQUE REFERENCES accounts(id) ON DELETE CASCADE,
    linked_at INTEGER NOT NULL
);

CREATE TABLE google_oauth_flows (
    state_hash TEXT PRIMARY KEY NOT NULL,
    browser_hash TEXT NOT NULL,
    nonce_hash TEXT NOT NULL,
    code_verifier TEXT NOT NULL,
    client_id TEXT NOT NULL,
    redirect_uri TEXT NOT NULL,
    purpose TEXT NOT NULL CHECK (purpose IN ('signin', 'plan', 'mcp', 'link')),
    plan_key TEXT CHECK (plan_key IN ('monthly', 'semiannual', 'annual')),
    mcp_flow_id TEXT REFERENCES mcp_authorizations(id) ON DELETE CASCADE,
    local_session_id TEXT REFERENCES sessions(id) ON DELETE CASCADE,
    expires_at INTEGER NOT NULL,
    CHECK ((purpose = 'plan') = (plan_key IS NOT NULL)),
    CHECK ((purpose = 'mcp') = (mcp_flow_id IS NOT NULL)),
    CHECK ((purpose = 'link') = (local_session_id IS NOT NULL))
);
CREATE INDEX idx_google_oauth_expiry ON google_oauth_flows (expires_at);
CREATE INDEX idx_google_oauth_mcp_flow ON google_oauth_flows (mcp_flow_id);
CREATE INDEX idx_google_oauth_session ON google_oauth_flows (local_session_id);
