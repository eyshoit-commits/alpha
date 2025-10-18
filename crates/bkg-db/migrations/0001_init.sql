-- Defines core tables for sandbox metadata and execution audit logs.
CREATE TABLE IF NOT EXISTS sandboxes (
    id TEXT PRIMARY KEY,
    namespace TEXT NOT NULL,
    name TEXT NOT NULL,
    runtime TEXT NOT NULL,
    status TEXT NOT NULL,
    cpu_limit_millis INTEGER NOT NULL,
    memory_limit_bytes INTEGER NOT NULL,
    disk_limit_bytes INTEGER NOT NULL,
    timeout_seconds INTEGER NOT NULL,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL,
    last_started_at TEXT,
    last_stopped_at TEXT,
    UNIQUE(namespace, name)
);

CREATE TABLE IF NOT EXISTS sandbox_executions (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    sandbox_id TEXT NOT NULL,
    executed_at TEXT NOT NULL,
    command TEXT NOT NULL,
    args TEXT NOT NULL,
    exit_code INTEGER,
    stdout TEXT,
    stderr TEXT,
    duration_ms INTEGER NOT NULL,
    timed_out INTEGER NOT NULL,
    FOREIGN KEY (sandbox_id) REFERENCES sandboxes(id) ON DELETE CASCADE
);

CREATE INDEX IF NOT EXISTS idx_sandboxes_namespace ON sandboxes(namespace);
CREATE INDEX IF NOT EXISTS idx_sandbox_exec_sandbox_id ON sandbox_executions(sandbox_id);
