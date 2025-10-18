//! Authentication scaffolding for bkg-db.

#![allow(dead_code)]

use std::borrow::Cow;

use anyhow::{anyhow, Result};
use chrono::{DateTime, TimeZone, Utc};
use jsonwebtoken::{Algorithm, DecodingKey, EncodingKey, Header, Validation};
use serde::{Deserialize, Serialize};

/// Representation of JWT claims used by downstream components (RLS, auditing).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
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

/// HMAC-SHA256 based JWT issuer/validator.
#[derive(Clone)]
pub struct JwtHmacAuth {
    encoding: EncodingKey,
    decoding: DecodingKey,
    header: Header,
    validation: Validation,
}

impl JwtHmacAuth {
    pub fn new(secret: impl AsRef<[u8]>) -> Self {
        let header = Header::new(Algorithm::HS256);
        let mut validation = Validation::new(Algorithm::HS256);
        validation.validate_exp = true;
        Self {
            encoding: EncodingKey::from_secret(secret.as_ref()),
            decoding: DecodingKey::from_secret(secret.as_ref()),
            header,
            validation,
        }
    }
}

#[derive(Debug, Serialize, Deserialize)]
struct JwtEnvelope<'a> {
    #[serde(rename = "sub")]
    subject: Cow<'a, str>,
    scope: Cow<'a, str>,
    #[serde(rename = "iat")]
    issued_at: i64,
    #[serde(rename = "exp", skip_serializing_if = "Option::is_none")]
    expires_at: Option<i64>,
}

impl JwtIssuer for JwtHmacAuth {
    fn issue(&self, claims: &TokenClaims) -> Result<String> {
        let envelope = JwtEnvelope {
            subject: Cow::Borrowed(&claims.subject),
            scope: Cow::Borrowed(&claims.scope),
            issued_at: claims.issued_at.timestamp(),
            expires_at: claims.expires_at.map(|ts| ts.timestamp()),
        };
        jsonwebtoken::encode(&self.header, &envelope, &self.encoding)
            .map_err(|err| anyhow!("failed to sign jwt: {err}"))
    }
}

impl JwtValidator for JwtHmacAuth {
    fn verify(&self, token: &str) -> Result<TokenClaims> {
        let token_data =
            jsonwebtoken::decode::<JwtEnvelope>(token, &self.decoding, &self.validation)
                .map_err(|err| anyhow!("invalid jwt: {err}"))?;
        let claims = token_data.claims;
        let issued_at = Utc
            .timestamp_opt(claims.issued_at, 0)
            .single()
            .ok_or_else(|| anyhow!("invalid issued_at timestamp"))?;
        let expires_at = match claims.expires_at {
            Some(ts) => Some(
                Utc.timestamp_opt(ts, 0)
                    .single()
                    .ok_or_else(|| anyhow!("invalid exp timestamp"))?,
            ),
            None => None,
        };

        Ok(TokenClaims {
            subject: claims.subject.into_owned(),
            scope: claims.scope.into_owned(),
            issued_at,
            expires_at,
        })
    }
}

impl std::fmt::Debug for JwtHmacAuth {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("JwtHmacAuth").finish()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn issue_and_verify_roundtrip() {
        let auth = JwtHmacAuth::new("secret-key");
        let claims = TokenClaims {
            subject: "user-123".into(),
            scope: "namespace:alpha".into(),
            issued_at: Utc::now(),
            expires_at: Some(Utc::now() + chrono::Duration::hours(1)),
        };

        let token = auth.issue(&claims).expect("token");
        let verified = auth.verify(&token).expect("verify");
        assert_eq!(verified.subject, claims.subject);
        assert_eq!(verified.scope, claims.scope);
        assert_eq!(verified.expires_at.is_some(), claims.expires_at.is_some());
    }

    #[test]
    fn reject_expired_token() {
        let auth = JwtHmacAuth::new("secret-key");
        let claims = TokenClaims {
            subject: "user-123".into(),
            scope: "namespace:alpha".into(),
            issued_at: Utc::now() - chrono::Duration::hours(2),
            expires_at: Some(Utc::now() - chrono::Duration::hours(1)),
        };

        let token = auth.issue(&claims).expect("token");
        let err = auth.verify(&token).expect_err("should fail");
        assert!(err.to_string().contains("invalid jwt"));
    }
}
