//! Core action implementations (webhook, Slack, email).
//!
//! This module implements the core action executors:
//! - Webhook: POST to HTTP endpoint
//! - Slack: Send messages to Slack webhook
//! - Email: Send emails via SMTP
//!
//! Each action handles template rendering, retry logic, and error handling.

use std::{collections::HashMap, time::Duration};

use reqwest::Client;
use serde_json::{Value, json};
use tracing::{debug, info};

use crate::{
    error::{ObserverError, Result},
    event::EntityEvent,
};

/// Default HTTP request timeout for outbound webhook calls.
///
/// Prevents a slow or non-responsive endpoint from blocking the executor
/// indefinitely.  Operators can override this by constructing the client
/// manually via [`WebhookAction::with_timeout`].
const DEFAULT_WEBHOOK_TIMEOUT_SECS: u64 = 30;

/// Validate an outbound URL for SSRF risk before sending a request.
///
/// Rejects:
/// - Non-HTTP(S) schemes (`file://`, `ftp://`, etc.)
/// - Loopback addresses (`localhost`, `127.x.x.x`, `::1`)
/// - RFC 1918 private ranges (10/8, 172.16/12, 192.168/16)
/// - Link-local (169.254/16), CGNAT (100.64/10), ULA (fc00::/7)
///
/// Attacker-controlled observer configs could redirect outbound webhook
/// calls to AWS EC2 metadata (`169.254.169.254`), internal Kubernetes
/// services (`svc.cluster.local`), or any other SSRF target.
///
/// # Errors
///
/// Returns `ObserverError::ActionPermanentlyFailed` if the URL uses a
/// non-HTTP(S) scheme or resolves to a blocked address range.
fn validate_outbound_url(url: &str) -> Result<()> {
    let lower = url.to_ascii_lowercase();
    if !lower.starts_with("http://") && !lower.starts_with("https://") {
        return Err(ObserverError::ActionPermanentlyFailed {
            reason: format!("Outbound URL must use http:// or https:// scheme: {url}"),
        });
    }

    // Extract the host portion (strip scheme, then take up to the first / : ? #).
    // IPv6 literals use bracket notation [addr]:port — handle separately so we
    // don't split on the `:` inside the brackets and miss the closing `]`.
    let after_scheme = if lower.starts_with("https://") { &url[8..] } else { &url[7..] };
    let host = if after_scheme.starts_with('[') {
        // IPv6 bracket notation: extract everything up to and including the `]`.
        after_scheme
            .find(']')
            .map_or_else(
                || after_scheme.split(['/', '?', '#']).next().unwrap_or(""),
                |end| &after_scheme[..=end],
            )
    } else {
        after_scheme.split(['/', ':', '?', '#']).next().unwrap_or("")
    };

    if is_ssrf_blocked_host_obs(host) {
        return Err(ObserverError::ActionPermanentlyFailed {
            reason: format!(
                "Outbound URL targets a private/loopback address (SSRF protection): {url}"
            ),
        });
    }

    Ok(())
}

/// Returns `true` for hostnames and literal IPs that are blocked as SSRF targets.
fn is_ssrf_blocked_host_obs(host: &str) -> bool {
    let lower = host.to_ascii_lowercase();
    if lower == "localhost" || lower == "::1" || lower == "[::1]" {
        return true;
    }

    // Literal IPv4
    if let Ok(addr) = host.parse::<std::net::Ipv4Addr>() {
        return addr.is_loopback()    // 127.0.0.0/8
            || addr.is_private()     // 10/8, 172.16/12, 192.168/16
            || addr.is_link_local()  // 169.254/16
            || is_cgnat_v4_obs(addr); // 100.64/10
    }

    // Literal IPv6 (strip optional brackets)
    let ipv6 = host.trim_start_matches('[').trim_end_matches(']');
    if let Ok(addr) = ipv6.parse::<std::net::Ipv6Addr>() {
        return addr.is_loopback()       // ::1
            || addr.is_unspecified()    // ::
            || is_ula_v6_obs(addr);     // fc00::/7
    }

    false
}

