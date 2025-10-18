use std::sync::Arc;

use anyhow::Result;
use serde_json::json;
use sqlx::PgPool;
use tempfile::tempdir;

use bkg_db::{
    api::{EmbeddedRestApi, RestApiServer},
    executor::{ExecutionContext, ScalarValue},
    kernel::InMemoryStorageEngine,
    rls::DatabasePolicyEngine,
    Database, NewRlsPolicy,
};

fn claims(subject: &str, scope: &str) -> serde_json::Value {
    json!({
        "subject": subject,
        "scope": scope
    })
}

#[sqlx::test(migrations = "./migrations_postgres")]
async fn namespace_isolation_enforced(_pool: PgPool) -> Result<()> {
    let database_url = std::env::var("DATABASE_URL")?;
    let database = Database::connect(&database_url).await?;
    database
        .upsert_rls_policy(NewRlsPolicy {
            table_name: "projects".into(),
            policy_name: "namespace_scope".into(),
            expression: json!({
                "eq": { "column": "namespace", "claim": "scope" }
            }),
        })
        .await?;

    let engine = Arc::new(DatabasePolicyEngine::new(database.clone()));
    let context = ExecutionContext::new(InMemoryStorageEngine::new());
    let api = EmbeddedRestApi::new(database.clone(), context, engine.clone());

    api.handle_query(json!({
        "sql": "INSERT INTO projects (id, namespace, name) VALUES (1, 'namespace:alpha', 'Alpha')",
        "claims": claims("user-1", "namespace:alpha")
    }))
    .await?;

    api.handle_query(json!({
        "sql": "INSERT INTO projects (id, namespace, name) VALUES (2, 'namespace:beta', 'Beta')",
        "claims": claims("user-2", "namespace:beta")
    }))
    .await?;

    let alpha_rows = api
        .handle_query(json!({
            "sql": "SELECT * FROM projects",
            "claims": claims("user-1", "namespace:alpha")
        }))
        .await?;
    assert_eq!(alpha_rows.rows.len(), 1);
    assert_eq!(
        alpha_rows.rows[0][1],
        ScalarValue::String("namespace:alpha".into())
    );

    let beta_rows = api
        .handle_query(json!({
            "sql": "SELECT * FROM projects",
            "claims": claims("user-2", "namespace:beta")
        }))
        .await?;
    assert_eq!(beta_rows.rows.len(), 1);

    let denied_update = api
        .handle_query(json!({
            "sql": "UPDATE projects SET name = 'Nope' WHERE id = 2",
            "claims": claims("user-1", "namespace:alpha")
        }))
        .await?;
    assert_eq!(denied_update.rows_affected, 0);

    let insert_err = api
        .handle_query(json!({
            "sql": "INSERT INTO projects (id, namespace, name) VALUES (3, 'namespace:beta', 'Gamma')",
            "claims": claims("user-3", "namespace:alpha")
        }))
        .await;
    assert!(insert_err.is_err());

    Ok(())
}

#[sqlx::test(migrations = "./migrations_postgres")]
async fn postgres_policy_crud(_pool: PgPool) -> Result<()> {
    let database_url = std::env::var("DATABASE_URL")?;
    let database = Database::connect(&database_url).await?;
    let expression = json!({
        "eq": { "column": "namespace", "claim": "scope" }
    });

    let created = database
        .upsert_rls_policy(NewRlsPolicy {
            table_name: "projects".into(),
            policy_name: "namespace_scope".into(),
            expression: expression.clone(),
        })
        .await?;
    assert_eq!(created.expression, expression);

    let fetched = database
        .fetch_rls_policy(created.id)
        .await?
        .expect("policy present");
    assert_eq!(fetched.expression, expression);

    let listed = database.list_rls_policies(Some("projects")).await?;
    assert_eq!(listed.len(), 1);

    database.delete_rls_policy(created.id).await?;
    let remaining = database.list_rls_policies(Some("projects")).await?;
    assert!(remaining.is_empty());
    Ok(())
}

#[sqlx::test(migrations = "./migrations_postgres")]
async fn wal_recovery_with_postgres_policies(_pool: PgPool) -> Result<()> {
    let database_url = std::env::var("DATABASE_URL")?;
    let database = Database::connect(&database_url).await?;
    database
        .upsert_rls_policy(NewRlsPolicy {
            table_name: "projects".into(),
            policy_name: "namespace_scope".into(),
            expression: json!({
                "eq": { "column": "namespace", "claim": "scope" }
            }),
        })
        .await?;

    let engine = Arc::new(DatabasePolicyEngine::new(database.clone()));
    let dir = tempdir()?;
    let wal_path = dir.path().join("api.wal");
    let storage = InMemoryStorageEngine::with_file_wal(&wal_path)?;
    let context = ExecutionContext::new(storage.clone());
    let api = EmbeddedRestApi::new(database.clone(), context, engine.clone());

    api.handle_query(json!({
        "sql": "INSERT INTO projects (id, namespace, name) VALUES (10, 'namespace:alpha', 'Alpha')",
        "claims": claims("user-1", "namespace:alpha")
    }))
    .await?;

    drop(api);

    let reloaded_storage = InMemoryStorageEngine::with_file_wal(&wal_path)?;
    let reloaded_context = ExecutionContext::try_new(reloaded_storage)?;
    let reloaded_api = EmbeddedRestApi::new(database, reloaded_context, engine);

    let rows = reloaded_api
        .handle_query(json!({
            "sql": "SELECT * FROM projects",
            "claims": claims("user-1", "namespace:alpha")
        }))
        .await?;
    assert_eq!(rows.rows.len(), 1);
    assert_eq!(rows.rows[0][2], ScalarValue::String("Alpha".into()));

    let restricted = reloaded_api
        .handle_query(json!({
            "sql": "SELECT * FROM projects",
            "claims": claims("user-2", "namespace:beta")
        }))
        .await?;
    assert!(restricted.rows.is_empty());

    Ok(())
}
