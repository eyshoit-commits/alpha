//! API layer scaffolding for bkg-db (HTTP, pgwire, gRPC).

#![allow(dead_code)]

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use anyhow::{anyhow, Result};
use async_trait::async_trait;
use chrono::{DateTime, Utc};
use serde_json::{json, Value};
use uuid::Uuid;

use crate::{
    auth::{JwtHmacAuth, TokenClaims},
    executor::{DefaultQueryExecutor, ExecutionContext, ExecutionResult, QueryExecutor},
    planner::{LogicalOptimizer, LogicalPlan, LogicalPlanner, PlannerDraft},
    rls::RlsPolicyEngine,
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
    policy_engine: Arc<dyn RlsPolicyEngine>,
    auth: Arc<JwtHmacAuth>,
}

impl EmbeddedRestApi {
    pub fn new(
        database: Database,
        context: ExecutionContext,
        policy_engine: Arc<dyn RlsPolicyEngine>,
        auth: Arc<JwtHmacAuth>,
    ) -> Self {
        Self {
            database,
            context,
            parser: DefaultSqlParser::new(),
            planner: PlannerDraft::new(),
            executor: DefaultQueryExecutor::new(),
            policy_engine,
            auth,
        }
    }

    fn prepare_plan(&self, sql: &str) -> Result<LogicalPlan> {
        let ast = self.parser.parse(sql)?;
        let logical = self.planner.build_logical_plan(&ast)?;
        self.planner.optimize(logical)
    }

