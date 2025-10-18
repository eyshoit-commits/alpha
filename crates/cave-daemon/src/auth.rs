use std::{collections::HashMap, sync::Arc, time::Duration};

use anyhow::Result;
use chrono::{DateTime, Duration as ChronoDuration, Utc};
use rand::{distributions::Alphanumeric, Rng};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use thiserror::Error;
use tokio::sync::RwLock;
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

#[derive(Clone, Default)]
pub struct AuthService {
    state: Arc<RwLock<AuthState>>,
}

#[derive(Default)]
struct AuthState {
    keys: HashMap<Uuid, KeyRecord>,
    index: HashMap<String, Uuid>,
}

struct KeyRecord {
    info: KeyInfo,
    hash: String,
}

impl AuthService {
    pub fn new() -> Self {
        Self::default()
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

        let info = KeyInfo {
            id: Uuid::new_v4(),
            scope,
            rate_limit,
            created_at: now,
            last_used_at: None,
            expires_at,
            key_prefix: token[..12.min(token.len())].to_string(),
        };

        let mut guard = self.state.write().await;
        guard.index.insert(hash.clone(), info.id);
        guard.keys.insert(
            info.id,
            KeyRecord {
                info: info.clone(),
                hash,
            },
        );

        Ok(IssuedKey { token, info })
    }

    pub async fn authorize<'a>(
        &self,
        token: &str,
        requirement: ScopeRequirement<'a>,
    ) -> Result<KeyInfo, AuthError> {
        let mut guard = self.state.write().await;
        let hash = hash_token(token);

        let key_id = guard
            .index
            .get(&hash)
            .cloned()
            .ok_or(AuthError::InvalidToken)?;
        let record = guard.keys.get_mut(&key_id).ok_or(AuthError::InvalidToken)?;

        if let Some(expiry) = record.info.expires_at {
            if expiry < Utc::now() {
                guard.index.remove(&hash);
                guard.keys.remove(&key_id);
                return Err(AuthError::InvalidToken);
            }
        }

        if !requirement.matches(&record.info.scope) {
            return Err(AuthError::Unauthorized);
        }

        record.info.last_used_at = Some(Utc::now());
        Ok(record.info.clone())
    }

    pub async fn list_keys(&self) -> Vec<KeyInfo> {
        let guard = self.state.read().await;
        guard.keys.values().map(|rec| rec.info.clone()).collect()
    }

    pub async fn revoke(&self, id: Uuid) -> Result<(), AuthError> {
        let mut guard = self.state.write().await;
        let record = guard.keys.remove(&id).ok_or(AuthError::NotFound)?;
        guard.index.remove(&record.hash);
        Ok(())
    }

    pub async fn has_keys(&self) -> bool {
        let guard = self.state.read().await;
        !guard.keys.is_empty()
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
