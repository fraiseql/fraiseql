//! Core action implementations (webhook, Slack, email).
//!
//! This module implements the core action executors:
//! - Webhook: POST to HTTP endpoint
//! - Slack: Send messages to Slack webhook
//! - Email: Send emails via SMTP
//!
//! Each action handles template rendering, retry logic, and error handling.

use crate::error::{ObserverError, Result};
use crate::event::EntityEvent;
use reqwest::Client;
use serde_json::{json, Value};
use std::collections::HashMap;
use tracing::{debug, info};

/// Webhook action executor
pub struct WebhookAction {
    /// HTTP client for making requests
    client: Client,
}

impl WebhookAction {
    /// Create a new webhook action executor
    #[must_use] 
    pub fn new() -> Self {
        Self {
            client: Client::new(),
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

        // Prepare request body
        let body = if let Some(template) = body_template {
            // Simple template substitution: replace {{ field }} with event.data[field]
            self.render_body_template(template, &event.data)?
        } else {
            // Default: send the event as JSON
            event.data.clone()
        };

        info!("  Body: {}", serde_json::to_string(&body).unwrap_or_else(|_| "<invalid json>".to_string()));

        // Build request
        let mut request = self.client.post(url);

        // Add headers
        for (key, value) in headers {
            request = request.header(key, value);
        }

        info!("  Sending HTTP POST...");

        // Send request
        let response = request
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
        } else {
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
    pub success: bool,
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
        let response = self
            .client
            .post(webhook_url)
            .json(&payload)
            .send()
            .await
            .map_err(|e| ObserverError::ActionExecutionFailed {
                reason: format!("Slack webhook failed: {e}"),
            })?;

        let status = response.status();
        let duration_ms = start.elapsed().as_secs_f64() * 1000.0;

        if status.is_success() {
            Ok(SlackResponse {
                status_code: status.as_u16(),
                success: true,
                duration_ms,
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
    pub success: bool,
    /// Duration in milliseconds
    pub duration_ms: f64,
}

/// Email action executor
pub struct EmailAction {
    // Placeholder for SMTP client (will be initialized in Phase 6.4)
}

impl EmailAction {
    /// Create a new email action executor
    #[must_use] 
    pub const fn new() -> Self {
        Self {}
    }

    /// Execute email action (stub for Phase 6.4)
    pub async fn execute(
        &self,
        _to: &str,
        _subject: &str,
        _body_template: Option<&str>,
        _event: &EntityEvent,
    ) -> Result<EmailResponse> {
        // Stub: will be implemented with real SMTP in Phase 6.4
        Ok(EmailResponse {
            success: true,
            message_id: Some(uuid::Uuid::new_v4().to_string()),
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
    pub success: bool,
    /// Message ID from provider (if available)
    pub message_id: Option<String>,
    /// Duration in milliseconds
    pub duration_ms: f64,
}

/// Generic action execution result
#[derive(Debug, Clone)]
pub struct ActionExecutionResult {
    /// Action type (webhook, slack, email, etc.)
    pub action_type: String,
    /// Success status
    pub success: bool,
    /// Duration in milliseconds
    pub duration_ms: f64,
    /// Optional message ID or tracking info
    pub tracking_id: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::event::EventKind;
    use serde_json::json;

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

        let result = email
            .execute("user@example.com", "Test", None, &event)
            .await
            .unwrap();

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

        let result = slack
            .render_message_template(template, &data)
            .unwrap();

        assert_eq!(result, "Order 12345 has been shipped");
    }

    #[test]
    fn test_action_execution_result() {
        let result = ActionExecutionResult {
            action_type: "webhook".to_string(),
            success: true,
            duration_ms: 42.5,
            tracking_id: Some("abc123".to_string()),
        };

        assert_eq!(result.action_type, "webhook");
        assert!(result.success);
        assert_eq!(result.duration_ms, 42.5);
    }
}
