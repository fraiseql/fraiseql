//! Typed configuration structures for compiled schema.
//!
//! These types replace untyped `serde_json::Value` fields in `CompiledSchema`
//! to enable compile-time validation, IDE autocompletion, and clearer domain modeling.

use serde::{Deserialize, Serialize};

/// Federation configuration for Apollo Federation v2 support.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct FederationConfig {
    /// Enable Apollo federation.
    #[serde(default)]
    pub enabled:         bool,
    /// Federation specification version (e.g., "v2").
    #[serde(default)]
    pub version:         Option<String>,
    /// Subgraph service name (used in Apollo Studio).
    #[serde(default)]
    pub service_name:    Option<String>,
    /// Subgraph SDL URL (exposed at `/__subgraph_schema`).
    #[serde(default)]
    pub schema_url:      Option<String>,
    /// Federated entities defined in this subgraph.
    #[serde(default)]
    pub entities:        Vec<FederationEntity>,
    /// Circuit breaker configuration for federation fan-out requests.
    #[serde(default)]
    pub circuit_breaker: Option<CircuitBreakerConfig>,
}

/// Federated entity configuration.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FederationEntity {
    /// Entity type name (e.g., "User", "Product").
    pub name:       String,
    /// Key fields that uniquely identify this entity.
    pub key_fields: Vec<String>,
}

/// Circuit breaker configuration for federation entity resolution.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CircuitBreakerConfig {
    /// Enable circuit breaker protection.
    #[serde(default)]
    pub enabled:               bool,
    /// Consecutive failures required to trip the circuit open.
    #[serde(default = "default_failure_threshold")]
    pub failure_threshold:     u32,
    /// Seconds to hold the circuit open before transitioning to `HalfOpen`.
    #[serde(default = "default_recovery_timeout")]
    pub recovery_timeout_secs: u64,
    /// Consecutive successes in `HalfOpen` required to close the circuit.
    #[serde(default = "default_success_threshold")]
    pub success_threshold:     u32,
    /// Per-entity overrides (e.g., Product has different thresholds than User).
    #[serde(default)]
    pub per_entity:            Vec<EntityCircuitBreakerOverride>,
}

const fn default_failure_threshold() -> u32 {
    5
}

const fn default_recovery_timeout() -> u64 {
    30
}

const fn default_success_threshold() -> u32 {
    2
}

impl Default for CircuitBreakerConfig {
    fn default() -> Self {
        Self {
            enabled:               false,
            failure_threshold:     default_failure_threshold(),
            recovery_timeout_secs: default_recovery_timeout(),
            success_threshold:     default_success_threshold(),
            per_entity:            Vec::new(),
        }
    }
}

/// Per-entity circuit breaker configuration override.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EntityCircuitBreakerOverride {
    /// Entity type name to apply override to.
    pub entity:            String,
    /// Custom failure threshold (optional).
    pub failure_threshold: Option<u32>,
    /// Custom recovery timeout (optional).
    pub recovery_timeout:  Option<u64>,
    /// Custom success threshold (optional).
    pub success_threshold: Option<u32>,
}

/// Security configuration compiled from fraiseql.toml.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct CompiledSecurityConfig {
    /// Default authorization policy.
    pub default_policy: Option<String>,
    /// Custom authorization rules.
    #[serde(default)]
    pub rules:          Vec<AuthorizationRule>,
    /// Authorization policies (RBAC/ABAC).
    #[serde(default)]
    pub policies:       Vec<AuthorizationPolicy>,
    /// Field-level authorization.
    #[serde(default)]
    pub field_auth:     Vec<FieldAuthRule>,
    /// Enterprise security features.
    #[serde(default)]
    pub enterprise:     EnterpriseSecurityConfig,
}

/// Custom authorization rule.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AuthorizationRule {
    /// Rule name.
    pub name:              String,
    /// Rule expression.
    pub rule:              String,
    /// Optional description.
    pub description:       Option<String>,
    /// Whether result can be cached.
    #[serde(default)]
    pub cacheable:         bool,
    /// Cache TTL in seconds.
    pub cache_ttl_seconds: Option<u32>,
}

/// Authorization policy (RBAC/ABAC).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AuthorizationPolicy {
    /// Policy name.
    pub name:              String,
    /// Policy type: RBAC, ABAC, CUSTOM, HYBRID.
    #[serde(rename = "type")]
    pub policy_type:       String,
    /// Optional rule expression.
    pub rule:              Option<String>,
    /// Roles this policy applies to.
    #[serde(default)]
    pub roles:             Vec<String>,
    /// Combination strategy: ANY, ALL, EXACTLY.
    pub strategy:          Option<String>,
    /// Attributes for ABAC.
    #[serde(default)]
    pub attributes:        Vec<String>,
    /// Optional description.
    pub description:       Option<String>,
    /// Cache TTL in seconds.
    pub cache_ttl_seconds: Option<u32>,
}

