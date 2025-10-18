use std::time::Duration;

use anyhow::Result;
use bkg_db::{ApiKeyRecord, ApiKeyScope as DbScope, Database};
use chrono::{DateTime, Duration as ChronoDuration, Utc};
use rand::{distributions::Alphanumeric, Rng};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use thiserror::Error;
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum KeyScope {
    Admin,
    Namespace { namespace: String },
}

#[derive(Debug, Clone, Serialize)]
pub struct KeyInfo {
    pub id: Uuid,
    pub scope: KeyScope,
    pub rate_limit: u32,
    pub created_at: DateTime<Utc>,
    pub last_used_at: Option<DateTime<Utc>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub expires_at: Option<DateTime<Utc>>,
    pub key_prefix: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct IssuedKey {
    pub token: String,
    pub info: KeyInfo,
}

#[derive(Debug, Error)]
pub enum AuthError {
    #[error("invalid or unknown API key")]
    InvalidToken,
    #[error("permission denied for required scope")]
    Unauthorized,
    #[error("key not found")]
    NotFound,
}

#[derive(Clone)]
pub struct AuthService {
    db: Database,
}

impl AuthService {
    pub fn new(db: Database) -> Self {
        Self { db }
    }

    pub async fn issue_key(
        &self,
        scope: KeyScope,
        rate_limit: u32,
        ttl: Option<Duration>,
    ) -> Result<IssuedKey> {
        let token = generate_token();
        let hash = hash_token(&token);
        let now = Utc::now();
        let expires_at = ttl.map(|dur| now + ChronoDuration::from_std(dur).unwrap_or_default());

        let token_prefix: String = token.chars().take(12).collect();
        let record = self
            .db
            .insert_api_key(
                &hash,
                &token_prefix,
                scope_to_db_scope(&scope),
                rate_limit,
                expires_at,
            )
            .await?;

        let mut info = key_info_from_record(record);
        info.key_prefix = token_prefix;
        info.scope = scope;
        info.created_at = now;

        Ok(IssuedKey { token, info })
    }

    pub async fn authorize<'a>(
        &self,
        token: &str,
        requirement: ScopeRequirement<'a>,
    ) -> Result<KeyInfo, AuthError> {
        let hash = hash_token(token);

        let mut record = self
            .db
            .find_api_key_by_hash(&hash)
            .await
            .map_err(|_| AuthError::InvalidToken)?
            .ok_or(AuthError::InvalidToken)?;

        if record.revoked {
            return Err(AuthError::InvalidToken);
        }

        if let Some(expiry) = record.expires_at {
            if expiry < Utc::now() {
                return Err(AuthError::InvalidToken);
            }
        }

        let info_scope = scope_from_db_scope(record.scope.clone());
        if !requirement.matches(&info_scope) {
            return Err(AuthError::Unauthorized);
        }

        let now = Utc::now();
        self.db
            .touch_api_key_usage(record.id, now)
            .await
            .map_err(|_| AuthError::InvalidToken)?;

        record.last_used_at = Some(now);
        let mut info = key_info_from_record(record);
        info.scope = info_scope;
        info.last_used_at = Some(now);
        Ok(info)
    }

    pub async fn list_keys(&self) -> Result<Vec<KeyInfo>> {
        let records = self.db.list_api_keys().await?;
        Ok(records.into_iter().map(key_info_from_record).collect())
    }

    pub async fn revoke(&self, id: Uuid) -> Result<(), AuthError> {
        if self
            .db
            .fetch_api_key(id)
            .await
            .map_err(|_| AuthError::NotFound)?
            .is_none()
        {
            return Err(AuthError::NotFound);
        }

        self.db
            .revoke_api_key(id)
            .await
            .map_err(|_| AuthError::NotFound)
    }

    pub async fn has_keys(&self) -> Result<bool> {
        Ok(!self.db.list_api_keys().await?.is_empty())
    }
}

#[derive(Debug, Clone, Copy)]
pub enum ScopeRequirement<'a> {
    Admin,
    Namespace(&'a str),
}

impl<'a> ScopeRequirement<'a> {
    fn matches(self, scope: &KeyScope) -> bool {
        match (self, scope) {
            (ScopeRequirement::Admin, KeyScope::Admin) => true,
            (ScopeRequirement::Admin, _) => false,
            (ScopeRequirement::Namespace(_), KeyScope::Admin) => true,
            (ScopeRequirement::Namespace(ns), KeyScope::Namespace { namespace }) => ns == namespace,
        }
    }
}

fn generate_token() -> String {
    rand::thread_rng()
        .sample_iter(&Alphanumeric)
        .take(48)
        .map(char::from)
        .collect()
}

fn hash_token(token: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(token.as_bytes());
    format!("{:x}", hasher.finalize())
}

fn scope_to_db_scope(scope: &KeyScope) -> DbScope {
    match scope {
        KeyScope::Admin => DbScope::Admin,
        KeyScope::Namespace { namespace } => DbScope::Namespace {
            namespace: namespace.clone(),
        },
    }
}

fn scope_from_db_scope(scope: DbScope) -> KeyScope {
    match scope {
        DbScope::Admin => KeyScope::Admin,
        DbScope::Namespace { namespace } => KeyScope::Namespace { namespace },
    }
}

fn key_info_from_record(record: ApiKeyRecord) -> KeyInfo {
    let ApiKeyRecord {
        id,
        token_prefix,
        scope,
        rate_limit,
        created_at,
        last_used_at,
        expires_at,
        revoked: _,
    } = record;

    KeyInfo {
        id,
        scope: scope_from_db_scope(scope),
        rate_limit,
        created_at,
        last_used_at,
        expires_at,
        key_prefix: token_prefix,
    }
}
