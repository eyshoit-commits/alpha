//! BKG persistence layer providing sandbox metadata & execution audit storage.
//!
//! This crate offers an async API around SQLite (sqlx) tailored for the
//! Phase‑0 requirements in the README. It encodes a namespace scoped sandbox
//! catalog, lifecycle timestamps, and execution audit events that the CAVE
//! kernel can persist while orchestrating runtimes.

use std::{path::Path, str::FromStr, sync::Arc, time::Duration};

use anyhow::{anyhow, Context, Result};
use async_trait::async_trait;
use chrono::{DateTime, Utc};
use futures::TryStreamExt;
use parking_lot::RwLock;
use serde::{Deserialize, Serialize};
use sqlx::{
    sqlite::{SqliteConnectOptions, SqlitePoolOptions, SqliteRow},
    Row, SqlitePool,
};
use thiserror::Error;
use tokio::task::JoinHandle;
use uuid::Uuid;

/// Default SQLite busy timeout in milliseconds when the DB is under load.
const SQLITE_BUSY_TIMEOUT_MS: u64 = 5_000;

/// Primary entry point to the persistence layer.
#[derive(Clone, Debug)]
pub struct Database {
    pool: SqlitePool,
}

impl Database {
    /// Establishes (or creates) a connection pool to the SQLite database located at
    /// the given URL (e.g. `sqlite:///var/lib/bkg/bkg.db`).
    pub async fn connect(database_url: &str) -> Result<Self> {
        let options = SqliteConnectOptions::from_str(database_url)?
            .create_if_missing(true)
            .busy_timeout(Duration::from_millis(SQLITE_BUSY_TIMEOUT_MS));

        let pool = SqlitePoolOptions::new()
            .min_connections(1)
            .max_connections(8)
            .connect_with(options)
            .await?;

        sqlx::query("PRAGMA foreign_keys = ON;")
            .execute(&pool)
            .await?;

        // Run embedded migrations. The directory is resolved relative to this crate.
        sqlx::migrate!("./migrations").run(&pool).await?;

        Ok(Self { pool })
    }

    /// Connects to a file path via `sqlite://` scheme.
    pub async fn connect_file(path: &Path) -> Result<Self> {
        let url = format!("sqlite://{}", path.display());
        Self::connect(&url).await
    }

    /// Exposes the underlying pool. Needed when other services want to compose
    /// queries (e.g. reporting or background tasks).
    pub fn pool(&self) -> &SqlitePool {
        &self.pool
    }

