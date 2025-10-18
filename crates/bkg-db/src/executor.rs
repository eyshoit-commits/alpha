//! Query executor scaffolding for bkg-db.

#![allow(dead_code)]

use std::collections::HashMap;
use std::sync::Arc;

use anyhow::{anyhow, Result};
use parking_lot::RwLock;
use serde::{Deserialize, Serialize};
use serde_json::{from_slice, Map, Number, Value};

use crate::auth::TokenClaims;
use crate::kernel::{InMemoryStorageEngine, StorageEngine, StorageTransaction, TransactionMode};
use crate::planner::{AggregatePlan, ComparisonOp, FilterExpr, FilterKind, LogicalPlan};
use crate::rls::{RlsPolicy, RlsPolicyEngine};

// TODO(bkg-db/executor): Streaming Iterator API, echte MVCC-Leseansichten,
// Projection/Filter Operatoren implementieren.

/// Execution context holding table data and storage engine.
#[derive(Clone)]
pub struct ExecutionContext {
    storage: InMemoryStorageEngine,
    tables: Arc<RwLock<HashMap<String, TableData>>>,
}

impl ExecutionContext {
    pub fn new(storage: InMemoryStorageEngine) -> Self {
        Self::try_new(storage).expect("failed to initialize ExecutionContext from WAL")
    }

    pub fn try_new(storage: InMemoryStorageEngine) -> Result<Self> {
        let ctx = Self {
            storage,
            tables: Arc::new(RwLock::new(HashMap::new())),
        };
        ctx.replay_wal()?;
        Ok(ctx)
    }

    pub fn wal_entries(&self) -> usize {
        self.storage.wal_entries()
    }

    /// Returns lightweight metadata describing the currently managed tables.
    pub fn table_summaries(&self) -> Vec<TableSummary> {
        let tables = self.tables.read();
        tables
            .iter()
            .map(|(name, data)| TableSummary {
                name: name.clone(),
                columns: data.columns.clone(),
                row_count: data.rows.len(),
            })
            .collect()
    }

    fn replay_wal(&self) -> Result<()> {
        let records = self.storage.wal_records();
        if records.is_empty() {
            return Ok(());
        }

        let mut tables = self.tables.write();
        tables.clear();

        for raw in records {
            let entry: WalEntry = from_slice(&raw)?;
            let table_entry = tables.entry(entry.table.clone()).or_default();

            if table_entry.columns.is_empty() {
                table_entry.columns = entry.columns.clone();
            } else if table_entry.columns != entry.columns {
                return Err(anyhow!(
                    "wal replay mismatch: expected columns {:?}, got {:?}",
                    table_entry.columns,
                    entry.columns
                ));
            }

            match entry.event {
                WalEventKind::Insert => {
                    let row = entry
                        .row_after
                        .ok_or_else(|| anyhow!("insert entry missing row_after"))?;
                    table_entry.rows.push(row);
                }
                WalEventKind::Update => {
                    let before = entry
                        .row_before
                        .ok_or_else(|| anyhow!("update entry missing row_before"))?;
                    let after = entry
                        .row_after
                        .ok_or_else(|| anyhow!("update entry missing row_after"))?;
                    let position = table_entry
                        .rows
                        .iter()
                        .position(|row| row == &before)
                        .ok_or_else(|| anyhow!("update entry row not found during replay"))?;
                    table_entry.rows[position] = after;
                }
                WalEventKind::Delete => {
                    let before = entry
                        .row_before
                        .ok_or_else(|| anyhow!("delete entry missing row_before"))?;
                    if let Some(position) = table_entry.rows.iter().position(|row| row == &before) {
                        table_entry.rows.remove(position);
                    }
                }
            }
        }

        Ok(())
    }
}

