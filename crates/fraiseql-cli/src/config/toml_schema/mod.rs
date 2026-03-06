//! Complete TOML schema configuration supporting types, queries, mutations, federation, observers,
//! caching
//!
//! This module extends FraiseQLConfig to support the full TOML-based schema definition.

pub mod domain;
pub mod federation;
pub mod security;

pub use domain::{Domain, DomainDiscovery, ResolvedIncludes, SchemaIncludes};
pub use federation::{
    FederationCircuitBreakerConfig, FederationConfig, FederationEntity,
    PerDatabaseCircuitBreakerOverride,
};
pub use security::{
    ApiKeySecurityConfig, AuthorizationPolicy, AuthorizationRule, CodeChallengeMethod,
    EncryptionAlgorithm, EnterpriseSecurityConfig, ErrorSanitizationTomlConfig, FieldAuthRule,
    KeySource, OidcClientConfig, PkceConfig, RateLimitingSecurityConfig, SecuritySettings,
    StateEncryptionConfig, StaticApiKeyEntry, TokenRevocationSecurityConfig,
    TrustedDocumentMode, TrustedDocumentsConfig,
};

use std::collections::BTreeMap;

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

use super::runtime::{DatabaseRuntimeConfig, ServerRuntimeConfig};
use super::expand_env_vars;

/// Global defaults for list-query auto-params.
///
/// Applied when a per-query `auto_params` does not specify a given flag.
/// Relay queries and single-item queries are never affected.
///
/// ```toml
/// [query_defaults]
/// where    = true
/// order_by = true
/// limit    = false  # e.g. Relay-first project
/// offset   = false
/// ```
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(default, deny_unknown_fields)]
pub struct QueryDefaults {
    /// Enable automatic `where` filter parameter (default: true)
    #[serde(rename = "where", default = "default_true")]
    pub where_clause: bool,
    /// Enable automatic `order_by` parameter (default: true)
    #[serde(default = "default_true")]
    pub order_by: bool,
    /// Enable automatic `limit` parameter (default: true)
    #[serde(default = "default_true")]
    pub limit: bool,
    /// Enable automatic `offset` parameter (default: true)
    #[serde(default = "default_true")]
    pub offset: bool,
}

impl Default for QueryDefaults {
    fn default() -> Self {
        Self { where_clause: true, order_by: true, limit: true, offset: true }
    }
}

fn default_true() -> bool {
    true
}

/// Complete TOML schema configuration
#[derive(Debug, Clone, Default, Deserialize, Serialize)]
#[serde(default, deny_unknown_fields)]
pub struct TomlSchema {
    /// Schema metadata
    #[serde(rename = "schema")]
    pub schema: SchemaMetadata,

    /// Database connection pool configuration (optional — all fields have defaults).
    ///
    /// Supports `${VAR}` environment variable interpolation in the `url` field.
    #[serde(rename = "database")]
    pub database: DatabaseRuntimeConfig,

    /// HTTP server runtime configuration (optional — all fields have defaults).
    ///
    /// CLI flags (`--port`, `--bind`) take precedence over these settings.
    #[serde(rename = "server")]
    pub server: ServerRuntimeConfig,

    /// Type definitions
    #[serde(rename = "types")]
    pub types: BTreeMap<String, TypeDefinition>,

    /// Query definitions
    #[serde(rename = "queries")]
    pub queries: BTreeMap<String, QueryDefinition>,

    /// Mutation definitions
    #[serde(rename = "mutations")]
    pub mutations: BTreeMap<String, MutationDefinition>,

    /// Federation configuration
    #[serde(rename = "federation")]
    pub federation: FederationConfig,

    /// Security configuration
    #[serde(rename = "security")]
    pub security: SecuritySettings,

    /// Observers/event system configuration
    #[serde(rename = "observers")]
    pub observers: ObserversConfig,

    /// Result caching configuration
    #[serde(rename = "caching")]
    pub caching: CachingConfig,

    /// Analytics configuration
    #[serde(rename = "analytics")]
    pub analytics: AnalyticsConfig,

    /// Observability configuration
    #[serde(rename = "observability")]
    pub observability: ObservabilityConfig,

