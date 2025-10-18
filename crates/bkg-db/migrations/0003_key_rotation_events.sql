-- Adds rotation metadata columns and queue for webhook delivery.
ALTER TABLE api_keys ADD COLUMN rotated_from TEXT;
ALTER TABLE api_keys ADD COLUMN rotated_at TEXT;

CREATE TABLE IF NOT EXISTS key_rotation_events (
    id TEXT PRIMARY KEY,
    new_key_id TEXT NOT NULL,
    previous_key_id TEXT NOT NULL,
    rotated_at TEXT NOT NULL,
    payload TEXT NOT NULL,
    signature TEXT NOT NULL,
    created_at TEXT NOT NULL,
    delivered INTEGER NOT NULL DEFAULT 0
);

CREATE INDEX IF NOT EXISTS idx_key_rotation_events_new_key ON key_rotation_events(new_key_id);
CREATE INDEX IF NOT EXISTS idx_key_rotation_events_created_at ON key_rotation_events(created_at);