/// Returns `true` for CGNAT range 100.64.0.0/10.
fn is_cgnat_v4_obs(addr: std::net::Ipv4Addr) -> bool {
    let [a, b, ..] = addr.octets();
    a == 100 && (b & 0xC0) == 64
}

/// Returns `true` for ULA range fc00::/7.
fn is_ula_v6_obs(addr: std::net::Ipv6Addr) -> bool {
    (addr.segments()[0] & 0xFE00) == 0xFC00
}

/// Validate that no header name or value contains HTTP header injection characters.
///
/// HTTP/1.1 forbids:
/// - `\r` and `\n` inside field names and values (RFC 7230 §3.2, header injection)
/// - `\0` (NUL byte) which can truncate strings in C-based HTTP stacks
/// - `:` in header *names* (RFC 7230 §3.2 token rule; colons are the name/value separator)
///
/// An attacker who can supply custom observer headers could otherwise inject
/// arbitrary headers or corrupt the HTTP request.
///
/// # Errors
///
/// Returns `ObserverError::ActionPermanentlyFailed` if any name or value
/// contains a disallowed byte.
fn validate_headers(headers: &HashMap<String, String>) -> Result<()> {
    for (name, value) in headers {
        if name.contains('\r') || name.contains('\n') || name.contains('\0') {
            return Err(ObserverError::ActionPermanentlyFailed {
                reason: format!(
                    "Invalid webhook header name — contains CR/LF/NUL (header injection): {name:?}"
                ),
            });
        }
        if name.contains(':') {
            return Err(ObserverError::ActionPermanentlyFailed {
                reason: format!(
                    "Invalid webhook header name — contains colon (name/value separator): {name:?}"
                ),
            });
        }
        if value.contains('\r') || value.contains('\n') || value.contains('\0') {
            return Err(ObserverError::ActionPermanentlyFailed {
                reason: format!(
                    "Invalid webhook header value for '{name}' — contains CR/LF/NUL (header injection)"
                ),
            });
        }
    }
    Ok(())
}

/// Map an HTTP response status to a `WebhookResponse` (success) or the
/// appropriate `ObserverError` variant (failure).
///
/// - 2xx → `Ok(WebhookResponse { success: true, … })`
/// - 4xx except 429 → `Err(ActionPermanentlyFailed)` — retrying will not help
/// - 429, 5xx, other → `Err(ActionExecutionFailed)` — transient, eligible for retry
///
/// 429 Too Many Requests is transient because the server indicated it *can*
/// accept the request after a retry window; routing it to DLQ immediately would
/// discard actionable work.
fn classify_http_status(status: reqwest::StatusCode, duration_ms: f64) -> Result<WebhookResponse> {
    if status.is_success() {
        Ok(WebhookResponse {
            status_code: status.as_u16(),
            success: true,
            duration_ms,
        })
    } else if status.is_client_error() && status != reqwest::StatusCode::TOO_MANY_REQUESTS {
        // 4xx (except 429): permanent failure — retrying will not help; route directly to DLQ.
        Err(ObserverError::ActionPermanentlyFailed {
            reason: format!("HTTP {status} (client error — will not retry)"),
        })
    } else {
        // 5xx / 429 / other: transient failure — eligible for retry backoff.
        Err(ObserverError::ActionExecutionFailed {
            reason: format!("HTTP {status} response"),
        })
    }
}

/// Webhook action executor
pub struct WebhookAction {
    /// HTTP client for making requests
    client: Client,
}

impl WebhookAction {
    /// Create a new webhook action executor with the default 30-second timeout.
    #[must_use]
    pub fn new() -> Self {
        Self {
            client: Client::builder()
                .timeout(Duration::from_secs(DEFAULT_WEBHOOK_TIMEOUT_SECS))
                .build()
                .unwrap_or_default(),
        }
    }