    /// Schema includes configuration for multi-file composition
    #[serde(default)]
    pub includes: SchemaIncludes,

    /// Domain discovery configuration for domain-based organization
    #[serde(default)]
    pub domain_discovery: DomainDiscovery,

    /// Global defaults for list-query auto-params.
    ///
    /// Provides project-wide defaults for `where`, `order_by`, `limit`, and `offset`
    /// parameters on list queries. Per-query `auto_params` overrides are partial —
    /// only the specified flags override the defaults. Relay queries and single-item
    /// queries are never affected.
    #[serde(default)]
    pub query_defaults: QueryDefaults,

    /// OAuth2 client identity for server-side PKCE flows.
    ///
    /// Required when `[security.pkce] enabled = true`.
    /// Holds the OIDC provider discovery URL, client_id, and a reference to
    /// the env var containing the client secret. Never stores the secret itself.
    #[serde(default)]
    pub auth: Option<OidcClientConfig>,

    /// WebSocket subscription configuration (hooks, limits).
    #[serde(default)]
    pub subscriptions: SubscriptionsConfig,

    /// Query validation limits (depth, complexity).
    #[serde(default)]
    pub validation: ValidationConfig,

    /// Debug/development settings (database EXPLAIN, SQL exposure).
    #[serde(default)]
    pub debug: DebugConfig,

    /// MCP (Model Context Protocol) server configuration.
    #[serde(default)]
    pub mcp: McpConfig,
}

/// MCP (Model Context Protocol) server configuration.
///
/// Enables AI/LLM tools to interact with FraiseQL queries and mutations
/// through the standardized Model Context Protocol.
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(default, deny_unknown_fields)]
pub struct McpConfig {
    /// Enable MCP server endpoint.
    pub enabled:      bool,
    /// Transport mode: "http", "stdio", or "both".
    pub transport:    String,
    /// HTTP path for MCP endpoint (e.g., "/mcp").
    pub path:         String,
    /// Require authentication for MCP requests.
    pub require_auth: bool,
    /// Whitelist of query/mutation names to expose (empty = all).
    #[serde(default)]
    pub include:      Vec<String>,
    /// Blacklist of query/mutation names to hide.
    #[serde(default)]
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

/// Schema metadata
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(default, deny_unknown_fields)]
pub struct SchemaMetadata {
    /// Schema name
    pub name:            String,
    /// Schema version
    pub version:         String,
    /// Optional schema description
    pub description:     Option<String>,
    /// Target database (postgresql, mysql, sqlite, sqlserver)
    pub database_target: String,
}

impl Default for SchemaMetadata {
    fn default() -> Self {
        Self {
            name:            "myapp".to_string(),
            version:         "1.0.0".to_string(),
            description:     None,
            database_target: "postgresql".to_string(),
        }
    }
}

/// Type definition in TOML
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(default, deny_unknown_fields)]
pub struct TypeDefinition {
    /// SQL source table or view
    pub sql_source:  String,
    /// Human-readable type description
    pub description: Option<String>,
    /// Field definitions
    pub fields:      BTreeMap<String, FieldDefinition>,
}

impl Default for TypeDefinition {
    fn default() -> Self {
        Self {
            sql_source:  "v_entity".to_string(),
            description: None,
            fields:      BTreeMap::new(),
        }
    }
}

/// Field definition
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct FieldDefinition {
    /// GraphQL field type (ID, String, Int, Boolean, DateTime, etc.)
    #[serde(rename = "type")]
    pub field_type:  String,
    /// Whether field can be null
    #[serde(default)]
    pub nullable:    bool,
    /// Field description
    pub description: Option<String>,
}

/// Query definition in TOML
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(default, deny_unknown_fields)]
pub struct QueryDefinition {
    /// Return type name
    pub return_type:  String,
    /// Whether query returns an array
    #[serde(default)]
    pub return_array: bool,
    /// SQL source for the query
    pub sql_source:   String,
    /// Query description
    pub description:  Option<String>,
    /// Query arguments
    pub args:         Vec<ArgumentDefinition>,
}

impl Default for QueryDefinition {
    fn default() -> Self {
        Self {
            return_type:  "String".to_string(),
            return_array: false,
            sql_source:   "v_entity".to_string(),
            description:  None,
            args:         vec![],
        }
    }
}

