//! Authentication scaffolding for bkg-db.

#![allow(dead_code)]

use anyhow::Result;
use chrono::{DateTime, Utc};

// TODO(bkg-db/auth): Implementiere JWT-Issuer, Key Rotation und Scope-Prüfung.

/// Placeholder representation of token claims used by RLS/Executor.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TokenClaims {
    pub subject: String,
    pub scope: String,
    pub issued_at: DateTime<Utc>,
    pub expires_at: Option<DateTime<Utc>>,
}

/// Trait for verifying JWT tokens and returning claims.
pub trait JwtValidator {
    fn verify(&self, token: &str) -> Result<TokenClaims>;
}

/// Trait for issuing signed tokens.
pub trait JwtIssuer {
    fn issue(&self, claims: &TokenClaims) -> Result<String>;
}

#[derive(Debug, Default, Clone)]
pub struct AuthBlueprint;

impl AuthBlueprint {
    pub fn new() -> Self {
        Self
    }
}
