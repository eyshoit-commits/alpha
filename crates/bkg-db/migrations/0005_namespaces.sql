-- Catalog of namespaces managed by bkg-db for sandbox isolation.
CREATE TABLE IF NOT EXISTS namespaces (
    id TEXT PRIMARY KEY,
    code TEXT NOT NULL UNIQUE,
    display_name TEXT NOT NULL,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_namespaces_code ON namespaces(code);
