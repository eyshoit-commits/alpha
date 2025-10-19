//! Realtime/CDC scaffolding for bkg-db.

#![allow(dead_code)]

use std::collections::HashMap;
use std::sync::Arc;

use anyhow::{anyhow, Result};
use parking_lot::RwLock;
use serde_json::Value;
use tokio::sync::broadcast::{self, Receiver, Sender};

/// Change notification derived from WAL entries.
#[derive(Debug, Clone)]
pub struct ChangeEvent {
    pub channel: String,
    pub payload: Value,
}

/// Pub/Sub contract for realtime subscribers.
pub trait RealtimeHub: Send + Sync {
    fn publish(&self, event: ChangeEvent) -> Result<()>;
    fn subscribe(&self, channel: &str) -> Result<RealtimeSubscription>;
}

/// Tokio broadcast-backed realtime hub that fans out WAL changes.
#[derive(Debug)]
pub struct WalRealtimeHub {
    capacity: usize,
    channels: RwLock<HashMap<String, Sender<ChangeEvent>>>,
}

impl WalRealtimeHub {
    /// Creates a new realtime hub.
    pub fn new(capacity: usize) -> Self {
        Self {
            capacity,
            channels: RwLock::new(HashMap::new()),
        }
    }

    fn ensure_channel(&self, channel: &str) -> Sender<ChangeEvent> {
        let mut channels = self.channels.write();
        channels
            .entry(channel.to_string())
            .or_insert_with(|| {
                let (sender, _receiver) = broadcast::channel(self.capacity);
                sender
            })
            .clone()
    }

    fn cleanup_channel(&self, channel: &str, sender: &Sender<ChangeEvent>) {
        if sender.receiver_count() == 0 {
            let mut channels = self.channels.write();
            if let Some(existing) = channels.get(channel) {
                if existing.receiver_count() == 0 {
                    channels.remove(channel);
                }
            }
        }
    }

    #[cfg(test)]
    fn receiver_count(&self, channel: &str) -> usize {
        self.channels
            .read()
            .get(channel)
            .map(|sender| sender.receiver_count())
            .unwrap_or(0)
    }
}

impl Default for WalRealtimeHub {
    fn default() -> Self {
        Self::new(512)
    }
}

impl RealtimeHub for WalRealtimeHub {
    fn publish(&self, event: ChangeEvent) -> Result<()> {
        let sender = self.ensure_channel(&event.channel);
        if let Err(error) = sender.send(event.clone()) {
            match error {
                broadcast::error::SendError(_) => {
                    self.cleanup_channel(&event.channel, &sender);
                }
            }
        } else {
            self.cleanup_channel(&event.channel, &sender);
        }
        Ok(())
    }

    fn subscribe(&self, channel: &str) -> Result<RealtimeSubscription> {
        let sender = self.ensure_channel(channel);
        let receiver = sender.subscribe();
        Ok(RealtimeSubscription {
            channel: channel.to_string(),
            receiver,
        })
    }
}

/// Handle returned to subscribers.
#[derive(Debug)]
pub struct RealtimeSubscription {
    channel: String,
    receiver: Receiver<ChangeEvent>,
}

impl RealtimeSubscription {
    /// Attempts to receive the next change notification.
    pub async fn recv(&mut self) -> Result<Option<ChangeEvent>> {
        match self.receiver.recv().await {
            Ok(event) => Ok(Some(event)),
            Err(broadcast::error::RecvError::Closed) => Ok(None),
            Err(broadcast::error::RecvError::Lagged(skipped)) => {
                Err(anyhow!("subscriber lagged behind by {skipped} messages"))
            }
        }
    }

    /// Non-blocking variant of [`recv`].
    pub fn try_recv(&mut self) -> Result<Option<ChangeEvent>> {
        match self.receiver.try_recv() {
            Ok(event) => Ok(Some(event)),
            Err(broadcast::error::TryRecvError::Closed) => Ok(None),
            Err(broadcast::error::TryRecvError::Empty) => Ok(None),
            Err(broadcast::error::TryRecvError::Lagged(skipped)) => {
                Err(anyhow!("subscriber lagged behind by {skipped} messages"))
            }
        }
    }

    /// Cancels the subscription and releases channel resources.
    pub fn cancel(mut self) -> Result<()> {
        self.receiver.close();
        Ok(())
    }

    /// Returns the logical channel name.
    pub fn channel(&self) -> &str {
        &self.channel
    }
}

