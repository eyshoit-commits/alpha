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

use std::{
    path::Path,
    str::FromStr,
    sync::{Arc, Once},
};

use anyhow::{anyhow, Context, Result};
use async_trait::async_trait;
use chrono::{DateTime, Utc};
use futures::TryStreamExt;
use parking_lot::RwLock;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sqlx::{
    any::{AnyPoolOptions, AnyRow},
    migrate::MigrateError,
    sqlite::{SqliteConnectOptions, SqlitePoolOptions},
    AnyPool, QueryBuilder, Row, SqlitePool,
};
use thiserror::Error;
use tokio::task::JoinHandle;
use uuid::Uuid;

/// Default SQLite busy timeout in milliseconds when the DB is under load.
const SQLITE_BUSY_TIMEOUT_MS: u64 = 5_000;

/// Supported database backends for the persistence layer.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DatabaseDriver {
    Sqlite,
    Postgres,
}

/// Primary entry point to the persistence layer.
#[derive(Clone, Debug)]
pub struct Database {
    pool: AnyPool,
    driver: DatabaseDriver,
}

impl Database {
    /// Establishes (or creates) a connection pool for the given database URL.
    pub async fn connect(database_url: &str) -> Result<Self> {
        static DRIVERS: Once = Once::new();
        DRIVERS.call_once(|| {
            sqlx::any::install_default_drivers();
        });

        let driver = if database_url.starts_with("postgres://")
            || database_url.starts_with("postgresql://")
        {
            DatabaseDriver::Postgres
        } else {
            DatabaseDriver::Sqlite
        };

        let max_connections = match driver {
            DatabaseDriver::Sqlite if database_url.contains(":memory:") => 1,
            _ => 8,
        };

        let pool = AnyPoolOptions::new()
            .min_connections(1)
            .max_connections(max_connections)
            .connect(database_url)
            .await?;

        // Run embedded migrations. The directory is resolved relative to this crate.
        let migration_result = match driver {
            DatabaseDriver::Sqlite => sqlx::migrate!("./migrations").run(&pool).await,
            DatabaseDriver::Postgres => sqlx::migrate!("./migrations_postgres").run(&pool).await,
        };
        if let Err(err) = migration_result {
            match &err {
                MigrateError::Execute(sqlx::Error::Database(db_err))
                    if db_err.message().contains("_sqlx_migrations")
                        && db_err
                            .code()
                            .map(|code| matches!(code.as_ref(), "2067" | "1555" | "23505"))
                            .unwrap_or(false) => {}
                _ => return Err(err.into()),
            }
        }

        Ok(Self { pool, driver })
    }

    /// Connects to a file path via `sqlite://` scheme.
    pub async fn connect_file(path: &Path) -> Result<Self> {
        let url = format!("sqlite://{}", path.display());
        Self::connect(&url).await
    }

    /// Exposes the underlying pool. Needed when other services want to compose
    /// queries (e.g. reporting or background tasks).
    pub fn pool(&self) -> &AnyPool {
        &self.pool
    }

    /// Returns the configured driver for this database handle.
    pub fn driver(&self) -> DatabaseDriver {
        self.driver
    }

    /// Retrieves an API key by identifier.
    pub async fn fetch_api_key(&self, id: Uuid) -> Result<Option<ApiKeyRecord>> {
        let select = match self.driver {
            DatabaseDriver::Sqlite => "SELECT * FROM api_keys WHERE id = ?",
            DatabaseDriver::Postgres => "SELECT * FROM api_keys WHERE id = $1",
        };
        let row = sqlx::query(select)
            .bind(encode_uuid(id))
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
        let now_str = encode_datetime(now);
        let expires_at_str = encode_optional_datetime(expires_at);
        let (scope_type, scope_namespace) = scope.columns();
        match self.driver {
            DatabaseDriver::Sqlite => {
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
                .await?
            }
            DatabaseDriver::Postgres => {
                sqlx::query(
                    r#"
                INSERT INTO api_keys (
                    id, token_hash, token_prefix, scope_type, scope_namespace,
                    rate_limit, created_at, expires_at, revoked, rotated_from, rotated_at
                ) VALUES ($1, $2, $3, $4, $5, $6, $7, $8, false, $9, $10)
                "#,
                )
                .bind(encode_uuid(id))
                .bind(token_hash)
                .bind(token_prefix)
                .bind(scope_type)
                .bind(scope_namespace)
                .bind(rate_limit as i64)
                .bind(encode_datetime(now))
                .bind(expires_at_str)
                .bind(rotated_from.map(encode_uuid))
                .bind(rotated_at.map(encode_datetime))
                .execute(&self.pool)
                .await?
            }
        };

        self.fetch_api_key(id)
            .await?
            .ok_or_else(|| anyhow!("api key inserted but missing when reloaded ({id})"))
    }

