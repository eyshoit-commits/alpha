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
    auth::{JwtHmacAuth, JwtIssuer, JwtValidator, TokenClaims},
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

    async fn record_api_key_usage(&self, subject: &str, timestamp: DateTime<Utc>) -> Result<()> {
        let key_id = match Uuid::parse_str(subject) {
            Ok(id) => id,
            Err(err) => return Err(anyhow!("claims.subject must be a UUID: {err}")),
        };

        let Some(record) = self.database.fetch_api_key(key_id).await? else {
            return Err(anyhow!("api key {key_id} not found"));
        };

        if record.revoked {
            return Err(anyhow!("api key {key_id} is revoked"));
        }
        if let Some(expiration) = record.expires_at {
            if expiration <= timestamp {
                return Err(anyhow!(
                    "api key {key_id} expired at {}",
                    expiration.to_rfc3339()
                ));
            }
        }

        self.database.touch_api_key_usage(key_id, timestamp).await?;

        Ok(())
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

    if let Some(exp) = expires_at {
        if exp <= issued_at {
            return Err(anyhow!(
                "'claims.expires_at' must be later than 'claims.issued_at'"
            ));
        }
    }

    Ok(TokenClaims {
        subject: subject.to_string(),
        scope: scope.to_string(),
        issued_at,
        expires_at,
    })
}

