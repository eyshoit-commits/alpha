-- Postgres schema for Row-Level Security policy storage.
CREATE TABLE IF NOT EXISTS rls_policies (
    id UUID PRIMARY KEY,
    table_name TEXT NOT NULL,
    policy_name TEXT NOT NULL,
    expression JSONB NOT NULL,
    created_at TIMESTAMPTZ NOT NULL,
    updated_at TIMESTAMPTZ NOT NULL,
    UNIQUE(table_name, policy_name)
);

CREATE INDEX IF NOT EXISTS idx_rls_policies_table ON rls_policies(table_name);