/// Field-level authorization rule.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FieldAuthRule {
    /// Type name.
    pub type_name:  String,
    /// Field name.
    pub field_name: String,
    /// Policy to enforce.
    pub policy:     String,
}

/// Enterprise security features.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EnterpriseSecurityConfig {
    /// Enable rate limiting.
    #[serde(default = "default_true")]
    pub rate_limiting_enabled:        bool,
    /// Max requests per window.
    #[serde(default = "default_auth_max_requests")]
    pub auth_endpoint_max_requests:   u32,
    /// Rate limit window in seconds.
    #[serde(default = "default_auth_window")]
    pub auth_endpoint_window_seconds: u64,
    /// Enable audit logging.
    #[serde(default = "default_true")]
    pub audit_logging_enabled:        bool,
    /// Audit log backend: postgresql, file, syslog.
    #[serde(default = "default_audit_backend")]
    pub audit_log_backend:            String,
    /// Audit log retention in days.
    #[serde(default = "default_audit_retention")]
    pub audit_retention_days:         u32,
    /// Enable error sanitization.
    #[serde(default = "default_true")]
    pub error_sanitization:           bool,
    /// Hide implementation details.
    #[serde(default = "default_true")]
    pub hide_implementation_details:  bool,
    /// Enable constant-time comparison.
    #[serde(default = "default_true")]
    pub constant_time_comparison:     bool,
    /// Enable PKCE for OAuth.
    #[serde(default = "default_true")]
    pub pkce_enabled:                 bool,
}

const fn default_true() -> bool {
    true
}

const fn default_auth_max_requests() -> u32 {
    100
}

const fn default_auth_window() -> u64 {
    60
}

fn default_audit_backend() -> String {
    "postgresql".to_string()
}

const fn default_audit_retention() -> u32 {
    365
}

impl Default for EnterpriseSecurityConfig {
    fn default() -> Self {
        Self {
            rate_limiting_enabled:        default_true(),
            auth_endpoint_max_requests:   default_auth_max_requests(),
            auth_endpoint_window_seconds: default_auth_window(),
            audit_logging_enabled:        default_true(),
            audit_log_backend:            default_audit_backend(),
            audit_retention_days:         default_audit_retention(),
            error_sanitization:           default_true(),
            hide_implementation_details:  default_true(),
            constant_time_comparison:     default_true(),
            pkce_enabled:                 default_true(),
        }
    }
}

/// Observers/event system configuration.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ObserversConfig {
    /// Enable observers system.
    #[serde(default)]
    pub enabled:   bool,
    /// Backend service: redis, nats, postgresql, mysql, in-memory.
    #[serde(default = "default_backend")]
    pub backend:   String,
    /// Redis connection URL.
    pub redis_url: Option<String>,
    /// NATS connection URL.
    pub nats_url:  Option<String>,
    /// Event handlers.
    #[serde(default)]
    pub handlers:  Vec<EventHandler>,
}

fn default_backend() -> String {
    "redis".to_string()
}

impl Default for ObserversConfig {
    fn default() -> Self {
        Self {
            enabled:   false,
            backend:   default_backend(),
            redis_url: None,
            nats_url:  None,
            handlers:  Vec::new(),
        }
    }
}

/// Event handler configuration.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EventHandler {
    /// Handler name.
    pub name:             String,
    /// Event type (e.g., "User.created", "Order.updated").
    pub event:            String,
    /// Action: slack, email, sms, webhook, push, log.
    pub action:           String,
    /// Webhook URL (for webhook action).
    pub webhook_url:      Option<String>,
    /// Slack channel (for slack action).
    pub slack_channel:    Option<String>,
    /// Email recipients (for email action).
    pub email_recipients: Option<Vec<String>>,
    /// Phone numbers (for sms action).
    pub phone_numbers:    Option<Vec<String>>,
    /// Push notification target (for push action).
    pub push_target:      Option<String>,
    /// Rate limit in seconds between notifications.
    pub rate_limit:       Option<u32>,
}

/// Debug/development configuration (compiled from `[debug]` in `fraiseql.toml`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct DebugConfig {
    /// Master switch — all debug features require this to be `true`.
    pub enabled:          bool,
    /// When `true`, the explain endpoint also runs `EXPLAIN` against the database.
    pub database_explain: bool,
    /// When `true`, the explain endpoint includes the generated SQL in the response.
    pub expose_sql:       bool,
}

impl Default for DebugConfig {
    fn default() -> Self {
        Self {
            enabled:          false,
            database_explain: false,
            expose_sql:       true,
        }
    }
}

/// Query validation limits (compiled from `[validation]` in `fraiseql.toml`).
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct ValidationConfig {
    /// Maximum allowed query nesting depth.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_query_depth:      Option<u32>,
    /// Maximum allowed query complexity score.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_query_complexity: Option<u32>,
}