/// Mutation definition in TOML
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(default, deny_unknown_fields)]
pub struct MutationDefinition {
    /// Return type name
    pub return_type: String,
    /// SQL function or procedure source
    pub sql_source:  String,
    /// Operation type (CREATE, UPDATE, DELETE)
    pub operation:   String,
    /// Mutation description
    pub description: Option<String>,
    /// Mutation arguments
    pub args:        Vec<ArgumentDefinition>,
}

impl Default for MutationDefinition {
    fn default() -> Self {
        Self {
            return_type: "String".to_string(),
            sql_source:  "fn_operation".to_string(),
            operation:   "CREATE".to_string(),
            description: None,
            args:        vec![],
        }
    }
}

/// Argument definition
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ArgumentDefinition {
    /// Argument name
    pub name:        String,
    /// Argument type
    #[serde(rename = "type")]
    pub arg_type:    String,
    /// Whether argument is required
    #[serde(default)]
    pub required:    bool,
    /// Default value if not provided
    pub default:     Option<serde_json::Value>,
    /// Argument description
    pub description: Option<String>,
}

/// Observers/event system configuration
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
}

impl Default for ObserversConfig {
    fn default() -> Self {
        Self {
            enabled:   false,
            backend:   "redis".to_string(),
            redis_url: None,
            nats_url:  None,
            handlers:  vec![],
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

/// Caching configuration
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(default, deny_unknown_fields)]
pub struct CachingConfig {
    /// Enable caching
    #[serde(default)]
    pub enabled:   bool,
    /// Cache backend (redis, memory, postgresql)
    pub backend:   String,
    /// Redis connection URL
    pub redis_url: Option<String>,
    /// Cache invalidation rules
    pub rules:     Vec<CacheRule>,
}

impl Default for CachingConfig {
    fn default() -> Self {
        Self {
            enabled:   false,
            backend:   "redis".to_string(),
            redis_url: None,
            rules:     vec![],
        }
    }
}

/// Cache invalidation rule
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct CacheRule {
    /// Query pattern to cache
    pub query:                 String,
    /// Time-to-live in seconds
    pub ttl_seconds:           u32,
    /// Events that trigger cache invalidation
    pub invalidation_triggers: Vec<String>,
}

/// Analytics configuration
#[derive(Debug, Clone, Default, Deserialize, Serialize)]
#[serde(default, deny_unknown_fields)]
pub struct AnalyticsConfig {
    /// Enable analytics
    #[serde(default)]
    pub enabled: bool,
    /// Analytics queries
    pub queries: Vec<AnalyticsQuery>,
}

/// Analytics query definition
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct AnalyticsQuery {
    /// Query name
    pub name:        String,
    /// SQL source for the query
    pub sql_source:  String,
    /// Query description
    pub description: Option<String>,
}

/// Observability configuration
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(default, deny_unknown_fields)]
pub struct ObservabilityConfig {
    /// Enable Prometheus metrics
    pub prometheus_enabled:            bool,
    /// Port for Prometheus metrics endpoint
    pub prometheus_port:               u16,
    /// Enable OpenTelemetry tracing
    pub otel_enabled:                  bool,
    /// OpenTelemetry exporter type
    pub otel_exporter:                 String,
    /// Jaeger endpoint for trace collection
    pub otel_jaeger_endpoint:          Option<String>,
    /// Enable health check endpoint
    pub health_check_enabled:          bool,
    /// Health check interval in seconds
    pub health_check_interval_seconds: u32,
    /// Log level threshold
    pub log_level:                     String,
    /// Log output format (json, text)
    pub log_format:                    String,
}

impl Default for ObservabilityConfig {
    fn default() -> Self {
        Self {
            prometheus_enabled:            false,
            prometheus_port:               9090,
            otel_enabled:                  false,
            otel_exporter:                 "jaeger".to_string(),
            otel_jaeger_endpoint:          None,
            health_check_enabled:          true,
            health_check_interval_seconds: 30,
            log_level:                     "info".to_string(),
            log_format:                    "json".to_string(),
        }
    }
}

impl TomlSchema {
    /// Load schema from TOML file
    pub fn from_file(path: &str) -> Result<Self> {
        let content =
            std::fs::read_to_string(path).context(format!("Failed to read TOML file: {path}"))?;
        Self::parse_toml(&content)
    }