    /// Retrieves an API key by its hashed token value (sha256).
    pub async fn find_api_key_by_hash(&self, token_hash: &str) -> Result<Option<ApiKeyRecord>> {
        let select = match self.driver {
            DatabaseDriver::Sqlite => "SELECT * FROM api_keys WHERE token_hash = ?",
            DatabaseDriver::Postgres => "SELECT * FROM api_keys WHERE token_hash = $1",
        };
        let row = sqlx::query(select)
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
        let revoke = match self.driver {
            DatabaseDriver::Sqlite => "UPDATE api_keys SET revoked = ? WHERE id = ?",
            DatabaseDriver::Postgres => "UPDATE api_keys SET revoked = $1 WHERE id = $2",
        };
        sqlx::query(revoke)
            .bind(true)
            .bind(encode_uuid(id))
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    /// Updates the `last_used_at` timestamp once authorization succeeds.
    pub async fn touch_api_key_usage(&self, id: Uuid, timestamp: DateTime<Utc>) -> Result<()> {
        let touch = match self.driver {
            DatabaseDriver::Sqlite => "UPDATE api_keys SET last_used_at = ? WHERE id = ?",
            DatabaseDriver::Postgres => "UPDATE api_keys SET last_used_at = $1 WHERE id = $2",
        };
        sqlx::query(touch)
            .bind(encode_datetime(timestamp))
            .bind(encode_uuid(id))
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
        match self.driver {
            DatabaseDriver::Sqlite => {
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
            }
            DatabaseDriver::Postgres => {
                sqlx::query(
                    r#"
                INSERT INTO key_rotation_events (
                    id, new_key_id, previous_key_id, rotated_at,
                    payload, signature, created_at, delivered
                ) VALUES ($1, $2, $3, $4, CAST($5 AS JSONB), $6, $7, false)
                "#,
                )
                .bind(encode_uuid(id))
                .bind(encode_uuid(new_key_id))
                .bind(encode_uuid(previous_key_id))
                .bind(encode_datetime(rotated_at))
                .bind(payload)
                .bind(signature)
                .bind(encode_datetime(created_at))
                .execute(&self.pool)
                .await?;
            }
        }

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
        let query = match self.driver {
            DatabaseDriver::Sqlite => "SELECT * FROM key_rotation_events ORDER BY created_at DESC",
            DatabaseDriver::Postgres => {
                r#"
            SELECT
                id::text AS id,
                new_key_id::text AS new_key_id,
                previous_key_id::text AS previous_key_id,
                rotated_at::text AS rotated_at,
                payload::text AS payload,
                signature,
                created_at::text AS created_at,
                delivered
            FROM key_rotation_events
            ORDER BY created_at DESC
            "#
            }
        };
        let mut rows = sqlx::query(query).fetch(&self.pool);

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
            let updated_at = Utc::now();
            let updated_at_str = encode_datetime(updated_at);
            let expression_json =
                serde_json::to_string(&policy.expression).context("serialize RLS expression")?;
            let query = match self.driver {
                DatabaseDriver::Sqlite => {
                    r#"
                UPDATE rls_policies
                SET expression = ?, updated_at = ?
                WHERE id = ?
                "#
                }
                DatabaseDriver::Postgres => {
                    r#"
                UPDATE rls_policies
                SET expression = CAST($1 AS JSONB), updated_at = $2
                WHERE id = $3
                "#
                }
            };
            sqlx::query(query)
                .bind(expression_json)
                .bind(updated_at_str)
                .bind(encode_uuid(existing.id))
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
            let now = Utc::now();
            let now_str = encode_datetime(now);
            let expression_json =
                serde_json::to_string(&policy.expression).context("serialize RLS expression")?;
            let query = match self.driver {
                DatabaseDriver::Sqlite => {
                    r#"
                INSERT INTO rls_policies (
                    id, table_name, policy_name, expression, created_at, updated_at
                ) VALUES (?, ?, ?, ?, ?, ?)
                "#
                }
                DatabaseDriver::Postgres => {
                    r#"
                INSERT INTO rls_policies (
                    id, table_name, policy_name, expression, created_at, updated_at
                ) VALUES ($1, $2, $3, CAST($4 AS JSONB), $5, $6)
                "#
                }
            };
            sqlx::query(query)
                .bind(encode_uuid(id))
                .bind(&policy.table_name)
                .bind(&policy.policy_name)
                .bind(expression_json)
                .bind(now_str.clone())
                .bind(now_str)
                .execute(&self.pool)
                .await?;

            self.fetch_rls_policy(id)
                .await?
                .ok_or_else(|| anyhow!("rls policy inserted but missing when reloaded ({id})"))
        }
    }

    /// Fetches a persisted RLS policy by identifier.
    pub async fn fetch_rls_policy(&self, id: Uuid) -> Result<Option<RlsPolicyRecord>> {
        let select = match self.driver {
            DatabaseDriver::Sqlite => "SELECT * FROM rls_policies WHERE id = ?",
            DatabaseDriver::Postgres =>
                "SELECT id, table_name, policy_name, expression::text AS expression, created_at, updated_at FROM rls_policies WHERE id = $1",
        };
        let row = sqlx::query(select)
            .bind(encode_uuid(id))
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
            Some(table) => {
                let query = match self.driver {
                    DatabaseDriver::Sqlite => {
                        r#"
                SELECT * FROM rls_policies
                WHERE table_name = ?
                ORDER BY policy_name ASC
                "#
                    }
                    DatabaseDriver::Postgres => {
                        r#"
                SELECT id, table_name, policy_name, expression::text AS expression, created_at, updated_at
                FROM rls_policies
                WHERE table_name = $1
                ORDER BY policy_name ASC
                "#
                    }
                };
                sqlx::query(query).bind(table).fetch(&self.pool)
            }
            None => {
                let query = match self.driver {
                    DatabaseDriver::Sqlite => {
                        r#"
                SELECT * FROM rls_policies
                ORDER BY table_name ASC, policy_name ASC
                "#
                    }
                    DatabaseDriver::Postgres => {
                        r#"
                SELECT id, table_name, policy_name, expression::text AS expression, created_at, updated_at
                FROM rls_policies
                ORDER BY table_name ASC, policy_name ASC
                "#
                    }
                };
                sqlx::query(query).fetch(&self.pool)
            }
        };

