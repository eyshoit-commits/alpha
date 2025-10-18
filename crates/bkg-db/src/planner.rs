//! Query planner scaffolding for bkg-db.

#![allow(dead_code)]

use anyhow::{anyhow, Result};
use sqlparser::ast::{
    Assignment, BinaryOperator, Expr, FunctionArg, FunctionArgExpr, ObjectName, Query, SelectItem,
    SetExpr, Statement, TableFactor, TableWithJoins,
};

use crate::executor::ScalarValue;
use crate::sql::SqlAst;

// TODO(bkg-db/planner): Implement rule/Cost-based Optimizer und erzeugung von
// physischen Ausführungsplänen (Joins, Aggregationen, Filter Pushdown etc.).

/// Simplified logical plan used by the executor.
#[derive(Debug, Clone)]
pub enum LogicalPlan {
    Insert {
        table: String,
        columns: Vec<String>,
        values: Vec<Vec<ScalarValue>>,
    },
    Select {
        table: String,
        filter: Option<FilterExpr>,
        aggregate: Option<AggregatePlan>,
    },
    Update {
        table: String,
        assignments: Vec<(String, ScalarValue)>,
        filter: Option<FilterExpr>,
    },
    Delete {
        table: String,
        filter: Option<FilterExpr>,
    },
}

#[derive(Debug, Clone)]
pub struct FilterExpr {
    pub kind: FilterKind,
}

