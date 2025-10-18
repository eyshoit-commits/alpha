//! Query executor scaffolding for bkg-db.

#![allow(dead_code)]

use std::collections::HashMap;
use std::sync::Arc;

use anyhow::{anyhow, Result};
use parking_lot::RwLock;
use serde::{Deserialize, Serialize};
use serde_json::json;

use crate::kernel::{InMemoryStorageEngine, StorageEngine, StorageTransaction, TransactionMode};
use crate::planner::LogicalPlan;

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
            LogicalPlan::Insert { table, values } => execute_insert(ctx, table, values),
            LogicalPlan::SelectAll { table } => execute_select(ctx, table),
        }
    }
}

fn execute_insert(
    ctx: &ExecutionContext,
    table: &str,
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
        for row in values {
            entry.rows.push(row.clone());
            let payload = serde_json::to_vec(&json!({
                "table": table,
                "values": row,
            }))?;
            tx.append_log(&payload);
        }
    }

    tx.commit()?;

    Ok(ExecutionResult {
        rows_affected: values.len() as u64,
        rows: Vec::new(),
    })
}

fn execute_select(ctx: &ExecutionContext, table: &str) -> Result<ExecutionResult> {
    let tables = ctx.tables.read();
    let rows = tables
        .get(table)
        .map(|t| t.rows.clone())
        .unwrap_or_default();
    Ok(ExecutionResult {
        rows_affected: rows.len() as u64,
        rows,
    })
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
    fn insert_and_select_roundtrip() {
        let dir = tempdir().expect("tempdir");
        let wal_path = dir.path().join("pipeline.wal");
        let storage = InMemoryStorageEngine::with_file_wal(&wal_path).expect("storage");
        let ctx = ExecutionContext::new(storage.clone());
        let parser = DefaultSqlParser::new();
        let planner = PlannerDraft::new();
        let executor = DefaultQueryExecutor::new();

        let insert_ast = parser
            .parse("INSERT INTO projects VALUES (1, 'alpha')")
            .expect("parse insert");
        let plan = planner
            .optimize(planner.build_logical_plan(&insert_ast).unwrap())
            .unwrap();
        let result = executor.execute(&ctx, &plan).expect("execute insert");
        assert_eq!(result.rows_affected, 1);
        assert_eq!(ctx.wal_entries(), 1);

        let select_ast = parser
            .parse("SELECT * FROM projects")
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

        drop(ctx);

        // Reload storage and ensure WAL persists.
        let storage_reload = InMemoryStorageEngine::with_file_wal(&wal_path).expect("storage");
        storage_reload.recover().expect("recover");
        assert_eq!(storage_reload.wal_entries(), 1);
    }
}
