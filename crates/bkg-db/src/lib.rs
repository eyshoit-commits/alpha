//! BKG persistence layer providing sandbox metadata & execution audit storage.
//!
//! This crate offers an async API around SQLite (sqlx) tailored for the
//! Phase‑0 requirements in the README. Es bildet aktuell die Sandbox-Kataloge
//! ab und dient gleichzeitig als Ausgangspunkt für den vollständigen bkg-db
//! Stack (siehe `docs/bkg-db.md`).

pub mod api;
pub mod audit;
pub mod auth;
pub mod executor;
pub mod kernel;
pub mod planner;
pub mod realtime;
pub mod rls;
pub mod sql;
pub mod storage;
pub mod telemetry;

use std::{path::Path, str::FromStr, sync::Arc, time::Duration};

use anyhow::{anyhow, Context, Result};
use async_trait::async_trait;
use chrono::{DateTime, Utc};
use futures::TryStreamExt;
use parking_lot::RwLock;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sqlx::{
    migrate::MigrateError,
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
        if let Err(err) = sqlx::migrate!("./migrations").run(&pool).await {
            match &err {
                MigrateError::Execute(sqlx::Error::Database(db_err))
                    if db_err.message().contains("_sqlx_migrations")
                        && db_err
                            .code()
                            .map(|code| code == "2067" || code == "1555")
                            .unwrap_or(false) => {}
                _ => return Err(err.into()),
            }
        }

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

    /// Retrieves an API key by identifier.
    pub async fn fetch_api_key(&self, id: Uuid) -> Result<Option<ApiKeyRecord>> {
        let row = sqlx::query("SELECT * FROM api_keys WHERE id = ?")
            .bind(id.to_string())
            .fetch_optional(&self.pool)
            .await?;

        row.map(map_api_key).transpose()
    }

    /// Persists a hashed API key and returns the stored record metadata.
    #[allow(clippy::too_many_arguments)]
    pub async fn insert_api_key(
        &self,
        token_hash: &str,
        token_prefix: &str,
        scope: ApiKeyScope,
        rate_limit: u32,
        expires_at: Option<DateTime<Utc>>,
        rotated_from: Option<Uuid>,
        rotated_at: Option<DateTime<Utc>>,
    ) -> Result<ApiKeyRecord> {
        let now = Utc::now();
        let id = Uuid::new_v4();
        let (scope_type, scope_namespace) = scope.columns();
        sqlx::query(
            r#"
            INSERT INTO api_keys (
                id, token_hash, token_prefix, scope_type, scope_namespace,
                rate_limit, created_at, expires_at, revoked, rotated_from, rotated_at
            ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, 0, ?, ?)
            "#,
        )
        .bind(id.to_string())
        .bind(token_hash)
        .bind(token_prefix)
        .bind(scope_type)
        .bind(scope_namespace)
        .bind(rate_limit as i64)
        .bind(now.to_rfc3339())
        .bind(expires_at.map(|v| v.to_rfc3339()))
        .bind(rotated_from.map(|value| value.to_string()))
        .bind(rotated_at.map(|ts| ts.to_rfc3339()))
        .execute(&self.pool)
        .await?;

        self.fetch_api_key(id)
            .await?
            .ok_or_else(|| anyhow!("api key inserted but missing when reloaded ({id})"))
    }

    /// Retrieves an API key by its hashed token value (sha256).
    pub async fn find_api_key_by_hash(&self, token_hash: &str) -> Result<Option<ApiKeyRecord>> {
        let row = sqlx::query("SELECT * FROM api_keys WHERE token_hash = ?")
            .bind(token_hash)
            .fetch_optional(&self.pool)
            .await?;

        row.map(map_api_key).transpose()
    }

    /// Returns metadata for all stored API keys (including revoked entries).
    pub async fn list_api_keys(&self) -> Result<Vec<ApiKeyRecord>> {
        let mut rows =
            sqlx::query("SELECT * FROM api_keys ORDER BY created_at DESC").fetch(&self.pool);

        let mut out = Vec::new();
        while let Some(row) = rows.try_next().await? {
            out.push(map_api_key(row)?);
        }
        Ok(out)
    }

    /// Marks an API key as revoked.
    pub async fn revoke_api_key(&self, id: Uuid) -> Result<()> {
        sqlx::query("UPDATE api_keys SET revoked = 1 WHERE id = ?")
            .bind(id.to_string())
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    /// Updates the `last_used_at` timestamp once authorization succeeds.
    pub async fn touch_api_key_usage(&self, id: Uuid, timestamp: DateTime<Utc>) -> Result<()> {
        sqlx::query("UPDATE api_keys SET last_used_at = ? WHERE id = ?")
            .bind(timestamp.to_rfc3339())
            .bind(id.to_string())
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    /// Persists a queued key-rotation webhook event for later delivery.
    pub async fn insert_key_rotation_event(
        &self,
        new_key_id: Uuid,
        previous_key_id: Uuid,
        rotated_at: DateTime<Utc>,
        payload: &str,
        signature: &str,
    ) -> Result<WebhookEventRecord> {
        let id = Uuid::new_v4();
        let created_at = Utc::now();
        sqlx::query(
            r#"
            INSERT INTO key_rotation_events (
                id, new_key_id, previous_key_id, rotated_at,
                payload, signature, created_at, delivered
            ) VALUES (?, ?, ?, ?, ?, ?, ?, 0)
            "#,
        )
        .bind(id.to_string())
        .bind(new_key_id.to_string())
        .bind(previous_key_id.to_string())
        .bind(rotated_at.to_rfc3339())
        .bind(payload)
        .bind(signature)
        .bind(created_at.to_rfc3339())
        .execute(&self.pool)
        .await?;

        let payload_value = serde_json::from_str(payload)?;
        Ok(WebhookEventRecord {
            id,
            new_key_id,
            previous_key_id,
            rotated_at,
            payload: payload_value,
            signature: signature.to_string(),
            created_at,
            delivered: false,
        })
    }

    /// Lists queued key-rotation webhook events.
    pub async fn list_key_rotation_events(&self) -> Result<Vec<WebhookEventRecord>> {
        let mut rows = sqlx::query("SELECT * FROM key_rotation_events ORDER BY created_at DESC")
            .fetch(&self.pool);

        let mut out = Vec::new();
        while let Some(row) = rows.try_next().await? {
            out.push(map_rotation_event(row)?);
        }
        Ok(out)
    }

    /// Creates or updates an RLS policy identified by (table_name, policy_name).
    pub async fn upsert_rls_policy(&self, policy: NewRlsPolicy) -> Result<RlsPolicyRecord> {
        if let Some(existing) = self
            .fetch_rls_policy_by_name(&policy.table_name, &policy.policy_name)
            .await?
        {
            let updated_at = Utc::now().to_rfc3339();
            sqlx::query(
                r#"
                UPDATE rls_policies
                SET expression = ?, updated_at = ?
                WHERE id = ?
                "#,
            )
            .bind(serde_json::to_string(&policy.expression)?)
            .bind(&updated_at)
            .bind(existing.id.to_string())
            .execute(&self.pool)
            .await?;

            self.fetch_rls_policy(existing.id).await?.ok_or_else(|| {
                anyhow!(
                    "rls policy updated but missing when reloaded ({})",
                    existing.id
                )
            })
        } else {
            let id = Uuid::new_v4();
            let now = Utc::now().to_rfc3339();
            sqlx::query(
                r#"
                INSERT INTO rls_policies (
                    id, table_name, policy_name, expression, created_at, updated_at
                ) VALUES (?, ?, ?, ?, ?, ?)
                "#,
            )
            .bind(id.to_string())
            .bind(&policy.table_name)
            .bind(&policy.policy_name)
            .bind(serde_json::to_string(&policy.expression)?)
            .bind(&now)
            .bind(&now)
            .execute(&self.pool)
            .await?;

            self.fetch_rls_policy(id)
                .await?
                .ok_or_else(|| anyhow!("rls policy inserted but missing when reloaded ({id})"))
        }
    }

    /// Fetches a persisted RLS policy by identifier.
    pub async fn fetch_rls_policy(&self, id: Uuid) -> Result<Option<RlsPolicyRecord>> {
        let row = sqlx::query("SELECT * FROM rls_policies WHERE id = ?")
            .bind(id.to_string())
            .fetch_optional(&self.pool)
            .await?;

        row.map(map_rls_policy).transpose()
    }

    /// Lists stored RLS policies optionally filtered by table name.
    pub async fn list_rls_policies(
        &self,
        table_name: Option<&str>,
    ) -> Result<Vec<RlsPolicyRecord>> {
        let mut rows = match table_name {
            Some(table) => sqlx::query(
                r#"
                SELECT * FROM rls_policies
                WHERE table_name = ?
                ORDER BY policy_name ASC
                "#,
            )
            .bind(table)
            .fetch(&self.pool),
            None => sqlx::query(
                r#"
                SELECT * FROM rls_policies
                ORDER BY table_name ASC, policy_name ASC
                "#,
            )
            .fetch(&self.pool),
        };

        let mut out = Vec::new();
        while let Some(row) = rows.try_next().await? {
            out.push(map_rls_policy(row)?);
        }
        Ok(out)
    }

    /// Removes a stored RLS policy.
    pub async fn delete_rls_policy(&self, id: Uuid) -> Result<()> {
        sqlx::query("DELETE FROM rls_policies WHERE id = ?")
            .bind(id.to_string())
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    async fn fetch_rls_policy_by_name(
        &self,
        table_name: &str,
        policy_name: &str,
    ) -> Result<Option<RlsPolicyRecord>> {
        let row = sqlx::query(
            r#"
            SELECT * FROM rls_policies
            WHERE table_name = ? AND policy_name = ?
            "#,
        )
        .bind(table_name)
        .bind(policy_name)
        .fetch_optional(&self.pool)
        .await?;

        row.map(map_rls_policy).transpose()
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

        row.map(map_sandbox).transpose()
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

#[cfg(test)]
mod tests {
    use super::ResourceLimits;

    #[test]
    fn resource_limits_default_match_policy() {
        let limits = ResourceLimits::default();
        assert_eq!(limits.cpu_limit_millis, 2_000);
        assert_eq!(limits.memory_limit_bytes, 1_024 * 1024 * 1024);
        assert_eq!(limits.disk_limit_bytes, 1_024 * 1024 * 1024);
        assert_eq!(limits.timeout_seconds, 120);
    }
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

fn map_api_key(row: SqliteRow) -> Result<ApiKeyRecord> {
    let id: String = row.try_get("id")?;
    let scope_type: String = row.try_get("scope_type")?;
    let scope_namespace: Option<String> = row.try_get("scope_namespace")?;
    let scope = ApiKeyScope::try_from_columns(scope_type, scope_namespace)?;

    Ok(ApiKeyRecord {
        id: Uuid::parse_str(&id)?,
        token_prefix: row.try_get("token_prefix")?,
        scope,
        rate_limit: row.try_get::<i64, _>("rate_limit")? as u32,
        created_at: parse_datetime(row.try_get("created_at")?)?,
        last_used_at: row
            .try_get::<Option<String>, _>("last_used_at")?
            .map(parse_datetime)
            .transpose()?,
        expires_at: row
            .try_get::<Option<String>, _>("expires_at")?
            .map(parse_datetime)
            .transpose()?,
        revoked: row.try_get::<i64, _>("revoked")? != 0,
        rotated_from: row
            .try_get::<Option<String>, _>("rotated_from")?
            .map(|value| Uuid::parse_str(&value))
            .transpose()?,
        rotated_at: row
            .try_get::<Option<String>, _>("rotated_at")?
            .map(parse_datetime)
            .transpose()?,
    })
}

fn map_rotation_event(row: SqliteRow) -> Result<WebhookEventRecord> {
    let id: String = row.try_get("id")?;
    let new_key_id: String = row.try_get("new_key_id")?;
    let previous_key_id: String = row.try_get("previous_key_id")?;
    let payload_json: String = row.try_get("payload")?;

    Ok(WebhookEventRecord {
        id: Uuid::parse_str(&id)?,
        new_key_id: Uuid::parse_str(&new_key_id)?,
        previous_key_id: Uuid::parse_str(&previous_key_id)?,
        rotated_at: parse_datetime(row.try_get("rotated_at")?)?,
        payload: serde_json::from_str(&payload_json)
            .context("failed to deserialize rotation webhook payload")?,
        signature: row.try_get("signature")?,
        created_at: parse_datetime(row.try_get("created_at")?)?,
        delivered: row.try_get::<i64, _>("delivered")? != 0,
    })
}

fn map_rls_policy(row: SqliteRow) -> Result<RlsPolicyRecord> {
    let id: String = row.try_get("id")?;
    let expression_json: String = row.try_get("expression")?;

    Ok(RlsPolicyRecord {
        id: Uuid::parse_str(&id)?,
        table_name: row.try_get("table_name")?,
        policy_name: row.try_get("policy_name")?,
        expression: serde_json::from_str(&expression_json)
            .context("failed to deserialize RLS policy expression")?,
        created_at: parse_datetime(row.try_get("created_at")?)?,
        updated_at: parse_datetime(row.try_get("updated_at")?)?,
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
            cpu_limit_millis: 2_000,
            memory_limit_bytes: 1_024 * 1024 * 1024,
            disk_limit_bytes: 1_024 * 1024 * 1024,
            timeout_seconds: 120,
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

/// Queued webhook event awaiting delivery.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct WebhookEventRecord {
    pub id: Uuid,
    pub new_key_id: Uuid,
    pub previous_key_id: Uuid,
    pub rotated_at: DateTime<Utc>,
    pub payload: Value,
    pub signature: String,
    pub created_at: DateTime<Utc>,
    pub delivered: bool,
}

/// Persistent representation of API key scope (admin or namespace bounded).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ApiKeyScope {
    Admin,
    Namespace { namespace: String },
}

impl ApiKeyScope {
    fn columns(&self) -> (&'static str, Option<&str>) {
        match self {
            ApiKeyScope::Admin => ("admin", None),
            ApiKeyScope::Namespace { namespace } => ("namespace", Some(namespace.as_str())),
        }
    }

    fn try_from_columns(scope_type: String, scope_namespace: Option<String>) -> Result<Self> {
        match scope_type.as_str() {
            "admin" => Ok(ApiKeyScope::Admin),
            "namespace" => scope_namespace
                .map(|ns| ApiKeyScope::Namespace { namespace: ns })
                .ok_or_else(|| anyhow!("namespace scope missing namespace value")),
            other => Err(anyhow!("unknown api key scope: {other}")),
        }
    }
}

/// Stored metadata for issued API keys.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ApiKeyRecord {
    pub id: Uuid,
    pub token_prefix: String,
    pub scope: ApiKeyScope,
    pub rate_limit: u32,
    pub created_at: DateTime<Utc>,
    pub last_used_at: Option<DateTime<Utc>>,
    pub expires_at: Option<DateTime<Utc>>,
    pub revoked: bool,
    pub rotated_from: Option<Uuid>,
    pub rotated_at: Option<DateTime<Utc>>,
}

/// Input payload when creating oder updating RLS policies.
#[derive(Debug, Clone)]
pub struct NewRlsPolicy {
    pub table_name: String,
    pub policy_name: String,
    pub expression: Value,
}

/// Persisted RLS policy representation.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct RlsPolicyRecord {
    pub id: Uuid,
    pub table_name: String,
    pub policy_name: String,
    pub expression: Value,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl RlsPolicyRecord {
    /// Converts the persisted record into the in-memory engine format.
    pub fn to_engine_policy(&self) -> crate::rls::RlsPolicy {
        crate::rls::RlsPolicy {
            name: self.policy_name.clone(),
            table: self.table_name.clone(),
            expression: self.expression.clone(),
        }
    }
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
    use chrono::Utc;
    use serde_json::json;

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

    #[tokio::test]
    async fn api_key_persistence_roundtrip() {
        let db = setup_db().await;
        let scope = ApiKeyScope::Namespace {
            namespace: "team-alpha".into(),
        };

        let record = db
            .insert_api_key(
                "hash-123",
                "hash-prefix",
                scope.clone(),
                100,
                None,
                None,
                None,
            )
            .await
            .unwrap();

        assert_eq!(record.scope, scope);
        assert_eq!(record.rate_limit, 100);
        assert!(!record.revoked);

        let fetched = db.find_api_key_by_hash("hash-123").await.unwrap().unwrap();

        assert_eq!(fetched.id, record.id);

        db.touch_api_key_usage(record.id, Utc::now()).await.unwrap();
        db.revoke_api_key(record.id).await.unwrap();

        let updated = db.fetch_api_key(record.id).await.unwrap().unwrap();
        assert!(updated.revoked);
        assert!(updated.last_used_at.is_some());
    }

    #[tokio::test]
    async fn rls_policy_persistence_roundtrip() {
        let db = setup_db().await;
        let expression = json!({
            "eq": {
                "column": "namespace",
                "claim": "scope"
            }
        });

        let created = db
            .upsert_rls_policy(NewRlsPolicy {
                table_name: "projects".into(),
                policy_name: "namespace-scope".into(),
                expression: expression.clone(),
            })
            .await
            .unwrap();

        assert_eq!(created.table_name, "projects");
        assert_eq!(created.policy_name, "namespace-scope");
        assert_eq!(created.expression, expression);

        let updated_expr = json!({
            "eq": {
                "column": "owner",
                "claim": "subject"
            }
        });

        let updated = db
            .upsert_rls_policy(NewRlsPolicy {
                table_name: "projects".into(),
                policy_name: "namespace-scope".into(),
                expression: updated_expr.clone(),
            })
            .await
            .unwrap();

        assert_eq!(updated.id, created.id);
        assert_eq!(updated.expression, updated_expr);
        assert!(updated.updated_at >= created.updated_at);

        let listed = db.list_rls_policies(Some("projects")).await.unwrap();
        assert_eq!(listed.len(), 1);
        assert_eq!(listed[0].policy_name, "namespace-scope");

        db.delete_rls_policy(updated.id).await.unwrap();

        let empty = db.list_rls_policies(Some("projects")).await.unwrap();
        assert!(empty.is_empty());
    }
}