        let mut out = Vec::new();
        while let Some(row) = rows.try_next().await? {
            out.push(map_rls_policy(row)?);
        }
        Ok(out)
    }

    /// Removes a stored RLS policy.
    pub async fn delete_rls_policy(&self, id: Uuid) -> Result<()> {
        let delete = match self.driver {
            DatabaseDriver::Sqlite => "DELETE FROM rls_policies WHERE id = ?",
            DatabaseDriver::Postgres => "DELETE FROM rls_policies WHERE id = $1",
        };
        sqlx::query(delete)
            .bind(encode_uuid(id))
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    async fn fetch_rls_policy_by_name(
        &self,
        table_name: &str,
        policy_name: &str,
    ) -> Result<Option<RlsPolicyRecord>> {
        let query = match self.driver {
            DatabaseDriver::Sqlite => {
                r#"
            SELECT * FROM rls_policies
            WHERE table_name = ? AND policy_name = ?
            "#
            }
            DatabaseDriver::Postgres => {
                r#"
            SELECT id, table_name, policy_name, expression::text AS expression, created_at, updated_at
            FROM rls_policies
            WHERE table_name = $1 AND policy_name = $2
            "#
            }
        };
        let row = sqlx::query(query)
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
        let id_str = encode_uuid(id);
        let now_str = encode_datetime(now);
        let insert = match self.driver {
            DatabaseDriver::Sqlite => {
                r#"
            INSERT INTO sandboxes (
                id, namespace, name, runtime, status,
                cpu_limit_millis, memory_limit_bytes, disk_limit_bytes,
                timeout_seconds, created_at, updated_at
            ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
            "#
            }
            DatabaseDriver::Postgres => {
                r#"
            INSERT INTO sandboxes (
                id, namespace, name, runtime, status,
                cpu_limit_millis, memory_limit_bytes, disk_limit_bytes,
                timeout_seconds, created_at, updated_at
            ) VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11)
            "#
            }
        };
        sqlx::query(insert)
            .bind(id_str.clone())
            .bind(data.namespace)
            .bind(data.name)
            .bind(data.runtime)
            .bind(SandboxStatus::Provisioned.as_str())
            .bind(data.cpu_limit_millis as i64)
            .bind(data.memory_limit_bytes as i64)
            .bind(data.disk_limit_bytes as i64)
            .bind(data.timeout_seconds as i64)
            .bind(now_str.clone())
            .bind(now_str)
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
        let select = match self.driver {
            DatabaseDriver::Sqlite => "SELECT * FROM sandboxes WHERE id = ?",
            DatabaseDriver::Postgres => {
                r#"
            SELECT
                id::text AS id,
                namespace,
                name,
                runtime,
                status,
                cpu_limit_millis,
                memory_limit_bytes,
                disk_limit_bytes,
                timeout_seconds,
                created_at::text AS created_at,
                updated_at::text AS updated_at,
                last_started_at::text AS last_started_at,
                last_stopped_at::text AS last_stopped_at
            FROM sandboxes
            WHERE id = $1
            "#
            }
        };
        let row = sqlx::query(select)
            .bind(encode_uuid(id))
            .fetch_optional(&self.pool)
            .await?;

        row.map(map_sandbox).transpose()
    }

    /// Lists all sandboxes within a namespace ordered by creation time descending.
    pub async fn list_sandboxes(&self, namespace: &str) -> Result<Vec<SandboxRecord>> {
        let query = match self.driver {
            DatabaseDriver::Sqlite => {
                "SELECT * FROM sandboxes WHERE namespace = ? ORDER BY created_at DESC"
            }
            DatabaseDriver::Postgres => {
                r#"
            SELECT
                id::text AS id,
                namespace,
                name,
                runtime,
                status,
                cpu_limit_millis,
                memory_limit_bytes,
                disk_limit_bytes,
                timeout_seconds,
                created_at::text AS created_at,
                updated_at::text AS updated_at,
                last_started_at::text AS last_started_at,
                last_stopped_at::text AS last_stopped_at
            FROM sandboxes
            WHERE namespace = $1
            ORDER BY created_at DESC
            "#
            }
        };
        let mut rows = sqlx::query(query).bind(namespace).fetch(&self.pool);

        let mut out = Vec::new();
        while let Some(row) = rows.try_next().await? {
            out.push(map_sandbox(row)?);
        }
        Ok(out)
    }

    /// Updates the lifecycle status and timestamp bookkeeping.
    pub async fn update_status(&self, id: Uuid, status: SandboxStatus) -> Result<()> {
        let now = Utc::now();
        let now_str = encode_datetime(now);
        let update = match self.driver {
            DatabaseDriver::Sqlite => {
                r#"
            UPDATE sandboxes
            SET status = ?, updated_at = ?
            WHERE id = ?
            "#
            }
            DatabaseDriver::Postgres => {
                r#"
            UPDATE sandboxes
            SET status = $1, updated_at = $2
            WHERE id = $3
            "#
            }
        };
        sqlx::query(update)
            .bind(status.as_str())
            .bind(now_str)
            .bind(encode_uuid(id))
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    pub async fn touch_last_started(&self, id: Uuid) -> Result<()> {
        let now = Utc::now();
        let now_str = encode_datetime(now);
        let update = match self.driver {
            DatabaseDriver::Sqlite => {
                r#"
            UPDATE sandboxes
            SET last_started_at = ?, updated_at = ?
            WHERE id = ?
            "#
            }
            DatabaseDriver::Postgres => {
                r#"
            UPDATE sandboxes
            SET last_started_at = $1, updated_at = $2
            WHERE id = $3
            "#
            }
        };
        sqlx::query(update)
            .bind(now_str.clone())
            .bind(now_str)
            .bind(encode_uuid(id))
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    pub async fn touch_last_stopped(&self, id: Uuid) -> Result<()> {
        let now = Utc::now();
        let now_str = encode_datetime(now);
        let update = match self.driver {
            DatabaseDriver::Sqlite => {
                r#"
            UPDATE sandboxes
            SET last_stopped_at = ?, updated_at = ?
            WHERE id = ?
            "#
            }
            DatabaseDriver::Postgres => {
                r#"
            UPDATE sandboxes
            SET last_stopped_at = $1, updated_at = $2
            WHERE id = $3
            "#
            }
        };
        sqlx::query(update)
            .bind(now_str.clone())
            .bind(now_str)
            .bind(encode_uuid(id))
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    /// Removes a sandbox (and cascading executions) from the catalog.
    pub async fn delete_sandbox(&self, id: Uuid) -> Result<()> {
        let delete = match self.driver {
            DatabaseDriver::Sqlite => "DELETE FROM sandboxes WHERE id = ?",
            DatabaseDriver::Postgres => "DELETE FROM sandboxes WHERE id = $1",
        };
        sqlx::query(delete)
            .bind(encode_uuid(id))
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    /// Persists an execution event.
    pub async fn record_execution(&self, entry: ExecutionRecord) -> Result<()> {
        let ExecutionRecord {
            sandbox_id,
            executed_at,
            command,
            args,
            exit_code,
            stdout,
            stderr,
            duration_ms,
            timed_out,
        } = entry;
        let args_json = serde_json::to_string(&args).context("serialize execution args")?;
        let query = match self.driver {
            DatabaseDriver::Sqlite => {
                r#"
            INSERT INTO sandbox_executions (
                sandbox_id, executed_at, command, args,
                exit_code, stdout, stderr, duration_ms, timed_out
            ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)
            "#
            }
            DatabaseDriver::Postgres => {
                r#"
            INSERT INTO sandbox_executions (
                sandbox_id, executed_at, command, args,
                exit_code, stdout, stderr, duration_ms, timed_out
            ) VALUES ($1, $2, $3, CAST($4 AS JSONB), $5, $6, $7, $8, $9)
            "#
            }
        };
        sqlx::query(query)
            .bind(encode_uuid(sandbox_id))
            .bind(encode_datetime(executed_at))
            .bind(command)
            .bind(args_json)
            .bind(exit_code)
            .bind(stdout)
            .bind(stderr)
            .bind(duration_ms as i64)
            .bind(timed_out)
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
        let query = match self.driver {
            DatabaseDriver::Sqlite => {
                r#"
            SELECT * FROM sandbox_executions
            WHERE sandbox_id = ?
            ORDER BY executed_at DESC
            LIMIT ?
            "#
            }
            DatabaseDriver::Postgres => {
                r#"
            SELECT
                id,
                sandbox_id::text AS sandbox_id,
                executed_at::text AS executed_at,
                command,
                args::text AS args,
                exit_code,
                stdout,
                stderr,
                duration_ms,
                timed_out
            FROM sandbox_executions
            WHERE sandbox_id = $1
            ORDER BY executed_at DESC
            LIMIT $2
            "#
            }
        };
        let mut rows = sqlx::query(query)
            .bind(encode_uuid(sandbox_id))
            .bind(limit as i64)
            .fetch(&self.pool);

        let mut out = Vec::new();
        while let Some(row) = rows.try_next().await? {
            out.push(map_execution(row)?);
        }
        Ok(out)
    }

    /// Lists all registered models ordered by creation time descending.
    pub async fn list_models(&self) -> Result<Vec<ModelRecord>> {
        let query = match self.driver {
            DatabaseDriver::Sqlite => "SELECT * FROM models ORDER BY created_at DESC",
            DatabaseDriver::Postgres => {
                r#"
            SELECT
                id::text AS id,
                name,
                provider,
                version,
                format,
                source_uri,
                checksum_sha256,
                size_bytes,
                stage,
                last_synced_at::text AS last_synced_at,
                created_at::text AS created_at,
                updated_at::text AS updated_at,
                tags::text AS tags,
                error_message
            FROM models
            ORDER BY created_at DESC
            "#
            }
        };
        let mut rows = sqlx::query(query).fetch(&self.pool);
        let mut out = Vec::new();
        while let Some(row) = rows.try_next().await? {
            out.push(map_model(row)?);
        }
        Ok(out)
    }

    /// Searches for a model with the provided name and version.
    pub async fn find_model_by_name_version(
        &self,
        name: &str,
        version: &str,
    ) -> Result<Option<ModelRecord>> {
        let query = match self.driver {
            DatabaseDriver::Sqlite => "SELECT * FROM models WHERE name = ? AND version = ?",
            DatabaseDriver::Postgres => {
                r#"
            SELECT
                id::text AS id,
                name,
                provider,
                version,
                format,
                source_uri,
                checksum_sha256,
                size_bytes,
                stage,
                last_synced_at::text AS last_synced_at,
                created_at::text AS created_at,
                updated_at::text AS updated_at,
                tags::text AS tags,
                error_message
            FROM models
            WHERE name = $1 AND version = $2
            "#
            }
        };
        let row = sqlx::query(query)
            .bind(name)
            .bind(version)
            .fetch_optional(&self.pool)
            .await?;
        row.map(map_model).transpose()
    }

    /// Persists a new model entry and returns the stored record.
    pub async fn create_model(&self, new: NewModel<'_>) -> Result<ModelRecord> {
        let NewModel {
            name,
            provider,
            version,
            format,
            source_uri,
            checksum_sha256,
            size_bytes,
            tags,
            stage,
            error_message,
        } = new;

        let id = Uuid::new_v4();
        let now = Utc::now();
        let now_str = encode_datetime(now);
        let tags_json = tags
            .map(|items| serde_json::to_string(items).context("serialize model tags"))
            .transpose()?;
        let size_value = size_bytes.map(|value| value as i64);
        let stage_value = stage.as_str();
        let last_synced: Option<String> = None;

        match self.driver {
            DatabaseDriver::Sqlite => {
                sqlx::query(
                    r#"
                INSERT INTO models (
                    id, name, provider, version, format, source_uri,
                    checksum_sha256, size_bytes, stage, last_synced_at,
                    created_at, updated_at, tags, error_message
                ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
                "#,
                )
                .bind(encode_uuid(id))
                .bind(name)
                .bind(provider)
                .bind(version)
                .bind(format)
                .bind(source_uri)
                .bind(checksum_sha256)
                .bind(size_value)
                .bind(stage_value)
                .bind(last_synced.clone())
                .bind(now_str.clone())
                .bind(now_str.clone())
                .bind(tags_json.clone())
                .bind(error_message)
                .execute(&self.pool)
                .await?;
            }
            DatabaseDriver::Postgres => {
                sqlx::query(
                    r#"
                INSERT INTO models (
                    id, name, provider, version, format, source_uri,
                    checksum_sha256, size_bytes, stage, last_synced_at,
                    created_at, updated_at, tags, error_message
                ) VALUES (
                    $1, $2, $3, $4, $5, $6,
                    $7, $8, $9, $10, $11, $12, $13, $14
                )
                "#,
                )
                .bind(encode_uuid(id))
                .bind(name)
                .bind(provider)
                .bind(version)
                .bind(format)
                .bind(source_uri)
                .bind(checksum_sha256)
                .bind(size_value)
                .bind(stage_value)
                .bind(last_synced.clone())
                .bind(now_str.clone())
                .bind(now_str.clone())
                .bind(tags_json.clone())
                .bind(error_message)
                .execute(&self.pool)
                .await?;
            }
        }

        self.fetch_model(id).await?.ok_or_else(|| {
            anyhow!(
                "model inserted but missing when reloaded (name={}, version={})",
                name,
                version
            )
        })
    }

    /// Retrieves a model by identifier.
    pub async fn fetch_model(&self, id: Uuid) -> Result<Option<ModelRecord>> {
        let query = match self.driver {
            DatabaseDriver::Sqlite => "SELECT * FROM models WHERE id = ?",
            DatabaseDriver::Postgres => {
                r#"
            SELECT
                id::text AS id,
                name,
                provider,
                version,
                format,
                source_uri,
                checksum_sha256,
                size_bytes,
                stage,
                last_synced_at::text AS last_synced_at,
                created_at::text AS created_at,
                updated_at::text AS updated_at,
                tags::text AS tags,
                error_message
            FROM models
            WHERE id = $1
            "#
            }
        };
        let row = sqlx::query(query)
            .bind(encode_uuid(id))
            .fetch_optional(&self.pool)
            .await?;
        row.map(map_model).transpose()
    }

    /// Updates the stage and bookkeeping metadata for a model.
    pub async fn update_model_stage(
        &self,
        id: Uuid,
        stage: ModelStage,
        error_message: Option<&str>,
        last_synced_at: Option<DateTime<Utc>>,
    ) -> Result<Option<ModelRecord>> {
        let stage_value = stage.as_str();
        let updated_at = Utc::now();
        let updated_str = encode_datetime(updated_at);
        let last_synced_str = encode_optional_datetime(last_synced_at);

        let result = match self.driver {
            DatabaseDriver::Sqlite => {
                sqlx::query(
                    r#"
                UPDATE models
                SET stage = ?, error_message = ?, last_synced_at = ?, updated_at = ?
                WHERE id = ?
                "#,
                )
                .bind(stage_value)
                .bind(error_message)
                .bind(last_synced_str.clone())
                .bind(updated_str.clone())
                .bind(encode_uuid(id))
                .execute(&self.pool)
                .await?
            }
            DatabaseDriver::Postgres => {
                sqlx::query(
                    r#"
                UPDATE models
                SET stage = $1, error_message = $2, last_synced_at = $3, updated_at = $4
                WHERE id = $5
                "#,
                )
                .bind(stage_value)
                .bind(error_message)
                .bind(last_synced_str.clone())
                .bind(updated_str.clone())
                .bind(encode_uuid(id))
                .execute(&self.pool)
                .await?
            }
        };

        if result.rows_affected() == 0 {
            return Ok(None);
        }

        self.fetch_model(id).await
    }

    /// Removes a model and its associated jobs.
    pub async fn delete_model(&self, id: Uuid) -> Result<bool> {
        let result = match self.driver {
            DatabaseDriver::Sqlite => {
                sqlx::query("DELETE FROM models WHERE id = ?")
                    .bind(encode_uuid(id))
                    .execute(&self.pool)
                    .await?
            }
            DatabaseDriver::Postgres => {
                sqlx::query("DELETE FROM models WHERE id = $1")
                    .bind(encode_uuid(id))
                    .execute(&self.pool)
                    .await?
            }
        };
        Ok(result.rows_affected() > 0)
    }

    /// Inserts a new model download job entry.
    pub async fn insert_model_job(&self, job: NewModelJob<'_>) -> Result<ModelDownloadJobRecord> {
        let NewModelJob {
            model_id,
            stage,
            progress,
            started_at,
            finished_at,
            error_message,
        } = job;

        let id = Uuid::new_v4();
        let started_str = encode_datetime(started_at);
        let finished_str = encode_optional_datetime(finished_at);
        let stage_value = stage.as_str();

        match self.driver {
            DatabaseDriver::Sqlite => {
                sqlx::query(
                    r#"
                INSERT INTO model_jobs (
                    id, model_id, stage, progress, started_at, finished_at, error_message
                ) VALUES (?, ?, ?, ?, ?, ?, ?)
                "#,
                )
                .bind(encode_uuid(id))
                .bind(encode_uuid(model_id))
                .bind(stage_value)
                .bind(progress as f64)
                .bind(started_str.clone())
                .bind(finished_str.clone())
                .bind(error_message)
                .execute(&self.pool)
                .await?;
            }
            DatabaseDriver::Postgres => {
                sqlx::query(
                    r#"
                INSERT INTO model_jobs (
                    id, model_id, stage, progress, started_at, finished_at, error_message
                ) VALUES ($1, $2, $3, $4, $5, $6, $7)
                "#,
                )
                .bind(encode_uuid(id))
                .bind(encode_uuid(model_id))
                .bind(stage_value)
                .bind(progress as f64)
                .bind(started_str.clone())
                .bind(finished_str.clone())
                .bind(error_message)
                .execute(&self.pool)
                .await?;
            }
        }

        self.fetch_model_job(id)
            .await?
            .ok_or_else(|| anyhow!("model job inserted but missing (model_id={})", model_id))
    }

    /// Returns download/refresh jobs for a model ordered by newest first.
    pub async fn list_model_jobs(&self, model_id: Uuid) -> Result<Vec<ModelDownloadJobRecord>> {
        let query = match self.driver {
            DatabaseDriver::Sqlite => {
                r#"
            SELECT * FROM model_jobs
            WHERE model_id = ?
            ORDER BY started_at DESC
            "#
            }
            DatabaseDriver::Postgres => {
                r#"
            SELECT
                id::text AS id,
                model_id::text AS model_id,
                stage,
                progress,
                started_at::text AS started_at,
                finished_at::text AS finished_at,
                error_message
            FROM model_jobs
            WHERE model_id = $1
            ORDER BY started_at DESC
            "#
            }
        };

        let mut rows = sqlx::query(query)
            .bind(encode_uuid(model_id))
            .fetch(&self.pool);
        let mut out = Vec::new();
        while let Some(row) = rows.try_next().await? {
            out.push(map_model_job(row)?);
        }
        Ok(out)
    }

    async fn fetch_model_job(&self, id: Uuid) -> Result<Option<ModelDownloadJobRecord>> {
        let query = match self.driver {
            DatabaseDriver::Sqlite => "SELECT * FROM model_jobs WHERE id = ?",
            DatabaseDriver::Postgres => {
                r#"
            SELECT
                id::text AS id,
                model_id::text AS model_id,
                stage,
                progress,
                started_at::text AS started_at,
                finished_at::text AS finished_at,
                error_message
            FROM model_jobs
            WHERE id = $1
            "#
            }
        };

        let row = sqlx::query(query)
            .bind(encode_uuid(id))
            .fetch_optional(&self.pool)
            .await?;
        row.map(map_model_job).transpose()
    }

    /// Persists an audit event entry.
    pub async fn create_audit_event(&self, event: NewAuditEvent<'_>) -> Result<AuditEventRecord> {
        let NewAuditEvent {
            namespace,
            actor,
            event_type,
            recorded_at,
            payload,
            signature_valid,
        } = event;

        let id = Uuid::new_v4();
        let recorded_str = encode_datetime(recorded_at);
        let payload_json = serde_json::to_string(payload).context("serialize audit payload")?;

        match self.driver {
            DatabaseDriver::Sqlite => {
                sqlx::query(
                    r#"
                INSERT INTO audit_events (
                    id, namespace, actor, event_type, recorded_at, payload, signature_valid
                ) VALUES (?, ?, ?, ?, ?, ?, ?)
                "#,
                )
                .bind(encode_uuid(id))
                .bind(namespace)
                .bind(actor)
                .bind(event_type)
                .bind(recorded_str.clone())
                .bind(payload_json.clone())
                .bind(signature_valid)
                .execute(&self.pool)
                .await?;
            }
            DatabaseDriver::Postgres => {
                sqlx::query(
                    r#"
                INSERT INTO audit_events (
                    id, namespace, actor, event_type, recorded_at, payload, signature_valid
                ) VALUES ($1, $2, $3, $4, $5, $6, $7)
                "#,
                )
                .bind(encode_uuid(id))
                .bind(namespace)
                .bind(actor)
                .bind(event_type)
                .bind(recorded_str.clone())
                .bind(payload_json.clone())
                .bind(signature_valid)
                .execute(&self.pool)
                .await?;
            }
        }

        self.fetch_audit_event(id).await?.ok_or_else(|| {
            anyhow!(
                "audit event inserted but missing (event_type={})",
                event_type
            )
        })
    }

    /// Lists audit events applying optional filters.
    pub async fn list_audit_events(
        &self,
        filters: AuditEventFilters<'_>,
    ) -> Result<Vec<AuditEventRecord>> {
        let select = match self.driver {
            DatabaseDriver::Sqlite => "SELECT * FROM audit_events",
            DatabaseDriver::Postgres => {
                r#"
            SELECT
                id::text AS id,
                namespace,
                actor,
                event_type,
                recorded_at::text AS recorded_at,
                payload::text AS payload,
                signature_valid
            FROM audit_events
            "#
            }
        };

        let mut builder = QueryBuilder::new(select);
        let mut has_where = false;

        if let Some(namespace) = filters.namespace {
            builder.push(if has_where { " AND " } else { " WHERE " });
            has_where = true;
            builder.push("namespace = ");
            builder.push_bind(namespace);
        }

        if let Some(actor) = filters.actor {
            builder.push(if has_where { " AND " } else { " WHERE " });
            has_where = true;
            builder.push("actor = ");
            builder.push_bind(actor);
        }

        if let Some(event_type) = filters.event_type {
            builder.push(if has_where { " AND " } else { " WHERE " });
            has_where = true;
            builder.push("event_type = ");
            builder.push_bind(event_type);
        }

        if let Some(since) = filters.since {
            builder.push(if has_where { " AND " } else { " WHERE " });
            has_where = true;
            builder.push("recorded_at >= ");
            builder.push_bind(encode_datetime(since));
        }

        if let Some(until) = filters.until {
            builder.push(if has_where { " AND " } else { " WHERE " });
            has_where = true;
            builder.push("recorded_at <= ");
            builder.push_bind(encode_datetime(until));
        }

        builder.push(" ORDER BY recorded_at DESC");
        let limit = filters.limit.unwrap_or(100).max(1) as i64;
        builder.push(" LIMIT ");
        builder.push_bind(limit);

        let mut rows = builder.build().fetch(&self.pool);
        let mut out = Vec::new();
        while let Some(row) = rows.try_next().await? {
            out.push(map_audit_event(row)?);
        }
        Ok(out)
    }

    async fn fetch_audit_event(&self, id: Uuid) -> Result<Option<AuditEventRecord>> {
        let query = match self.driver {
            DatabaseDriver::Sqlite => "SELECT * FROM audit_events WHERE id = ?",
            DatabaseDriver::Postgres => {
                r#"
            SELECT
                id::text AS id,
                namespace,
                actor,
                event_type,
                recorded_at::text AS recorded_at,
                payload::text AS payload,
                signature_valid
            FROM audit_events
            WHERE id = $1
            "#
            }
        };

        let row = sqlx::query(query)
            .bind(encode_uuid(id))
            .fetch_optional(&self.pool)
            .await?;
        row.map(map_audit_event).transpose()
    }
}

fn is_unique_violation(error: &sqlx::Error) -> bool {
    match error {
        sqlx::Error::Database(db_err) => {
            if let Some(code) = db_err.code() {
                matches!(code.as_ref(), "2067" | "1555" | "23505")
            } else {
                db_err.message().contains("UNIQUE") || db_err.message().contains("unique")
            }
        }
        _ => false,
    }
}

fn parse_datetime(value: String) -> Result<DateTime<Utc>> {
    DateTime::parse_from_rfc3339(&value)
        .map(|dt| dt.with_timezone(&Utc))
        .map_err(|err| anyhow!("invalid RFC3339 timestamp '{}': {}", value, err))
}

fn encode_datetime(value: DateTime<Utc>) -> String {
    value.to_rfc3339()
}

fn encode_optional_datetime(value: Option<DateTime<Utc>>) -> Option<String> {
    value.map(encode_datetime)
}

fn encode_uuid(value: Uuid) -> String {
    value.to_string()
}

fn parse_uuid(value: String) -> Result<Uuid> {
    Uuid::parse_str(&value).map_err(|err| anyhow!("invalid UUID '{}': {}", value, err))
}

fn decode_datetime(row: &AnyRow, column: &str) -> Result<DateTime<Utc>> {
    let raw: String = row.try_get(column)?;
    parse_datetime(raw)
}

fn decode_optional_datetime(row: &AnyRow, column: &str) -> Result<Option<DateTime<Utc>>> {
    match row.try_get::<String, _>(column) {
        Ok(raw) => parse_datetime(raw).map(Some),
        Err(err) if is_unexpected_null(&err) => Ok(None),
        Err(err) => Err(err.into()),
    }
}

fn is_unexpected_null(err: &sqlx::Error) -> bool {
    match err {
        sqlx::Error::Decode(inner) => contains_null(inner.as_ref()),
        sqlx::Error::ColumnDecode { source, .. } => contains_null(source.as_ref()),
        _ => false,
    }
}

fn contains_null(err: &(dyn std::error::Error + 'static)) -> bool {
    if err.to_string().contains("NULL") {
        return true;
    }

    if let Some(source) = err.source() {
        return contains_null(source);
    }

    false
}

fn decode_bool(row: &AnyRow, column: &str) -> Result<bool> {
    match row.try_get::<bool, _>(column) {
        Ok(value) => Ok(value),
        Err(_) => {
            let raw: i64 = row.try_get(column)?;
            Ok(raw != 0)
        }
    }
}

fn decode_json_value(row: &AnyRow, column: &str, ctx: &str) -> Result<Value> {
    let raw: String = row.try_get(column)?;
    serde_json::from_str(&raw).with_context(|| ctx.to_owned())
}

fn decode_string_list(row: &AnyRow, column: &str) -> Result<Vec<String>> {
    let raw: String = row.try_get(column)?;
    serde_json::from_str(&raw)
        .with_context(|| format!("failed to deserialize JSON array column '{column}'"))
}

fn decode_optional_string(row: &AnyRow, column: &str) -> Result<Option<String>> {
    match row.try_get::<String, _>(column) {
        Ok(value) => Ok(Some(value)),
        Err(err) if is_unexpected_null(&err) => Ok(None),
        Err(err) => Err(err.into()),
    }
}

fn decode_optional_string_list(row: &AnyRow, column: &str) -> Result<Option<Vec<String>>> {
    match row.try_get::<String, _>(column) {
        Ok(value) => {
            if value.is_empty() {
                Ok(Some(Vec::new()))
            } else {
                serde_json::from_str(&value)
                    .with_context(|| format!("failed to deserialize JSON array column '{column}'"))
                    .map(Some)
            }
        }
        Err(err) if is_unexpected_null(&err) => Ok(None),
        Err(err) => Err(err.into()),
    }
}

fn decode_optional_bool(row: &AnyRow, column: &str) -> Result<Option<bool>> {
    match row.try_get::<Option<bool>, _>(column) {
        Ok(value) => Ok(value),
        Err(_) => {
            let raw: Option<i64> = row.try_get(column)?;
            Ok(raw.map(|value| value != 0))
        }
    }
}

fn decode_f32(row: &AnyRow, column: &str) -> Result<f32> {
    match row.try_get::<f32, _>(column) {
        Ok(value) => Ok(value),
        Err(_) => {
            let raw: f64 = row.try_get(column)?;
            Ok(raw as f32)
        }
    }
}

fn map_sandbox(row: AnyRow) -> Result<SandboxRecord> {
    let status: String = row.try_get("status")?;
    let id = parse_uuid(row.try_get::<String, _>("id")?)?;

    Ok(SandboxRecord {
        id,
        namespace: row.try_get("namespace")?,
        name: row.try_get("name")?,
        runtime: row.try_get("runtime")?,
        status: SandboxStatus::from_str(&status)?,
        cpu_limit_millis: row.try_get::<i64, _>("cpu_limit_millis")? as u32,
        memory_limit_bytes: row.try_get::<i64, _>("memory_limit_bytes")? as u64,
        disk_limit_bytes: row.try_get::<i64, _>("disk_limit_bytes")? as u64,
        timeout_seconds: row.try_get::<i64, _>("timeout_seconds")? as u32,
        created_at: decode_datetime(&row, "created_at")?,
        updated_at: decode_datetime(&row, "updated_at")?,
        last_started_at: decode_optional_datetime(&row, "last_started_at")?,
        last_stopped_at: decode_optional_datetime(&row, "last_stopped_at")?,
    })
}

#[cfg(test)]
mod resource_limit_tests {
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

fn map_execution(row: AnyRow) -> Result<ExecutionRecord> {
    let sandbox_id = parse_uuid(row.try_get::<String, _>("sandbox_id")?)?;
    Ok(ExecutionRecord {
        sandbox_id,
        executed_at: decode_datetime(&row, "executed_at")?,
        command: row.try_get("command")?,
        args: decode_string_list(&row, "args")?,
        exit_code: row.try_get("exit_code")?,
        stdout: decode_optional_string(&row, "stdout")?,
        stderr: decode_optional_string(&row, "stderr")?,
        duration_ms: row.try_get::<i64, _>("duration_ms")? as u64,
        timed_out: decode_bool(&row, "timed_out")?,
    })
}

fn map_model(row: AnyRow) -> Result<ModelRecord> {
    let id = parse_uuid(row.try_get::<String, _>("id")?)?;
    let stage: String = row.try_get("stage")?;
    Ok(ModelRecord {
        id,
        name: row.try_get("name")?,
        provider: row.try_get("provider")?,
        version: row.try_get("version")?,
        format: row.try_get("format")?,
        source_uri: row.try_get("source_uri")?,
        checksum_sha256: decode_optional_string(&row, "checksum_sha256")?,
        size_bytes: row
            .try_get::<Option<i64>, _>("size_bytes")?
            .map(|value| value as u64),
        stage: ModelStage::from_str(&stage)?,
        last_synced_at: decode_optional_datetime(&row, "last_synced_at")?,
        created_at: decode_datetime(&row, "created_at")?,
        updated_at: decode_datetime(&row, "updated_at")?,
        tags: decode_optional_string_list(&row, "tags")?,
        error_message: decode_optional_string(&row, "error_message")?,
    })
}

fn map_model_job(row: AnyRow) -> Result<ModelDownloadJobRecord> {
    let id = parse_uuid(row.try_get::<String, _>("id")?)?;
    let model_id = parse_uuid(row.try_get::<String, _>("model_id")?)?;
    let stage: String = row.try_get("stage")?;
    Ok(ModelDownloadJobRecord {
        id,
        model_id,
        stage: ModelStage::from_str(&stage)?,
        progress: decode_f32(&row, "progress")?,
        started_at: decode_datetime(&row, "started_at")?,
        finished_at: decode_optional_datetime(&row, "finished_at")?,
        error_message: decode_optional_string(&row, "error_message")?,
    })
}

fn map_audit_event(row: AnyRow) -> Result<AuditEventRecord> {
    let id = parse_uuid(row.try_get::<String, _>("id")?)?;
    let payload_json: String = row.try_get("payload")?;
    Ok(AuditEventRecord {
        id,
        namespace: decode_optional_string(&row, "namespace")?,
        actor: decode_optional_string(&row, "actor")?,
        event_type: row.try_get("event_type")?,
        recorded_at: decode_datetime(&row, "recorded_at")?,
        payload: serde_json::from_str(&payload_json)
            .context("failed to deserialize audit payload")?,
        signature_valid: decode_optional_bool(&row, "signature_valid")?,
    })
}

fn map_api_key(row: AnyRow) -> Result<ApiKeyRecord> {
    let scope_type: String = row.try_get("scope_type")?;
    let scope_namespace: Option<String> = row.try_get("scope_namespace")?;
    let scope = ApiKeyScope::try_from_columns(scope_type, scope_namespace)?;
    let id = parse_uuid(row.try_get::<String, _>("id")?)?;

    Ok(ApiKeyRecord {
        id,
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
            .map(|value| Uuid::parse_str(value.as_str()))
            .transpose()?,
        rotated_at: row
            .try_get::<Option<String>, _>("rotated_at")?
            .map(parse_datetime)
            .transpose()?,
    })
}

fn map_rotation_event(row: AnyRow) -> Result<WebhookEventRecord> {
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
        delivered: decode_bool(&row, "delivered")?,
    })
}

fn map_rls_policy(row: AnyRow) -> Result<RlsPolicyRecord> {
    let id = parse_uuid(row.try_get::<String, _>("id")?)?;
    Ok(RlsPolicyRecord {
        id,
        table_name: row.try_get("table_name")?,
        policy_name: row.try_get("policy_name")?,
        expression: decode_json_value(
            &row,
            "expression",
            "failed to deserialize RLS policy expression",
        )?,
        created_at: decode_datetime(&row, "created_at")?,
        updated_at: decode_datetime(&row, "updated_at")?,
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

/// Input payload for creating a new model entry.
#[derive(Debug, Clone)]
pub struct NewModel<'a> {
    pub name: &'a str,
    pub provider: &'a str,
    pub version: &'a str,
    pub format: &'a str,
    pub source_uri: &'a str,
    pub checksum_sha256: Option<&'a str>,
    pub size_bytes: Option<u64>,
    pub tags: Option<&'a [String]>,
    pub stage: ModelStage,
    pub error_message: Option<&'a str>,
}

/// Input payload for recording model job progress.
#[derive(Debug, Clone)]
pub struct NewModelJob<'a> {
    pub model_id: Uuid,
    pub stage: ModelStage,
    pub progress: f32,
    pub started_at: DateTime<Utc>,
    pub finished_at: Option<DateTime<Utc>>,
    pub error_message: Option<&'a str>,
}

/// Input payload for persisting audit events.
#[derive(Debug, Clone)]
pub struct NewAuditEvent<'a> {
    pub namespace: Option<&'a str>,
    pub actor: Option<&'a str>,
    pub event_type: &'a str,
    pub recorded_at: DateTime<Utc>,
    pub payload: &'a Value,
    pub signature_valid: Option<bool>,
}

/// Filters applied when listing audit events.
#[derive(Debug, Clone, Default)]
pub struct AuditEventFilters<'a> {
    pub namespace: Option<&'a str>,
    pub event_type: Option<&'a str>,
    pub since: Option<DateTime<Utc>>,
    pub until: Option<DateTime<Utc>>,
    pub limit: Option<u32>,
    pub actor: Option<&'a str>,
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

/// Enumerates all lifecycle stages for model downloads/registrations.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ModelStage {
    Unknown,
    Registered,
    Queued,
    Downloading,
    Verifying,
    Ready,
    Failed,
}

impl ModelStage {
    pub fn as_str(&self) -> &'static str {
        match self {
            ModelStage::Unknown => "unknown",
            ModelStage::Registered => "registered",
            ModelStage::Queued => "queued",
            ModelStage::Downloading => "downloading",
            ModelStage::Verifying => "verifying",
            ModelStage::Ready => "ready",
            ModelStage::Failed => "failed",
        }
    }
}

impl FromStr for ModelStage {
    type Err = anyhow::Error;

    fn from_str(value: &str) -> Result<Self> {
        match value {
            "unknown" => Ok(ModelStage::Unknown),
            "registered" => Ok(ModelStage::Registered),
            "queued" => Ok(ModelStage::Queued),
            "downloading" => Ok(ModelStage::Downloading),
            "verifying" => Ok(ModelStage::Verifying),
            "ready" => Ok(ModelStage::Ready),
            "failed" => Ok(ModelStage::Failed),
            other => Err(anyhow!("unknown model stage: {other}")),
        }
    }
}

/// Persisted metadata row for registered models.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ModelRecord {
    pub id: Uuid,
    pub name: String,
    pub provider: String,
    pub version: String,
    pub format: String,
    pub source_uri: String,
    pub size_bytes: Option<u64>,
    pub checksum_sha256: Option<String>,
    pub stage: ModelStage,
    pub last_synced_at: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub tags: Option<Vec<String>>,
    pub error_message: Option<String>,
}

/// Download or refresh job metadata tied to a model.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ModelDownloadJobRecord {
    pub id: Uuid,
    pub model_id: Uuid,
    pub stage: ModelStage,
    pub progress: f32,
    pub started_at: DateTime<Utc>,
    pub finished_at: Option<DateTime<Utc>>,
    pub error_message: Option<String>,
}

/// Stored audit event entry.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct AuditEventRecord {
    pub id: Uuid,
    pub namespace: Option<String>,
    pub actor: Option<String>,
    pub event_type: String,
    pub recorded_at: DateTime<Utc>,
    pub payload: Value,
    pub signature_valid: Option<bool>,
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
