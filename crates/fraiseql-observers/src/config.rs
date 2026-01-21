//! Configuration structures for the observer system.

use crate::error::{ObserverError, Result};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Observer runtime configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ObserverRuntimeConfig {
    /// Channel buffer size for incoming events (default: 1000)
    #[serde(default = "default_channel_capacity")]
    pub channel_capacity: usize,

    /// Maximum concurrent action executions (default: 50)
    #[serde(default = "default_max_concurrency")]
    pub max_concurrency: usize,

    /// What to do when channel is full
    #[serde(default)]
    pub overflow_policy: OverflowPolicy,

    /// Backlog threshold for alerts (default: 500)
    #[serde(default = "default_backlog_threshold")]
    pub backlog_alert_threshold: usize,

    /// Graceful shutdown timeout (default: "30s")
    #[serde(default = "default_shutdown_timeout")]
    pub shutdown_timeout: String,

    /// Observer definitions
    #[serde(default)]
    pub observers: HashMap<String, ObserverDefinition>,
}

fn default_channel_capacity() -> usize {
    1000
}

fn default_max_concurrency() -> usize {
    50
}

fn default_backlog_threshold() -> usize {
    500
}

fn default_shutdown_timeout() -> String {
    "30s".to_string()
}

/// What to do when the event channel is full
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OverflowPolicy {
    /// Drop new events when channel is full (default)
    #[default]
    Drop,
    /// Block sender (can cause issues with PG listener)
    Block,
    /// Drop oldest events to make room
    DropOldest,
}

/// Single observer definition
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ObserverDefinition {
    /// Event type this observer watches (INSERT, UPDATE, DELETE, CUSTOM)
    pub event_type: String,

    /// Entity type this observer watches (e.g., "Order", "User")
    pub entity: String,

    /// Optional condition that must be true for actions to execute
    #[serde(default)]
    pub condition: Option<String>,

    /// Actions to execute when observer is triggered
    pub actions: Vec<ActionConfig>,

    /// Retry configuration
    #[serde(default)]
    pub retry: RetryConfig,

    /// Failure handling policy
    #[serde(default)]
    pub on_failure: FailurePolicy,
}

/// Retry configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RetryConfig {
    /// Maximum number of retry attempts (default: 3)
    #[serde(default = "default_max_attempts")]
    pub max_attempts: u32,

    /// Initial retry delay in milliseconds (default: 100)
    #[serde(default = "default_initial_delay")]
    pub initial_delay_ms: u64,

    /// Maximum retry delay in milliseconds (default: 30000)
    #[serde(default = "default_max_delay")]
    pub max_delay_ms: u64,

    /// Backoff strategy
    #[serde(default)]
    pub backoff_strategy: BackoffStrategy,
}

impl Default for RetryConfig {
    fn default() -> Self {
        Self {
            max_attempts: default_max_attempts(),
            initial_delay_ms: default_initial_delay(),
            max_delay_ms: default_max_delay(),
            backoff_strategy: BackoffStrategy::default(),
        }
    }
}

fn default_max_attempts() -> u32 {
    3
}

fn default_initial_delay() -> u64 {
    100
}

fn default_max_delay() -> u64 {
    30000
}

/// Backoff strategy for retries
#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BackoffStrategy {
    /// Exponential backoff (2^attempt * initial_delay)
    #[default]
    Exponential,
    /// Linear backoff (attempt * initial_delay)
    Linear,
    /// Fixed delay between retries
    Fixed,
}

/// What to do when an action fails permanently
#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FailurePolicy {
    /// Log the error (default)
    #[default]
    Log,
    /// Send an alert
    Alert,
    /// Move to dead letter queue for manual retry
    Dlq,
}