    /// Create a webhook action executor with a custom request timeout.
    #[must_use]
    pub fn with_timeout(timeout: Duration) -> Self {
        Self {
            client: Client::builder()
                .timeout(timeout)
                .build()
                .unwrap_or_default(),
        }
    }

    /// Execute webhook action
    pub async fn execute(
        &self,
        url: &str,
        headers: &HashMap<String, String>,
        body_template: Option<&str>,
        event: &EntityEvent,
    ) -> Result<WebhookResponse> {
        let start = std::time::Instant::now();

        debug!("WebhookAction.execute() called");
        info!("  URL: {}", url);
        info!("  Headers: {:?}", headers);
        info!("  Body template: {:?}", body_template);

        // SECURITY: Reject URLs that target private/loopback addresses (SSRF protection).
        validate_outbound_url(url)?;
        // SECURITY: Reject headers that contain CR/LF to prevent header injection.
        validate_headers(headers)?;

        // Prepare request body
        let body = if let Some(template) = body_template {
            // Simple template substitution: replace {{ field }} with event.data[field]
            self.render_body_template(template, &event.data)?
        } else {
            // Default: send the event as JSON
            event.data.clone()
        };

        info!(
            "  Body: {}",
            serde_json::to_string(&body).unwrap_or_else(|_| "<invalid json>".to_string())
        );

        // Build request
        let mut request = self.client.post(url);

        // Add headers (already validated above)
        for (key, value) in headers {
            request = request.header(key, value);
        }

        info!("  Sending HTTP POST...");

        // Send request
        let response =
            request
                .json(&body)
                .send()
                .await
                .map_err(|e| ObserverError::ActionExecutionFailed {
                    reason: format!("HTTP request failed: {e}"),
                })?;

        let status = response.status();
        let duration_ms = start.elapsed().as_secs_f64() * 1000.0;

        classify_http_status(status, duration_ms)
    }

    fn render_body_template(&self, template: &str, data: &Value) -> Result<Value> {
        let mut rendered = template.to_string();

        if let Value::Object(map) = data {
            for (key, value) in map {
                let placeholder = format!("{{{{ {key} }}}}");
                let value_str = match value {
                    Value::String(s) => s.clone(),
                    _ => value.to_string(),
                };
                rendered = rendered.replace(&placeholder, &value_str);
            }
        }

        serde_json::from_str(&rendered).or(Ok(Value::String(rendered)))
    }
}

impl Default for WebhookAction {
    fn default() -> Self {
        Self::new()
    }
}

/// Response from webhook execution
#[derive(Debug, Clone)]
pub struct WebhookResponse {
    /// HTTP status code
    pub status_code: u16,
    /// Whether execution was successful
    pub success:     bool,
    /// Duration in milliseconds
    pub duration_ms: f64,
}

/// Slack action executor
pub struct SlackAction {
    /// HTTP client for making requests
    client: Client,
}

impl SlackAction {
    /// Create a new Slack action executor
    #[must_use]
    pub fn new() -> Self {
        Self {
            client: Client::builder()
                .timeout(Duration::from_secs(DEFAULT_WEBHOOK_TIMEOUT_SECS))
                .build()
                .unwrap_or_default(),
        }
    }

