//! Webhook-based subscription lifecycle hooks.
//!
//! Sends HTTP POST requests to configured URLs on subscription lifecycle events.
//! `on_connect` and `on_subscribe` are fail-closed (timeout → reject).
//! `on_disconnect` and `on_unsubscribe` are fire-and-forget.

use std::time::Duration;

use async_trait::async_trait;
use tracing::{error, warn};

use super::lifecycle::SubscriptionLifecycle;

/// Subscription lifecycle hooks that call external HTTP endpoints.
pub struct WebhookLifecycle {
    client:             reqwest::Client,
    on_connect_url:     Option<String>,
    on_disconnect_url:  Option<String>,
    on_subscribe_url:   Option<String>,
    on_unsubscribe_url: Option<String>,
    #[allow(dead_code)] // Reason: kept for future use in fail-closed unsubscribe logic.
    timeout:            Duration,
}

impl WebhookLifecycle {
    /// Create a new webhook lifecycle from configured URLs.
    ///
    /// `timeout_ms` controls the maximum time to wait for `on_connect` and
    /// `on_subscribe` responses. `on_disconnect` and `on_unsubscribe` are
    /// fire-and-forget (timeout is irrelevant for those hooks).
    #[must_use]
    pub fn new(
        on_connect_url:     Option<String>,
        on_disconnect_url:  Option<String>,
        on_subscribe_url:   Option<String>,
        on_unsubscribe_url: Option<String>,
        timeout_ms:         u64,
    ) -> Self {
        let timeout = Duration::from_millis(timeout_ms);
        let client = reqwest::Client::builder()
            .timeout(timeout)
            .build()
            .unwrap_or_else(|e| {
                warn!(
                    error = %e,
                    "Failed to build reqwest client with timeout; using default client. \
                     Webhook lifecycle hooks may not respect the configured timeout."
                );
                reqwest::Client::default()
            });
        Self {
            client,
            on_connect_url,
            on_disconnect_url,
            on_subscribe_url,
            on_unsubscribe_url,
            timeout,
        }
    }

    /// Build from typed subscriptions configuration.
    ///
    /// Returns `None` if no hooks are configured.
    #[must_use]
    pub fn from_config(
        config: &fraiseql_core::schema::SubscriptionsConfig,
    ) -> Option<Self> {
        let hooks = config.hooks.as_ref()?;
        if hooks.on_connect.is_none()
            && hooks.on_disconnect.is_none()
            && hooks.on_subscribe.is_none()
            && hooks.on_unsubscribe.is_none()
        {
            return None;
        }
        Some(Self::new(
            hooks.on_connect.clone(),
            hooks.on_disconnect.clone(),
            hooks.on_subscribe.clone(),
            hooks.on_unsubscribe.clone(),
            hooks.timeout_ms,
        ))
    }

    /// Build from compiled schema JSON (`subscriptions.hooks` section).
    ///
    /// Returns `None` if no hooks are configured.
    #[must_use]
    pub fn from_schema_json(subscriptions: &serde_json::Value) -> Option<Self> {
        let hooks = subscriptions.get("hooks")?;
        let on_connect = hooks.get("on_connect").and_then(|v| v.as_str()).map(String::from);
        let on_disconnect = hooks.get("on_disconnect").and_then(|v| v.as_str()).map(String::from);
        let on_subscribe = hooks.get("on_subscribe").and_then(|v| v.as_str()).map(String::from);
        let on_unsubscribe = hooks.get("on_unsubscribe").and_then(|v| v.as_str()).map(String::from);

        // If no hooks are configured, return None.
        if on_connect.is_none() && on_disconnect.is_none() && on_subscribe.is_none() && on_unsubscribe.is_none() {
            return None;
        }

        let timeout_ms = hooks
            .get("timeout_ms")
            .and_then(|v| v.as_u64())
            .unwrap_or(500);

        Some(Self::new(on_connect, on_disconnect, on_subscribe, on_unsubscribe, timeout_ms))
    }
}

// Reason: SubscriptionLifecycle is defined with #[async_trait]; all implementations must match
// its transformed method signatures to satisfy the trait contract
// async_trait: dyn-dispatch required; remove when RTN + Send is stable (RFC 3425)
#[async_trait]
impl SubscriptionLifecycle for WebhookLifecycle {
    async fn on_connect(
        &self,
        params: &serde_json::Value,
        connection_id: &str,
    ) -> Result<(), String> {
        let Some(ref url) = self.on_connect_url else {
            return Ok(());
        };

        let body = serde_json::json!({
            "event": "connect",
            "connection_id": connection_id,
            "params": params,
        });

        match self.client.post(url).json(&body).send().await {
            Ok(resp) if resp.status().is_success() => Ok(()),
            Ok(resp) => {
                let status = resp.status();
                let text = resp
                    .text()
                    .await
                    .unwrap_or_else(|e| format!("(error reading response body: {e})"));
                warn!(
                    url = %url,
                    status = %status,
                    "on_connect webhook rejected connection"
                );
                Err(text)
            }
            Err(e) => {
                error!(url = %url, error = %e, "on_connect webhook failed");
                Err(format!("webhook timeout or error: {e}"))
            }
        }
    }

