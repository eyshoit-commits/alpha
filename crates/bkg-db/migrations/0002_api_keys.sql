-- Defines persistent storage for issued API keys (hashed tokens).
CREATE TABLE IF NOT EXISTS api_keys (
    id TEXT PRIMARY KEY,
    token_hash TEXT NOT NULL UNIQUE,
    token_prefix TEXT NOT NULL,
    scope_type TEXT NOT NULL,
    scope_namespace TEXT,
    rate_limit INTEGER NOT NULL,
    created_at TEXT NOT NULL,
    last_used_at TEXT,
    expires_at TEXT,
    revoked INTEGER NOT NULL DEFAULT 0
);

CREATE INDEX IF NOT EXISTS idx_api_keys_hash ON api_keys(token_hash);
CREATE INDEX IF NOT EXISTS idx_api_keys_namespace ON api_keys(scope_namespace);
