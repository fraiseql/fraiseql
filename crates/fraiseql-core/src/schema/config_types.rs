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

    // ── CrudNamingConfig ─────────────────────────────────────────────────────

    #[test]
    fn crud_trinity_resolves_create() {
        let cfg = CrudNamingConfig {
            function_naming: Some(CrudNamingPreset::Trinity),
            ..Default::default()
        };
        assert_eq!(cfg.resolve("CREATE", "user"), Some("create_user".to_string()));
    }

    #[test]
    fn crud_trinity_resolves_update() {
        let cfg = CrudNamingConfig {
            function_naming: Some(CrudNamingPreset::Trinity),
            ..Default::default()
        };
        assert_eq!(cfg.resolve("UPDATE", "user"), Some("update_user".to_string()));
    }

    #[test]
    fn crud_trinity_resolves_delete() {
        let cfg = CrudNamingConfig {
            function_naming: Some(CrudNamingPreset::Trinity),
            ..Default::default()
        };
        assert_eq!(cfg.resolve("DELETE", "user"), Some("delete_user".to_string()));
    }

    #[test]
    fn crud_function_schema_prefix_applied() {
        let cfg = CrudNamingConfig {
            function_schema: Some("app".to_string()),
            function_naming: Some(CrudNamingPreset::Trinity),
            ..Default::default()
        };
        assert_eq!(cfg.resolve("CREATE", "user"), Some("app.create_user".to_string()));
    }

    #[test]
    fn crud_function_schema_prefix_applied_to_custom_template() {
        let cfg = CrudNamingConfig {
            function_schema: Some("app".to_string()),
            create_template: Some("insert_{entity}".to_string()),
            ..Default::default()
        };
        assert_eq!(cfg.resolve("CREATE", "order"), Some("app.insert_order".to_string()));
    }

    #[test]
    fn crud_custom_template_overrides_preset() {
        let cfg = CrudNamingConfig {
            function_naming: Some(CrudNamingPreset::Trinity),
            create_template: Some("insert_{entity}".to_string()),
            ..Default::default()
        };
        // Custom template wins over trinity
        assert_eq!(cfg.resolve("CREATE", "user"), Some("insert_user".to_string()));
        // Other operations fall back to trinity
        assert_eq!(cfg.resolve("UPDATE", "user"), Some("update_user".to_string()));
    }

    #[test]
    fn crud_no_config_returns_none() {
        let cfg = CrudNamingConfig::default();
        assert_eq!(cfg.resolve("CREATE", "user"), None);
        assert_eq!(cfg.resolve("UPDATE", "user"), None);
        assert_eq!(cfg.resolve("DELETE", "user"), None);
    }

    #[test]
    fn crud_unknown_operation_returns_none() {
        let cfg = CrudNamingConfig {
            function_naming: Some(CrudNamingPreset::Trinity),
            ..Default::default()
        };
        assert_eq!(cfg.resolve("UPSERT", "user"), None);
    }

    #[test]
    fn crud_operation_case_insensitive() {
        let cfg = CrudNamingConfig {
            function_naming: Some(CrudNamingPreset::Trinity),
            ..Default::default()
        };
        assert_eq!(cfg.resolve("create", "user"), Some("create_user".to_string()));
        assert_eq!(cfg.resolve("Create", "user"), Some("create_user".to_string()));
    }

    #[test]
    fn crud_entity_with_underscores() {
        let cfg = CrudNamingConfig {
            function_naming: Some(CrudNamingPreset::Trinity),
            ..Default::default()
        };
        assert_eq!(cfg.resolve("CREATE", "user_profile"), Some("create_user_profile".to_string()));
    }

    #[test]
    fn crud_serde_roundtrip_trinity() {
        let cfg = CrudNamingConfig {
            function_schema: Some("app".to_string()),
            function_naming: Some(CrudNamingPreset::Trinity),
            ..Default::default()
        };
        let json = serde_json::to_string(&cfg).unwrap();
        let restored: CrudNamingConfig = serde_json::from_str(&json).unwrap();
        assert_eq!(cfg, restored);
    }

    #[test]
    fn crud_serde_roundtrip_custom_templates() {
        let cfg = CrudNamingConfig {
            function_schema: Some("app".to_string()),
            create_template: Some("insert_{entity}".to_string()),
            update_template: Some("upsert_{entity}".to_string()),
            delete_template: Some("remove_{entity}".to_string()),
            ..Default::default()
        };
        let json = serde_json::to_string(&cfg).unwrap();
        let restored: CrudNamingConfig = serde_json::from_str(&json).unwrap();
        assert_eq!(cfg, restored);
    }

    #[test]
    fn test_naming_convention_default_is_preserve() {
        assert_eq!(NamingConvention::default(), NamingConvention::Preserve);
    }

    #[test]
    fn test_naming_convention_serde_roundtrip() {
        let camel = NamingConvention::CamelCase;
        let json = serde_json::to_string(&camel).unwrap();
        assert_eq!(json, r#""camelCase""#);
        let restored: NamingConvention = serde_json::from_str(&json).unwrap();
        assert_eq!(restored, NamingConvention::CamelCase);

        let preserve = NamingConvention::Preserve;
        let json = serde_json::to_string(&preserve).unwrap();
        assert_eq!(json, r#""preserve""#);
        let restored: NamingConvention = serde_json::from_str(&json).unwrap();
        assert_eq!(restored, NamingConvention::Preserve);
    }
}

/// Naming convention for GraphQL operation names.
///
/// Controls how operation names (queries, mutations, subscriptions) are exposed
/// in the introspection schema and resolved at execution time.
///
/// Python/TypeScript SDKs emit `snake_case` operation names (e.g., `create_dns_server`).
/// Standard GraphQL convention is `camelCase` (`createDnsServer`). This setting
/// controls whether the compiler/runtime converts names automatically.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[non_exhaustive]
pub enum NamingConvention {
    /// Preserve operation names as authored (`snake_case` from Python, etc.).
    #[default]
    #[serde(rename = "preserve")]
    Preserve,
    /// Convert operation names to camelCase for GraphQL convention.
    #[serde(rename = "camelCase")]
    CamelCase,
}

/// Built-in CRUD naming preset for automatic `sql_source` resolution.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[non_exhaustive]
pub enum CrudNamingPreset {
    /// Trinity pattern: `create_{entity}` / `update_{entity}` / `delete_{entity}`.
    #[serde(rename = "trinity")]
    Trinity,
}

/// CRUD function naming configuration, compiled from `[crud]` in `fraiseql.toml`.
///
/// When a mutation's `sql_source` is absent, the compiler resolves the PostgreSQL
/// function name using these templates and the entity name derived from the
/// mutation's `return_type`.
///
/// **Precedence** (highest first):
/// 1. Explicit `sql_source` on the mutation — always wins.
/// 2. Per-operation custom template (`create_template`, `update_template`, `delete_template`).
/// 3. Built-in preset (`function_naming = "trinity"`).
///
/// `function_schema` is applied as a prefix to the resolved name
/// (e.g. `"app"` + `"create_user"` → `"app.create_user"`).
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct CrudNamingConfig {
    /// PostgreSQL schema prefix (e.g. `"app"` → `"app.create_user"`).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub function_schema: Option<String>,
    /// Built-in naming preset (expands to fixed templates).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub function_naming: Option<CrudNamingPreset>,
    /// Custom template for CREATE mutations (e.g. `"insert_{entity}"`).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub create_template: Option<String>,
    /// Custom template for UPDATE mutations.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub update_template: Option<String>,
    /// Custom template for DELETE mutations.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub delete_template: Option<String>,
}

impl CrudNamingConfig {
    /// Resolve a PostgreSQL function name for the given CRUD operation and entity.
    ///
    /// Returns `None` if neither a custom template nor a preset can satisfy the
    /// operation. The caller is responsible for emitting a compile error in that
    /// case.
    ///
    /// `operation` is compared case-insensitively (`"CREATE"`, `"create"`, etc.).
    /// `entity` should already be in `snake_case` (e.g. `"user_profile"`).
    #[must_use]
    pub fn resolve(&self, operation: &str, entity: &str) -> Option<String> {
        let template = match operation.to_uppercase().as_str() {
            "CREATE" => self.create_template.as_deref().or_else(|| {
                self.function_naming.map(|p| match p {
                    CrudNamingPreset::Trinity => "create_{entity}",
                })
            }),
            "UPDATE" => self.update_template.as_deref().or_else(|| {
                self.function_naming.map(|p| match p {
                    CrudNamingPreset::Trinity => "update_{entity}",
                })
            }),
            "DELETE" => self.delete_template.as_deref().or_else(|| {
                self.function_naming.map(|p| match p {
                    CrudNamingPreset::Trinity => "delete_{entity}",
                })
            }),
            _ => None,
        }?;

        #[allow(clippy::literal_string_with_formatting_args)]
        // Reason: `{entity}` is a template placeholder string, not a format macro argument.
        let fn_name = template.replace("{entity}", entity);
        Some(match &self.function_schema {
            Some(schema) => format!("{schema}.{fn_name}"),
            None => fn_name,
        })
    }
}

/// Where a session variable's value comes from.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", tag = "source")]
pub enum SessionVariableSource {
    /// Pull from a JWT claim (e.g. `"sub"`, `"tenant_id"`, or a custom claim).
    Jwt {
        /// JWT claim name to look up (e.g. `"sub"`, `"tenant_id"`).
        claim: String,
    },
    /// Pull from an HTTP request header forwarded via `SecurityContext.attributes`.
    Header {
        /// HTTP header name (e.g. `"x-tenant-id"`).
        header: String,
    },
    /// A fixed literal value.
    Literal {
        /// The literal string value to inject.
        value: String,
    },
}

/// One session variable declaration.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SessionVariableMapping {
    /// The PostgreSQL setting name (e.g. `"app.tenant_id"`).
    pub name:   String,
    /// Where the value comes from.
    pub source: SessionVariableSource,
}

