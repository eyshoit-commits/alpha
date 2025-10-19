CREATE TABLE IF NOT EXISTS audit_events (
    id TEXT PRIMARY KEY,
    namespace TEXT,
    actor TEXT,
    event_type TEXT NOT NULL,
    recorded_at TEXT NOT NULL,
    payload TEXT NOT NULL,
    signature_valid INTEGER
);

CREATE INDEX IF NOT EXISTS audit_events_recorded_at
    ON audit_events(recorded_at DESC);

CREATE INDEX IF NOT EXISTS audit_events_namespace
    ON audit_events(namespace);

CREATE INDEX IF NOT EXISTS audit_events_event_type
    ON audit_events(event_type);