/// Action configuration (tagged union)
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ActionConfig {
    /// HTTP POST webhook to external URL
    Webhook {
        /// URL to POST to
        url: Option<String>,
        /// Environment variable containing the URL
        url_env: Option<String>,
        /// Optional HTTP headers
        #[serde(default)]
        headers: HashMap<String, String>,
        /// Template for request body
        #[serde(default)]
        body_template: Option<String>,
    },

    /// Send message to Slack webhook
    Slack {
        /// Slack webhook URL
        webhook_url: Option<String>,
        /// Environment variable containing webhook URL
        webhook_url_env: Option<String>,
        /// Channel to send to (if not in webhook URL)
        #[serde(default)]
        channel: Option<String>,
        /// Message template
        #[serde(default)]
        message_template: Option<String>,
    },

    /// Send email via SMTP
    Email {
        /// Recipient email address
        to: Option<String>,
        /// Template for recipient (e.g., "{{ data.email }}")
        to_template: Option<String>,
        /// Email subject
        subject: Option<String>,
        /// Subject template
        subject_template: Option<String>,
        /// Email body template
        body_template: Option<String>,
        /// Reply-to address
        #[serde(default)]
        reply_to: Option<String>,
    },

    /// Send SMS (stub for Phase 6, full implementation later)
    Sms {
        /// Phone number to send to
        phone: Option<String>,
        /// Template for phone number
        phone_template: Option<String>,
        /// Message template
        message_template: Option<String>,
    },

    /// Send push notification (stub for Phase 6)
    Push {
        /// Device token
        device_token: Option<String>,
        /// Title template
        title_template: Option<String>,
        /// Body template
        body_template: Option<String>,
    },

    /// Update search index (stub for Phase 6)
    Search {
        /// Index name
        index: String,
        /// Document ID template
        id_template: Option<String>,
    },

    /// Invalidate cache (stub for Phase 6)
    Cache {
        /// Cache key pattern
        key_pattern: String,
        /// Action: "invalidate" or "refresh"
        action: String,
    },
}