#[derive(Debug, Clone)]
pub enum FilterKind {
    Comparison {
        column: String,
        op: ComparisonOp,
        value: ScalarValue,
    },
    And(Box<FilterExpr>, Box<FilterExpr>),
    Or(Box<FilterExpr>, Box<FilterExpr>),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ComparisonOp {
    Eq,
    NotEq,
    Gt,
    Lt,
    Gte,
    Lte,
}

#[derive(Debug, Clone)]
pub enum AggregatePlan {
    CountStar,
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
                table_name,
                columns,
                source,
                ..
            } => build_insert_plan(table_name, columns, source),
            Statement::Update {
                table,
                assignments,
                selection,
                ..
            } => build_update_plan(table, assignments, selection.as_ref()),
            Statement::Delete {
                from, selection, ..
            } => build_delete_plan(from, selection.as_ref()),
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

fn build_insert_plan(
    table_name: &ObjectName,
    columns: &[sqlparser::ast::Ident],
    source: &Query,
) -> Result<LogicalPlan> {
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

    let columns = if columns.is_empty() {
        Vec::new()
    } else {
        columns.iter().map(|c| c.value.clone()).collect()
    };

    Ok(LogicalPlan::Insert {
        table,
        columns,
        values: rows,
    })
}

fn build_select_plan(query: &Query) -> Result<LogicalPlan> {
    let body = &query.body;
    let select = match body.as_ref() {
        SetExpr::Select(select) => select,
        _ => return Err(anyhow!("unsupported query body")),
    };

    let aggregate = detect_aggregate(&select.projection)?;
    if aggregate.is_none()
        && (select.projection.len() != 1
            || !matches!(select.projection[0], SelectItem::Wildcard(_)))
    {
        return Err(anyhow!("only SELECT * or SELECT COUNT(*) is supported"));
    }

    if select.from.len() != 1 {
        return Err(anyhow!("only single-table queries are supported"));
    }

    let table = match &select.from[0].relation {
        TableFactor::Table { name, .. } => name.to_string(),
        _ => return Err(anyhow!("unsupported table factor")),
    };

    let filter = if let Some(selection) = &select.selection {
        parse_filter(selection)?
    } else {
        None
    };

    Ok(LogicalPlan::Select {
        table,
        filter,
        aggregate,
    })
}

fn build_update_plan(
    table: &TableWithJoins,
    assignments: &[Assignment],
    selection: Option<&Expr>,
) -> Result<LogicalPlan> {
    let table_name = match &table.relation {
        TableFactor::Table { name, .. } => name.to_string(),
        _ => return Err(anyhow!("unsupported table factor in UPDATE")),
    };

    let mut parsed_assignments = Vec::new();
    for assignment in assignments {
        if assignment.id.len() != 1 {
            return Err(anyhow!("multi-column assignments are not supported"));
        }
        let column = assignment.id[0].value.clone();
        parsed_assignments.push((column, value_to_scalar(&assignment.value)?));
    }

    let filter = if let Some(expr) = selection {
        parse_filter(expr)?
    } else {
        None
    };

    Ok(LogicalPlan::Update {
        table: table_name,
        assignments: parsed_assignments,
        filter,
    })
}

fn build_delete_plan(from: &[TableWithJoins], selection: Option<&Expr>) -> Result<LogicalPlan> {
    if from.len() != 1 {
        return Err(anyhow!("only single-table DELETE is supported"));
    }

    let table_name = match &from[0].relation {
        TableFactor::Table { name, .. } => name.to_string(),
        _ => return Err(anyhow!("unsupported table factor in DELETE")),
    };

    let filter = if let Some(expr) = selection {
        parse_filter(expr)?
    } else {
        None
    };

    Ok(LogicalPlan::Delete {
        table: table_name,
        filter,
    })
}

fn value_to_scalar(value: &Expr) -> Result<ScalarValue> {
    use sqlparser::ast::Value;

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

fn parse_filter(expr: &Expr) -> Result<Option<FilterExpr>> {
    match expr {
        Expr::BinaryOp { left, op, right } => match op {
            BinaryOperator::And => Ok(Some(FilterExpr {
                kind: FilterKind::And(
                    Box::new(parse_filter(left)?.ok_or_else(|| anyhow!("invalid filter"))?),
                    Box::new(parse_filter(right)?.ok_or_else(|| anyhow!("invalid filter"))?),
                ),
            })),
            BinaryOperator::Or => Ok(Some(FilterExpr {
                kind: FilterKind::Or(
                    Box::new(parse_filter(left)?.ok_or_else(|| anyhow!("invalid filter"))?),
                    Box::new(parse_filter(right)?.ok_or_else(|| anyhow!("invalid filter"))?),
                ),
            })),
            BinaryOperator::Eq
            | BinaryOperator::NotEq
            | BinaryOperator::Gt
            | BinaryOperator::Lt
            | BinaryOperator::GtEq
            | BinaryOperator::LtEq => match (&**left, &**right) {
                (Expr::Identifier(ident), value) => Ok(Some(FilterExpr {
                    kind: FilterKind::Comparison {
                        column: ident.value.clone(),
                        op: map_operator(op),
                        value: value_to_scalar(value)?,
                    },
                })),
                _ => Err(anyhow!("unsupported WHERE clause")),
            },
            _ => Err(anyhow!("only AND/OR with comparison filters are supported")),
        },
        _ => Err(anyhow!("unsupported WHERE clause")),
    }
}

fn detect_aggregate(projection: &[SelectItem]) -> Result<Option<AggregatePlan>> {
    if projection.len() != 1 {
        return Ok(None);
    }

    match &projection[0] {
        SelectItem::UnnamedExpr(Expr::Function(func)) => {
            if func.name.0.len() == 1
                && func.name.0[0].value.eq_ignore_ascii_case("count")
                && func.args.len() == 1
                && matches!(
                    func.args[0],
                    FunctionArg::Unnamed(FunctionArgExpr::Wildcard)
                )
            {
                Ok(Some(AggregatePlan::CountStar))
            } else {
                Err(anyhow!("unsupported aggregate function"))
            }
        }
        _ => Ok(None),
    }
}

fn map_operator(op: &BinaryOperator) -> ComparisonOp {
    match op {
        BinaryOperator::Eq => ComparisonOp::Eq,
        BinaryOperator::NotEq => ComparisonOp::NotEq,
        BinaryOperator::Gt => ComparisonOp::Gt,
        BinaryOperator::Lt => ComparisonOp::Lt,
        BinaryOperator::GtEq => ComparisonOp::Gte,
        BinaryOperator::LtEq => ComparisonOp::Lte,
        _ => ComparisonOp::Eq,
    }
}
