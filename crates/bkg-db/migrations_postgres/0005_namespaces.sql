-- Catalog of namespaces managed by bkg-db for sandbox isolation (Postgres version).
CREATE TABLE IF NOT EXISTS namespaces (
    id UUID PRIMARY KEY,
    code TEXT NOT NULL UNIQUE,
    display_name TEXT NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX IF NOT EXISTS idx_namespaces_code ON namespaces(code);