/// Top-level session variables configuration in the compiled schema.
///
/// When populated, the executor calls `set_config()` before each query and
/// mutation to inject per-request values (JWT claims, HTTP headers, or literals)
/// as PostgreSQL transaction-scoped settings.  SQL functions and RLS policies can
/// then read these via `current_setting('app.tenant_id', true)`.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct SessionVariablesConfig {
    /// Per-request session variable mappings.
    #[serde(default)]
    pub variables:         Vec<SessionVariableMapping>,
    /// Inject the built-in `fraiseql.started_at` timestamp before every mutation.
    #[serde(default = "session_default_true")]
    pub inject_started_at: bool,
}

const fn session_default_true() -> bool {
    true
}

/// How DELETE endpoints report success.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[non_exhaustive]
pub enum DeleteResponse {
    /// Return `204 No Content` (default).
    #[default]
    NoContent,
    /// Return `200` with the deleted entity in the body.
    Entity,
}

/// Relationship cardinality between REST resources.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[non_exhaustive]
pub enum Cardinality {
    /// Parent has many children (array embed).
    OneToMany,
    /// Child points to one parent (object embed).
    ManyToOne,
    /// Exactly one related resource (object or null).
    OneToOne,
}

/// A relationship between two schema types for REST resource embedding.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Relationship {
    /// Relationship name (used in `?select=posts` embedding syntax).
    pub name:           String,
    /// Target GraphQL type name (e.g., "Post").
    pub target_type:    String,
    /// Cardinality of the relationship.
    pub cardinality:    Cardinality,
    /// Foreign key column on the child table (e.g., `fk_author`).
    #[serde(default)]
    pub foreign_key:    String,
    /// Referenced key column on the parent table (e.g., `id`).
    #[serde(default)]
    pub referenced_key: String,
}

