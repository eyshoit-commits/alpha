//! Row-Level Security engine for bkg-db.

#![allow(dead_code)]

use std::collections::HashMap;

use anyhow::{anyhow, Result};
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::auth::TokenClaims;

/// Representation of a stored RLS policy.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RlsPolicy {
    pub name: String,
    pub table: String,
    pub expression: Value,
}

/// Policy engine contract applied before query execution.
pub trait RlsPolicyEngine {
    fn evaluate(&self, policy: &RlsPolicy, claims: &TokenClaims, row: &Value) -> Result<bool>;
    fn policies_for_table(&self, table: &str) -> Result<Vec<RlsPolicy>>;
}

/// In-memory policy engine used during development/tests.
#[derive(Debug, Default, Clone)]
pub struct InMemoryPolicyEngine {
    policies: HashMap<String, Vec<RlsPolicy>>,
}

impl InMemoryPolicyEngine {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn add_policy(&mut self, policy: RlsPolicy) {
        self.policies
            .entry(policy.table.clone())
            .or_default()
            .push(policy);
    }
}

impl RlsPolicyEngine for InMemoryPolicyEngine {
    fn evaluate(&self, policy: &RlsPolicy, claims: &TokenClaims, row: &Value) -> Result<bool> {
        evaluate_expression(&policy.expression, claims, row)
    }

    fn policies_for_table(&self, table: &str) -> Result<Vec<RlsPolicy>> {
        Ok(self.policies.get(table).cloned().unwrap_or_default())
    }
}

fn evaluate_expression(expr: &Value, claims: &TokenClaims, row: &Value) -> Result<bool> {
    let obj = expr
        .as_object()
        .ok_or_else(|| anyhow!("policy expression must be an object"))?;

    if let Some(eq) = obj.get("eq") {
        let rule = eq
            .as_object()
            .ok_or_else(|| anyhow!("eq expression must be object"))?;
        let column = rule
            .get("column")
            .and_then(Value::as_str)
            .ok_or_else(|| anyhow!("eq.column must be string"))?;
        let claim = rule
            .get("claim")
            .and_then(Value::as_str)
            .ok_or_else(|| anyhow!("eq.claim must be string"))?;

        let row_value = row
            .get(column)
            .ok_or_else(|| anyhow!("column '{column}' missing from row"))?;
        let claim_value = match claim {
            "subject" => Value::String(claims.subject.clone()),
            "scope" => Value::String(claims.scope.clone()),
            other => return Err(anyhow!("unsupported claim '{other}'")),
        };
        return Ok(row_value == &claim_value);
    }

    if let Some(and) = obj.get("and") {
        let arr = and.as_array().ok_or_else(|| anyhow!("and must be array"))?;
        for sub in arr {
            if !evaluate_expression(sub, claims, row)? {
                return Ok(false);
            }
        }
        return Ok(true);
    }

    if let Some(or) = obj.get("or") {
        let arr = or.as_array().ok_or_else(|| anyhow!("or must be array"))?;
        for sub in arr {
            if evaluate_expression(sub, claims, row)? {
                return Ok(true);
            }
        }
        return Ok(false);
    }

    Err(anyhow!("unsupported RLS expression: {expr}"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn claims(sub: &str, scope: &str) -> TokenClaims {
        TokenClaims {
            subject: sub.into(),
            scope: scope.into(),
            issued_at: chrono::Utc::now(),
            expires_at: None,
        }
    }

    #[test]
    fn owner_policy() {
        let policy = RlsPolicy {
            name: "owner-only".into(),
            table: "projects".into(),
            expression: json!({
                "eq": {
                    "column": "owner",
                    "claim": "subject"
                }
            }),
        };

        let engine = InMemoryPolicyEngine::new();
        let row = json!({"owner": "user-1", "name": "Alpha"});
        assert!(engine
            .evaluate(&policy, &claims("user-1", "ns"), &row)
            .unwrap());
        assert!(!engine
            .evaluate(&policy, &claims("user-2", "ns"), &row)
            .unwrap());
    }

    #[test]
    fn owner_or_admin_policy() {
        let policy = RlsPolicy {
            name: "owner-or-admin".into(),
            table: "tasks".into(),
            expression: json!({
                "or": [
                    { "eq": { "column": "owner", "claim": "subject" } },
                    { "eq": { "column": "scope", "claim": "scope" } }
                ]
            }),
        };

        let engine = InMemoryPolicyEngine::new();
        let row = json!({"owner": "user-1", "scope": "namespace:admin"});

        assert!(engine
            .evaluate(&policy, &claims("user-1", "namespace:member"), &row)
            .unwrap());
        assert!(engine
            .evaluate(&policy, &claims("user-2", "namespace:admin"), &row)
            .unwrap());
        assert!(!engine
            .evaluate(&policy, &claims("user-2", "namespace:member"), &row)
            .unwrap());
    }
}
