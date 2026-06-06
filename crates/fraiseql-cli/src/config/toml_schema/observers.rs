//! Observer/event system configuration for TOML schema.

use serde::{Deserialize, Serialize};

/// Observers/event system configuration — the **declarative compile-time**
/// shape of the `[observers]` table.
///
/// This is NOT the server's runtime schema. The same `fraiseql.toml` is fed to
/// both `fraiseql compile` (this struct) and `fraiseql-server`; the server's
/// runtime tuning lives under `[observers.runtime]` (see the server's
/// `ObserverRuntimeSettings` / issue #342). The [`runtime`](Self::runtime)
/// field is declared-and-ignored here so a shared config carrying the server's
/// `[observers.runtime]` sub-table still compiles — the compiler does not
/// validate its contents.
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(default, deny_unknown_fields)]
pub struct ObserversConfig {
    /// Enable observers system
    #[serde(default)]
    pub enabled:   bool,
    /// Backend service (redis, nats, postgresql, mysql, in-memory)
    pub backend:   String,
    /// Redis connection URL (required when backend = "redis")
    pub redis_url: Option<String>,
    /// NATS connection URL (required when backend = "nats")
    ///
    /// Example: `nats://localhost:4222`
    /// Can be overridden at runtime via the `FRAISEQL_NATS_URL` environment variable.
    pub nats_url:  Option<String>,
    /// Event handlers
    pub handlers:  Vec<EventHandler>,
    /// Server-runtime tuning sub-table (`[observers.runtime]`).
    ///
    /// Declared-and-ignored: owned and validated by `fraiseql-server`, not the
    /// compiler. Captured as an opaque value so a shared `fraiseql.toml` that
    /// carries server runtime tuning still passes `fraiseql compile` (#342).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub runtime:   Option<toml::Value>,
}

impl Default for ObserversConfig {
    fn default() -> Self {
        Self {
            enabled:   false,
            backend:   "redis".to_string(),
            redis_url: None,
            nats_url:  None,
            handlers:  vec![],
            runtime:   None,
        }
    }
}

/// Event handler configuration
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct EventHandler {
    /// Handler name
    pub name:           String,
    /// Event type to handle
    pub event:          String,
    /// Action to perform (slack, email, sms, webhook, push, etc.)
    pub action:         String,
    /// Webhook URL for webhook actions
    pub webhook_url:    Option<String>,
    /// Retry strategy
    pub retry_strategy: Option<String>,
    /// Maximum retry attempts
    pub max_retries:    Option<u32>,
    /// Handler description
    pub description:    Option<String>,
}
