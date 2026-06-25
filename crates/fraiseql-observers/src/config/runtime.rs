//! Observer runtime configuration: top-level config, observer definitions, actions.

use std::collections::HashMap;

use serde::{Deserialize, Serialize};

use super::{ClickHouseConfig, JobQueueConfig, PerformanceConfig, RedisConfig, TransportConfig};
use crate::error::{ObserverError, Result};

// ============================================================================
// Observer Runtime Configuration
// ============================================================================

/// Observer runtime configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ObserverRuntimeConfig {
    /// Transport configuration (postgres, nats, `in_memory`)
    #[serde(default)]
    pub transport: TransportConfig,

    /// Redis configuration (for dedup + caching)
    #[serde(default)]
    pub redis: Option<RedisConfig>,

    /// `ClickHouse` configuration (for analytics sink)
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

    /// Maximum number of entries the dead letter queue may hold.
    ///
    /// When the DLQ reaches this limit, the newest entry is dropped (the current
    /// failing action is discarded) and a warning is logged. This prevents
    /// unbounded memory growth under sustained action failures.
    ///
    /// Default: `None` (unbounded — matches previous behaviour for
    /// backwards compatibility).
    ///
    /// Recommended production value: `10_000`.
    #[serde(default)]
    pub max_dlq_size: Option<usize>,

    /// Observer definitions
    #[serde(default)]
    pub observers: HashMap<String, ObserverDefinition>,
}

