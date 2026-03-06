use serde::Serialize;

use super::{SubscriptionError, transport::TransportAdapter, types::SubscriptionEvent};

/// Webhook transport adapter configuration.
#[derive(Debug, Clone)]
pub struct WebhookTransportConfig {
    /// Target URL for webhook delivery.
    pub url: String,

    /// Secret key for HMAC-SHA256 signature.
    pub secret: Option<String>,

    /// Request timeout in milliseconds.
    pub timeout_ms: u64,

    /// Maximum retry attempts.
    pub max_retries: u32,

    /// Initial retry delay in milliseconds (exponential backoff).
    pub retry_delay_ms: u64,

    /// Custom headers to include in requests.
    pub headers: std::collections::HashMap<String, String>,
}

impl WebhookTransportConfig {
    /// Create a new webhook configuration.
    #[must_use]
    pub fn new(url: impl Into<String>) -> Self {
        Self {
            url:            url.into(),
            secret:         None,
            timeout_ms:     30_000,
            max_retries:    3,
            retry_delay_ms: 1000,
            headers:        std::collections::HashMap::new(),
        }
    }

    /// Set the signing secret for HMAC-SHA256 signatures.
    #[must_use]
    pub fn with_secret(mut self, secret: impl Into<String>) -> Self {
        self.secret = Some(secret.into());
        self
    }

    /// Set the request timeout.
    #[must_use]
    pub fn with_timeout(mut self, timeout_ms: u64) -> Self {
        self.timeout_ms = timeout_ms;
        self
    }

    /// Set maximum retry attempts.
    #[must_use]
    pub fn with_max_retries(mut self, max_retries: u32) -> Self {
        self.max_retries = max_retries;
        self
    }

    /// Set initial retry delay.
    #[must_use]
    pub fn with_retry_delay(mut self, delay_ms: u64) -> Self {
        self.retry_delay_ms = delay_ms;
        self
    }

    /// Add a custom header.
    #[must_use]
    pub fn with_header(mut self, name: impl Into<String>, value: impl Into<String>) -> Self {
        self.headers.insert(name.into(), value.into());
        self
    }
}

/// Webhook payload format for event delivery.
#[derive(Debug, Clone, Serialize)]
pub struct WebhookPayload {
    /// Unique event identifier.
    pub event_id: String,

    /// Subscription name that triggered the event.
    pub subscription_name: String,

    /// Entity type (e.g., "Order").
    pub entity_type: String,

    /// Entity primary key.
    pub entity_id: String,

    /// Operation type.
    pub operation: String,

    /// Event data.
    pub data: serde_json::Value,

    /// Previous data (for UPDATE operations).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub old_data: Option<serde_json::Value>,

    /// Event timestamp (ISO 8601).
    pub timestamp: String,

    /// Sequence number for ordering.
    pub sequence_number: u64,
}

impl WebhookPayload {
    /// Create a webhook payload from a subscription event.
    #[must_use]
    pub fn from_event(event: &SubscriptionEvent, subscription_name: &str) -> Self {
        Self {
            event_id:          event.event_id.clone(),
            subscription_name: subscription_name.to_string(),
            entity_type:       event.entity_type.clone(),
            entity_id:         event.entity_id.clone(),
            operation:         format!("{:?}", event.operation),
            data:              event.data.clone(),
            old_data:          event.old_data.clone(),
            timestamp:         event.timestamp.to_rfc3339(),
            sequence_number:   event.sequence_number,
        }
    }
}

/// Webhook transport adapter for HTTP POST delivery.
///
/// Delivers subscription events via HTTP POST with:
/// - HMAC-SHA256 signature (X-FraiseQL-Signature header)
/// - Exponential backoff retry logic
/// - Configurable timeouts
///
/// # Example
///
/// ```no_run
/// // Requires: live HTTP endpoint for webhook delivery.
/// use fraiseql_core::runtime::subscription::{WebhookAdapter, WebhookTransportConfig};
///
/// let config = WebhookTransportConfig::new("https://api.example.com/webhooks")
///     .with_secret("my_secret_key")
///     .with_max_retries(3);
///
/// let adapter = WebhookAdapter::new(config);
/// adapter.deliver(&event, "orderCreated").await?;
/// ```
pub struct WebhookAdapter {
    config: WebhookTransportConfig,
    client: reqwest::Client,
}

