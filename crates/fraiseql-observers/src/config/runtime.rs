//! Observer runtime configuration: top-level config, observer definitions, actions.

use std::collections::HashMap;

use serde::{Deserialize, Serialize};

use crate::error::{ObserverError, Result};

use super::{
    ClickHouseConfig, JobQueueConfig, PerformanceConfig, RedisConfig, TransportConfig,
};

// ============================================================================
// Observer Runtime Configuration
// ============================================================================

/// Observer runtime configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ObserverRuntimeConfig {
    /// Transport configuration (postgres, nats, in_memory)
    #[serde(default)]
    pub transport: TransportConfig,

    /// Redis configuration (for dedup + caching)
    #[serde(default)]
    pub redis: Option<RedisConfig>,

    /// ClickHouse configuration (for analytics sink)
    #[serde(default)]
    pub clickhouse: Option<ClickHouseConfig>,

    /// Job queue configuration (for async action execution)
    #[serde(default)]
    pub job_queue: Option<JobQueueConfig>,

    /// Performance optimization features
    #[serde(default)]
    pub performance: PerformanceConfig,

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

pub(super) const fn default_channel_capacity() -> usize {
    1000
}

pub(super) const fn default_max_concurrency() -> usize {
    50
}

pub(super) const fn default_backlog_threshold() -> usize {
    500
}

pub(super) fn default_shutdown_timeout() -> String {
    "30s".to_string()
}

// ============================================================================
// Overflow Policy
// ============================================================================

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

// ============================================================================
// Observer Definition
// ============================================================================

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

// ============================================================================
// Retry Configuration
// ============================================================================

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
            max_attempts:     default_max_attempts(),
            initial_delay_ms: default_initial_delay(),
            max_delay_ms:     default_max_delay(),
            backoff_strategy: BackoffStrategy::default(),
        }
    }
}

const fn default_max_attempts() -> u32 {
    3
}

const fn default_initial_delay() -> u64 {
    100
}

const fn default_max_delay() -> u64 {
    30000
}

/// Backoff strategy for retries
#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BackoffStrategy {
    /// Exponential backoff (2^attempt * `initial_delay`)
    #[default]
    Exponential,
    /// Linear backoff (attempt * `initial_delay`)
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

// ============================================================================
// Action Configuration
// ============================================================================

/// Action configuration (tagged union)
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ActionConfig {
    /// HTTP POST webhook to external URL
    Webhook {
        /// URL to POST to
        url:           Option<String>,
        /// Environment variable containing the URL
        url_env:       Option<String>,
        /// Optional HTTP headers
        #[serde(default)]
        headers:       HashMap<String, String>,
        /// Template for request body
        #[serde(default)]
        body_template: Option<String>,
    },

    /// Send message to Slack webhook
    Slack {
        /// Slack webhook URL
        webhook_url:      Option<String>,
        /// Environment variable containing webhook URL
        webhook_url_env:  Option<String>,
        /// Channel to send to (if not in webhook URL)
        #[serde(default)]
        channel:          Option<String>,
        /// Message template
        #[serde(default)]
        message_template: Option<String>,
    },

    /// Send email via SMTP
    Email {
        /// Recipient email address
        to:               Option<String>,
        /// Template for recipient (e.g., "{{ data.email }}")
        to_template:      Option<String>,
        /// Email subject
        subject:          Option<String>,
        /// Subject template
        subject_template: Option<String>,
        /// Email body template
        body_template:    Option<String>,
        /// Reply-to address
        #[serde(default)]
        reply_to:         Option<String>,
    },

    /// Send SMS (stub for, full implementation later)
    Sms {
        /// Phone number to send to
        phone:            Option<String>,
        /// Template for phone number
        phone_template:   Option<String>,
        /// Message template
        message_template: Option<String>,
    },

    /// Send push notification (stub for)
    Push {
        /// Device token
        device_token:   Option<String>,
        /// Title template
        title_template: Option<String>,
        /// Body template
        body_template:  Option<String>,
    },

    /// Update search index (stub for)
    Search {
        /// Index name
        index:       String,
        /// Document ID template
        id_template: Option<String>,
    },

    /// Invalidate cache (stub for)
    Cache {
        /// Cache key pattern
        key_pattern: String,
        /// Action: "invalidate" or "refresh"
        action:      String,
    },
}

