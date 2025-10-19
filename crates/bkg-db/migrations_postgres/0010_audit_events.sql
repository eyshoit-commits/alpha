-- Adds audit event storage for Postgres deployments.
CREATE TABLE IF NOT EXISTS audit_events (
    id UUID PRIMARY KEY,
    namespace TEXT,
    actor TEXT,
    event_type TEXT NOT NULL,
    recorded_at TIMESTAMPTZ NOT NULL,
    payload TEXT NOT NULL,
    signature_valid BOOLEAN
);

CREATE INDEX IF NOT EXISTS idx_audit_events_recorded_at ON audit_events(recorded_at);
CREATE INDEX IF NOT EXISTS idx_audit_events_event_type ON audit_events(event_type);
CREATE INDEX IF NOT EXISTS idx_audit_events_namespace ON audit_events(namespace);
