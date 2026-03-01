//! Subscription lifecycle hooks.
//!
//! The [`SubscriptionLifecycle`] trait provides callbacks invoked at key points
//! in the WebSocket subscription lifecycle. Implementations can perform
//! authentication, rate limiting, audit logging, or custom authorisation.

use async_trait::async_trait;

/// Callbacks for subscription lifecycle events.
///
/// All methods have default no-op implementations, so you only need to
/// override the hooks you care about.
///
/// # Fail-closed vs fire-and-forget
///
/// - `on_connect` / `on_subscribe` are **fail-closed**: returning `Err(reason)`
///   rejects the connection or subscription.
/// - `on_disconnect` / `on_unsubscribe` are **fire-and-forget**: the connection
///   is already closing and there is nothing to reject.
#[async_trait]
pub trait SubscriptionLifecycle: Send + Sync + 'static {
    /// Called after `connection_init` is received, before `connection_ack`.
    ///
    /// Return `Err(reason)` to reject the connection with close code 4400.
    async fn on_connect(
        &self,
        _params: &serde_json::Value,
        _connection_id: &str,
    ) -> Result<(), String> {
        Ok(())
    }

    /// Called when the WebSocket connection closes (for any reason).
    async fn on_disconnect(&self, _connection_id: &str) {}

    /// Called before a subscription is registered with the manager.
    ///
    /// Return `Err(reason)` to reject the subscription (the connection stays open).
    async fn on_subscribe(
        &self,
        _subscription_name: &str,
        _variables: &serde_json::Value,
        _connection_id: &str,
    ) -> Result<(), String> {
        Ok(())
    }

    /// Called when a client sends `complete` for a subscription.
    async fn on_unsubscribe(&self, _subscription_id: &str, _connection_id: &str) {}
}

/// No-op lifecycle that accepts everything.
pub struct NoopLifecycle;

#[async_trait]
impl SubscriptionLifecycle for NoopLifecycle {}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn noop_lifecycle_accepts_connect() {
        let lifecycle = NoopLifecycle;
        let result = lifecycle
            .on_connect(&serde_json::json!({}), "conn-1")
            .await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn noop_lifecycle_accepts_subscribe() {
        let lifecycle = NoopLifecycle;
        let result = lifecycle
            .on_subscribe("orderCreated", &serde_json::json!({}), "conn-1")
            .await;
        assert!(result.is_ok());
    }
}