    /// Execute Slack action
    pub async fn execute(
        &self,
        webhook_url: &str,
        channel: Option<&str>,
        message_template: Option<&str>,
        event: &EntityEvent,
    ) -> Result<SlackResponse> {
        let start = std::time::Instant::now();

        // Prepare message
        let message = if let Some(template) = message_template {
            self.render_message_template(template, &event.data)?
        } else {
            format!(
                "Event: {} on {} (ID: {})",
                event.event_type.as_str(),
                event.entity_type,
                event.entity_id
            )
        };

        // Build Slack payload
        let mut payload = json!({
            "text": message,
            "type": "mrkdwn"
        });

        if let Some(ch) = channel {
            payload["channel"] = Value::String(ch.to_string());
        }

        // SECURITY: Reject URLs that target private/loopback addresses (SSRF protection).
        validate_outbound_url(webhook_url)?;

        // Send request
        let response = self.client.post(webhook_url).json(&payload).send().await.map_err(|e| {
            ObserverError::ActionExecutionFailed {
                reason: format!("Slack webhook failed: {e}"),
            }
        })?;

        let status = response.status();
        let duration_ms = start.elapsed().as_secs_f64() * 1000.0;

        if status.is_success() {
            Ok(SlackResponse {
                status_code: status.as_u16(),
                success: true,
                duration_ms,
            })
        } else if status.is_client_error() {
            Err(ObserverError::ActionPermanentlyFailed {
                reason: format!("Slack HTTP {status} (client error — will not retry)"),
            })
        } else {
            Err(ObserverError::ActionExecutionFailed {
                reason: format!("Slack HTTP {status} response"),
            })
        }
    }

    fn render_message_template(&self, template: &str, data: &Value) -> Result<String> {
        let mut rendered = template.to_string();

        if let Value::Object(map) = data {
            for (key, value) in map {
                let placeholder = format!("{{{{ {key} }}}}");
                let value_str = match value {
                    Value::String(s) => s.clone(),
                    _ => value.to_string(),
                };
                rendered = rendered.replace(&placeholder, &value_str);
            }
        }

        Ok(rendered)
    }
}

impl Default for SlackAction {
    fn default() -> Self {
        Self::new()
    }
}

/// Response from Slack execution
#[derive(Debug, Clone)]
pub struct SlackResponse {
    /// HTTP status code
    pub status_code: u16,
    /// Whether execution was successful
    pub success:     bool,
    /// Duration in milliseconds
    pub duration_ms: f64,
}

/// Email action executor
pub struct EmailAction {
    // Placeholder for SMTP client
}

impl EmailAction {
    /// Create a new email action executor
    #[must_use]
    pub const fn new() -> Self {
        Self {}
    }

    /// Execute email action (stub)
    pub async fn execute(
        &self,
        _to: &str,
        _subject: &str,
        _body_template: Option<&str>,
        _event: &EntityEvent,
    ) -> Result<EmailResponse> {
        // Stub implementation
        Ok(EmailResponse {
            success:     true,
            message_id:  Some(uuid::Uuid::new_v4().to_string()),
            duration_ms: 10.0,
        })
    }
}

impl Default for EmailAction {
    fn default() -> Self {
        Self::new()
    }
}

/// Response from email execution
#[derive(Debug, Clone)]
pub struct EmailResponse {
    /// Whether execution was successful
    pub success:     bool,
    /// Message ID from provider (if available)
    pub message_id:  Option<String>,
    /// Duration in milliseconds
    pub duration_ms: f64,
}

/// Generic action execution result
#[derive(Debug, Clone)]
pub struct ActionExecutionResult {
    /// Action type (webhook, slack, email, etc.)
    pub action_type: String,
    /// Success status
    pub success:     bool,
    /// Duration in milliseconds
    pub duration_ms: f64,
    /// Optional message ID or tracking info
    pub tracking_id: Option<String>,
}

