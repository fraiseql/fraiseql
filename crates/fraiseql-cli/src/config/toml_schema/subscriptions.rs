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
#[derive(Debug, Clone, Default, Deserialize, Serialize, schemars::JsonSchema)]
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

    /// Open WebSocket connections one principal may hold at once, within its tenant.
    /// A positive number or `"unlimited"`; omitted, 32. A further upgrade is refused
    /// with `429 SUBSCRIPTION_CONNECTIONS_EXHAUSTED` until one of them closes.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_connections_per_principal: Option<fraiseql_core::schema::ConnectionCap>,

    /// Open WebSocket connections the authenticated principals of one tenant may hold
    /// at once. A positive number or `"unlimited"`; omitted, 1000. A tenant registered
    /// with its own `max_subscription_connections` quota uses that instead.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_connections_per_tenant: Option<fraiseql_core::schema::ConnectionCap>,

    /// Open WebSocket connections with no principal, across the server. A positive
    /// number or `"unlimited"`; omitted, 1000. They never count against a tenant's
    /// or a principal's connections, whatever tenant they name.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_anonymous_connections: Option<fraiseql_core::schema::ConnectionCap>,

    /// Open WebSocket connections with no principal from one client address, read as
    /// the rate limiter reads it (`[security.rate_limiting] trust_proxy_headers` and
    /// `trusted_proxy_cidrs`). A positive number or `"unlimited"`; omitted, 32.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_anonymous_connections_per_ip: Option<fraiseql_core::schema::ConnectionCap>,
}

/// Webhook URLs invoked during subscription lifecycle events.
#[derive(Debug, Clone, Default, Deserialize, Serialize, schemars::JsonSchema)]
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