impl ObserverRuntimeConfig {
    /// Validate the runtime configuration.
    ///
    /// # Errors
    ///
    /// Returns `ObserverError::InvalidConfig` if any field has an invalid value.
    pub fn validate(&self) -> crate::error::Result<()> {
        if let Some(max) = self.max_dlq_size {
            if max == 0 {
                return Err(ObserverError::InvalidConfig {
                    message: "max_dlq_size must be greater than zero".to_string(),
                });
            }
        }
        Ok(())
    }
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
#[non_exhaustive]
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

impl ObserverDefinition {
    /// Pre-compile the observer's condition string into a `ConditionAst`.
    ///
    /// Calling this at registration time surfaces DSL syntax errors early —
    /// before any events arrive — so misconfigured observers fail at startup
    /// rather than silently at runtime.
    ///
    /// Returns `None` when the observer has no condition.
    ///
    /// # Errors
    ///
    /// Returns `ObserverError::InvalidCondition` if the condition string cannot
    /// be parsed (e.g. syntax error or depth limit exceeded).
    pub fn compile_condition(
        &self,
    ) -> crate::error::Result<Option<crate::condition::ConditionAst>> {
        if let Some(condition) = &self.condition {
            let parser = crate::condition::ConditionParser::new();
            Ok(Some(parser.parse(condition)?))
        } else {
            Ok(None)
        }
    }
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
#[non_exhaustive]
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
#[non_exhaustive]
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
///
/// Marked `#[non_exhaustive]` so that new action types (e.g., `Kafka`, `Pubsub`)
/// can be added in future minor versions without breaking downstream exhaustive
/// `match` expressions.
#[non_exhaustive]
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ActionConfig {
    /// HTTP POST webhook to external URL
    Webhook {
        /// URL to POST to
        url:                Option<String>,
        /// Environment variable containing the URL
        url_env:            Option<String>,
        /// Optional HTTP headers
        #[serde(default)]
        headers:            HashMap<String, String>,
        /// Template for request body
        #[serde(default)]
        body_template:      Option<String>,
        /// HMAC signing secret *literal* (per-subscription).
        ///
        /// Mutually exclusive with `signing_secret_env`. Use this for
        /// DB-backed / admin-API-managed observers where each subscription needs
        /// its own key and the static env-var model cannot carry per-row secrets
        /// (#467). Stored in the observer's `actions` JSONB at rest and redacted
        /// in admin-API responses and logs. If set but empty, dispatch fails
        /// loud rather than sending an unsigned payload (mirrors
        /// `signing_secret_env`). Setting both `signing_secret` and
        /// `signing_secret_env` on the same action is rejected at dispatch.
        #[serde(default)]
        signing_secret:     Option<String>,
        /// Name of the environment variable holding the HMAC signing secret.
        ///
        /// When set, the outbound payload is signed with HMAC-SHA256 and the
        /// `X-FraiseQL-Signature-256: t=<unix_ts>,v1=<hex>` header is attached
        /// (Stripe-compatible, verifiable with
        /// `fraiseql-webhooks`'s `StripeVerifier`). This is the env var *name*,
        /// never the secret literal. If set but the env var is absent or empty,
        /// dispatch fails loud rather than sending an unsigned payload (#345).
        #[serde(default)]
        signing_secret_env: Option<String>,
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
    ///
    /// # Errors
    ///
    /// Returns [`ObserverError::InvalidActionConfig`] if required fields such as
    /// `url`, `webhook_url`, or `to` are absent or empty for the given action variant,
    /// or [`ObserverError::UnsupportedActionType`] for action types with no wired
    /// transport (`sms`, `push`, `search`, `cache`).
    pub fn validate(&self) -> Result<()> {
        match self {
            Self::Webhook {
                url,
                url_env,
                body_template,
                signing_secret,
                signing_secret_env,
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
                if signing_secret_env.as_ref().is_some_and(std::string::String::is_empty) {
                    return Err(ObserverError::InvalidActionConfig {
                        reason: "Webhook signing_secret_env cannot be empty (it is the env var \
                                 NAME holding the secret, or omit it for unsigned delivery)"
                            .to_string(),
                    });
                }
                if signing_secret.as_ref().is_some_and(std::string::String::is_empty) {
                    return Err(ObserverError::InvalidActionConfig {
                        reason: "Webhook signing_secret cannot be empty (set the per-subscription \
                                 HMAC secret literal, or omit it for unsigned delivery)"
                            .to_string(),
                    });
                }
                if signing_secret.is_some() && signing_secret_env.is_some() {
                    return Err(ObserverError::InvalidActionConfig {
                        reason: "Webhook action sets both 'signing_secret' and \
                                 'signing_secret_env'; set exactly one (#467)"
                            .to_string(),
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
            // Cache invalidation has a real Redis transport (#428). Validate it
            // structurally here (non-empty pattern, supported sub-action); the
            // transport's availability is enforced at dispatch, exactly like the
            // email action with no SMTP backend. `"refresh"` is not implemented
            // yet, so it fails loud at config-load.
            Self::Cache {
                key_pattern,
                action,
            } => {
                if key_pattern.is_empty() {
                    return Err(ObserverError::InvalidActionConfig {
                        reason: "Cache action requires a non-empty 'key_pattern'".to_string(),
                    });
                }
                if action != "invalidate" {
                    return Err(ObserverError::InvalidActionConfig {
                        reason: format!(
                            "Cache action {action:?} is not supported; only \"invalidate\" is \
                             implemented (#428)"
                        ),
                    });
                }
                Ok(())
            },
            // Not implemented: no real transport is wired for these action types.
            // They previously fabricated `success: true` at dispatch and sent
            // nothing (H24). Reject them at config-load time so a misconfigured
            // observer refuses to start rather than silently no-op. Real
            // transports are tracked as follow-up work.
            Self::Sms { .. } | Self::Push { .. } | Self::Search { .. } => {
                Err(ObserverError::UnsupportedActionType {
                    action_type: self.action_type().to_string(),
                })
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
    ///
    /// # Errors
    ///
    /// Returns [`ObserverError::InvalidConfig`] if `lease_duration_ms`,
    /// `health_check_interval_ms`, or `max_listeners` is 0, or if
    /// `failover_threshold_ms` is less than `health_check_interval_ms`.
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
