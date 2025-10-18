//! API layer scaffolding for bkg-db (HTTP, pgwire, gRPC).

#![allow(dead_code)]

use std::sync::atomic::{AtomicBool, Ordering};

use anyhow::{anyhow, Result};
use async_trait::async_trait;
use serde_json::{json, Value};
use uuid::Uuid;

use crate::{
    executor::{DefaultQueryExecutor, ExecutionContext, ExecutionResult, QueryExecutor},
    planner::{LogicalOptimizer, LogicalPlanner, PlannerDraft},
    sql::{DefaultSqlParser, SqlParser},
    Database, NewRlsPolicy, RlsPolicyRecord,
};

// TODO(bkg-db/api): Implement REST, pgwire und gRPC Server inkl. Auth & RLS Hooks.

#[async_trait]
pub trait RestApiServer: Send + Sync {
    async fn handle_query(&self, body: Value) -> Result<ExecutionResult>;
    async fn handle_auth(&self, body: Value) -> Result<Value>;
    async fn handle_policy(&self, body: Value) -> Result<Value>;
    async fn handle_schema(&self) -> Result<Value>;
}

pub trait PgWireServer: Send + Sync {
    fn start(&self) -> Result<()>;
    fn stop(&self) -> Result<()>;
}

pub trait GrpcApiServer: Send + Sync {
    fn start(&self) -> Result<()>;
    fn stop(&self) -> Result<()>;
}

/// Minimal in-process REST surface for the prototype pipeline.
#[derive(Clone)]
pub struct EmbeddedRestApi {
    database: Database,
    context: ExecutionContext,
    parser: DefaultSqlParser,
    planner: PlannerDraft,
    executor: DefaultQueryExecutor,
}

impl EmbeddedRestApi {
    pub fn new(database: Database, context: ExecutionContext) -> Self {
        Self {
            database,
            context,
            parser: DefaultSqlParser::new(),
            planner: PlannerDraft::new(),
            executor: DefaultQueryExecutor::new(),
        }
    }

    fn run_sql(&self, sql: &str) -> Result<ExecutionResult> {
        let ast = self.parser.parse(sql)?;
        let logical = self.planner.build_logical_plan(&ast)?;
        let optimized = self.planner.optimize(logical)?;
        self.executor.execute(&self.context, &optimized)
    }

    fn policy_to_json(record: &RlsPolicyRecord) -> Value {
        json!({
            "id": record.id.to_string(),
            "table": record.table_name,
            "policy": record.policy_name,
            "expression": record.expression.clone(),
            "created_at": record.created_at.to_rfc3339(),
            "updated_at": record.updated_at.to_rfc3339(),
        })
    }
}

#[async_trait]
impl RestApiServer for EmbeddedRestApi {
    async fn handle_query(&self, body: Value) -> Result<ExecutionResult> {
        let sql = body
            .get("sql")
            .and_then(Value::as_str)
            .ok_or_else(|| anyhow!("'sql' field is required"))?;
        self.run_sql(sql)
    }

    async fn handle_auth(&self, _body: Value) -> Result<Value> {
        Ok(json!({
            "status": "not_implemented",
            "message": "Auth endpoint wiring pending"
        }))
    }

    async fn handle_policy(&self, body: Value) -> Result<Value> {
        let action = body.get("action").and_then(Value::as_str).unwrap_or("list");

        match action {
            "list" => {
                let table = body.get("table_name").and_then(Value::as_str);
                let policies = self.database.list_rls_policies(table).await?;
                let entries: Vec<Value> = policies.iter().map(Self::policy_to_json).collect();
                Ok(json!({ "policies": entries }))
            }
            "upsert" => {
                let table = body
                    .get("table_name")
                    .and_then(Value::as_str)
                    .ok_or_else(|| anyhow!("'table_name' is required"))?;
                let name = body
                    .get("policy_name")
                    .and_then(Value::as_str)
                    .ok_or_else(|| anyhow!("'policy_name' is required"))?;
                let expression = body
                    .get("expression")
                    .cloned()
                    .ok_or_else(|| anyhow!("'expression' is required"))?;
                let record = self
                    .database
                    .upsert_rls_policy(NewRlsPolicy {
                        table_name: table.to_string(),
                        policy_name: name.to_string(),
                        expression,
                    })
                    .await?;
                Ok(Self::policy_to_json(&record))
            }
            "delete" => {
                let id_value = body
                    .get("id")
                    .and_then(Value::as_str)
                    .ok_or_else(|| anyhow!("'id' is required"))?;
                let id = Uuid::parse_str(id_value)?;
                self.database.delete_rls_policy(id).await?;
                Ok(json!({ "status": "deleted", "id": id.to_string() }))
            }
            other => Err(anyhow!("unsupported policy action '{other}'")),
        }
    }

