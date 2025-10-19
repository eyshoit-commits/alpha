-- Adds model registry tables for Postgres deployments.
CREATE TABLE IF NOT EXISTS models (
    id UUID PRIMARY KEY,
    name TEXT NOT NULL,
    provider TEXT NOT NULL,
    version TEXT NOT NULL,
    format TEXT NOT NULL,
    source_uri TEXT NOT NULL,
    checksum_sha256 TEXT,
    size_bytes BIGINT,
    stage TEXT NOT NULL,
    last_synced_at TIMESTAMPTZ,
    created_at TIMESTAMPTZ NOT NULL,
    updated_at TIMESTAMPTZ NOT NULL,
    tags TEXT,
    error_message TEXT,
    UNIQUE(name, version)
);

CREATE TABLE IF NOT EXISTS model_jobs (
    id UUID PRIMARY KEY,
    model_id UUID NOT NULL REFERENCES models(id) ON DELETE CASCADE,
    stage TEXT NOT NULL,
    progress REAL NOT NULL,
    started_at TIMESTAMPTZ NOT NULL,
    finished_at TIMESTAMPTZ,
    error_message TEXT
);

CREATE INDEX IF NOT EXISTS idx_model_jobs_model_id ON model_jobs(model_id);