impl ActionConfig {
    /// Get the action type name
    #[must_use]
    pub const fn action_type(&self) -> &'static str {
        match self {
            Self::Webhook { .. } => "webhook",
            Self::Slack { .. } => "slack",
            Self::Email { .. } => "email",
            Self::Sms { .. } => "sms",
            Self::Push { .. } => "push",
            Self::Search { .. } => "search",
            Self::Cache { .. } => "cache",
        }
    }

    /// Validate the action configuration
    pub fn validate(&self) -> Result<()> {
        match self {
            Self::Webhook {
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
                if body_template.as_ref().is_some_and(std::string::String::is_empty) {
                    return Err(ObserverError::InvalidActionConfig {
                        reason: "Webhook body_template cannot be empty".to_string(),
                    });
                }
                Ok(())
            },
            Self::Slack {
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
            },
            Self::Email {
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
            },
            Self::Sms {
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
            },
            Self::Push {
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
            },
            Self::Search { index, .. } => {
                if index.is_empty() {
                    return Err(ObserverError::InvalidActionConfig {
                        reason: "Search action requires 'index'".to_string(),
                    });
                }
                Ok(())
            },
            Self::Cache {
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
            },
        }
    }
}

// ============================================================================
// Multi-Listener Configuration
// ============================================================================

/// Multi-listener configuration for high-availability setup
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MultiListenerConfig {
    /// Enable multi-listener coordination (default: false)
    #[serde(default)]
    pub enabled: bool,

    /// Unique listener ID for this instance (default: random UUID)
    #[serde(default = "default_listener_id")]
    pub listener_id: String,

    /// Lease duration in milliseconds (default: 30000)
    #[serde(default = "default_lease_duration_ms")]
    pub lease_duration_ms: u64,

    /// Health check interval in milliseconds (default: 5000)
    #[serde(default = "default_health_check_interval_ms")]
    pub health_check_interval_ms: u64,

    /// Failover threshold in milliseconds (default: 60000)
    #[serde(default = "default_failover_threshold_ms")]
    pub failover_threshold_ms: u64,

    /// Maximum listeners in group (default: 10)
    #[serde(default = "default_max_listeners")]
    pub max_listeners: usize,
}

fn default_listener_id() -> String {
    format!("listener-{}", uuid::Uuid::new_v4())
}

const fn default_lease_duration_ms() -> u64 {
    30000
}

const fn default_health_check_interval_ms() -> u64 {
    5000
}

const fn default_failover_threshold_ms() -> u64 {
    60000
}

const fn default_max_listeners() -> usize {
    10
}

impl Default for MultiListenerConfig {
    fn default() -> Self {
        Self {
            enabled:                  false,
            listener_id:              default_listener_id(),
            lease_duration_ms:        default_lease_duration_ms(),
            health_check_interval_ms: default_health_check_interval_ms(),
            failover_threshold_ms:    default_failover_threshold_ms(),
            max_listeners:            default_max_listeners(),
        }
    }
}

impl MultiListenerConfig {
    /// Create a new multi-listener config with default values
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Validate the configuration
    pub fn validate(&self) -> Result<()> {
        if self.lease_duration_ms == 0 {
            return Err(ObserverError::InvalidConfig {
                message: "lease_duration_ms must be > 0".to_string(),
            });
        }

        if self.health_check_interval_ms == 0 {
            return Err(ObserverError::InvalidConfig {
                message: "health_check_interval_ms must be > 0".to_string(),
            });
        }

        if self.failover_threshold_ms < self.health_check_interval_ms {
            return Err(ObserverError::InvalidConfig {
                message: "failover_threshold_ms must be >= health_check_interval_ms".to_string(),
            });
        }

        if self.max_listeners == 0 {
            return Err(ObserverError::InvalidConfig {
                message: "max_listeners must be > 0".to_string(),
            });
        }

        Ok(())
    }

    /// Enable multi-listener coordination
    #[must_use]
    pub const fn enable(mut self) -> Self {
        self.enabled = true;
        self
    }

    /// Set listener ID
    #[must_use]
    pub fn with_listener_id(mut self, listener_id: String) -> Self {
        self.listener_id = listener_id;
        self
    }

    /// Set lease duration
    #[must_use]
    pub const fn with_lease_duration_ms(mut self, lease_duration_ms: u64) -> Self {
        self.lease_duration_ms = lease_duration_ms;
        self
    }
}