    /// Registers a new sandbox in the catalog and returns the persisted record.
    pub async fn create_sandbox(&self, data: NewSandbox<'_>) -> Result<SandboxRecord> {
        let now = Utc::now();
        let id = Uuid::new_v4();
        sqlx::query(
            r#"
            INSERT INTO sandboxes (
                id, namespace, name, runtime, status,
                cpu_limit_millis, memory_limit_bytes, disk_limit_bytes,
                timeout_seconds, created_at, updated_at
            ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
            "#,
        )
        .bind(id.to_string())
        .bind(data.namespace)
        .bind(data.name)
        .bind(data.runtime)
        .bind(SandboxStatus::Provisioned.as_str())
        .bind(data.cpu_limit_millis as i64)
        .bind(data.memory_limit_bytes as i64)
        .bind(data.disk_limit_bytes as i64)
        .bind(data.timeout_seconds as i64)
        .bind(now.to_rfc3339())
        .bind(now.to_rfc3339())
        .execute(&self.pool)
        .await
        .map_err(|err| {
            if is_unique_violation(&err) {
                anyhow::Error::new(SandboxError::DuplicateSandbox(
                    data.namespace.to_owned(),
                    data.name.to_owned(),
                ))
            } else {
                err.into()
            }
        })?;

        self.fetch_sandbox(id).await?.ok_or_else(|| {
            anyhow!(
                "sandbox inserted but missing when reloaded (namespace={}, name={})",
                data.namespace,
                data.name
            )
        })
    }

    /// Retrieves a sandbox by its identifier.
    pub async fn fetch_sandbox(&self, id: Uuid) -> Result<Option<SandboxRecord>> {
        let row = sqlx::query("SELECT * FROM sandboxes WHERE id = ?")
            .bind(id.to_string())
            .fetch_optional(&self.pool)
            .await?;

        row.map(|row| map_sandbox(row)).transpose()
    }

    /// Lists all sandboxes within a namespace ordered by creation time descending.
    pub async fn list_sandboxes(&self, namespace: &str) -> Result<Vec<SandboxRecord>> {
        let mut rows =
            sqlx::query("SELECT * FROM sandboxes WHERE namespace = ? ORDER BY created_at DESC")
                .bind(namespace)
                .fetch(&self.pool);

        let mut out = Vec::new();
        while let Some(row) = rows.try_next().await? {
            out.push(map_sandbox(row)?);
        }
        Ok(out)
    }

    /// Updates the lifecycle status and timestamp bookkeeping.
    pub async fn update_status(&self, id: Uuid, status: SandboxStatus) -> Result<()> {
        let now = Utc::now().to_rfc3339();
        sqlx::query(
            r#"
            UPDATE sandboxes
            SET status = ?, updated_at = ?
            WHERE id = ?
            "#,
        )
        .bind(status.as_str())
        .bind(&now)
        .bind(id.to_string())
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    pub async fn touch_last_started(&self, id: Uuid) -> Result<()> {
        let now = Utc::now().to_rfc3339();
        sqlx::query(
            r#"
            UPDATE sandboxes
            SET last_started_at = ?, updated_at = ?
            WHERE id = ?
            "#,
        )
        .bind(&now)
        .bind(&now)
        .bind(id.to_string())
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    pub async fn touch_last_stopped(&self, id: Uuid) -> Result<()> {
        let now = Utc::now().to_rfc3339();
        sqlx::query(
            r#"
            UPDATE sandboxes
            SET last_stopped_at = ?, updated_at = ?
            WHERE id = ?
            "#,
        )
        .bind(&now)
        .bind(&now)
        .bind(id.to_string())
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    /// Removes a sandbox (and cascading executions) from the catalog.
    pub async fn delete_sandbox(&self, id: Uuid) -> Result<()> {
        sqlx::query("DELETE FROM sandboxes WHERE id = ?")
            .bind(id.to_string())
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    /// Persists an execution event.
    pub async fn record_execution(&self, entry: ExecutionRecord) -> Result<()> {
        sqlx::query(
            r#"
            INSERT INTO sandbox_executions (
                sandbox_id, executed_at, command, args,
                exit_code, stdout, stderr, duration_ms, timed_out
            ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)
            "#,
        )
        .bind(entry.sandbox_id.to_string())
        .bind(entry.executed_at.to_rfc3339())
        .bind(entry.command)
        .bind(serde_json::to_string(&entry.args)?)
        .bind(entry.exit_code)
        .bind(entry.stdout)
        .bind(entry.stderr)
        .bind(entry.duration_ms as i64)
        .bind(entry.timed_out as i32)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    /// Returns the most recent execution events for a sandbox.
    pub async fn list_executions(
        &self,
        sandbox_id: Uuid,
        limit: u32,
    ) -> Result<Vec<ExecutionRecord>> {
        let mut rows = sqlx::query(
            r#"
            SELECT * FROM sandbox_executions
            WHERE sandbox_id = ?
            ORDER BY executed_at DESC
            LIMIT ?
            "#,
        )
        .bind(sandbox_id.to_string())
        .bind(limit as i64)
        .fetch(&self.pool);

        let mut out = Vec::new();
        while let Some(row) = rows.try_next().await? {
            out.push(map_execution(row)?);
        }
        Ok(out)
    }
}

fn is_unique_violation(error: &sqlx::Error) -> bool {
    matches!(error, sqlx::Error::Database(db_err) if db_err.message().contains("UNIQUE"))
}

fn parse_datetime(value: String) -> Result<DateTime<Utc>> {
    DateTime::parse_from_rfc3339(&value)
        .map(|dt| dt.with_timezone(&Utc))
        .map_err(|err| anyhow!("invalid RFC3339 timestamp '{}': {}", value, err))
}

fn map_sandbox(row: SqliteRow) -> Result<SandboxRecord> {
    let id: String = row.try_get("id")?;
    let status: String = row.try_get("status")?;

    Ok(SandboxRecord {
        id: Uuid::parse_str(&id)?,
        namespace: row.try_get("namespace")?,
        name: row.try_get("name")?,
        runtime: row.try_get("runtime")?,
        status: SandboxStatus::from_str(&status)?,
        cpu_limit_millis: row.try_get::<i64, _>("cpu_limit_millis")? as u32,
        memory_limit_bytes: row.try_get::<i64, _>("memory_limit_bytes")? as u64,
        disk_limit_bytes: row.try_get::<i64, _>("disk_limit_bytes")? as u64,
        timeout_seconds: row.try_get::<i64, _>("timeout_seconds")? as u32,
        created_at: parse_datetime(row.try_get("created_at")?)?,
        updated_at: parse_datetime(row.try_get("updated_at")?)?,
        last_started_at: row
            .try_get::<Option<String>, _>("last_started_at")?
            .map(parse_datetime)
            .transpose()?,
        last_stopped_at: row
            .try_get::<Option<String>, _>("last_stopped_at")?
            .map(parse_datetime)
            .transpose()?,
    })
}

fn map_execution(row: SqliteRow) -> Result<ExecutionRecord> {
    let args_json: String = row.try_get("args")?;
    let timed_out: i32 = row.try_get("timed_out")?;
    let sandbox_id_raw: String = row.try_get("sandbox_id")?;

    Ok(ExecutionRecord {
        sandbox_id: Uuid::parse_str(&sandbox_id_raw)?,
        executed_at: parse_datetime(row.try_get("executed_at")?)?,
        command: row.try_get("command")?,
        args: serde_json::from_str(&args_json)
            .context("failed to deserialize execution args JSON")?,
        exit_code: row.try_get("exit_code")?,
        stdout: row.try_get("stdout")?,
        stderr: row.try_get("stderr")?,
        duration_ms: row.try_get::<i64, _>("duration_ms")? as u64,
        timed_out: timed_out != 0,
    })
}

/// Errors returned by the database layer.
#[derive(Debug, Error, Clone)]
pub enum SandboxError {
    #[error("sandbox '{1}' already exists in namespace '{0}'")]
    DuplicateSandbox(String, String),
    #[error("sandbox '{0}' not found")]
    NotFound(Uuid),
}

/// Resource limits captured in the catalog. All values are already converted to
/// consistent units (milliseconds, bytes, etc.).
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub struct ResourceLimits {
    pub cpu_limit_millis: u32,
    pub memory_limit_bytes: u64,
    pub disk_limit_bytes: u64,
    pub timeout_seconds: u32,
}

impl Default for ResourceLimits {
    fn default() -> Self {
        Self {
            cpu_limit_millis: 1_000,
            memory_limit_bytes: 512 * 1024 * 1024,
            disk_limit_bytes: 500 * 1024 * 1024,
            timeout_seconds: 60,
        }
    }
}

/// Input payload for sandbox creation operations.
#[derive(Debug, Clone)]
pub struct NewSandbox<'a> {
    pub namespace: &'a str,
    pub name: &'a str,
    pub runtime: &'a str,
    pub cpu_limit_millis: u32,
    pub memory_limit_bytes: u64,
    pub disk_limit_bytes: u64,
    pub timeout_seconds: u32,
}

impl<'a> NewSandbox<'a> {
    pub fn with_limits(
        namespace: &'a str,
        name: &'a str,
        runtime: &'a str,
        limits: ResourceLimits,
    ) -> Self {
        Self {
            namespace,
            name,
            runtime,
            cpu_limit_millis: limits.cpu_limit_millis,
            memory_limit_bytes: limits.memory_limit_bytes,
            disk_limit_bytes: limits.disk_limit_bytes,
            timeout_seconds: limits.timeout_seconds,
        }
    }
}

/// Persisted sandbox metadata row.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct SandboxRecord {
    pub id: Uuid,
    pub namespace: String,
    pub name: String,
    pub runtime: String,
    pub status: SandboxStatus,
    pub cpu_limit_millis: u32,
    pub memory_limit_bytes: u64,
    pub disk_limit_bytes: u64,
    pub timeout_seconds: u32,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub last_started_at: Option<DateTime<Utc>>,
    pub last_stopped_at: Option<DateTime<Utc>>,
}

impl SandboxRecord {
    pub fn limits(&self) -> ResourceLimits {
        ResourceLimits {
            cpu_limit_millis: self.cpu_limit_millis,
            memory_limit_bytes: self.memory_limit_bytes,
            disk_limit_bytes: self.disk_limit_bytes,
            timeout_seconds: self.timeout_seconds,
        }
    }
}

/// Audit row capturing a sandbox execution.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ExecutionRecord {
    pub sandbox_id: Uuid,
    pub executed_at: DateTime<Utc>,
    pub command: String,
    pub args: Vec<String>,
    pub exit_code: Option<i32>,
    pub stdout: Option<String>,
    pub stderr: Option<String>,
    pub duration_ms: u64,
    pub timed_out: bool,
}

/// High-level sandbox lifecycle statuses persisted in the DB (also used in API responses).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum SandboxStatus {
    Provisioned,
    Preparing,
    Starting,
    Running,
    Stopped,
    Failed,
}

impl SandboxStatus {
    pub fn as_str(&self) -> &'static str {
        match self {
            SandboxStatus::Provisioned => "provisioned",
            SandboxStatus::Preparing => "preparing",
            SandboxStatus::Starting => "starting",
            SandboxStatus::Running => "running",
            SandboxStatus::Stopped => "stopped",
            SandboxStatus::Failed => "failed",
        }
    }
}

impl FromStr for SandboxStatus {
    type Err = anyhow::Error;