    /// Parse schema from TOML string.
    ///
    /// Expands `${VAR}` environment variable placeholders before parsing.
    pub fn parse_toml(content: &str) -> Result<Self> {
        let expanded = expand_env_vars(content);
        toml::from_str(&expanded).context("Failed to parse TOML schema")
    }

    /// Validate schema
    pub fn validate(&self) -> Result<()> {
        // Validate that all query return types exist
        for (query_name, query_def) in &self.queries {
            if !self.types.contains_key(&query_def.return_type) {
                anyhow::bail!(
                    "Query '{query_name}' references undefined type '{}'",
                    query_def.return_type
                );
            }
        }

        // Validate that all mutation return types exist
        for (mut_name, mut_def) in &self.mutations {
            if !self.types.contains_key(&mut_def.return_type) {
                anyhow::bail!(
                    "Mutation '{mut_name}' references undefined type '{}'",
                    mut_def.return_type
                );
            }
        }

        // Validate field auth rules reference existing policies
        for field_auth in &self.security.field_auth {
            let policy_exists = self.security.policies.iter().any(|p| p.name == field_auth.policy);
            if !policy_exists {
                anyhow::bail!("Field auth references undefined policy '{}'", field_auth.policy);
            }
        }

        // Validate federation entities reference existing types
        for entity in &self.federation.entities {
            if !self.types.contains_key(&entity.name) {
                anyhow::bail!("Federation entity '{}' references undefined type", entity.name);
            }
        }

        self.server.validate()?;
        self.database.validate()?;

        // Validate federation circuit breaker configuration
        if let Some(cb) = &self.federation.circuit_breaker {
            if cb.failure_threshold == 0 {
                anyhow::bail!(
                    "federation.circuit_breaker.failure_threshold must be greater than 0"
                );
            }
            if cb.recovery_timeout_secs == 0 {
                anyhow::bail!(
                    "federation.circuit_breaker.recovery_timeout_secs must be greater than 0"
                );
            }
            if cb.success_threshold == 0 {
                anyhow::bail!(
                    "federation.circuit_breaker.success_threshold must be greater than 0"
                );
            }

            // Validate per-database overrides reference defined entity names
            let entity_names: std::collections::HashSet<&str> =
                self.federation.entities.iter().map(|e| e.name.as_str()).collect();
            for override_cfg in &cb.per_database {
                if !entity_names.contains(override_cfg.database.as_str()) {
                    anyhow::bail!(
                        "federation.circuit_breaker.per_database entry '{}' does not match \
                         any defined federation entity",
                        override_cfg.database
                    );
                }
                if override_cfg.failure_threshold == Some(0) {
                    anyhow::bail!(
                        "federation.circuit_breaker.per_database['{}'].failure_threshold \
                         must be greater than 0",
                        override_cfg.database
                    );
                }
                if override_cfg.recovery_timeout_secs == Some(0) {
                    anyhow::bail!(
                        "federation.circuit_breaker.per_database['{}'].recovery_timeout_secs \
                         must be greater than 0",
                        override_cfg.database
                    );
                }
                if override_cfg.success_threshold == Some(0) {
                    anyhow::bail!(
                        "federation.circuit_breaker.per_database['{}'].success_threshold \
                         must be greater than 0",
                        override_cfg.database
                    );
                }
            }
        }

        Ok(())
    }