/// MCP (Model Context Protocol) server configuration (compiled from `[mcp]` in `fraiseql.toml`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct McpConfig {
    /// Whether MCP is enabled.
    pub enabled:      bool,
    /// Transport mode: "http", "stdio", or "both".
    pub transport:    String,
    /// HTTP path for MCP endpoint (e.g., "/mcp").
    pub path:         String,
    /// Require authentication for MCP requests.
    pub require_auth: bool,
    /// Whitelist of query/mutation names to expose (empty = all).
    pub include:      Vec<String>,
    /// Blacklist of query/mutation names to hide.
    pub exclude:      Vec<String>,
}

impl Default for McpConfig {
    fn default() -> Self {
        Self {
            enabled:      false,
            transport:    "http".to_string(),
            path:         "/mcp".to_string(),
            require_auth: true,
            include:      Vec::new(),
            exclude:      Vec::new(),
        }
    }
}

/// `WebSocket` subscription configuration (compiled from `[subscriptions]` in `fraiseql.toml`).
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct SubscriptionsConfig {
    /// Maximum subscriptions per `WebSocket` connection (`None` = unlimited).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_subscriptions_per_connection: Option<u32>,
    /// Webhook lifecycle hooks.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub hooks: Option<SubscriptionHooksConfig>,
}

/// Webhook URLs invoked during subscription lifecycle events.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct SubscriptionHooksConfig {
    /// URL to POST on `WebSocket` `connection_init` (fail-closed).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub on_connect:     Option<String>,
    /// URL to POST on `WebSocket` disconnect (fire-and-forget).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub on_disconnect:  Option<String>,
    /// URL to POST before a subscription is registered (fail-closed).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub on_subscribe:   Option<String>,
    /// URL to POST when a subscription is removed (fire-and-forget).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub on_unsubscribe: Option<String>,
    /// Timeout in milliseconds for fail-closed hooks (default: 500).
    pub timeout_ms:     u64,
}

impl Default for SubscriptionHooksConfig {
    fn default() -> Self {
        Self {
            on_connect:     None,
            on_disconnect:  None,
            on_subscribe:   None,
            on_unsubscribe: None,
            timeout_ms:     500,
        }
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)] // Reason: test code, panics are acceptable

    use super::*;

    #[test]
    fn test_federation_config_default() {
        let config = FederationConfig::default();
        assert!(!config.enabled);
        assert_eq!(config.version, None);
        assert!(config.entities.is_empty());
        assert!(config.circuit_breaker.is_none());
    }

    #[test]
    fn test_circuit_breaker_config_default() {
        let config = CircuitBreakerConfig::default();
        assert!(!config.enabled);
        assert_eq!(config.failure_threshold, 5);
        assert_eq!(config.recovery_timeout_secs, 30);
        assert_eq!(config.success_threshold, 2);
        assert!(config.per_entity.is_empty());
    }

    #[test]
    fn test_security_config_default() {
        let config = CompiledSecurityConfig::default();
        assert!(config.default_policy.is_none());
        assert!(config.rules.is_empty());
        assert!(config.policies.is_empty());
        assert!(config.field_auth.is_empty());
        assert!(config.enterprise.rate_limiting_enabled);
    }

    #[test]
    fn test_observers_config_default() {
        let config = ObserversConfig::default();
        assert!(!config.enabled);
        assert_eq!(config.backend, "redis");
        assert!(config.handlers.is_empty());
    }

    #[test]
    fn test_federation_config_serde() {
        let json = r#"{
            "enabled": true,
            "version": "v2",
            "entities": [{"name": "User", "key_fields": ["id"]}],
            "circuit_breaker": {
                "enabled": true,
                "failure_threshold": 3,
                "recovery_timeout_secs": 15,
                "success_threshold": 1
            }
        }"#;

        let config: FederationConfig = serde_json::from_str(json).unwrap();
        assert!(config.enabled);
        assert_eq!(config.version, Some("v2".to_string()));
        assert_eq!(config.entities.len(), 1);
        assert_eq!(config.entities[0].name, "User");

        let cb = config.circuit_breaker.unwrap();
        assert!(cb.enabled);
        assert_eq!(cb.failure_threshold, 3);
    }

    #[test]
    fn test_entity_override() {
        let config = CircuitBreakerConfig {
            per_entity: vec![EntityCircuitBreakerOverride {
                entity:            "Product".to_string(),
                failure_threshold: Some(2),
                recovery_timeout:  None,
                success_threshold: None,
            }],
            ..Default::default()
        };

        assert_eq!(config.per_entity[0].entity, "Product");
        assert_eq!(config.per_entity[0].failure_threshold, Some(2));
    }

    #[test]
    fn test_roundtrip_serialization() {
        let config = FederationConfig {
            enabled:         true,
            version:         Some("v2".to_string()),
            service_name:    Some("my-service".to_string()),
            schema_url:      None,
            entities:        vec![FederationEntity {
                name:       "User".to_string(),
                key_fields: vec!["id".to_string()],
            }],
            circuit_breaker: Some(CircuitBreakerConfig::default()),
        };

        let json = serde_json::to_string(&config).unwrap();
        let restored: FederationConfig = serde_json::from_str(&json).unwrap();

        assert_eq!(config, restored);
    }
}
