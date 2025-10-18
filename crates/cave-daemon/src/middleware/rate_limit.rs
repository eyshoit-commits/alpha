use std::{
    collections::HashMap,
    convert::Infallible,
    hash::{Hash, Hasher},
    sync::Arc,
    time::{Duration, Instant},
};

use axum::{
    http::{header, Request, StatusCode},
    response::{IntoResponse, Response},
};
use base64::{engine::general_purpose::STANDARD_NO_PAD, Engine as _};
use futures::future::BoxFuture;
use http::HeaderValue;
use serde::Serialize;
use sha2::{Digest, Sha256};
use tokio::sync::Mutex;
use tower::{Layer, Service};
use tracing::warn;

#[derive(Clone, Debug)]
pub struct RateLimitConfig {
    pub admin_per_minute: u64,
    pub namespace_per_minute: u64,
    pub session_per_minute: u64,
    pub window: Duration,
}

impl Default for RateLimitConfig {
    fn default() -> Self {
        Self {
            admin_per_minute: 1_000,
            namespace_per_minute: 100,
            session_per_minute: 50,
            window: Duration::from_secs(60),
        }
    }
}

#[derive(Clone)]
pub struct RateLimitLayer {
    state: Arc<RateLimitState>,
}

pub fn rate_limit_layer(config: RateLimitConfig) -> RateLimitLayer {
    RateLimitLayer {
        state: Arc::new(RateLimitState::new(config)),
    }
}

impl<S> Layer<S> for RateLimitLayer {
    type Service = RateLimitService<S>;

    fn layer(&self, inner: S) -> Self::Service {
        RateLimitService {
            inner,
            state: self.state.clone(),
        }
    }
}

#[derive(Clone)]
pub struct RateLimitService<S> {
    inner: S,
    state: Arc<RateLimitState>,
}

impl<S, ReqBody> Service<Request<ReqBody>> for RateLimitService<S>
where
    S: Service<Request<ReqBody>, Response = Response, Error = Infallible> + Clone + Send + 'static,
    S::Future: Send + 'static,
    ReqBody: Send + 'static,
{
    type Response = Response;
    type Error = Infallible;
    type Future = BoxFuture<'static, Result<Self::Response, Self::Error>>;

    fn poll_ready(
        &mut self,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<Result<(), Self::Error>> {
        self.inner.poll_ready(cx)
    }

    fn call(&mut self, request: Request<ReqBody>) -> Self::Future {
        let mut inner = self.inner.clone();
        let state = self.state.clone();

        Box::pin(async move {
            match state.check(&request).await {
                Ok(()) => inner.call(request).await,
                Err(rejection) => Ok(rejection.into_response()),
            }
        })
    }
}

#[derive(Clone, Debug)]
struct RateLimitState {
    config: RateLimitConfig,
    counters: Mutex<HashMap<RateKey, Counter>>,
}

impl RateLimitState {
    fn new(config: RateLimitConfig) -> Self {
        Self {
            config,
            counters: Mutex::new(HashMap::new()),
        }
    }

    async fn check<B>(&self, request: &Request<B>) -> Result<(), RateLimitRejection> {
        let path = request.uri().path();
        let Some(class) = classify(path) else {
            return Ok(());
        };

        let limit = self.config.limit_for(class);
        if limit == 0 {
            return Err(RateLimitRejection::new(
                class,
                0,
                self.config.window,
                self.config.window,
            ));
        }

        let fingerprint = identity_fingerprint(request);
        let mut counters = self.counters.lock().await;
        let now = Instant::now();
        let window = self.config.window;
        let entry = counters
            .entry(RateKey {
                class,
                identity: fingerprint.clone(),
            })
            .or_insert_with(|| Counter {
                window_start: now,
                count: 0,
            });

        let elapsed = now.saturating_duration_since(entry.window_start);
        if elapsed >= window {
            entry.window_start = now;
            entry.count = 0;
        }

        if entry.count >= limit {
            let retry_after = window
                .checked_sub(elapsed)
                .unwrap_or_default()
                .max(Duration::from_secs(1));
            warn!(
                category = class.as_str(),
                identity_fingerprint = fingerprint,
                limit,
                "rate limit exceeded"
            );
            return Err(RateLimitRejection::new(class, limit, window, retry_after));
        }

        entry.count += 1;
        Ok(())
    }
}

#[derive(Clone, Debug, Eq)]
struct RateKey {
    class: RateClass,
    identity: String,
}

impl PartialEq for RateKey {
    fn eq(&self, other: &Self) -> bool {
        self.class == other.class && self.identity == other.identity
    }
}

impl Hash for RateKey {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.class.hash(state);
        self.identity.hash(state);
    }
}

