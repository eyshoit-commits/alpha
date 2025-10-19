-- Adds model registry tables and audit event storage.
CREATE TABLE IF NOT EXISTS models (
    id TEXT PRIMARY KEY,
    name TEXT NOT NULL,
    provider TEXT NOT NULL,
    version TEXT NOT NULL,
    format TEXT NOT NULL,
    source_uri TEXT NOT NULL,
    checksum_sha256 TEXT,
    size_bytes INTEGER,
    stage TEXT NOT NULL,
    last_synced_at TEXT,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL,
    tags TEXT,
    error_message TEXT,
    UNIQUE(name, version)
);

CREATE TABLE IF NOT EXISTS model_jobs (
    id TEXT PRIMARY KEY,
    model_id TEXT NOT NULL,
    stage TEXT NOT NULL,
    progress REAL NOT NULL,
    started_at TEXT NOT NULL,
    finished_at TEXT,
    error_message TEXT,
    FOREIGN KEY (model_id) REFERENCES models(id) ON DELETE CASCADE
);

CREATE INDEX IF NOT EXISTS idx_model_jobs_model_id ON model_jobs(model_id);

CREATE TABLE IF NOT EXISTS audit_events (
    id TEXT PRIMARY KEY,
    namespace TEXT,
    actor TEXT,
    event_type TEXT NOT NULL,
    recorded_at TEXT NOT NULL,
    payload TEXT NOT NULL,
    signature_valid INTEGER
);

CREATE INDEX IF NOT EXISTS idx_audit_events_recorded_at ON audit_events(recorded_at);
CREATE INDEX IF NOT EXISTS idx_audit_events_event_type ON audit_events(event_type);
CREATE INDEX IF NOT EXISTS idx_audit_events_namespace ON audit_events(namespace);
