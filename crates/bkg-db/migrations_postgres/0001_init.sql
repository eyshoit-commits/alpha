-- Postgres schema for sandbox catalog and execution audit tables.
CREATE TABLE IF NOT EXISTS sandboxes (
    id UUID PRIMARY KEY,
    namespace TEXT NOT NULL,
    name TEXT NOT NULL,
    runtime TEXT NOT NULL,
    status TEXT NOT NULL,
    cpu_limit_millis INTEGER NOT NULL,
    memory_limit_bytes BIGINT NOT NULL,
    disk_limit_bytes BIGINT NOT NULL,
    timeout_seconds INTEGER NOT NULL,
    created_at TIMESTAMPTZ NOT NULL,
    updated_at TIMESTAMPTZ NOT NULL,
    last_started_at TIMESTAMPTZ,
    last_stopped_at TIMESTAMPTZ,
    UNIQUE(namespace, name)
);

CREATE TABLE IF NOT EXISTS sandbox_executions (
    id BIGSERIAL PRIMARY KEY,
    sandbox_id UUID NOT NULL REFERENCES sandboxes(id) ON DELETE CASCADE,
    executed_at TIMESTAMPTZ NOT NULL,
    command TEXT NOT NULL,
    args JSONB NOT NULL,
    exit_code INTEGER,
    stdout TEXT,
    stderr TEXT,
    duration_ms BIGINT NOT NULL,
    timed_out BOOLEAN NOT NULL DEFAULT FALSE
);

CREATE INDEX IF NOT EXISTS idx_sandboxes_namespace ON sandboxes(namespace);
CREATE INDEX IF NOT EXISTS idx_sandbox_exec_sandbox_id ON sandbox_executions(sandbox_id);
