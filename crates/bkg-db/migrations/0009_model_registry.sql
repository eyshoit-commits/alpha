CREATE TABLE IF NOT EXISTS model_registry (
    id TEXT PRIMARY KEY,
    name TEXT NOT NULL,
    provider TEXT NOT NULL,
    version TEXT NOT NULL,
    format TEXT NOT NULL,
    source_uri TEXT NOT NULL,
    size_bytes INTEGER,
    checksum_sha256 TEXT,
    stage TEXT NOT NULL,
    last_synced_at TEXT,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL,
    tags TEXT NOT NULL DEFAULT '[]',
    error_message TEXT
);

CREATE UNIQUE INDEX IF NOT EXISTS model_registry_unique_name
    ON model_registry(name, provider, version);

CREATE TABLE IF NOT EXISTS model_download_jobs (
    id TEXT PRIMARY KEY,
    model_id TEXT NOT NULL,
    stage TEXT NOT NULL,
    progress REAL NOT NULL,
    started_at TEXT NOT NULL,
    finished_at TEXT,
    error_message TEXT,
    FOREIGN KEY(model_id) REFERENCES model_registry(id) ON DELETE CASCADE
);

CREATE INDEX IF NOT EXISTS model_download_jobs_model_id
    ON model_download_jobs(model_id);
