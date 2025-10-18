-- Adds rotation metadata columns and webhook event queue for Postgres deployments.
ALTER TABLE api_keys ADD COLUMN IF NOT EXISTS rotated_from UUID;
ALTER TABLE api_keys ADD COLUMN IF NOT EXISTS rotated_at TIMESTAMPTZ;

CREATE TABLE IF NOT EXISTS key_rotation_events (
    id UUID PRIMARY KEY,
    new_key_id UUID NOT NULL REFERENCES api_keys(id) ON DELETE CASCADE,
    previous_key_id UUID NOT NULL REFERENCES api_keys(id) ON DELETE CASCADE,
    rotated_at TIMESTAMPTZ NOT NULL,
    payload JSONB NOT NULL,
    signature TEXT NOT NULL,
    created_at TIMESTAMPTZ NOT NULL,
    delivered BOOLEAN NOT NULL DEFAULT FALSE
);

CREATE INDEX IF NOT EXISTS idx_key_rotation_events_new_key
    ON key_rotation_events(new_key_id);
CREATE INDEX IF NOT EXISTS idx_key_rotation_events_created_at
    ON key_rotation_events(created_at);
