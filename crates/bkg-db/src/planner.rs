//! Query planner scaffolding for bkg-db.

#![allow(dead_code)]

use anyhow::Result;

use crate::sql::SqlAst;

// TODO(bkg-db/planner): Implement rule/Cost-based Optimizer und erzeugung von
// physischen Ausführungsplänen.

/// Simplified logical plan placeholder.
#[derive(Debug, Clone)]
pub struct LogicalPlan {
    pub statement: String,
}

/// Planner contract to create logical plans from SQL ASTs.
pub trait LogicalPlanner {
    fn build_logical_plan(&self, ast: &SqlAst) -> Result<LogicalPlan>;
}

/// Optimizer contract to refine logical plans.
pub trait LogicalOptimizer {
    fn optimize(&self, plan: LogicalPlan) -> Result<LogicalPlan>;
}

/// Draft structure to coordinate planner components.
#[derive(Debug, Default, Clone)]
pub struct PlannerDraft;

impl PlannerDraft {
    pub fn new() -> Self {
        Self
    }
}
