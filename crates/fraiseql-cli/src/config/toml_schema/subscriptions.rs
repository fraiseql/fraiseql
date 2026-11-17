//! WebSocket subscription configuration for TOML schema.

use serde::{Deserialize, Serialize};

/// WebSocket subscription configuration.
///
/// ```toml
/// [subscriptions]
/// max_subscriptions_per_connection = 50
///
/// [subscriptions.hooks]
/// on_connect = "http://localhost:8001/hooks/ws-connect"
/// on_disconnect = "http://localhost:8001/hooks/ws-disconnect"
/// on_subscribe = "http://localhost:8001/hooks/ws-subscribe"
/// timeout_ms = 500
///
/// [subscriptions.capture.tb_ticket]
/// document_column = "data"
/// ```
#[derive(Debug, Clone, Default, Deserialize, Serialize)]
#[serde(default, deny_unknown_fields)]
pub struct SubscriptionsConfig {
    /// Maximum subscriptions per WebSocket connection.
    /// `None` (or omitted) means unlimited.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_subscriptions_per_connection: Option<u32>,

    /// Webhook lifecycle hooks.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub hooks: Option<SubscriptionHooksConfig>,

    /// How the capture trigger reads each table it sits on, by the table's name as a
    /// type's `subscribable_tables` spells it.
    ///
    /// ```toml
    /// [subscriptions.capture.tb_ticket]
    /// document_column = "data"
    /// ```
    #[serde(default, skip_serializing_if = "std::collections::BTreeMap::is_empty")]
    pub capture: std::collections::BTreeMap<String, fraiseql_core::schema::CaptureTableConfig>,
}

/// Webhook URLs invoked during subscription lifecycle events.
#[derive(Debug, Clone, Default, Deserialize, Serialize)]
#[serde(default, deny_unknown_fields)]
pub struct SubscriptionHooksConfig {
    /// URL to POST on WebSocket `connection_init` (fail-closed).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub on_connect: Option<String>,

    /// URL to POST on WebSocket disconnect (fire-and-forget).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub on_disconnect: Option<String>,

    /// URL to POST before a subscription is registered (fail-closed).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub on_subscribe: Option<String>,

    /// Timeout in milliseconds for fail-closed hooks (default: 500).
    #[serde(default = "default_hook_timeout_ms")]
    pub timeout_ms: u64,
}

fn default_hook_timeout_ms() -> u64 {
    500
}