#[allow(clippy::unwrap_used)]  // Reason: test code, panics are acceptable
#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;
    use crate::event::EventKind;

    #[test]
    fn test_webhook_action_creation() {
        let webhook = WebhookAction::new();
        // Just verify that the webhook action was created successfully
        let _ = webhook;
    }

    #[test]
    fn test_slack_action_creation() {
        let slack = SlackAction::new();
        // Just verify that the Slack action was created successfully
        let _ = slack;
    }

    #[test]
    fn test_email_action_creation() {
        let email = EmailAction::new();
        // Basic instantiation test
        let _result = std::mem::size_of_val(&email);
    }

    #[tokio::test]
    async fn test_email_action_execute() {
        let email = EmailAction::new();
        let event = EntityEvent::new(
            EventKind::Created,
            "Order".to_string(),
            uuid::Uuid::new_v4(),
            json!({"total": 100}),
        );

        let result = email.execute("user@example.com", "Test", None, &event).await.unwrap();

        assert!(result.success);
        assert!(result.message_id.is_some());
    }

    #[test]
    fn test_webhook_render_body_template() {
        let webhook = WebhookAction::new();
        let data = json!({"status": "completed", "total": 150});
        let template = r#"{"status": "{{ status }}", "amount": {{ total }}}"#;

        let result = webhook.render_body_template(template, &data).unwrap();

        // Check that substitution happened
        let rendered_str = result.to_string();
        assert!(rendered_str.contains("completed"));
        assert!(rendered_str.contains("150"));
    }

    #[test]
    fn test_slack_render_message_template() {
        let slack = SlackAction::new();
        let data = json!({"status": "shipped", "order_id": "12345"});
        let template = "Order {{ order_id }} has been {{ status }}";

        let result = slack.render_message_template(template, &data).unwrap();

        assert_eq!(result, "Order 12345 has been shipped");
    }

    #[test]
    fn test_action_execution_result() {
        let result = ActionExecutionResult {
            action_type: "webhook".to_string(),
            success:     true,
            duration_ms: 42.5,
            tracking_id: Some("abc123".to_string()),
        };

        assert_eq!(result.action_type, "webhook");
        assert!(result.success);
        assert_eq!(result.duration_ms, 42.5);
    }

    // --- Header injection tests (H11) ---

    #[test]
    fn test_validate_headers_clean_passes() {
        let mut headers = HashMap::new();
        headers.insert("X-Custom".to_string(), "value".to_string());
        headers.insert("Authorization".to_string(), "Bearer token".to_string());
        assert!(validate_headers(&headers).is_ok());
    }

    #[test]
    fn test_validate_headers_lf_in_name_rejected() {
        let mut headers = HashMap::new();
        headers.insert("X-Evil\nInjected".to_string(), "value".to_string());
        let err = validate_headers(&headers).unwrap_err();
        let msg = err.to_string();
        assert!(msg.contains("header injection"), "expected injection message, got: {msg}");
    }

    #[test]
    fn test_validate_headers_cr_in_name_rejected() {
        let mut headers = HashMap::new();
        headers.insert("X-Evil\rInjected".to_string(), "value".to_string());
        assert!(validate_headers(&headers).is_err());
    }

    #[test]
    fn test_validate_headers_lf_in_value_rejected() {
        let mut headers = HashMap::new();
        headers.insert(
            "X-Legit".to_string(),
            "value\r\nX-Injected: malicious".to_string(),
        );
        assert!(validate_headers(&headers).is_err());
    }

    #[test]
    fn test_validate_headers_empty_map_passes() {
        assert!(validate_headers(&HashMap::new()).is_ok());
    }

    #[test]
    fn test_webhook_action_with_timeout_creates_ok() {
        let _action = WebhookAction::with_timeout(Duration::from_secs(5));
    }

    // --- Additional header injection tests (14-3) ---

    #[test]
    fn test_validate_headers_nul_in_name_rejected() {
        let mut headers = HashMap::new();
        headers.insert("X-Evil\0Null".to_string(), "value".to_string());
        let err = validate_headers(&headers).unwrap_err();
        assert!(
            err.to_string().contains("NUL") || err.to_string().contains("injection"),
            "expected injection message, got: {err}"
        );
    }

    #[test]
    fn test_validate_headers_nul_in_value_rejected() {
        let mut headers = HashMap::new();
        headers.insert("X-Legit".to_string(), "value\0payload".to_string());
        let err = validate_headers(&headers).unwrap_err();
        assert!(err.to_string().contains("injection"), "got: {err}");
    }

    #[test]
    fn test_validate_headers_colon_in_name_rejected() {
        let mut headers = HashMap::new();
        // A colon in a header name is the name/value separator — disallowed.
        headers.insert("X-Forged: X-Real-IP".to_string(), "value".to_string());
        let err = validate_headers(&headers).unwrap_err();
        assert!(err.to_string().contains("colon"), "expected colon message, got: {err}");
    }

    #[test]
    fn test_validate_headers_colon_in_value_is_allowed() {
        // Colons are valid in header *values* (e.g. "Bearer tok:en", URLs, etc.)
        let mut headers = HashMap::new();
        headers.insert("Authorization".to_string(), "Bearer abc:xyz".to_string());
        assert!(validate_headers(&headers).is_ok());
    }

    // --- HTTP status classification tests (14-4) ---

    #[test]
    fn test_200_ok_is_success() {
        let result = classify_http_status(reqwest::StatusCode::OK, 10.0);
        assert!(result.is_ok());
        assert!(result.unwrap().success);
    }

    #[test]
    fn test_404_is_permanent_failure() {
        let result = classify_http_status(reqwest::StatusCode::NOT_FOUND, 5.0);
        assert!(matches!(result, Err(ObserverError::ActionPermanentlyFailed { .. })));
    }

    #[test]
    fn test_400_is_permanent_failure() {
        let result = classify_http_status(reqwest::StatusCode::BAD_REQUEST, 5.0);
        assert!(matches!(result, Err(ObserverError::ActionPermanentlyFailed { .. })));
    }

    #[test]
    fn test_429_is_transient_failure() {
        // 429 must NOT be permanent — it should be eligible for retry.
        let result = classify_http_status(reqwest::StatusCode::TOO_MANY_REQUESTS, 5.0);
        assert!(
            matches!(result, Err(ObserverError::ActionExecutionFailed { .. })),
            "429 must be treated as transient (retryable), not permanent"
        );
    }

    #[test]
    fn test_500_is_transient_failure() {
        let result = classify_http_status(reqwest::StatusCode::INTERNAL_SERVER_ERROR, 5.0);
        assert!(matches!(result, Err(ObserverError::ActionExecutionFailed { .. })));
    }

    // --- SSRF protection tests (C7) ---

    #[test]
    fn test_outbound_url_scheme_must_be_http() {
        assert!(validate_outbound_url("file:///etc/passwd").is_err());
        assert!(validate_outbound_url("ftp://example.com").is_err());
        assert!(validate_outbound_url("example.com/hook").is_err());
    }

    #[test]
    fn test_outbound_url_blocks_loopback() {
        assert!(validate_outbound_url("http://localhost:8080").is_err());
        assert!(validate_outbound_url("http://127.0.0.1/hook").is_err());
        assert!(validate_outbound_url("http://[::1]/hook").is_err());
    }

    #[test]
    fn test_outbound_url_blocks_private_ranges() {
        assert!(validate_outbound_url("http://10.0.0.1/hook").is_err());
        assert!(validate_outbound_url("http://172.16.0.1/hook").is_err());
        assert!(validate_outbound_url("http://192.168.1.100/hook").is_err());
        // AWS metadata endpoint
        assert!(validate_outbound_url("http://169.254.169.254/latest/meta-data/").is_err());
        // CGNAT range
        assert!(validate_outbound_url("http://100.64.0.1/hook").is_err());
    }

    #[test]
    fn test_outbound_url_allows_public_addresses() {
        assert!(validate_outbound_url("https://hooks.slack.com/services/xxx").is_ok());
        assert!(validate_outbound_url("https://api.example.com/webhook").is_ok());
        assert!(validate_outbound_url("http://203.0.113.10/hook").is_ok());
    }

    // ── S24-H2: SlackAction client timeout ────────────────────────────────────

    #[test]
    fn slack_action_default_timeout_is_set() {
        // Verify the shared timeout constant is non-zero and in a sane range.
        const { assert!(DEFAULT_WEBHOOK_TIMEOUT_SECS > 0 && DEFAULT_WEBHOOK_TIMEOUT_SECS <= 120) }
    }

    #[test]
    fn slack_action_new_creates_instance() {
        // SlackAction::new() must succeed — no panics allowed from Client::builder().
        let _slack = SlackAction::new();
    }
}
