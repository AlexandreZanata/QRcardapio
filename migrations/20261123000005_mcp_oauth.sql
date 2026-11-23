CREATE TABLE mcp_authorizations (
    id TEXT PRIMARY KEY NOT NULL,
    browser_hash TEXT NOT NULL,
    client_id TEXT NOT NULL,
    redirect_uri TEXT NOT NULL,
    state TEXT,
    challenge TEXT NOT NULL,
    resource TEXT NOT NULL,
    scope TEXT NOT NULL CHECK (scope IN ('menu.read', 'menu.read menu.write')),
    expires_at INTEGER NOT NULL
);
CREATE INDEX idx_mcp_authorizations_expiry ON mcp_authorizations (expires_at);

CREATE TABLE mcp_codes (
    code_hash TEXT PRIMARY KEY NOT NULL,
    establishment_id TEXT NOT NULL,
    account_id TEXT NOT NULL,
    client_id TEXT NOT NULL,
    redirect_uri TEXT NOT NULL,
    challenge TEXT NOT NULL,
    resource TEXT NOT NULL,
    scope TEXT NOT NULL CHECK (scope IN ('menu.read', 'menu.read menu.write')),
    expires_at INTEGER NOT NULL,
    FOREIGN KEY (establishment_id, account_id) REFERENCES establishment_members (establishment_id, account_id) ON DELETE CASCADE
);
CREATE INDEX idx_mcp_codes_expiry ON mcp_codes (expires_at);

CREATE TABLE mcp_grants (
    id TEXT PRIMARY KEY NOT NULL,
    establishment_id TEXT NOT NULL,
    account_id TEXT NOT NULL,
    client_id TEXT NOT NULL,
    resource TEXT NOT NULL,
    scope TEXT NOT NULL CHECK (scope IN ('menu.read', 'menu.read menu.write')),
    access_hash TEXT NOT NULL UNIQUE,
    refresh_hash TEXT NOT NULL UNIQUE,
    access_expires_at INTEGER NOT NULL,
    refresh_expires_at INTEGER NOT NULL,
    created_at TEXT NOT NULL,
    FOREIGN KEY (establishment_id, account_id) REFERENCES establishment_members (establishment_id, account_id) ON DELETE CASCADE,
    UNIQUE (establishment_id, account_id, client_id)
);
CREATE INDEX idx_mcp_grants_expiry ON mcp_grants (refresh_expires_at);

CREATE TABLE mcp_menu_operations (
    establishment_id TEXT NOT NULL REFERENCES establishments(id) ON DELETE CASCADE,
    account_id TEXT NOT NULL REFERENCES accounts(id) ON DELETE CASCADE,
    operation_key TEXT NOT NULL,
    input_hash TEXT NOT NULL,
    result_json TEXT NOT NULL,
    expires_at INTEGER NOT NULL,
    PRIMARY KEY (establishment_id, account_id, operation_key)
) WITHOUT ROWID;
CREATE INDEX idx_mcp_operations_expiry ON mcp_menu_operations (expires_at);