impl ActionConfig {
    /// Get the action type name
    pub fn action_type(&self) -> &'static str {
        match self {
            ActionConfig::Webhook { .. } => "webhook",
            ActionConfig::Slack { .. } => "slack",
            ActionConfig::Email { .. } => "email",
            ActionConfig::Sms { .. } => "sms",
            ActionConfig::Push { .. } => "push",
            ActionConfig::Search { .. } => "search",
            ActionConfig::Cache { .. } => "cache",
        }
    }

    /// Validate the action configuration
    pub fn validate(&self) -> Result<()> {
        match self {
            ActionConfig::Webhook {
                url,
                url_env,
                body_template,
                ..
            } => {
                if url.is_none() && url_env.is_none() {
                    return Err(ObserverError::InvalidActionConfig {
                        reason: "Webhook action requires 'url' or 'url_env'".to_string(),
                    });
                }
                if body_template.as_ref().map_or(false, |t| t.is_empty()) {
                    return Err(ObserverError::InvalidActionConfig {
                        reason: "Webhook body_template cannot be empty".to_string(),
                    });
                }
                Ok(())
            }
            ActionConfig::Slack {
                webhook_url,
                webhook_url_env,
                ..
            } => {
                if webhook_url.is_none() && webhook_url_env.is_none() {
                    return Err(ObserverError::InvalidActionConfig {
                        reason: "Slack action requires 'webhook_url' or 'webhook_url_env'"
                            .to_string(),
                    });
                }
                Ok(())
            }
            ActionConfig::Email {
                to,
                to_template,
                subject,
                body_template,
                ..
            } => {
                if to.is_none() && to_template.is_none() {
                    return Err(ObserverError::InvalidActionConfig {
                        reason: "Email action requires 'to' or 'to_template'".to_string(),
                    });
                }
                if subject.is_none() {
                    return Err(ObserverError::InvalidActionConfig {
                        reason: "Email action requires 'subject'".to_string(),
                    });
                }
                if body_template.is_none() {
                    return Err(ObserverError::InvalidActionConfig {
                        reason: "Email action requires 'body_template'".to_string(),
                    });
                }
                Ok(())
            }
            ActionConfig::Sms {
                phone,
                phone_template,
                message_template,
            } => {
                if phone.is_none() && phone_template.is_none() {
                    return Err(ObserverError::InvalidActionConfig {
                        reason: "SMS action requires 'phone' or 'phone_template'".to_string(),
                    });
                }
                if message_template.is_none() {
                    return Err(ObserverError::InvalidActionConfig {
                        reason: "SMS action requires 'message_template'".to_string(),
                    });
                }
                Ok(())
            }
            ActionConfig::Push {
                device_token,
                title_template,
                body_template,
            } => {
                if device_token.is_none() {
                    return Err(ObserverError::InvalidActionConfig {
                        reason: "Push action requires 'device_token'".to_string(),
                    });
                }
                if title_template.is_none() || body_template.is_none() {
                    return Err(ObserverError::InvalidActionConfig {
                        reason: "Push action requires 'title_template' and 'body_template'"
                            .to_string(),
                    });
                }
                Ok(())
            }
            ActionConfig::Search { index, .. } => {
                if index.is_empty() {
                    return Err(ObserverError::InvalidActionConfig {
                        reason: "Search action requires 'index'".to_string(),
                    });
                }
                Ok(())
            }
            ActionConfig::Cache {
                key_pattern,
                action,
            } => {
                if key_pattern.is_empty() {
                    return Err(ObserverError::InvalidActionConfig {
                        reason: "Cache action requires 'key_pattern'".to_string(),
                    });
                }
                if action != "invalidate" && action != "refresh" {
                    return Err(ObserverError::InvalidActionConfig {
                        reason: "Cache action must be 'invalidate' or 'refresh'".to_string(),
                    });
                }
                Ok(())
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_observer_runtime_config_defaults() {
        let config = ObserverRuntimeConfig {
            channel_capacity: 0,
            max_concurrency: 0,
            overflow_policy: OverflowPolicy::Drop,
            backlog_alert_threshold: 0,
            shutdown_timeout: String::new(),
            observers: HashMap::new(),
        };

        assert_eq!(default_channel_capacity(), 1000);
        assert_eq!(default_max_concurrency(), 50);
        assert_eq!(default_backlog_threshold(), 500);
        assert_eq!(default_shutdown_timeout(), "30s");
    }

    #[test]
    fn test_retry_config_defaults() {
        let config = RetryConfig::default();
        assert_eq!(config.max_attempts, 3);
        assert_eq!(config.initial_delay_ms, 100);
        assert_eq!(config.max_delay_ms, 30000);
    }

    #[test]
    fn test_action_type_names() {
        assert_eq!(
            ActionConfig::Webhook {
                url: None,
                url_env: None,
                headers: HashMap::new(),
                body_template: None,
            }
            .action_type(),
            "webhook"
        );

        assert_eq!(
            ActionConfig::Email {
                to: None,
                to_template: None,
                subject: None,
                subject_template: None,
                body_template: None,
                reply_to: None,
            }
            .action_type(),
            "email"
        );
    }

    #[test]
    fn test_webhook_action_validation() {
        let invalid = ActionConfig::Webhook {
            url: None,
            url_env: None,
            headers: HashMap::new(),
            body_template: None,
        };

        assert!(invalid.validate().is_err());

        let valid = ActionConfig::Webhook {
            url: Some("https://example.com".to_string()),
            url_env: None,
            headers: HashMap::new(),
            body_template: Some("{}".to_string()),
        };

        assert!(valid.validate().is_ok());
    }

    #[test]
    fn test_email_action_validation() {
        let invalid = ActionConfig::Email {
            to: None,
            to_template: None,
            subject: None,
            subject_template: None,
            body_template: None,
            reply_to: None,
        };

        assert!(invalid.validate().is_err());

        let valid = ActionConfig::Email {
            to: Some("user@example.com".to_string()),
            to_template: None,
            subject: Some("Test".to_string()),
            subject_template: None,
            body_template: Some("Body".to_string()),
            reply_to: None,
        };

        assert!(valid.validate().is_ok());
    }
}
