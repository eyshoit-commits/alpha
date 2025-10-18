//! Query executor scaffolding for bkg-db.

#![allow(dead_code)]

use anyhow::Result;

use crate::planner::LogicalPlan;

// TODO(bkg-db/executor): Implementiere einen physischen Executor (Batch/Stream)
// der mit dem MVCC-Kernel interagiert und Ergebnisse zurückgibt.

/// Execution context placeholder (session, transaction handles, etc.).
#[derive(Debug, Default, Clone)]
pub struct ExecutionContext;

/// Placeholder execution result structure.
#[derive(Debug, Default, Clone)]
pub struct ExecutionResult {
    pub rows_affected: u64,
}

/// Executor contract translating logical plans into results.
pub trait QueryExecutor {
    fn execute(&self, ctx: &ExecutionContext, plan: &LogicalPlan) -> Result<ExecutionResult>;
}

#[derive(Debug, Default, Clone)]
pub struct ExecutorDraft;

impl ExecutorDraft {
    pub fn new() -> Self {
        Self
    }
}