    /// Convert to intermediate schema format (compatible with language-generated types.json)
    pub fn to_intermediate_schema(&self) -> serde_json::Value {
        let mut types_json = serde_json::Map::new();

        for (type_name, type_def) in &self.types {
            let mut fields_json = serde_json::Map::new();

            for (field_name, field_def) in &type_def.fields {
                fields_json.insert(
                    field_name.clone(),
                    serde_json::json!({
                        "type": field_def.field_type,
                        "nullable": field_def.nullable,
                        "description": field_def.description,
                    }),
                );
            }

            types_json.insert(
                type_name.clone(),
                serde_json::json!({
                    "name": type_name,
                    "sql_source": type_def.sql_source,
                    "description": type_def.description,
                    "fields": fields_json,
                }),
            );
        }

        let mut queries_json = serde_json::Map::new();

        for (query_name, query_def) in &self.queries {
            let args: Vec<serde_json::Value> = query_def
                .args
                .iter()
                .map(|arg| {
                    serde_json::json!({
                        "name": arg.name,
                        "type": arg.arg_type,
                        "required": arg.required,
                        "default": arg.default,
                        "description": arg.description,
                    })
                })
                .collect();

            queries_json.insert(
                query_name.clone(),
                serde_json::json!({
                    "name": query_name,
                    "return_type": query_def.return_type,
                    "return_array": query_def.return_array,
                    "sql_source": query_def.sql_source,
                    "description": query_def.description,
                    "args": args,
                }),
            );
        }

        serde_json::json!({
            "types": types_json,
            "queries": queries_json,
        })
    }
}

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

/// Query validation limits (depth and complexity).
///
/// ```toml
/// [validation]
/// max_query_depth = 10
/// max_query_complexity = 100
/// ```
#[derive(Debug, Clone, Default, Deserialize, Serialize)]
#[serde(default, deny_unknown_fields)]
pub struct ValidationConfig {
    /// Maximum allowed query nesting depth. `None` uses the server default (10).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_query_depth: Option<u32>,

    /// Maximum allowed query complexity score. `None` uses the server default (100).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_query_complexity: Option<u32>,
}

/// Debug/development configuration.
///
/// Controls features that should only be enabled during development or
/// in trusted environments. All flags default to off.
///
/// ```toml
/// [debug]
/// enabled = true
/// database_explain = true
/// expose_sql = true
/// ```
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(default, deny_unknown_fields)]
pub struct DebugConfig {
    /// Master switch — all debug features require this to be `true`.
    pub enabled: bool,

    /// When `true`, the explain endpoint will also run `EXPLAIN` against the
    /// database and include the query plan in the response.
    pub database_explain: bool,

    /// When `true`, the explain endpoint includes the generated SQL in the
    /// response. Defaults to `true` (SQL is shown even without
    /// `database_explain`).
    pub expose_sql: bool,
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

#[allow(clippy::unwrap_used)]  // Reason: test code, panics are acceptable
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_toml_schema() {
        let toml = r#"
[schema]
name = "myapp"
version = "1.0.0"
database_target = "postgresql"

[types.User]
sql_source = "v_user"

[types.User.fields.id]
type = "ID"
nullable = false

[types.User.fields.name]
type = "String"
nullable = false

[queries.users]
return_type = "User"
return_array = true
sql_source = "v_user"
"#;
        let schema = TomlSchema::parse_toml(toml).expect("Failed to parse");
        assert_eq!(schema.schema.name, "myapp");
        assert!(schema.types.contains_key("User"));
    }

    #[test]
    fn test_validate_schema() {
        let schema = TomlSchema::default();
        assert!(schema.validate().is_ok());
    }

    // --- Issue #38: nats_url ---

    #[test]
    fn test_observers_config_nats_url_round_trip() {
        let toml = r#"
[schema]
name = "myapp"
version = "1.0.0"
database_target = "postgresql"

[observers]
enabled = true
backend = "nats"
nats_url = "nats://localhost:4222"
"#;
        let schema = TomlSchema::parse_toml(toml).expect("Failed to parse");
        assert_eq!(schema.observers.backend, "nats");
        assert_eq!(
            schema.observers.nats_url.as_deref(),
            Some("nats://localhost:4222")
        );
        assert!(schema.observers.redis_url.is_none());
    }

    #[test]
    fn test_observers_config_redis_url_unchanged() {
        let toml = r#"
[schema]
name = "myapp"
version = "1.0.0"
database_target = "postgresql"

[observers]
enabled = true
backend = "redis"
redis_url = "redis://localhost:6379"
"#;
        let schema = TomlSchema::parse_toml(toml).expect("Failed to parse");
        assert_eq!(schema.observers.backend, "redis");
        assert_eq!(
            schema.observers.redis_url.as_deref(),
            Some("redis://localhost:6379")
        );
        assert!(schema.observers.nats_url.is_none());
    }

