//! SQL pipeline scaffolding (parser, planner, executor interfaces).

#![allow(dead_code)]

use anyhow::{anyhow, Result};
use sqlparser::ast::Statement;
use sqlparser::dialect::PostgreSqlDialect;
use sqlparser::parser::Parser;

// TODO(bkg-db/sql): Erweiterte Analyse (Schema Validation, Param Binding, AST
// Normalisierung) ergänzen.

/// Wrapper um sqlparser AST.
#[derive(Debug, Clone)]
pub struct SqlAst {
    pub statement: Statement,
}

/// Contract for SQL parsers.
pub trait SqlParser {
    fn parse(&self, sql: &str) -> Result<SqlAst>;
}

/// Contract for validating/normalising SQL statements before planning.
pub trait SqlValidator {
    fn validate(&self, ast: &SqlAst) -> Result<()>;
}

/// Default parser using the PostgreSQL dialect.
pub struct DefaultSqlParser {
    dialect: PostgreSqlDialect,
}

impl Default for DefaultSqlParser {
    fn default() -> Self {
        Self {
            dialect: PostgreSqlDialect {},
        }
    }
}

impl DefaultSqlParser {
    pub fn new() -> Self {
        Self::default()
    }
}

impl SqlParser for DefaultSqlParser {
    fn parse(&self, sql: &str) -> Result<SqlAst> {
        let mut statements = Parser::parse_sql(&self.dialect, sql)
            .map_err(|err| anyhow!("SQL parse error: {err}"))?;
        if statements.len() != 1 {
            return Err(anyhow!("only single statements are supported"));
        }
        Ok(SqlAst {
            statement: statements.remove(0),
        })
    }
}

/// Simple validator placeholder (kein-op).
pub struct NoopValidator;

impl SqlValidator for NoopValidator {
    fn validate(&self, _ast: &SqlAst) -> Result<()> {
        Ok(())
    }
}