    fn from_str(s: &str) -> Result<Self> {
        match s {
            "provisioned" => Ok(SandboxStatus::Provisioned),
            "preparing" => Ok(SandboxStatus::Preparing),
            "starting" => Ok(SandboxStatus::Starting),
            "running" => Ok(SandboxStatus::Running),
            "stopped" => Ok(SandboxStatus::Stopped),
            "failed" => Ok(SandboxStatus::Failed),
            other => Err(anyhow!("unknown sandbox status: {}", other)),
        }
    }
}

/// Helper trait for background jobs that need a graceful shutdown.
#[async_trait]
pub trait BackgroundWorker: Send + Sync {
    async fn run(self: Arc<Self>) -> Result<()>;
}

/// A guard that owns the join handle of a running worker.
pub struct WorkerGuard {
    handle: JoinHandle<Result<()>>,
}

impl WorkerGuard {
    pub fn new(handle: JoinHandle<Result<()>>) -> Self {
        Self { handle }
    }

    pub async fn join(self) -> Result<()> {
        self.handle.await??;
        Ok(())
    }
}

/// A simple registry for background tasks (e.g. rotation jobs). This will be useful once
/// rotation and housekeeping tasks are implemented. For now it allows the daemon to own
/// the join handles and surface errors.
#[derive(Default)]
pub struct WorkerRegistry {
    workers: RwLock<Vec<WorkerGuard>>,
}

impl WorkerRegistry {
    pub fn register(&self, handle: JoinHandle<Result<()>>) {
        self.workers.write().push(WorkerGuard::new(handle));
    }