#[derive(Debug, Default, Clone)]
struct TableData {
    columns: Vec<String>,
    rows: Vec<Vec<ScalarValue>>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TableSummary {
    pub name: String,
    pub columns: Vec<String>,
    pub row_count: usize,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
enum WalEventKind {
    Insert,
    Update,
    Delete,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct WalEntry {
    event: WalEventKind,
    table: String,
    columns: Vec<String>,
    row_before: Option<Vec<ScalarValue>>,
    row_after: Option<Vec<ScalarValue>>,
}

impl WalEntry {
    fn insert(table: &str, columns: &[String], row_after: Vec<ScalarValue>) -> Self {
        Self {
            event: WalEventKind::Insert,
            table: table.to_string(),
            columns: columns.to_vec(),
            row_before: None,
            row_after: Some(row_after),
        }
    }

    fn update(
        table: &str,
        columns: &[String],
        before: Vec<ScalarValue>,
        after: Vec<ScalarValue>,
    ) -> Self {
        Self {
            event: WalEventKind::Update,
            table: table.to_string(),
            columns: columns.to_vec(),
            row_before: Some(before),
            row_after: Some(after),
        }
    }

    fn delete(table: &str, columns: &[String], before: Vec<ScalarValue>) -> Self {
        Self {
            event: WalEventKind::Delete,
            table: table.to_string(),
            columns: columns.to_vec(),
            row_before: Some(before),
            row_after: None,
        }
    }
}

/// Placeholder execution result structure.
#[derive(Debug, Default, Clone)]
pub struct ExecutionResult {
    pub rows_affected: u64,
    pub rows: Vec<Vec<ScalarValue>>,
}

/// Executor contract translating logical plans into results.
pub trait QueryExecutor {
    fn execute(
        &self,
        ctx: &ExecutionContext,
        plan: &LogicalPlan,
        claims: &TokenClaims,
        policies: &[RlsPolicy],
        engine: &dyn RlsPolicyEngine,
    ) -> Result<ExecutionResult>;
}

#[derive(Debug, Default, Clone)]
pub struct DefaultQueryExecutor;

impl DefaultQueryExecutor {
    pub fn new() -> Self {
        Self
    }
}

impl QueryExecutor for DefaultQueryExecutor {
    fn execute(
        &self,
        ctx: &ExecutionContext,
        plan: &LogicalPlan,
        claims: &TokenClaims,
        policies: &[RlsPolicy],
        engine: &dyn RlsPolicyEngine,
    ) -> Result<ExecutionResult> {
        match plan {
            LogicalPlan::Insert {
                table,
                columns,
                values,
            } => execute_insert(ctx, table, columns, values, claims, policies, engine),
            LogicalPlan::Select {
                table,
                filter,
                aggregate,
            } => execute_select(ctx, table, filter, aggregate, claims, policies, engine),
            LogicalPlan::Update {
                table,
                assignments,
                filter,
            } => execute_update(ctx, table, assignments, filter, claims, policies, engine),
            LogicalPlan::Delete { table, filter } => {
                execute_delete(ctx, table, filter, claims, policies, engine)
            }
        }
    }
}

fn execute_insert(
    ctx: &ExecutionContext,
    table: &str,
    columns: &[String],
    values: &[Vec<ScalarValue>],
    claims: &TokenClaims,
    policies: &[RlsPolicy],
    engine: &dyn RlsPolicyEngine,
) -> Result<ExecutionResult> {
    if values.is_empty() {
        return Ok(ExecutionResult::default());
    }

    let mut tx = ctx.storage.begin_transaction(TransactionMode::ReadWrite)?;

    {
        let mut tables = ctx.tables.write();
        let entry = tables.entry(table.to_string()).or_default();

        if entry.columns.is_empty() {
            if !columns.is_empty() {
                entry.columns = columns.to_vec();
            } else {
                entry.columns = (0..values[0].len())
                    .map(|idx| format!("col{idx}"))
                    .collect();
            }
        } else if !columns.is_empty() && entry.columns != columns {
            return Err(anyhow!(
                "column list does not match existing schema for table {table}"
            ));
        }

        for row in values {
            if row.len() != entry.columns.len() {
                return Err(anyhow!("row length does not match table schema"));
            }
            if !row_allowed(engine, policies, claims, &entry.columns, row)? {
                return Err(anyhow!(
                    "row violates RLS policy for table '{table}' during insert"
                ));
            }
            entry.rows.push(row.clone());
            let wal_entry = WalEntry::insert(table, &entry.columns, row.clone());
            let payload = serde_json::to_vec(&wal_entry)?;
            tx.append_log(&payload)?;
        }
    }

    tx.commit()?;

    Ok(ExecutionResult {
        rows_affected: values.len() as u64,
        rows: Vec::new(),
    })
}

fn execute_select(
    ctx: &ExecutionContext,
    table: &str,
    filter: &Option<FilterExpr>,
    aggregate: &Option<AggregatePlan>,
    claims: &TokenClaims,
    policies: &[RlsPolicy],
    engine: &dyn RlsPolicyEngine,
) -> Result<ExecutionResult> {
    let tables = ctx.tables.read();
    let entry = tables
        .get(table)
        .ok_or_else(|| anyhow!("table '{table}' not found"))?;

    let prepared_filter = prepare_filter(&entry.columns, filter)?;

    let mut matched_rows = Vec::new();
    for row in &entry.rows {
        if filter_matches(&prepared_filter, row)?
            && row_allowed(engine, policies, claims, &entry.columns, row)?
        {
            matched_rows.push(row.clone());
        }
    }

    if let Some(aggregate) = aggregate {
        let value = match aggregate {
            AggregatePlan::CountStar => ScalarValue::Int64(matched_rows.len() as i64),
        };
        return Ok(ExecutionResult {
            rows_affected: 1,
            rows: vec![vec![value]],
        });
    }

    Ok(ExecutionResult {
        rows_affected: matched_rows.len() as u64,
        rows: matched_rows,
    })
}

fn execute_update(
    ctx: &ExecutionContext,
    table: &str,
    assignments: &[(String, ScalarValue)],
    filter: &Option<FilterExpr>,
    claims: &TokenClaims,
    policies: &[RlsPolicy],
    engine: &dyn RlsPolicyEngine,
) -> Result<ExecutionResult> {
    let mut tx = ctx.storage.begin_transaction(TransactionMode::ReadWrite)?;

    let mut tables = ctx.tables.write();
    let entry = tables
        .get_mut(table)
        .ok_or_else(|| anyhow!("table '{table}' not found"))?;

    let prepared_filter = prepare_filter(&entry.columns, filter)?;

    let mut indexed_assignments = Vec::new();
    for (column, value) in assignments {
        let idx = find_column_index(&entry.columns, column)?;
        indexed_assignments.push((idx, value.clone()));
    }

    let mut affected = 0u64;
    for row in entry.rows.iter_mut() {
        if filter_matches(&prepared_filter, row)?
            && row_allowed(engine, policies, claims, &entry.columns, row)?
        {
            let before = row.clone();
            let mut updated = row.clone();
            for (idx, value) in &indexed_assignments {
                updated[*idx] = value.clone();
            }
            if !row_allowed(engine, policies, claims, &entry.columns, &updated)? {
                return Err(anyhow!(
                    "row violates RLS policy for table '{table}' during update"
                ));
            }
            *row = updated.clone();
            let wal_entry = WalEntry::update(table, &entry.columns, before, updated);
            let payload = serde_json::to_vec(&wal_entry)?;
            tx.append_log(&payload)?;
            affected += 1;
        }
    }

    tx.commit()?;

    Ok(ExecutionResult {
        rows_affected: affected,
        rows: Vec::new(),
    })
}

fn execute_delete(
    ctx: &ExecutionContext,
    table: &str,
    filter: &Option<FilterExpr>,
    claims: &TokenClaims,
    policies: &[RlsPolicy],
    engine: &dyn RlsPolicyEngine,
) -> Result<ExecutionResult> {
    let mut tx = ctx.storage.begin_transaction(TransactionMode::ReadWrite)?;

    let mut tables = ctx.tables.write();
    let entry = tables
        .get_mut(table)
        .ok_or_else(|| anyhow!("table '{table}' not found"))?;

    let prepared_filter = prepare_filter(&entry.columns, filter)?;

    let mut kept = Vec::with_capacity(entry.rows.len());
    let mut removed = Vec::new();

    for row in entry.rows.iter() {
        if filter_matches(&prepared_filter, row)?
            && row_allowed(engine, policies, claims, &entry.columns, row)?
        {
            removed.push(row.clone());
        } else {
            kept.push(row.clone());
        }
    }

    entry.rows = kept;

    for row in &removed {
        let wal_entry = WalEntry::delete(table, &entry.columns, row.clone());
        let payload = serde_json::to_vec(&wal_entry)?;
        tx.append_log(&payload)?;
    }

    tx.commit()?;

    Ok(ExecutionResult {
        rows_affected: removed.len() as u64,
        rows: Vec::new(),
    })
}

fn row_allowed(
    engine: &dyn RlsPolicyEngine,
    policies: &[RlsPolicy],
    claims: &TokenClaims,
    columns: &[String],
    row: &[ScalarValue],
) -> Result<bool> {
    if policies.is_empty() {
        return Ok(true);
    }
    let json_row = row_to_json(columns, row)?;
    for policy in policies {
        if engine.evaluate(policy, claims, &json_row)? {
            return Ok(true);
        }
    }
    Ok(false)
}

fn row_to_json(columns: &[String], row: &[ScalarValue]) -> Result<Value> {
    let mut map = Map::new();
    for (column, value) in columns.iter().zip(row.iter()) {
        map.insert(column.clone(), scalar_to_value(value)?);
    }
    Ok(Value::Object(map))
}

fn scalar_to_value(value: &ScalarValue) -> Result<Value> {
    Ok(match value {
        ScalarValue::Int64(v) => Value::Number((*v).into()),
        ScalarValue::Float64(v) => {
            Value::Number(Number::from_f64(*v).ok_or_else(|| anyhow!("unsupported float value"))?)
        }
        ScalarValue::Bool(v) => Value::Bool(*v),
        ScalarValue::String(v) => Value::String(v.clone()),
        ScalarValue::Null => Value::Null,
    })
}

#[derive(Debug, Clone)]
enum PreparedFilter {
    Comparison {
        index: usize,
        op: ComparisonOp,
        value: ScalarValue,
    },
    And(Box<PreparedFilter>, Box<PreparedFilter>),
    Or(Box<PreparedFilter>, Box<PreparedFilter>),
}

fn prepare_filter(
    columns: &[String],
    filter: &Option<FilterExpr>,
) -> Result<Option<PreparedFilter>> {
    match filter {
        Some(expr) => Ok(Some(prepare_filter_expr(columns, expr)?)),
        None => Ok(None),
    }
}

fn prepare_filter_expr(columns: &[String], expr: &FilterExpr) -> Result<PreparedFilter> {
    match &expr.kind {
        FilterKind::Comparison { column, op, value } => {
            let index = find_column_index(columns, column)?;
            Ok(PreparedFilter::Comparison {
                index,
                op: *op,
                value: value.clone(),
            })
        }
        FilterKind::And(lhs, rhs) => Ok(PreparedFilter::And(
            Box::new(prepare_filter_expr(columns, lhs)?),
            Box::new(prepare_filter_expr(columns, rhs)?),
        )),
        FilterKind::Or(lhs, rhs) => Ok(PreparedFilter::Or(
            Box::new(prepare_filter_expr(columns, lhs)?),
            Box::new(prepare_filter_expr(columns, rhs)?),
        )),
    }
}

fn filter_matches(filter: &Option<PreparedFilter>, row: &[ScalarValue]) -> Result<bool> {
    match filter {
        Some(expr) => evaluate_prepared_filter(expr, row),
        None => Ok(true),
    }
}

fn evaluate_prepared_filter(filter: &PreparedFilter, row: &[ScalarValue]) -> Result<bool> {
    match filter {
        PreparedFilter::Comparison { index, op, value } => {
            let current = row
                .get(*index)
                .ok_or_else(|| anyhow!("column index out of bounds"))?;
            compare_values(*op, current, value)
        }
        PreparedFilter::And(lhs, rhs) => {
            Ok(evaluate_prepared_filter(lhs, row)? && evaluate_prepared_filter(rhs, row)?)
        }
        PreparedFilter::Or(lhs, rhs) => {
            Ok(evaluate_prepared_filter(lhs, row)? || evaluate_prepared_filter(rhs, row)?)
        }
    }
}

fn compare_values(op: ComparisonOp, left: &ScalarValue, right: &ScalarValue) -> Result<bool> {
    match (left, right) {
        (ScalarValue::Int64(a), ScalarValue::Int64(b)) => Ok(match op {
            ComparisonOp::Eq => a == b,
            ComparisonOp::NotEq => a != b,
            ComparisonOp::Gt => a > b,
            ComparisonOp::Lt => a < b,
            ComparisonOp::Gte => a >= b,
            ComparisonOp::Lte => a <= b,
        }),
        (ScalarValue::Float64(a), ScalarValue::Float64(b)) => Ok(match op {
            ComparisonOp::Eq => a == b,
            ComparisonOp::NotEq => a != b,
            ComparisonOp::Gt => a > b,
            ComparisonOp::Lt => a < b,
            ComparisonOp::Gte => a >= b,
            ComparisonOp::Lte => a <= b,
        }),
        (ScalarValue::String(a), ScalarValue::String(b)) => Ok(match op {
            ComparisonOp::Eq => a == b,
            ComparisonOp::NotEq => a != b,
            ComparisonOp::Gt => a > b,
            ComparisonOp::Lt => a < b,
            ComparisonOp::Gte => a >= b,
            ComparisonOp::Lte => a <= b,
        }),
        (ScalarValue::Bool(a), ScalarValue::Bool(b)) => Ok(match op {
            ComparisonOp::Eq => a == b,
            ComparisonOp::NotEq => a != b,
            _ => return Err(anyhow!("unsupported comparison for boolean")),
        }),
        (ScalarValue::Null, ScalarValue::Null) => Ok(matches!(op, ComparisonOp::Eq)),
        _ => Err(anyhow!(
            "unsupported comparison between {left:?} and {right:?}"
        )),
    }
}

fn find_column_index(columns: &[String], column: &str) -> Result<usize> {
    columns
        .iter()
        .position(|c| c == column)
        .ok_or_else(|| anyhow!("column '{column}' not found"))
}

/// Scalar values used by the executor and planner.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum ScalarValue {
    Int64(i64),
    Float64(f64),
    Bool(bool),
    String(String),
    Null,
}

impl ScalarValue {
    pub fn as_string(&self) -> Result<String> {
        match self {
            ScalarValue::String(s) => Ok(s.clone()),
            other => Err(anyhow!("value is not a string: {other:?}")),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::auth::TokenClaims;
    use crate::kernel::InMemoryStorageEngine;
    use crate::planner::{LogicalOptimizer, LogicalPlanner, PlannerDraft};
    use crate::rls::{InMemoryPolicyEngine, RlsPolicy};
    use crate::sql::{DefaultSqlParser, SqlParser};
    use serde_json::json;
    use tempfile::tempdir;

    #[test]
    fn insert_update_delete_and_select() {
        let dir = tempdir().expect("tempdir");
        let wal_path = dir.path().join("pipeline.wal");
        let storage = InMemoryStorageEngine::with_file_wal(&wal_path).expect("storage");
        let ctx = ExecutionContext::new(storage.clone());
        let parser = DefaultSqlParser::new();
        let planner = PlannerDraft::new();
        let executor = DefaultQueryExecutor::new();
        let engine = InMemoryPolicyEngine::new();
        let claims = TokenClaims {
            subject: "user-1".into(),
            scope: "namespace:alpha".into(),
            issued_at: chrono::Utc::now(),
            expires_at: None,
        };
        let policies: Vec<RlsPolicy> = Vec::new();

        let insert_ast = parser
            .parse("INSERT INTO projects (id, name) VALUES (1, 'alpha')")
            .expect("parse insert");
        let plan = planner
            .optimize(planner.build_logical_plan(&insert_ast).unwrap())
            .unwrap();
        let result = executor
            .execute(&ctx, &plan, &claims, &policies, &engine)
            .expect("execute insert");
        assert_eq!(result.rows_affected, 1);
        assert_eq!(ctx.wal_entries(), 1);

        let select_ast = parser
            .parse("SELECT * FROM projects WHERE name = 'alpha'")
            .expect("parse select");
        let select_plan = planner
            .optimize(planner.build_logical_plan(&select_ast).unwrap())
            .unwrap();
        let rows = executor
            .execute(&ctx, &select_plan, &claims, &policies, &engine)
            .expect("execute select");
        assert_eq!(rows.rows.len(), 1);
        assert_eq!(rows.rows[0][0], ScalarValue::Int64(1));
        assert_eq!(rows.rows[0][1], ScalarValue::String("alpha".into()));

        let count_ast = parser
            .parse("SELECT COUNT(*) FROM projects WHERE name = 'alpha'")
            .expect("parse count");
        let count_plan = planner
            .optimize(planner.build_logical_plan(&count_ast).unwrap())
            .unwrap();
        let count_rows = executor
            .execute(&ctx, &count_plan, &claims, &policies, &engine)
            .expect("execute count");
        assert_eq!(count_rows.rows.len(), 1);
        assert_eq!(count_rows.rows[0][0], ScalarValue::Int64(1));

        let update_ast = parser
            .parse("UPDATE projects SET name = 'beta' WHERE id = 1")
            .expect("parse update");
        let update_plan = planner
            .optimize(planner.build_logical_plan(&update_ast).unwrap())
            .unwrap();
        let updated = executor
            .execute(&ctx, &update_plan, &claims, &policies, &engine)
            .expect("execute update");
        assert_eq!(updated.rows_affected, 1);
        assert_eq!(ctx.wal_entries(), 2);

        let neg_select = parser
            .parse("SELECT * FROM projects WHERE name = 'alpha'")
            .expect("parse select");
        let neg_plan = planner
            .optimize(planner.build_logical_plan(&neg_select).unwrap())
            .unwrap();
        let neg_rows = executor
            .execute(&ctx, &neg_plan, &claims, &policies, &engine)
            .expect("execute select");
        assert!(neg_rows.rows.is_empty());

        let delete_ast = parser
            .parse("DELETE FROM projects WHERE id = 1")
            .expect("parse delete");
        let delete_plan = planner
            .optimize(planner.build_logical_plan(&delete_ast).unwrap())
            .unwrap();
        let deleted = executor
            .execute(&ctx, &delete_plan, &claims, &policies, &engine)
            .expect("execute delete");
        assert_eq!(deleted.rows_affected, 1);
        assert_eq!(ctx.wal_entries(), 3);

        let post_delete_select = parser
            .parse("SELECT * FROM projects")
            .expect("parse select all");
        let post_delete_plan = planner
            .optimize(planner.build_logical_plan(&post_delete_select).unwrap())
            .unwrap();
        let post_delete_rows = executor
            .execute(&ctx, &post_delete_plan, &claims, &policies, &engine)
            .expect("execute select all");
        assert!(post_delete_rows.rows.is_empty());

        drop(ctx);

        let storage_reload = InMemoryStorageEngine::with_file_wal(&wal_path).expect("storage");
        storage_reload.recover().expect("recover");
        assert_eq!(storage_reload.wal_entries(), 3);
    }

    #[test]
    fn wal_replay_restores_rows() {
        let dir = tempdir().expect("tempdir");
        let wal_path = dir.path().join("restore.wal");
        let storage = InMemoryStorageEngine::with_file_wal(&wal_path).expect("storage");
        let ctx = ExecutionContext::new(storage.clone());
        let parser = DefaultSqlParser::new();
        let planner = PlannerDraft::new();
        let executor = DefaultQueryExecutor::new();
        let engine = InMemoryPolicyEngine::new();
        let claims = TokenClaims {
            subject: "user-1".into(),
            scope: "namespace:alpha".into(),
            issued_at: chrono::Utc::now(),
            expires_at: None,
        };
        let policies: Vec<RlsPolicy> = Vec::new();

        // Insert a project row.
        let insert_ast = parser
            .parse("INSERT INTO projects (id, name) VALUES (1, 'alpha')")
            .expect("parse insert");
        let insert_plan = planner
            .optimize(planner.build_logical_plan(&insert_ast).unwrap())
            .unwrap();
        executor
            .execute(&ctx, &insert_plan, &claims, &policies, &engine)
            .expect("execute insert");

        // Update the row to ensure WAL captures before/after images.
        let update_ast = parser
            .parse("UPDATE projects SET name = 'beta' WHERE id = 1")
            .expect("parse update");
        let update_plan = planner
            .optimize(planner.build_logical_plan(&update_ast).unwrap())
            .unwrap();
        executor
            .execute(&ctx, &update_plan, &claims, &policies, &engine)
            .expect("execute update");

        drop(ctx);

        let storage_reload = InMemoryStorageEngine::with_file_wal(&wal_path).expect("storage");
        storage_reload.recover().expect("recover");
        let ctx_reload = ExecutionContext::try_new(storage_reload.clone()).expect("ctx");

        let select_ast = parser
            .parse("SELECT * FROM projects")
            .expect("parse select");
        let select_plan = planner
            .optimize(planner.build_logical_plan(&select_ast).unwrap())
            .unwrap();
        let rows = executor
            .execute(&ctx_reload, &select_plan, &claims, &policies, &engine)
            .expect("execute select");
        assert_eq!(rows.rows.len(), 1);
        assert_eq!(rows.rows[0][0], ScalarValue::Int64(1));
        assert_eq!(rows.rows[0][1], ScalarValue::String("beta".into()));
    }

    #[test]
    fn rls_prevents_cross_namespace_access() {
        let storage = InMemoryStorageEngine::new();
        let ctx = ExecutionContext::new(storage);
        let parser = DefaultSqlParser::new();
        let planner = PlannerDraft::new();
        let executor = DefaultQueryExecutor::new();
        let engine = InMemoryPolicyEngine::new();

        let policies = vec![RlsPolicy {
            name: "namespace-scope".into(),
            table: "projects".into(),
            expression: json!({
                "eq": { "column": "namespace", "claim": "scope" }
            }),
        }];

        let alpha_claims = TokenClaims {
            subject: "user-alpha".into(),
            scope: "namespace:alpha".into(),
            issued_at: chrono::Utc::now(),
            expires_at: None,
        };

        let insert_alpha = parser
            .parse(
                "INSERT INTO projects (id, namespace, name) VALUES (1, 'namespace:alpha', 'Alpha')",
            )
            .expect("parse alpha insert");
        let insert_alpha_plan = planner
            .optimize(planner.build_logical_plan(&insert_alpha).unwrap())
            .unwrap();
        let result = executor
            .execute(&ctx, &insert_alpha_plan, &alpha_claims, &policies, &engine)
            .expect("alpha insert allowed");
        assert_eq!(result.rows_affected, 1);

        let beta_claims = TokenClaims {
            subject: "user-beta".into(),
            scope: "namespace:beta".into(),
            issued_at: chrono::Utc::now(),
            expires_at: None,
        };

        let insert_beta = parser
            .parse(
                "INSERT INTO projects (id, namespace, name) VALUES (2, 'namespace:beta', 'Beta')",
            )
            .expect("parse beta insert");
        let insert_beta_plan = planner
            .optimize(planner.build_logical_plan(&insert_beta).unwrap())
            .unwrap();
        let beta_result =
            executor.execute(&ctx, &insert_beta_plan, &alpha_claims, &policies, &engine);
        assert!(beta_result.is_err());

        let select_ast = parser
            .parse("SELECT * FROM projects")
            .expect("parse select");
        let select_plan = planner
            .optimize(planner.build_logical_plan(&select_ast).unwrap())
            .unwrap();
        let alpha_rows = executor
            .execute(&ctx, &select_plan, &alpha_claims, &policies, &engine)
            .expect("alpha select");
        assert_eq!(alpha_rows.rows.len(), 1);

        let beta_rows = executor
            .execute(&ctx, &select_plan, &beta_claims, &policies, &engine)
            .expect("beta select");
        assert!(beta_rows.rows.is_empty());

        let update_beta = parser
            .parse("UPDATE projects SET name = 'Nope' WHERE id = 1")
            .expect("parse update");
        let update_plan = planner
            .optimize(planner.build_logical_plan(&update_beta).unwrap())
            .unwrap();
        let update_result = executor
            .execute(&ctx, &update_plan, &beta_claims, &policies, &engine)
            .expect("beta update");
        assert_eq!(update_result.rows_affected, 0);
    }
}
