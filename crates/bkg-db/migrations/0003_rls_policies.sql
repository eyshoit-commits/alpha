-- Stores Row-Level Security policies for tables managed by bkg-db.
CREATE TABLE IF NOT EXISTS rls_policies (
    id TEXT PRIMARY KEY,
    table_name TEXT NOT NULL,
    policy_name TEXT NOT NULL,
    expression TEXT NOT NULL,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL,
    UNIQUE(table_name, policy_name)
);

CREATE INDEX IF NOT EXISTS idx_rls_policies_table ON rls_policies(table_name);