/// Shared handle used by other modules.
pub type SharedRealtimeHub = Arc<dyn RealtimeHub>;

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use serde_json::json;
    use tokio::time::timeout;

    use super::*;
    use crate::auth::TokenClaims;
    use crate::executor::{DefaultQueryExecutor, ExecutionContext};
    use crate::kernel::InMemoryStorageEngine;
    use crate::planner::PlannerDraft;
    use crate::rls::{InMemoryPolicyEngine, RlsPolicy};
    use crate::sql::{DefaultSqlParser, SqlParser};

    #[tokio::test]
    async fn wal_events_are_broadcast() {
        let storage = InMemoryStorageEngine::new();
        let hub = Arc::new(WalRealtimeHub::new(128));
        let ctx =
            ExecutionContext::with_realtime(storage.clone(), hub.clone() as SharedRealtimeHub);
        let parser = DefaultSqlParser::new();
        let planner = PlannerDraft::new();
        let executor = DefaultQueryExecutor::new();
        let engine = InMemoryPolicyEngine::new();
        let policies: Vec<RlsPolicy> = Vec::new();
        let mut subscription = hub
            .subscribe("projects")
            .expect("subscribe to realtime hub");

        let claims = TokenClaims {
            subject: "user-1".into(),
            scope: "namespace:alpha".into(),
            issued_at: chrono::Utc::now(),
            expires_at: None,
        };

        let insert_ast = parser
            .parse("INSERT INTO projects (id, name) VALUES (1, 'alpha')")
            .expect("parse insert");
        let insert_plan = planner
            .optimize(planner.build_logical_plan(&insert_ast).unwrap())
            .unwrap();
        executor
            .execute(&ctx, &insert_plan, &claims, &policies, &engine)
            .expect("execute insert");

        let insert_event = timeout(Duration::from_millis(250), subscription.recv())
            .await
            .expect("event within timeout")
            .expect("event result")
            .expect("event payload");
        assert_eq!(insert_event.channel, "projects");
        assert_eq!(insert_event.payload["kind"], json!("insert"));

        let update_ast = parser
            .parse("UPDATE projects SET name = 'beta' WHERE id = 1")
            .expect("parse update");
        let update_plan = planner
            .optimize(planner.build_logical_plan(&update_ast).unwrap())
            .unwrap();
        executor
            .execute(&ctx, &update_plan, &claims, &policies, &engine)
            .expect("execute update");
        let update_event = timeout(Duration::from_millis(250), subscription.recv())
            .await
            .expect("update event within timeout")
            .expect("update event result")
            .expect("update event payload");
        assert_eq!(update_event.payload["kind"], json!("update"));

        let delete_ast = parser
            .parse("DELETE FROM projects WHERE id = 1")
            .expect("parse delete");
        let delete_plan = planner
            .optimize(planner.build_logical_plan(&delete_ast).unwrap())
            .unwrap();
        executor
            .execute(&ctx, &delete_plan, &claims, &policies, &engine)
            .expect("execute delete");
        let delete_event = timeout(Duration::from_millis(250), subscription.recv())
            .await
            .expect("delete event within timeout")
            .expect("delete event result")
            .expect("delete event payload");
        assert_eq!(delete_event.payload["kind"], json!("delete"));

        for idx in 0..20 {
            let insert_ast = parser
                .parse(&format!(
                    "INSERT INTO projects (id, name) VALUES ({}, 'batch')",
                    idx + 2
                ))
                .expect("parse batch insert");
            let insert_plan = planner
                .optimize(planner.build_logical_plan(&insert_ast).unwrap())
                .unwrap();
            executor
                .execute(&ctx, &insert_plan, &claims, &policies, &engine)
                .expect("execute batch insert");
        }

        for _ in 0..20 {
            let batch_event = timeout(Duration::from_millis(250), subscription.recv())
                .await
                .expect("batched event in timeout")
                .expect("batched event result")
                .expect("batched event payload");
            assert_eq!(batch_event.payload["kind"], json!("insert"));
        }

        subscription.cancel().expect("cancel subscription");
    }

    #[tokio::test]
    async fn unsubscribe_removes_receivers() {
        let storage = InMemoryStorageEngine::new();
        let hub = Arc::new(WalRealtimeHub::new(4));
        let ctx =
            ExecutionContext::with_realtime(storage.clone(), hub.clone() as SharedRealtimeHub);
        let parser = DefaultSqlParser::new();
        let planner = PlannerDraft::new();
        let executor = DefaultQueryExecutor::new();
        let engine = InMemoryPolicyEngine::new();
        let policies: Vec<RlsPolicy> = Vec::new();
        let claims = TokenClaims {
            subject: "user-1".into(),
            scope: "namespace:alpha".into(),
            issued_at: chrono::Utc::now(),
            expires_at: None,
        };

        {
            let subscription = hub.subscribe("projects").expect("subscribe to hub");
            assert_eq!(hub.receiver_count("projects"), 1);
            subscription.cancel().expect("cancel subscription");
        }

        assert_eq!(hub.receiver_count("projects"), 0);

        let insert_ast = parser
            .parse("INSERT INTO projects (id, name) VALUES (1, 'alpha')")
            .expect("parse insert");
        let insert_plan = planner
            .optimize(planner.build_logical_plan(&insert_ast).unwrap())
            .unwrap();
        executor
            .execute(&ctx, &insert_plan, &claims, &policies, &engine)
            .expect("execute insert");
    }
}