/// REST transport configuration (compiled from `[rest]` in `fraiseql.toml`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct RestConfig {
    /// Whether the REST transport is enabled.
    pub enabled:                 bool,
    /// Base path for REST endpoints (e.g., `"/rest/v1"`).
    pub path:                    String,
    /// Maximum rows per page (clamps `?limit=` and `?first=`).
    pub max_page_size:           u64,
    /// Default page size when no `?limit=` is specified.
    pub default_page_size:       u64,
    /// Batch size for NDJSON streaming responses.
    pub ndjson_batch_size:       u64,
    /// Maximum affected rows for bulk PATCH/DELETE.
    pub max_bulk_affected:       u64,
    /// Maximum byte length for `?filter=` JSON values.
    pub max_filter_bytes:        u64,
    /// How DELETE endpoints report success.
    pub delete_response:         DeleteResponse,
    /// Default result cache TTL in seconds (0 = no caching).
    pub default_cache_ttl:       u64,
    /// CDN `s-maxage` value in seconds for `Cache-Control` headers (`None` = omit).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cdn_max_age:             Option<u64>,
    /// Whether REST endpoints require authentication by default.
    pub require_auth:            bool,
    /// SSE heartbeat interval in seconds.
    pub sse_heartbeat_seconds:   u64,
    /// Maximum depth for resource embedding (`?select=posts(comments)`).
    pub max_embedding_depth:     u32,
    /// Whitelist of type names to expose as REST resources (empty = all).
    pub include:                 Vec<String>,
    /// Blacklist of type names to exclude from REST resources.
    pub exclude:                 Vec<String>,
    /// Whether to enable `ETag` / `If-None-Match` conditional response support.
    pub etag:                    bool,
    /// TTL in seconds for idempotency key deduplication.
    pub idempotency_ttl_seconds: u64,
}

impl Default for RestConfig {
    fn default() -> Self {
        Self {
            enabled:                 false,
            path:                    "/rest/v1".to_string(),
            max_page_size:           1_000,
            default_page_size:       100,
            ndjson_batch_size:       500,
            max_bulk_affected:       10_000,
            max_filter_bytes:        4_096,
            delete_response:         DeleteResponse::NoContent,
            default_cache_ttl:       0,
            cdn_max_age:             None,
            require_auth:            false,
            sse_heartbeat_seconds:   30,
            max_embedding_depth:     3,
            include:                 Vec::new(),
            exclude:                 Vec::new(),
            etag:                    true,
            idempotency_ttl_seconds: 300,
        }
    }
}

/// gRPC transport configuration (compiled from `[grpc]` in `fraiseql.toml`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct GrpcConfig {
    /// Whether the gRPC transport is enabled.
    pub enabled:           bool,
    /// Whitelist of type names to include (empty = all).
    pub include_types:     Vec<String>,
    /// Blacklist of type names to exclude.
    pub exclude_types:     Vec<String>,
    /// Path to the `FileDescriptorSet` binary (`.binpb`).
    pub descriptor_path:   String,
    /// Whether to enable gRPC Server Reflection.
    pub reflection:        bool,
    /// Batch size for server-streaming responses.
    pub stream_batch_size: u32,
}

impl Default for GrpcConfig {
    fn default() -> Self {
        Self {
            enabled:           false,
            include_types:     Vec::new(),
            exclude_types:     Vec::new(),
            descriptor_path:   String::new(),
            reflection:        true,
            stream_batch_size: 500,
        }
    }
}
