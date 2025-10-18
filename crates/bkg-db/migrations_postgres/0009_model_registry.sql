CREATE TABLE IF NOT EXISTS model_registry (
    id UUID PRIMARY KEY,
    name TEXT NOT NULL,
    provider TEXT NOT NULL,
    version TEXT NOT NULL,
    format TEXT NOT NULL,
    source_uri TEXT NOT NULL,
    size_bytes BIGINT,
    checksum_sha256 TEXT,
    stage TEXT NOT NULL,
    last_synced_at TIMESTAMPTZ,
    created_at TIMESTAMPTZ NOT NULL,
    updated_at TIMESTAMPTZ NOT NULL,
    tags JSONB NOT NULL DEFAULT '[]'::jsonb,
    error_message TEXT
);

CREATE UNIQUE INDEX IF NOT EXISTS model_registry_unique_name
    ON model_registry(name, provider, version);

CREATE TABLE IF NOT EXISTS model_download_jobs (
    id UUID PRIMARY KEY,
    model_id UUID NOT NULL REFERENCES model_registry(id) ON DELETE CASCADE,
    stage TEXT NOT NULL,
    progress DOUBLE PRECISION NOT NULL,
    started_at TIMESTAMPTZ NOT NULL,
    finished_at TIMESTAMPTZ,
    error_message TEXT
);

CREATE INDEX IF NOT EXISTS model_download_jobs_model_id
    ON model_download_jobs(model_id);
