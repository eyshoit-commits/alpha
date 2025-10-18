//! Query planner scaffolding for bkg-db.

#![allow(dead_code)]

use anyhow::{anyhow, Result};
use sqlparser::ast::{ObjectName, Query, SelectItem, SetExpr, Statement, TableFactor};

use crate::executor::ScalarValue;
use crate::sql::SqlAst;

// TODO(bkg-db/planner): Implement rule/Cost-based Optimizer und erzeugung von
// physischen Ausführungsplänen (Joins, Aggregationen, Filter Pushdown etc.).

/// Simplified logical plan used by the executor.
#[derive(Debug, Clone)]
pub enum LogicalPlan {
    Insert {
        table: String,
        values: Vec<Vec<ScalarValue>>,
    },
    SelectAll {
        table: String,
    },
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

impl LogicalPlanner for PlannerDraft {
    fn build_logical_plan(&self, ast: &SqlAst) -> Result<LogicalPlan> {
        match &ast.statement {
            Statement::Insert {
                table_name, source, ..
            } => build_insert_plan(table_name, source),
            Statement::Query(query) => build_select_plan(query),
            other => Err(anyhow!("statement not supported yet: {other:?}")),
        }
    }
}

impl LogicalOptimizer for PlannerDraft {
    fn optimize(&self, plan: LogicalPlan) -> Result<LogicalPlan> {
        // TODO(bkg-db/planner): implement rule-based optimizations.
        Ok(plan)
    }
}

fn build_insert_plan(table_name: &ObjectName, source: &Query) -> Result<LogicalPlan> {
    let table = table_name.to_string();
    let query_body = source.body.as_ref();
    let values = match query_body {
        SetExpr::Values(values) => &values.rows,
        _ => return Err(anyhow!("only VALUES inserts are supported")),
    };

    let mut rows = Vec::new();
    for row in values {
        let mut row_values = Vec::new();
        for value in row {
            row_values.push(value_to_scalar(value)?);
        }
        rows.push(row_values);
    }

    Ok(LogicalPlan::Insert {
        table,
        values: rows,
    })
}

fn build_select_plan(query: &Query) -> Result<LogicalPlan> {
    let body = &query.body;
    let select = match body.as_ref() {
        SetExpr::Select(select) => select,
        _ => return Err(anyhow!("unsupported query body")),
    };

    if select.projection.len() != 1 || !matches!(select.projection[0], SelectItem::Wildcard(_)) {
        return Err(anyhow!("only SELECT * is supported"));
    }

    if select.from.len() != 1 {
        return Err(anyhow!("only single-table queries are supported"));
    }

    let table = match &select.from[0].relation {
        TableFactor::Table { name, .. } => name.to_string(),
        _ => return Err(anyhow!("unsupported table factor")),
    };

    Ok(LogicalPlan::SelectAll { table })
}

fn value_to_scalar(value: &sqlparser::ast::Expr) -> Result<ScalarValue> {
    use sqlparser::ast::{Expr, Value};

    match value {
        Expr::Value(Value::Number(num, _)) => {
            if let Ok(i) = num.parse::<i64>() {
                Ok(ScalarValue::Int64(i))
            } else if let Ok(f) = num.parse::<f64>() {
                Ok(ScalarValue::Float64(f))
            } else {
                Err(anyhow!("unsupported numeric literal: {num}"))
            }
        }
        Expr::Value(Value::SingleQuotedString(s)) => Ok(ScalarValue::String(s.clone())),
        Expr::Value(Value::Boolean(b)) => Ok(ScalarValue::Bool(*b)),
        Expr::Value(Value::Null) => Ok(ScalarValue::Null),
        other => Err(anyhow!("unsupported literal expression: {other:?}")),
    }
}