fn claims_to_json(claims: &TokenClaims) -> Value {
    let mut obj = serde_json::Map::new();
    obj.insert("subject".to_string(), Value::String(claims.subject.clone()));
    obj.insert("scope".to_string(), Value::String(claims.scope.clone()));
    obj.insert(
        "issued_at".to_string(),
        Value::String(claims.issued_at.to_rfc3339()),
    );
    if let Some(exp) = claims.expires_at {
        obj.insert("expires_at".to_string(), Value::String(exp.to_rfc3339()));
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
            .unwrap_or("issue");

        match action {
            "issue" => {
                let claims = parse_claims(&body)?;
                let token = self.auth.issue(&claims)?;

                self.record_api_key_usage(&claims.subject, claims.issued_at)
                    .await?;

                Ok(json!({
                    "token": token,
                    "claims": claims_to_json(&claims),
                }))
            }
            "verify" => {
                let token = body
                    .get("token")
                    .and_then(Value::as_str)
                    .ok_or_else(|| anyhow!("'token' field is required"))?;
                let claims = self.auth.verify(token)?;
                self.record_api_key_usage(&claims.subject, Utc::now())
                    .await?;
                Ok(json!({
                    "claims": claims_to_json(&claims),
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
    use serde_json::json;

    const TEST_DB_URL: &str = "sqlite::memory:";

    async fn setup_rest_api() -> (EmbeddedRestApi, Database) {
        let database = Database::connect(TEST_DB_URL).await.unwrap();
        let context = ExecutionContext::new(InMemoryStorageEngine::new());
        let policy_engine = Arc::new(DatabasePolicyEngine::new(database.clone()));
        let auth = Arc::new(JwtHmacAuth::new("secret-key"));
        let api = EmbeddedRestApi::new(database.clone(), context, policy_engine, auth);
        (api, database)
    }

    #[tokio::test]
    async fn rest_api_executes_sql_queries() {
        let (api, _) = setup_rest_api().await;

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
    async fn rest_api_manages_rls_policies() {
        let (api, _) = setup_rest_api().await;

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

    #[tokio::test]
    async fn rest_api_issues_and_verifies_tokens() {
        use crate::ApiKeyScope;

        let (api, database) = setup_rest_api().await;
        let record = database
            .insert_api_key(
                "hash-123",
                "hash-",
                ApiKeyScope::Admin,
                120,
                None,
                None,
                None,
            )
            .await
            .unwrap();

        let issued_at = Utc::now();
        let expires_at = issued_at + chrono::Duration::hours(1);
        let issue_response = api
            .handle_auth(json!({
                "action": "issue",
                "claims": {
                    "subject": record.id.to_string(),
                    "scope": "admin",
                    "issued_at": issued_at.to_rfc3339(),
                    "expires_at": expires_at.to_rfc3339(),
                }
            }))
            .await
            .unwrap();

        let token = issue_response["token"].as_str().unwrap();
        let issued_claims = issue_response["claims"].as_object().unwrap();
        assert_eq!(
            issued_claims["subject"].as_str().unwrap(),
            record.id.to_string()
        );
        assert_eq!(issued_claims["scope"].as_str().unwrap(), "admin");

        let stored_after_issue = database.fetch_api_key(record.id).await.unwrap().unwrap();
        assert_eq!(stored_after_issue.last_used_at, Some(issued_at));

        let verify_response = api
            .handle_auth(json!({
                "action": "verify",
                "token": token
            }))
            .await
            .unwrap();

        let verified = verify_response["claims"].as_object().unwrap();
        assert_eq!(verified["subject"].as_str().unwrap(), record.id.to_string());
        assert_eq!(verified["scope"].as_str().unwrap(), "admin");
        assert_eq!(
            verified["issued_at"].as_str().unwrap(),
            issued_claims["issued_at"].as_str().unwrap()
        );
        assert_eq!(
            verified["expires_at"].as_str().unwrap(),
            issued_claims["expires_at"].as_str().unwrap()
        );

        let stored_after_verify = database.fetch_api_key(record.id).await.unwrap().unwrap();
        let last_used = stored_after_verify.last_used_at.unwrap();
        assert!(last_used >= issued_at);
        assert!(last_used >= stored_after_issue.last_used_at.unwrap());
    }

    #[tokio::test]
    async fn rest_api_rejects_invalid_auth_payloads() {
        let (api, _) = setup_rest_api().await;

        let missing_claims = api
            .handle_auth(json!({
                "action": "issue"
            }))
            .await
            .unwrap_err();
        assert!(missing_claims
            .to_string()
            .contains("'claims' object is required"));

        let missing_token = api
            .handle_auth(json!({
                "action": "verify"
            }))
            .await
            .unwrap_err();
        assert!(missing_token
            .to_string()
            .contains("'token' field is required"));

        let invalid_token = api
            .handle_auth(json!({
                "action": "verify",
                "token": "invalid-token"
            }))
            .await
            .unwrap_err();
        assert!(invalid_token.to_string().contains("invalid jwt"));

        let invalid_subject = api
            .handle_auth(json!({
                "action": "issue",
                "claims": {
                    "subject": "not-a-uuid",
                    "scope": "admin"
                }
            }))
            .await
            .unwrap_err();
        assert!(invalid_subject
            .to_string()
            .contains("claims.subject must be a UUID"));

        let invalid_expiry = api
            .handle_auth(json!({
                "action": "issue",
                "claims": {
                    "subject": Uuid::new_v4().to_string(),
                    "scope": "admin",
                    "issued_at": Utc::now().to_rfc3339(),
                    "expires_at": (Utc::now() - chrono::Duration::minutes(5)).to_rfc3339()
                }
            }))
            .await
            .unwrap_err();
        assert!(invalid_expiry
            .to_string()
            .contains("'claims.expires_at' must be later"));

        let unsupported = api
            .handle_auth(json!({
                "action": "unknown"
            }))
            .await
            .unwrap_err();
        assert!(unsupported.to_string().contains("unsupported auth action"));
    }

    #[tokio::test]
    async fn rest_api_issue_rejects_missing_or_revoked_keys() {
        use crate::ApiKeyScope;

        let (api, database) = setup_rest_api().await;
        let unknown_id = Uuid::new_v4();

        let missing_key = api
            .handle_auth(json!({
                "action": "issue",
                "claims": {
                    "subject": unknown_id.to_string(),
                    "scope": "admin"
                }
            }))
            .await
            .unwrap_err();
        assert!(missing_key.to_string().contains("api key"));

        let record = database
            .insert_api_key(
                "hash-456",
                "hash-456-prefix",
                ApiKeyScope::Admin,
                60,
                None,
                None,
                None,
            )
            .await
            .unwrap();
        database.revoke_api_key(record.id).await.unwrap();

        let revoked_key = api
            .handle_auth(json!({
                "action": "issue",
                "claims": {
                    "subject": record.id.to_string(),
                    "scope": "admin"
                }
            }))
            .await
            .unwrap_err();
        assert!(revoked_key.to_string().contains("revoked"));
    }

    #[tokio::test]
    async fn rest_api_issue_rejects_expired_keys() {
        use crate::ApiKeyScope;

        let (api, database) = setup_rest_api().await;
        let expired_at = Utc::now() - chrono::Duration::hours(1);
        let record = database
            .insert_api_key(
                "hash-expired",
                "hash-exp-prefix",
                ApiKeyScope::Admin,
                60,
                Some(expired_at),
                None,
                None,
            )
            .await
            .unwrap();

        let err = api
            .handle_auth(json!({
                "action": "issue",
                "claims": {
                    "subject": record.id.to_string(),
                    "scope": "admin"
                }
            }))
            .await
            .unwrap_err();
        assert!(err.to_string().contains("expired"));
    }

    #[tokio::test]
    async fn rest_api_verify_rejects_invalid_keys() {
        use crate::ApiKeyScope;

        let (api, database) = setup_rest_api().await;
        let auth = JwtHmacAuth::new("secret-key");

        let bogus_claims = TokenClaims {
            subject: Uuid::new_v4().to_string(),
            scope: "admin".into(),
            issued_at: Utc::now(),
            expires_at: Some(Utc::now() + chrono::Duration::hours(1)),
        };
        let bogus_token = auth.issue(&bogus_claims).unwrap();
        let missing_key = api
            .handle_auth(json!({
                "action": "verify",
                "token": bogus_token
            }))
            .await
            .unwrap_err();
        assert!(missing_key.to_string().contains("api key"));

        let record = database
            .insert_api_key(
                "hash-valid",
                "hash-val-prefix",
                ApiKeyScope::Admin,
                60,
                None,
                None,
                None,
            )
            .await
            .unwrap();
        let issue_response = api
            .handle_auth(json!({
                "action": "issue",
                "claims": {
                    "subject": record.id.to_string(),
                    "scope": "admin"
                }
            }))
            .await
            .unwrap();
        let token = issue_response["token"].as_str().unwrap();

        database.revoke_api_key(record.id).await.unwrap();

        let revoked_err = api
            .handle_auth(json!({
                "action": "verify",
                "token": token
            }))
            .await
            .unwrap_err();
        assert!(revoked_err.to_string().contains("revoked"));
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
