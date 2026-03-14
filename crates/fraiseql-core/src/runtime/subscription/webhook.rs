use async_trait::async_trait;
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
    pub const fn with_timeout(mut self, timeout_ms: u64) -> Self {
        self.timeout_ms = timeout_ms;
        self
    }

    /// Set maximum retry attempts.
    #[must_use]
    pub const fn with_max_retries(mut self, max_retries: u32) -> Self {
        self.max_retries = max_retries;
        self
    }

    /// Set initial retry delay.
    #[must_use]
    pub const fn with_retry_delay(mut self, delay_ms: u64) -> Self {
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
/// let adapter = WebhookAdapter::new(config)?;
/// adapter.deliver(&event, "orderCreated").await?;
/// ```
pub struct WebhookAdapter {
    config: WebhookTransportConfig,
    client: reqwest::Client,
}

impl WebhookAdapter {
    /// Create a new webhook adapter.
    ///
    /// # Errors
    ///
    /// Returns an error if the URL targets a private/reserved IP (SSRF protection),
    /// or if the underlying HTTP client cannot be initialized.
    pub fn new(config: WebhookTransportConfig) -> Result<Self, SubscriptionError> {
        validate_webhook_url(&config.url)?;

        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_millis(config.timeout_ms))
            .build()
            .map_err(|e| SubscriptionError::Internal(format!("HTTP client init failed: {e}")))?;

        Ok(Self { config, client })
    }

    /// Compute HMAC-SHA256 signature for payload.
    fn compute_signature(&self, payload: &str) -> Option<String> {
        use hmac::{Hmac, Mac};
        use sha2::Sha256;

        let secret = self.config.secret.as_ref()?;

        #[allow(clippy::expect_used)]
        // Reason: SHA-256 HMAC (FIPS 198-1) accepts keys of any size;
        //         new_from_slice only fails for fixed-block-size ciphers (e.g., AES-CMAC).
        let mut mac = Hmac::<Sha256>::new_from_slice(secret.as_bytes())
            .expect("SHA-256 HMAC accepts any key size");
        mac.update(payload.as_bytes());

        let result = mac.finalize();
        Some(hex::encode(result.into_bytes()))
    }
}

// Reason: TransportAdapter is defined with #[async_trait]; all implementations must match
// its transformed method signatures to satisfy the trait contract
// async_trait: dyn-dispatch required; remove when RTN + Send is stable (RFC 3425)
#[async_trait]
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

/// Validate a webhook target URL for SSRF risks.
///
/// Rejects URLs targeting private/loopback/link-local addresses to prevent
/// server-side request forgery via attacker-controlled webhook configurations.
///
/// # Errors
///
/// Returns `SubscriptionError::Internal` if the URL is invalid or targets a
/// forbidden host (private IP, loopback, link-local).
pub fn validate_webhook_url(url: &str) -> Result<(), SubscriptionError> {
    let parsed = reqwest::Url::parse(url)
        .map_err(|e| SubscriptionError::Internal(format!("Invalid webhook URL '{url}': {e}")))?;

    let host_raw = parsed
        .host_str()
        .ok_or_else(|| SubscriptionError::Internal(format!("Webhook URL has no host: {url}")))?;

    // Strip IPv6 brackets added by the url crate (e.g. "[::1]" → "::1").
    let host = if host_raw.starts_with('[') && host_raw.ends_with(']') {
        &host_raw[1..host_raw.len() - 1]
    } else {
        host_raw
    };

    let lower_host = host.to_ascii_lowercase();
    if lower_host == "localhost" || lower_host.ends_with(".localhost") {
        return Err(SubscriptionError::Internal(format!(
            "Webhook URL targets a loopback host ({host}) — SSRF protection blocked"
        )));
    }

    if let Ok(ip) = host.parse::<std::net::IpAddr>() {
        if is_webhook_ssrf_blocked_ip(&ip) {
            return Err(SubscriptionError::Internal(format!(
                "Webhook URL targets a private/reserved IP ({ip}) — SSRF protection blocked"
            )));
        }
    }

    Ok(())
}

/// Returns `true` for IP ranges that webhook delivery must never contact.
fn is_webhook_ssrf_blocked_ip(ip: &std::net::IpAddr) -> bool {
    match ip {
        std::net::IpAddr::V4(v4) => {
            let o = v4.octets();
            o[0] == 127                                          // loopback 127/8
            || o[0] == 10                                        // RFC 1918 10/8
            || (o[0] == 172 && (16..=31).contains(&o[1]))       // RFC 1918 172.16/12
            || (o[0] == 192 && o[1] == 168)                     // RFC 1918 192.168/16
            || (o[0] == 169 && o[1] == 254)                     // link-local 169.254/16
            || (o[0] == 100 && (64..=127).contains(&o[1]))      // CGNAT 100.64/10
            || o == [0, 0, 0, 0] // unspecified
        },
        std::net::IpAddr::V6(v6) => {
            v6.is_loopback()                                     // ::1
            || v6.is_unspecified()                               // ::
            || {
                let s = v6.segments();
                (s[0] & 0xfe00) == 0xfc00                        // ULA fc00::/7
                || (s[0] & 0xffc0) == 0xfe80                    // link-local fe80::/10
                || (s[0] == 0 && s[1] == 0 && s[2] == 0        // ::ffff:0:0/96
                    && s[3] == 0 && s[4] == 0 && s[5] == 0xffff)
            }
        },
    }
}
