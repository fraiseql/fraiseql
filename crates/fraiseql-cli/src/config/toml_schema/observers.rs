//! Observer/event system configuration for TOML schema.

use serde::{Deserialize, Serialize};

/// Observers/event system configuration
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(default, deny_unknown_fields)]
pub struct ObserversConfig {
    /// Enable observers system
    #[serde(default)]
    pub enabled: bool,
    /// Backend service (redis, nats, postgresql, mysql, in-memory)
    pub backend: String,
    /// Redis connection URL (required when backend = "redis")
    pub redis_url: Option<String>,
    /// NATS connection URL (required when backend = "nats")
    ///
    /// Example: `nats://localhost:4222`
    /// Can be overridden at runtime via the `FRAISEQL_NATS_URL` environment variable.
    pub nats_url: Option<String>,
    /// Event handlers
    pub handlers: Vec<EventHandler>,
}

impl Default for ObserversConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            backend: "redis".to_string(),
            redis_url: None,
            nats_url: None,
            handlers: vec![],
        }
    }
}

/// Event handler configuration
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct EventHandler {
    /// Handler name
    pub name: String,
    /// Event type to handle
    pub event: String,
    /// Action to perform (slack, email, sms, webhook, push, etc.)
    pub action: String,
    /// Webhook URL for webhook actions
    pub webhook_url: Option<String>,
    /// Retry strategy
    pub retry_strategy: Option<String>,
    /// Maximum retry attempts
    pub max_retries: Option<u32>,
    /// Handler description
    pub description: Option<String>,
}