    #[test]
    fn test_observers_config_nats_url_default_is_none() {
        let config = ObserversConfig::default();
        assert!(config.nats_url.is_none());
    }

    // --- Issue #39: federation circuit breaker ---

    #[test]
    fn test_federation_circuit_breaker_round_trip() {
        let toml = r#"
[schema]
name = "myapp"
version = "1.0.0"
database_target = "postgresql"

[types.Product]
sql_source = "v_product"

[federation]
enabled = true
apollo_version = 2

[[federation.entities]]
name = "Product"
key_fields = ["id"]

[federation.circuit_breaker]
enabled = true
failure_threshold = 3
recovery_timeout_secs = 60
success_threshold = 1
"#;
        let schema = TomlSchema::parse_toml(toml).expect("Failed to parse");
        let cb = schema.federation.circuit_breaker.as_ref().expect("Expected circuit_breaker");
        assert!(cb.enabled);
        assert_eq!(cb.failure_threshold, 3);
        assert_eq!(cb.recovery_timeout_secs, 60);
        assert_eq!(cb.success_threshold, 1);
        assert!(cb.per_database.is_empty());
    }

    #[test]
    fn test_federation_circuit_breaker_zero_failure_threshold_rejected() {
        let toml = r#"
[schema]
name = "myapp"
version = "1.0.0"
database_target = "postgresql"

[federation]
enabled = true

[federation.circuit_breaker]
enabled = true
failure_threshold = 0
recovery_timeout_secs = 30
success_threshold = 2
"#;
        let schema = TomlSchema::parse_toml(toml).expect("Failed to parse");
        let err = schema.validate().unwrap_err();
        assert!(err.to_string().contains("failure_threshold"), "{err}");
    }

    #[test]
    fn test_federation_circuit_breaker_zero_recovery_timeout_rejected() {
        let toml = r#"
[schema]
name = "myapp"
version = "1.0.0"
database_target = "postgresql"

[federation]
enabled = true

[federation.circuit_breaker]
enabled = true
failure_threshold = 5
recovery_timeout_secs = 0
success_threshold = 2
"#;
        let schema = TomlSchema::parse_toml(toml).expect("Failed to parse");
        let err = schema.validate().unwrap_err();
        assert!(err.to_string().contains("recovery_timeout_secs"), "{err}");
    }

    #[test]
    fn test_federation_circuit_breaker_per_database_unknown_entity_rejected() {
        let toml = r#"
[schema]
name = "myapp"
version = "1.0.0"
database_target = "postgresql"

[types.Product]
sql_source = "v_product"

[federation]
enabled = true

[[federation.entities]]
name = "Product"
key_fields = ["id"]

[federation.circuit_breaker]
enabled = true
failure_threshold = 5
recovery_timeout_secs = 30
success_threshold = 2

[[federation.circuit_breaker.per_database]]
database = "NonExistentEntity"
failure_threshold = 3
"#;
        let schema = TomlSchema::parse_toml(toml).expect("Failed to parse");
        let err = schema.validate().unwrap_err();
        assert!(err.to_string().contains("NonExistentEntity"), "{err}");
    }

    #[test]
    fn test_federation_circuit_breaker_per_database_valid() {
        let toml = r#"
[schema]
name = "myapp"
version = "1.0.0"
database_target = "postgresql"

[types.Product]
sql_source = "v_product"

[federation]
enabled = true

[[federation.entities]]
name = "Product"
key_fields = ["id"]

[federation.circuit_breaker]
enabled = true
failure_threshold = 5
recovery_timeout_secs = 30
success_threshold = 2

[[federation.circuit_breaker.per_database]]
database = "Product"
failure_threshold = 3
recovery_timeout_secs = 15
"#;
        let schema = TomlSchema::parse_toml(toml).expect("Failed to parse");
        assert!(schema.validate().is_ok());
        let cb = schema.federation.circuit_breaker.as_ref().unwrap();
        assert_eq!(cb.per_database.len(), 1);
        assert_eq!(cb.per_database[0].database, "Product");
        assert_eq!(cb.per_database[0].failure_threshold, Some(3));
        assert_eq!(cb.per_database[0].recovery_timeout_secs, Some(15));
    }