    async fn ensure_active_key(&self, key_id: Uuid) -> Result<()> {
        let record = self
            .database
            .fetch_api_key(key_id)
            .await?
            .ok_or_else(|| anyhow!("api key '{key_id}' not found"))?;

        if record.revoked {
            return Err(anyhow!("api key '{key_id}' is revoked"));
        }

        if let Some(expiry) = record.expires_at {
            if expiry < Utc::now() {
                return Err(anyhow!("api key '{key_id}' is expired"));
            }
        }

        self.database
            .touch_api_key_usage(key_id, Utc::now())
            .await?;

        Ok(())
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

fn plan_table_name(plan: &LogicalPlan) -> Result<&str> {
    match plan {
        LogicalPlan::Insert { table, .. }
        | LogicalPlan::Select { table, .. }
        | LogicalPlan::Update { table, .. }
        | LogicalPlan::Delete { table, .. } => Ok(table.as_str()),
    }
}

fn parse_timestamp(value: Option<&Value>, field: &str) -> Result<Option<DateTime<Utc>>> {
    match value {
        Some(Value::String(raw)) => {
            let dt = DateTime::parse_from_rfc3339(raw)
                .map_err(|err| anyhow!("invalid timestamp for {field}: {err}"))?;
            Ok(Some(dt.with_timezone(&Utc)))
        }
        Some(_) => Err(anyhow!("{field} must be an RFC3339 string")),
        None => Ok(None),
    }
}

fn parse_claims(body: &Value) -> Result<TokenClaims> {
    let claims_obj = body
        .get("claims")
        .and_then(Value::as_object)
        .ok_or_else(|| anyhow!("'claims' object is required"))?;
    let subject = claims_obj
        .get("subject")
        .and_then(Value::as_str)
        .ok_or_else(|| anyhow!("'claims.subject' is required"))?;
    let scope = claims_obj
        .get("scope")
        .and_then(Value::as_str)
        .ok_or_else(|| anyhow!("'claims.scope' is required"))?;
    let issued_at = parse_timestamp(claims_obj.get("issued_at"), "claims.issued_at")?
        .unwrap_or_else(|| Utc::now());
    let expires_at = parse_timestamp(claims_obj.get("expires_at"), "claims.expires_at")?;

    Ok(TokenClaims {
        subject: subject.to_string(),
        scope: scope.to_string(),
        issued_at,
        expires_at,
    })
}

fn claims_to_json(claims: &TokenClaims) -> Value {
    let mut obj = serde_json::Map::new();
    obj.insert("subject".into(), Value::String(claims.subject.clone()));
    obj.insert("scope".into(), Value::String(claims.scope.clone()));
    obj.insert(
        "issued_at".into(),
        Value::String(claims.issued_at.to_rfc3339()),
    );
    if let Some(expires_at) = claims.expires_at {
        obj.insert("expires_at".into(), Value::String(expires_at.to_rfc3339()));
    }
    Value::Object(obj)
}

#[async_trait]
impl RestApiServer for EmbeddedRestApi {
    async fn handle_query(&self, body: Value) -> Result<ExecutionResult> {
        let sql = body
            .get("sql")
            .and_then(Value::as_str)
            .ok_or_else(|| anyhow!("'sql' field is required"))?;
        let claims = parse_claims(&body)?;
        let plan = self.prepare_plan(sql)?;
        let table = plan_table_name(&plan)?;
        let policies = self.policy_engine.policies_for_table(table).await?;
        let result = self.executor.execute(
            &self.context,
            &plan,
            &claims,
            &policies,
            self.policy_engine.as_ref(),
        )?;
        Ok(result)
    }

    async fn handle_auth(&self, body: Value) -> Result<Value> {
        let action = body
            .get("action")
            .and_then(Value::as_str)
            .ok_or_else(|| anyhow!("'action' field is required"))?;

        match action {
            "issue" => {
                let claims = parse_claims(&body)?;
                if let Some(key_id_value) = body.get("key_id") {
                    let key_id = key_id_value
                        .as_str()
                        .ok_or_else(|| anyhow!("'key_id' must be a string"))?;
                    let key_id =
                        Uuid::parse_str(key_id).map_err(|err| anyhow!("invalid key_id: {err}"))?;
                    self.ensure_active_key(key_id).await?;
                }
                let token = self.auth.issue(&claims)?;
                Ok(json!({
                    "token": token,
                    "claims": claims_to_json(&claims)
                }))
            }
            "verify" => {
                let token = body
                    .get("token")
                    .and_then(Value::as_str)
                    .ok_or_else(|| anyhow!("'token' field is required"))?;
                let claims = self.auth.verify(token)?;
                if let Some(key_id_value) = body.get("key_id") {
                    let key_id = key_id_value
                        .as_str()
                        .ok_or_else(|| anyhow!("'key_id' must be a string"))?;
                    let key_id =
                        Uuid::parse_str(key_id).map_err(|err| anyhow!("invalid key_id: {err}"))?;
                    self.ensure_active_key(key_id).await?;
                }
                Ok(json!({
                    "valid": true,
                    "claims": claims_to_json(&claims)
                }))
            }
            other => Err(anyhow!("unsupported auth action '{other}'")),
        }
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
    use crate::rls::DatabasePolicyEngine;
    use crate::ApiKeyScope;
    use serde_json::json;

    const TEST_DB_URL: &str = "sqlite::memory:";

    async fn setup_rest_api() -> EmbeddedRestApi {
        let database = Database::connect(TEST_DB_URL).await.unwrap();
        let context = ExecutionContext::new(InMemoryStorageEngine::new());
        let policy_engine = Arc::new(DatabasePolicyEngine::new(database.clone()));
        let auth = Arc::new(JwtHmacAuth::new("secret-key"));
        EmbeddedRestApi::new(database, context, policy_engine, auth)
    }

    #[tokio::test]
    async fn rest_api_executes_sql_queries() {
        let api = setup_rest_api().await;

        api.handle_query(json!({
            "sql": "INSERT INTO projects (id, name) VALUES (1, 'alpha')",
            "claims": {
                "subject": "user-1",
                "scope": "namespace:alpha"
            }
        }))
        .await
        .unwrap();

        let result = api
            .handle_query(json!({
                "sql": "SELECT * FROM projects",
                "claims": {
                    "subject": "user-1",
                    "scope": "namespace:alpha"
                }
            }))
            .await
            .unwrap();

        assert_eq!(result.rows.len(), 1);
        assert_eq!(result.rows[0][0], ScalarValue::Int64(1));
    }

    #[tokio::test]
    async fn auth_endpoint_issues_and_verifies_tokens() {
        let api = setup_rest_api().await;
        let record = api
            .database
            .insert_api_key(
                "hash-123",
                "adm-123",
                ApiKeyScope::Admin,
                100,
                None,
                None,
                None,
            )
            .await
            .unwrap();

        let issued = api
            .handle_auth(json!({
                "action": "issue",
                "claims": {
                    "subject": "user-42",
                    "scope": "admin"
                },
                "key_id": record.id.to_string()
            }))
            .await
            .unwrap();

        let token = issued["token"].as_str().unwrap();
        assert!(!token.is_empty());
        assert_eq!(issued["claims"]["subject"].as_str().unwrap(), "user-42");

        let verified = api
            .handle_auth(json!({
                "action": "verify",
                "token": token
            }))
            .await
            .unwrap();

        assert!(verified["valid"].as_bool().unwrap());
        assert_eq!(verified["claims"]["scope"].as_str().unwrap(), "admin");

        let refreshed = api
            .database
            .fetch_api_key(record.id)
            .await
            .unwrap()
            .unwrap();
        assert!(refreshed.last_used_at.is_some());
    }

    #[tokio::test]
    async fn auth_endpoint_rejects_invalid_token() {
        let api = setup_rest_api().await;
        let err = api
            .handle_auth(json!({
                "action": "verify",
                "token": "definitely-invalid"
            }))
            .await
            .expect_err("should reject invalid token");
        assert!(err.to_string().contains("invalid jwt"));
    }

    #[tokio::test]
    async fn auth_endpoint_rejects_unknown_key() {
        let api = setup_rest_api().await;
        let err = api
            .handle_auth(json!({
                "action": "issue",
                "claims": {
                    "subject": "user-42",
                    "scope": "admin"
                },
                "key_id": Uuid::new_v4().to_string()
            }))
            .await
            .expect_err("should fail for unknown key");
        assert!(err.to_string().contains("not found"));
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