#[derive(Clone, Debug)]
struct Counter {
    window_start: Instant,
    count: u64,
}

#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash)]
enum RateClass {
    Admin,
    Namespace,
    Session,
}

impl RateClass {
    fn as_str(self) -> &'static str {
        match self {
            RateClass::Admin => "admin",
            RateClass::Namespace => "namespace",
            RateClass::Session => "session",
        }
    }
}

impl RateLimitConfig {
    fn limit_for(&self, class: RateClass) -> u64 {
        match class {
            RateClass::Admin => self.admin_per_minute,
            RateClass::Namespace => self.namespace_per_minute,
            RateClass::Session => self.session_per_minute,
        }
    }
}

#[derive(Debug)]
struct RateLimitRejection {
    class: RateClass,
    limit: u64,
    window: Duration,
    retry_after: Duration,
}

impl RateLimitRejection {
    fn new(class: RateClass, limit: u64, window: Duration, retry_after: Duration) -> Self {
        Self {
            class,
            limit,
            window,
            retry_after,
        }
    }
}

impl IntoResponse for RateLimitRejection {
    fn into_response(self) -> Response {
        let retry_after_secs = self.retry_after.as_secs().max(1);
        let body = RateLimitBody {
            error: "rate_limit_exceeded",
            category: self.class.as_str(),
            limit: self.limit,
            window_seconds: self.window.as_secs(),
            retry_after_seconds: retry_after_secs,
        };
        let mut response = (StatusCode::TOO_MANY_REQUESTS, axum::Json(body)).into_response();
        response.headers_mut().insert(
            header::RETRY_AFTER,
            HeaderValue::from_str(&retry_after_secs.to_string())
                .unwrap_or_else(|_| HeaderValue::from_static("1")),
        );
        response
    }
}

#[derive(Serialize)]
struct RateLimitBody {
    error: &'static str,
    category: &'static str,
    limit: u64,
    window_seconds: u64,
    retry_after_seconds: u64,
}

fn classify(path: &str) -> Option<RateClass> {
    if path.starts_with("/api/v1/auth/") {
        Some(RateClass::Admin)
    } else if path.starts_with("/api/v1/sandboxes") {
        Some(RateClass::Namespace)
    } else if path.starts_with("/api/v1/sessions") {
        Some(RateClass::Session)
    } else {
        None
    }
}

fn identity_fingerprint<B>(request: &Request<B>) -> String {
    let Some(value) = request
        .headers()
        .get(header::AUTHORIZATION)
        .and_then(|value| value.to_str().ok())
        .map(str::trim)
        .filter(|value| !value.is_empty())
    else {
        return "anonymous".to_string();
    };

    let digest = Sha256::digest(value.as_bytes());
    STANDARD_NO_PAD.encode(digest)
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::http::Request;

    #[test]
    fn classify_routes() {
        assert_eq!(classify("/api/v1/auth/keys"), Some(RateClass::Admin));
        assert_eq!(classify("/api/v1/sandboxes"), Some(RateClass::Namespace));
        assert_eq!(classify("/api/v1/sessions/123"), Some(RateClass::Session));
        assert_eq!(classify("/healthz"), None);
    }

    #[tokio::test]
    async fn enforces_limits_per_identity() {
        let config = RateLimitConfig {
            admin_per_minute: 2,
            namespace_per_minute: 5,
            session_per_minute: 1,
            window: Duration::from_secs(60),
        };
        let state = RateLimitState::new(config);
        let request = Request::builder()
            .uri("/api/v1/auth/keys")
            .header(header::AUTHORIZATION, "Bearer admin-token")
            .body(())
            .unwrap();

        state.check(&request).await.unwrap();
        state.check(&request).await.unwrap();
        let err = state.check(&request).await.unwrap_err();
        assert_eq!(err.class, RateClass::Admin);
    }
}