    async fn on_disconnect(&self, connection_id: &str) {
        let Some(ref url) = self.on_disconnect_url else {
            return;
        };

        let body = serde_json::json!({
            "event": "disconnect",
            "connection_id": connection_id,
        });

        // Fire-and-forget: spawn a task so we don't block the connection cleanup.
        let client = self.client.clone();
        let url = url.clone();
        tokio::spawn(async move {
            if let Err(e) = client.post(&url).json(&body).send().await {
                warn!(url = %url, error = %e, "on_disconnect webhook failed");
            }
        });
    }

    async fn on_subscribe(
        &self,
        subscription_name: &str,
        variables: &serde_json::Value,
        connection_id: &str,
    ) -> Result<(), String> {
        let Some(ref url) = self.on_subscribe_url else {
            return Ok(());
        };

        let body = serde_json::json!({
            "event": "subscribe",
            "connection_id": connection_id,
            "subscription_name": subscription_name,
            "variables": variables,
        });

        match self.client.post(url).json(&body).send().await {
            Ok(resp) if resp.status().is_success() => Ok(()),
            Ok(resp) => {
                let status = resp.status();
                let text = resp
                    .text()
                    .await
                    .unwrap_or_else(|e| format!("(error reading response body: {e})"));
                warn!(
                    url = %url,
                    status = %status,
                    "on_subscribe webhook rejected subscription"
                );
                Err(text)
            }
            Err(e) => {
                error!(url = %url, error = %e, "on_subscribe webhook failed");
                Err(format!("webhook timeout or error: {e}"))
            }
        }
    }

    async fn on_unsubscribe(&self, subscription_id: &str, connection_id: &str) {
        let Some(ref url) = self.on_unsubscribe_url else {
            return;
        };

        let body = serde_json::json!({
            "event": "unsubscribe",
            "connection_id": connection_id,
            "subscription_id": subscription_id,
        });

        let client = self.client.clone();
        let url = url.clone();
        tokio::spawn(async move {
            if let Err(e) = client.post(&url).json(&body).send().await {
                warn!(url = %url, error = %e, "on_unsubscribe webhook failed");
            }
        });
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)] // Reason: test code, panics acceptable
    #![allow(clippy::cast_precision_loss)] // Reason: test metrics reporting
    #![allow(clippy::cast_sign_loss)] // Reason: test data uses small positive integers
    #![allow(clippy::cast_possible_truncation)] // Reason: test data values are bounded
    #![allow(clippy::cast_possible_wrap)] // Reason: test data values are bounded
    #![allow(clippy::missing_panics_doc)] // Reason: test helpers
    #![allow(clippy::missing_errors_doc)] // Reason: test helpers
    #![allow(missing_docs)] // Reason: test code
    #![allow(clippy::items_after_statements)] // Reason: test helpers defined near use site

    use super::*;

    #[test]
    fn from_schema_json_no_hooks() {
        let json = serde_json::json!({});
        assert!(WebhookLifecycle::from_schema_json(&json).is_none());
    }

    #[test]
    fn from_schema_json_empty_hooks() {
        let json = serde_json::json!({"hooks": {}});
        assert!(WebhookLifecycle::from_schema_json(&json).is_none());
    }

    #[test]
    fn from_schema_json_with_connect_url() {
        let json = serde_json::json!({
            "hooks": {
                "on_connect": "http://localhost:8001/hooks/ws-connect",
                "timeout_ms": 300
            }
        });
        let wh = WebhookLifecycle::from_schema_json(&json).unwrap();
        assert_eq!(wh.on_connect_url, Some("http://localhost:8001/hooks/ws-connect".to_string()));
        assert!(wh.on_disconnect_url.is_none());
        assert!(wh.on_subscribe_url.is_none());
        assert_eq!(wh.timeout, Duration::from_millis(300));
    }

    #[test]
    fn from_schema_json_default_timeout() {
        let json = serde_json::json!({
            "hooks": {
                "on_disconnect": "http://localhost:8001/hooks/ws-disconnect"
            }
        });
        let wh = WebhookLifecycle::from_schema_json(&json).unwrap();
        assert_eq!(wh.timeout, Duration::from_millis(500));
    }
}
