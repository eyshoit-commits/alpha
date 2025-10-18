//! Query executor scaffolding for bkg-db.

#![allow(dead_code)]

use std::collections::HashMap;
use std::sync::Arc;

use anyhow::{anyhow, Result};
use parking_lot::RwLock;
use serde::{Deserialize, Serialize};
use serde_json::json;

use crate::kernel::{InMemoryStorageEngine, StorageEngine, StorageTransaction, TransactionMode};
use crate::planner::{AggregatePlan, ComparisonOp, FilterExpr, FilterKind, LogicalPlan};

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
        Self {
            storage,
            tables: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    pub fn wal_entries(&self) -> usize {
        self.storage.wal_entries()
    }
}

#[derive(Debug, Default, Clone)]
struct TableData {
    columns: Vec<String>,
    rows: Vec<Vec<ScalarValue>>,
}

/// Placeholder execution result structure.
#[derive(Debug, Default, Clone)]
pub struct ExecutionResult {
    pub rows_affected: u64,
    pub rows: Vec<Vec<ScalarValue>>,
}

/// Executor contract translating logical plans into results.
pub trait QueryExecutor {
    fn execute(&self, ctx: &ExecutionContext, plan: &LogicalPlan) -> Result<ExecutionResult>;
}

#[derive(Debug, Default, Clone)]
pub struct DefaultQueryExecutor;

impl DefaultQueryExecutor {
    pub fn new() -> Self {
        Self
    }
}

impl QueryExecutor for DefaultQueryExecutor {
    fn execute(&self, ctx: &ExecutionContext, plan: &LogicalPlan) -> Result<ExecutionResult> {
        match plan {
            LogicalPlan::Insert {
                table,
                columns,
                values,
            } => execute_insert(ctx, table, columns, values),
            LogicalPlan::Select {
                table,
                filter,
                aggregate,
            } => execute_select(ctx, table, filter, aggregate),
            LogicalPlan::Update {
                table,
                assignments,
                filter,
            } => execute_update(ctx, table, assignments, filter),
            LogicalPlan::Delete { table, filter } => execute_delete(ctx, table, filter),
        }
    }
}

fn execute_insert(
    ctx: &ExecutionContext,
    table: &str,
    columns: &[String],
    values: &[Vec<ScalarValue>],
) -> Result<ExecutionResult> {
    if values.is_empty() {
        return Ok(ExecutionResult::default());
    }

    let mut tx = ctx.storage.begin_transaction(TransactionMode::ReadWrite)?;

    {
        let mut tables = ctx.tables.write();
        let entry = tables
            .entry(table.to_string())
            .or_insert_with(TableData::default);

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
            entry.rows.push(row.clone());
            let payload = serde_json::to_vec(&json!({
                "event": "insert",
                "table": table,
                "columns": entry.columns.clone(),
                "values": row,
            }))?;
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
) -> Result<ExecutionResult> {
    let tables = ctx.tables.read();
    let entry = tables
        .get(table)
        .ok_or_else(|| anyhow!("table '{table}' not found"))?;

    let prepared_filter = prepare_filter(&entry.columns, filter)?;

    let mut matched_rows = Vec::new();
    for row in &entry.rows {
        if filter_matches(&prepared_filter, row)? {
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
        if filter_matches(&prepared_filter, row)? {
            for (idx, value) in &indexed_assignments {
                row[*idx] = value.clone();
            }
            let snapshot = row.clone();
            let payload = serde_json::to_vec(&json!({
                "event": "update",
                "table": table,
                "columns": entry.columns.clone(),
                "values": snapshot,
            }))?;
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
        if filter_matches(&prepared_filter, row)? {
            removed.push(row.clone());
        } else {
            kept.push(row.clone());
        }
    }

    entry.rows = kept;

    for row in &removed {
        let payload = serde_json::to_vec(&json!({
            "event": "delete",
            "table": table,
            "columns": entry.columns.clone(),
            "values": row,
        }))?;
        tx.append_log(&payload)?;
    }

    tx.commit()?;

    Ok(ExecutionResult {
        rows_affected: removed.len() as u64,
        rows: Vec::new(),
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
    use crate::kernel::InMemoryStorageEngine;
    use crate::planner::{LogicalOptimizer, LogicalPlanner, PlannerDraft};
    use crate::sql::{DefaultSqlParser, SqlParser};
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

        let insert_ast = parser
            .parse("INSERT INTO projects (id, name) VALUES (1, 'alpha')")
            .expect("parse insert");
        let plan = planner
            .optimize(planner.build_logical_plan(&insert_ast).unwrap())
            .unwrap();
        let result = executor.execute(&ctx, &plan).expect("execute insert");
        assert_eq!(result.rows_affected, 1);
        assert_eq!(ctx.wal_entries(), 1);

        let select_ast = parser
            .parse("SELECT * FROM projects WHERE name = 'alpha'")
            .expect("parse select");
        let select_plan = planner
            .optimize(planner.build_logical_plan(&select_ast).unwrap())
            .unwrap();
        let rows = executor
            .execute(&ctx, &select_plan)
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
        let count_rows = executor.execute(&ctx, &count_plan).expect("execute count");
        assert_eq!(count_rows.rows.len(), 1);
        assert_eq!(count_rows.rows[0][0], ScalarValue::Int64(1));

        let update_ast = parser
            .parse("UPDATE projects SET name = 'beta' WHERE id = 1")
            .expect("parse update");
        let update_plan = planner
            .optimize(planner.build_logical_plan(&update_ast).unwrap())
            .unwrap();
        let updated = executor
            .execute(&ctx, &update_plan)
            .expect("execute update");
        assert_eq!(updated.rows_affected, 1);
        assert_eq!(ctx.wal_entries(), 2);

        let neg_select = parser
            .parse("SELECT * FROM projects WHERE name = 'alpha'")
            .expect("parse select");
        let neg_plan = planner
            .optimize(planner.build_logical_plan(&neg_select).unwrap())
            .unwrap();
        let neg_rows = executor.execute(&ctx, &neg_plan).expect("execute select");
        assert!(neg_rows.rows.is_empty());

        let delete_ast = parser
            .parse("DELETE FROM projects WHERE id = 1")
            .expect("parse delete");
        let delete_plan = planner
            .optimize(planner.build_logical_plan(&delete_ast).unwrap())
            .unwrap();
        let deleted = executor
            .execute(&ctx, &delete_plan)
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
            .execute(&ctx, &post_delete_plan)
            .expect("execute select all");
        assert!(post_delete_rows.rows.is_empty());

        drop(ctx);

        let storage_reload = InMemoryStorageEngine::with_file_wal(&wal_path).expect("storage");
        storage_reload.recover().expect("recover");
        assert_eq!(storage_reload.wal_entries(), 3);
    }
}