    pub async fn wait_all(self) -> Result<()> {
        for guard in self.workers.into_inner() {
            guard.join().await?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const TEST_DB_URL: &str = "sqlite::memory:";

    async fn setup_db() -> Database {
        Database::connect(TEST_DB_URL).await.unwrap()
    }

    #[tokio::test]
    async fn create_and_fetch_sandbox_roundtrip() {
        let db = setup_db().await;
        let record = db
            .create_sandbox(NewSandbox::with_limits(
                "namespace-a",
                "sandbox-alpha",
                "process",
                ResourceLimits::default(),
            ))
            .await
            .unwrap();

        assert_eq!(record.namespace, "namespace-a");

        let fetched = db.fetch_sandbox(record.id).await.unwrap().unwrap();
        assert_eq!(fetched.id, record.id);
        assert_eq!(fetched.status, SandboxStatus::Provisioned);
    }

    #[tokio::test]
    async fn duplicate_sandbox_names_are_rejected() {
        let db = setup_db().await;
        let limits = ResourceLimits::default();
        db.create_sandbox(NewSandbox::with_limits("ns", "same", "process", limits))
            .await
            .unwrap();

        let err = db
            .create_sandbox(NewSandbox::with_limits("ns", "same", "process", limits))
            .await
            .unwrap_err();

        let sandbox_err = err.downcast::<SandboxError>().unwrap();
        assert!(matches!(sandbox_err, SandboxError::DuplicateSandbox(_, _)));
    }

    #[tokio::test]
    async fn record_execution_roundtrip() {
        let db = setup_db().await;
        let sandbox = db
            .create_sandbox(NewSandbox::with_limits(
                "ns",
                "runner",
                "process",
                ResourceLimits::default(),
            ))
            .await
            .unwrap();

        let entry = ExecutionRecord {
            sandbox_id: sandbox.id,
            executed_at: Utc::now(),
            command: "echo".into(),
            args: vec!["hello".into()],
            exit_code: Some(0),
            stdout: Some("hello\n".into()),
            stderr: None,
            duration_ms: 25,
            timed_out: false,
        };

        db.record_execution(entry.clone()).await.unwrap();

        let entries = db.list_executions(sandbox.id, 10).await.unwrap();

        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].stdout, entry.stdout);
    }
}
