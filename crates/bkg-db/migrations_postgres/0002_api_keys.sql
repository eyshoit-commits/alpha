-- Postgres schema for persistent API key storage.
CREATE TABLE IF NOT EXISTS api_keys (
    id UUID PRIMARY KEY,
    token_hash TEXT NOT NULL UNIQUE,
    token_prefix TEXT NOT NULL,
    scope_type TEXT NOT NULL,
    scope_namespace TEXT,
    rate_limit INTEGER NOT NULL,
    created_at TIMESTAMPTZ NOT NULL,
    last_used_at TIMESTAMPTZ,
    expires_at TIMESTAMPTZ,
    revoked BOOLEAN NOT NULL DEFAULT FALSE
);

CREATE INDEX IF NOT EXISTS idx_api_keys_hash ON api_keys(token_hash);
CREATE INDEX IF NOT EXISTS idx_api_keys_namespace ON api_keys(scope_namespace);