    #[test]
    fn test_toml_schema_parses_server_section() {
        let toml = r#"
[schema]
name = "myapp"
version = "1.0.0"
database_target = "postgresql"

[server]
host = "127.0.0.1"
port = 9999

[server.cors]
origins = ["https://example.com"]
"#;
        let schema = TomlSchema::parse_toml(toml).expect("Failed to parse");
        assert_eq!(schema.server.host, "127.0.0.1");
        assert_eq!(schema.server.port, 9999);
        assert_eq!(schema.server.cors.origins, ["https://example.com"]);
    }

    #[test]
    fn test_toml_schema_database_uses_runtime_config() {
        let toml = r#"
[schema]
name = "myapp"
version = "1.0.0"
database_target = "postgresql"

[database]
url      = "postgresql://localhost/mydb"
pool_min = 5
pool_max = 30
ssl_mode = "require"
"#;
        let schema = TomlSchema::parse_toml(toml).expect("Failed to parse");
        assert_eq!(schema.database.url, Some("postgresql://localhost/mydb".to_string()));
        assert_eq!(schema.database.pool_min, 5);
        assert_eq!(schema.database.pool_max, 30);
        assert_eq!(schema.database.ssl_mode, "require");
    }

    #[test]
    fn test_env_var_expansion_in_toml_schema() {
        temp_env::with_var("SCHEMA_TEST_DB_URL", Some("postgres://test/fraiseql"), || {
            let toml = r#"
[schema]
name = "myapp"
version = "1.0.0"
database_target = "postgresql"

[database]
url = "${SCHEMA_TEST_DB_URL}"
"#;
            let schema = TomlSchema::parse_toml(toml).expect("Failed to parse");
            assert_eq!(
                schema.database.url,
                Some("postgres://test/fraiseql".to_string())
            );
        });
    }

    #[test]
    fn test_toml_schema_defaults_without_server_section() {
        let toml = r#"
[schema]
name = "myapp"
version = "1.0.0"
database_target = "postgresql"
"#;
        let schema = TomlSchema::parse_toml(toml).expect("Failed to parse");
        // Defaults should apply
        assert_eq!(schema.server.host, "0.0.0.0");
        assert_eq!(schema.server.port, 8080);
        assert_eq!(schema.database.pool_min, 2);
        assert_eq!(schema.database.pool_max, 20);
        assert!(schema.database.url.is_none());
    }

    #[test]
    fn test_rate_limiting_config_parses_per_user_rps() {
        let toml = r"
[security.rate_limiting]
enabled = true
requests_per_second = 100
requests_per_second_per_user = 250
";
        let schema: TomlSchema = toml::from_str(toml).unwrap();
        let rl = schema.security.rate_limiting.unwrap();
        assert_eq!(rl.requests_per_second_per_user, Some(250));
    }

    #[test]
    fn test_rate_limiting_config_per_user_rps_defaults_to_none() {
        let toml = r"
[security.rate_limiting]
enabled = true
requests_per_second = 50
";
        let schema: TomlSchema = toml::from_str(toml).unwrap();
        let rl = schema.security.rate_limiting.unwrap();
        assert_eq!(rl.requests_per_second_per_user, None);
    }

    #[test]
    fn test_validation_config_parses_limits() {
        let toml = r"
[validation]
max_query_depth = 5
max_query_complexity = 50
";
        let schema: TomlSchema = toml::from_str(toml).unwrap();
        assert_eq!(schema.validation.max_query_depth, Some(5));
        assert_eq!(schema.validation.max_query_complexity, Some(50));
    }

    #[test]
    fn test_validation_config_defaults_to_none() {
        let toml = "";
        let schema: TomlSchema = toml::from_str(toml).unwrap();
        assert_eq!(schema.validation.max_query_depth, None);
        assert_eq!(schema.validation.max_query_complexity, None);
    }

    #[test]
    fn test_validation_config_partial() {
        let toml = r"
[validation]
max_query_depth = 3
";
        let schema: TomlSchema = toml::from_str(toml).unwrap();
        assert_eq!(schema.validation.max_query_depth, Some(3));
        assert_eq!(schema.validation.max_query_complexity, None);
    }
}
