use std::time::Duration;

use anyhow::{anyhow, Result};
use base64::{engine::general_purpose::STANDARD, Engine as _};
use bkg_db::{ApiKeyRecord, ApiKeyScope as DbScope, Database};
use chrono::{DateTime, Duration as ChronoDuration, Utc};
use hmac::{Hmac, Mac};
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
    #[serde(skip_serializing_if = "Option::is_none")]
    pub rotated_from: Option<Uuid>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub rotated_at: Option<DateTime<Utc>>,
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
    #[error("rotation webhook secret is not configured")]
    WebhookNotConfigured,
    #[error("rotation webhook signature mismatch")]
    InvalidSignature,
    #[error("internal auth error: {0}")]
    Internal(String),
}

#[derive(Clone)]
pub struct AuthService {
    db: Database,
    rotation_secret: Option<Vec<u8>>,
}

impl AuthService {
    pub fn new(db: Database, rotation_secret: Option<Vec<u8>>) -> Self {
        Self {
            db,
            rotation_secret,
        }
    }

    pub async fn issue_key(
        &self,
        scope: KeyScope,
        rate_limit: u32,
        ttl: Option<Duration>,
    ) -> Result<IssuedKey> {
        self.issue_key_inner(scope, rate_limit, ttl, None, None)
            .await
            .map_err(|err| anyhow!(err))
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

    pub async fn rotate_key(
        &self,
        previous_id: Uuid,
        rate_limit: Option<u32>,
        ttl: Option<Duration>,
    ) -> Result<RotationOutcome, AuthError> {
        let mut previous = self
            .db
            .fetch_api_key(previous_id)
            .await
            .map_err(|err| AuthError::Internal(err.to_string()))?
            .ok_or(AuthError::NotFound)?;

        if previous.revoked {
            return Err(AuthError::NotFound);
        }

        let scope = scope_from_db_scope(previous.scope.clone());
        let now = Utc::now();
        let ttl = match ttl {
            Some(value) => Some(value),
            None => match previous.expires_at {
                Some(expiry) if expiry > now => {
                    let remaining = expiry - now;
                    remaining.to_std().ok()
                }
                _ => None,
            },
        };
        let rate_limit = rate_limit.unwrap_or(previous.rate_limit);

        let issued = self
            .issue_key_inner(scope.clone(), rate_limit, ttl, Some(previous_id), Some(now))
            .await?;

        self.db
            .revoke_api_key(previous_id)
            .await
            .map_err(|err| AuthError::Internal(err.to_string()))?;

        previous.revoked = true;
        let mut previous_info = key_info_from_record(previous.clone());
        previous_info.scope = scope.clone();

        let payload = RotationWebhookPayload {
            event: "key.rotated".to_string(),
            key_id: issued.info.id,
            previous_key_id: previous_id,
            rotated_at: issued.info.rotated_at.unwrap_or(issued.info.created_at),
            scope: scope.clone(),
            owner: scope_owner(&scope),
            key_prefix: issued.info.key_prefix.clone(),
        };

        let signature = self.sign_rotation_payload(&payload)?;
        let payload_json =
            serde_json::to_string(&payload).map_err(|err| AuthError::Internal(err.to_string()))?;

        let event = self
            .db
            .insert_key_rotation_event(
                issued.info.id,
                previous_id,
                payload.rotated_at,
                &payload_json,
                &signature,
            )
            .await
            .map_err(|err| AuthError::Internal(err.to_string()))?;

        Ok(RotationOutcome {
            new_key: issued,
            previous: previous_info,
            webhook: RotationWebhook {
                event_id: event.id,
                payload,
                signature,
            },
        })
    }

    pub fn verify_rotation_signature(
        &self,
        payload: &RotationWebhookPayload,
        signature: &str,
    ) -> Result<(), AuthError> {
        let expected = self.sign_rotation_payload(payload)?;
        if expected == signature {
            Ok(())
        } else {
            Err(AuthError::InvalidSignature)
        }
    }

    async fn issue_key_inner(
        &self,
        scope: KeyScope,
        rate_limit: u32,
        ttl: Option<Duration>,
        rotated_from: Option<Uuid>,
        rotated_at: Option<DateTime<Utc>>,
    ) -> Result<IssuedKey, AuthError> {
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
                rotated_from,
                rotated_at,
            )
            .await
            .map_err(|err| AuthError::Internal(err.to_string()))?;

        let mut info = key_info_from_record(record);
        info.key_prefix = token_prefix.clone();
        info.scope = scope.clone();
        info.created_at = now;

        Ok(IssuedKey { token, info })
    }

    fn sign_rotation_payload(&self, payload: &RotationWebhookPayload) -> Result<String, AuthError> {
        let secret = self
            .rotation_secret
            .as_ref()
            .ok_or(AuthError::WebhookNotConfigured)?;
        let mut mac = Hmac::<Sha256>::new_from_slice(secret)
            .map_err(|err| AuthError::Internal(err.to_string()))?;
        let json =
            serde_json::to_vec(payload).map_err(|err| AuthError::Internal(err.to_string()))?;
        mac.update(&json);
        let signature = mac.finalize().into_bytes();
        Ok(STANDARD.encode(signature))
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RotationWebhookPayload {
    pub event: String,
    pub key_id: Uuid,
    pub previous_key_id: Uuid,
    pub rotated_at: DateTime<Utc>,
    pub scope: KeyScope,
    pub owner: String,
    pub key_prefix: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct RotationWebhook {
    pub event_id: Uuid,
    pub payload: RotationWebhookPayload,
    pub signature: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct RotationOutcome {
    pub new_key: IssuedKey,
    pub previous: KeyInfo,
    pub webhook: RotationWebhook,
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
        rotated_from,
        rotated_at,
    } = record;

    KeyInfo {
        id,
        scope: scope_from_db_scope(scope),
        rate_limit,
        created_at,
        last_used_at,
        expires_at,
        key_prefix: token_prefix,
        rotated_from,
        rotated_at,
    }
}

fn scope_owner(scope: &KeyScope) -> String {
    match scope {
        KeyScope::Admin => "admin".to_string(),
        KeyScope::Namespace { namespace } => namespace.clone(),
    }
}