    async fn handle_schema(&self) -> Result<Value> {
        let tables = self.context.table_summaries();
        let snapshot: Vec<Value> = tables
            .into_iter()
            .map(|table| {
                json!({
                    "name": table.name,
                    "columns": table.columns,
                    "rows": table.row_count,
                })
            })
            .collect();
        Ok(json!({ "tables": snapshot }))
    }
}

/// No-op pgwire server placeholder used while the real protocol implementation is in flight.
#[derive(Debug, Default)]
pub struct PgWireStub {
    started: AtomicBool,
}

impl PgWireStub {
    pub fn new() -> Self {
        Self {
            started: AtomicBool::new(false),
        }
    }

    pub fn is_started(&self) -> bool {
        self.started.load(Ordering::SeqCst)
    }
}

impl PgWireServer for PgWireStub {
    fn start(&self) -> Result<()> {
        if self.started.swap(true, Ordering::SeqCst) {
            return Err(anyhow!("pgwire stub already running"));
        }
        Ok(())
    }

    fn stop(&self) -> Result<()> {
        self.started.store(false, Ordering::SeqCst);
        Ok(())
    }
}

#[derive(Debug, Default, Clone)]
pub struct ApiSurfaceBlueprint;

impl ApiSurfaceBlueprint {
    pub fn new() -> Self {
        Self
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::executor::{ExecutionContext, ScalarValue};
    use crate::kernel::InMemoryStorageEngine;
    use serde_json::json;

    const TEST_DB_URL: &str = "sqlite::memory:";

    async fn setup_rest_api() -> EmbeddedRestApi {
        let database = Database::connect(TEST_DB_URL).await.unwrap();
        let context = ExecutionContext::new(InMemoryStorageEngine::new());
        EmbeddedRestApi::new(database, context)
    }

    #[tokio::test]
    async fn rest_api_executes_sql_queries() {
        let api = setup_rest_api().await;

        api.handle_query(json!({
            "sql": "INSERT INTO projects (id, name) VALUES (1, 'alpha')"
        }))
        .await
        .unwrap();

        let result = api
            .handle_query(json!({
                "sql": "SELECT * FROM projects"
            }))
            .await
            .unwrap();

        assert_eq!(result.rows.len(), 1);
        assert_eq!(result.rows[0][0], ScalarValue::Int64(1));
    }

    #[tokio::test]
    async fn rest_api_manages_rls_policies() {
        let api = setup_rest_api().await;

        let initial = api
            .handle_policy(json!({
                "action": "list",
                "table_name": "projects"
            }))
            .await
            .unwrap();
        assert!(initial["policies"].as_array().unwrap().is_empty());

        let upsert = api
            .handle_policy(json!({
                "action": "upsert",
                "table_name": "projects",
                "policy_name": "owner-only",
                "expression": {
                    "eq": { "column": "owner", "claim": "subject" }
                }
            }))
            .await
            .unwrap();
        let policy_id = upsert["id"].as_str().unwrap().to_string();

        let listed = api
            .handle_policy(json!({
                "action": "list",
                "table_name": "projects"
            }))
            .await
            .unwrap();
        assert_eq!(listed["policies"].as_array().unwrap().len(), 1);

        api.handle_policy(json!({
            "action": "delete",
            "id": policy_id
        }))
        .await
        .unwrap();

        let empty = api
            .handle_policy(json!({
                "action": "list",
                "table_name": "projects"
            }))
            .await
            .unwrap();
        assert!(empty["policies"].as_array().unwrap().is_empty());
    }

    #[test]
    fn pgwire_stub_transitions() {
        let server = PgWireStub::new();
        assert!(!server.is_started());
        server.start().unwrap();
        assert!(server.is_started());
        assert!(server.start().is_err());
        server.stop().unwrap();
        assert!(!server.is_started());
    }
}
