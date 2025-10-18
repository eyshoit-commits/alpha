//! SQL pipeline scaffolding (parser, planner, executor interfaces).

#![allow(dead_code)]

use anyhow::Result;

// TODO(bkg-db/sql): Ersetze diese Platzhalter durch eine echte SQL-Pipeline mit
// Parser (sqlparser), Validator und AST-Normalisierung.

/// Placeholder AST representation used while the real parser is under development.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SqlAst {
    pub raw_sql: String,
}

impl SqlAst {
    pub fn new(raw_sql: impl Into<String>) -> Self {
        Self {
            raw_sql: raw_sql.into(),
        }
    }
}

/// Contract for SQL parsers.
pub trait SqlParser {
    fn parse(&self, sql: &str) -> Result<SqlAst>;
}

/// Contract for validating/normalising SQL statements before planning.
pub trait SqlValidator {
    fn validate(&self, ast: &SqlAst) -> Result<()>;
}