impl WebhookAdapter {
    /// Create a new webhook adapter.
    ///
    /// # Panics
    ///
    /// Panics if the HTTP client cannot be built (should not happen in practice).
    #[must_use]
    pub fn new(config: WebhookTransportConfig) -> Self {
        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_millis(config.timeout_ms))
            .build()
            .expect("Failed to build HTTP client");

        Self { config, client }
    }

    /// Compute HMAC-SHA256 signature for payload.
    fn compute_signature(&self, payload: &str) -> Option<String> {
        use hmac::{Hmac, Mac};
        use sha2::Sha256;

        let secret = self.config.secret.as_ref()?;

        let mut mac =
            Hmac::<Sha256>::new_from_slice(secret.as_bytes()).expect("HMAC can take any size key");
        mac.update(payload.as_bytes());

        let result = mac.finalize();
        Some(hex::encode(result.into_bytes()))
    }
}

#[async_trait::async_trait]
impl TransportAdapter for WebhookAdapter {
    async fn deliver(
        &self,
        event: &SubscriptionEvent,
        subscription_name: &str,
    ) -> Result<(), SubscriptionError> {
        let payload = WebhookPayload::from_event(event, subscription_name);
        let payload_json = serde_json::to_string(&payload).map_err(|e| {
            SubscriptionError::Internal(format!("Failed to serialize payload: {e}"))
        })?;

        let mut attempt = 0;
        let mut delay = self.config.retry_delay_ms;

        loop {
            attempt += 1;

            let mut request = self
                .client
                .post(&self.config.url)
                .header("Content-Type", "application/json")
                .header("X-FraiseQL-Event-Id", &event.event_id)
                .header("X-FraiseQL-Event-Type", subscription_name);

            // Add signature if secret is configured
            if let Some(signature) = self.compute_signature(&payload_json) {
                request = request.header("X-FraiseQL-Signature", format!("sha256={signature}"));
            }

            // Add custom headers
            for (name, value) in &self.config.headers {
                request = request.header(name, value);
            }

            let result = request.body(payload_json.clone()).send().await;

            match result {
                Ok(response) if response.status().is_success() => {
                    tracing::debug!(
                        url = %self.config.url,
                        event_id = %event.event_id,
                        attempt = attempt,
                        "Webhook delivered successfully"
                    );
                    return Ok(());
                },
                Ok(response) => {
                    let status = response.status();
                    tracing::warn!(
                        url = %self.config.url,
                        event_id = %event.event_id,
                        status = %status,
                        attempt = attempt,
                        "Webhook delivery failed with status"
                    );

                    // Don't retry on client errors (4xx) except 429
                    if status.is_client_error() && status.as_u16() != 429 {
                        return Err(SubscriptionError::Internal(format!(
                            "Webhook delivery failed: {status}"
                        )));
                    }
                },
                Err(e) => {
                    tracing::warn!(
                        url = %self.config.url,
                        event_id = %event.event_id,
                        error = %e,
                        attempt = attempt,
                        "Webhook delivery error"
                    );
                },
            }

            // Check if we should retry
            if attempt >= self.config.max_retries {
                return Err(SubscriptionError::Internal(format!(
                    "Webhook delivery failed after {} attempts",
                    attempt
                )));
            }

            // Exponential backoff
            tracing::debug!(delay_ms = delay, "Retrying webhook delivery");
            tokio::time::sleep(std::time::Duration::from_millis(delay)).await;
            delay *= 2;
        }
    }

    fn name(&self) -> &'static str {
        "webhook"
    }

    async fn health_check(&self) -> bool {
        // Simple health check - verify URL is reachable
        match self.client.head(&self.config.url).send().await {
            Ok(response) => response.status().is_success() || response.status().as_u16() == 405,
            Err(_) => false,
        }
    }
}

impl std::fmt::Debug for WebhookAdapter {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("WebhookAdapter")
            .field("url", &self.config.url)
            .field("has_secret", &self.config.secret.is_some())
            .finish_non_exhaustive()
    }
}
