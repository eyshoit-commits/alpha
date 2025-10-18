//! Row-Level Security scaffolding for bkg-db.

#![allow(dead_code)]

use anyhow::Result;
use serde_json::Value;

use crate::auth::TokenClaims;

// TODO(bkg-db/rls): Implementiere Policy Engine mit JSON Policies, Bindings und
// Evaluierung gegen Zeilenwerte.

/// Representation of a stored RLS policy.
#[derive(Debug, Clone)]
pub struct RlsPolicy {
    pub name: String,
    pub expression: Value,
}

/// Policy engine contract applied before query execution.
pub trait RlsPolicyEngine {
    fn evaluate(&self, policy: &RlsPolicy, claims: &TokenClaims, row: &Value) -> Result<bool>;
    fn policies_for_table(&self, table: &str) -> Result<Vec<RlsPolicy>>;
}

#[derive(Debug, Default, Clone)]
pub struct RlsEngineDraft;

impl RlsEngineDraft {
    pub fn new() -> Self {
        Self
    }
}
