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

/// Validate that no header name or value contains HTTP header injection characters.
///
/// HTTP/1.1 forbids `\r` and `\n` inside field names and values (RFC 7230 §3.2).
/// An attacker who can supply custom observer headers could otherwise inject
/// arbitrary headers into the outbound request.
///
/// # Errors
///
/// Returns `ObserverError::ActionPermanentlyFailed` if any name or value
/// contains a CR (`\r`) or LF (`\n`) byte.
fn validate_headers(headers: &HashMap<String, String>) -> Result<()> {
    for (name, value) in headers {
        if name.contains('\r') || name.contains('\n') {
            return Err(ObserverError::ActionPermanentlyFailed {
                reason: format!(
                    "Invalid webhook header name — contains CR/LF (header injection): {name:?}"
                ),
            });
        }
        if value.contains('\r') || value.contains('\n') {
            return Err(ObserverError::ActionPermanentlyFailed {
                reason: format!(
                    "Invalid webhook header value for '{name}' — contains CR/LF (header injection)"
                ),
            });
        }
    }
    Ok(())
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
                .unwrap_or_else(|_| Client::new()),
        }
    }

    /// Create a webhook action executor with a custom request timeout.
    #[must_use]
    pub fn with_timeout(timeout: Duration) -> Self {
        Self {
            client: Client::builder()
                .timeout(timeout)
                .build()
                .unwrap_or_else(|_| Client::new()),
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

        if status.is_success() {
            Ok(WebhookResponse {
                status_code: status.as_u16(),
                success: true,
                duration_ms,
            })
        } else if status.is_client_error() {
            // 4xx: permanent failure — retrying will not help; route directly to DLQ.
            Err(ObserverError::ActionPermanentlyFailed {
                reason: format!("HTTP {status} (client error — will not retry)"),
            })
        } else {
            // 5xx / other: transient failure — eligible for retry backoff.
            Err(ObserverError::ActionExecutionFailed {
                reason: format!("HTTP {status} response"),
            })
        }
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
            client: Client::new(),
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
}
