//! Typed configuration structures for compiled schema.
//!
//! These types replace untyped `serde_json::Value` fields in `CompiledSchema`
//! to enable compile-time validation, IDE autocompletion, and clearer domain modeling.

use std::collections::HashMap;

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
    ///
    /// Defaults to `["id"]` when omitted from the compiled schema JSON.
    #[serde(default = "default_key_fields")]
    pub key_fields: Vec<String>,
}

fn default_key_fields() -> Vec<String> {
    vec!["id".to_string()]
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
    /// Seconds to hold the circuit open before transitioning to HalfOpen.
    #[serde(default = "default_recovery_timeout")]
    pub recovery_timeout_secs: u64,
    /// Consecutive successes in HalfOpen required to close the circuit.
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
    /// When `true`, the mutation waits for this observer to complete before
    /// returning the response.  Defaults to `false` (fire-and-forget).
    #[serde(default)]
    pub synchronous:      bool,
}

/// Development-mode configuration (compiled from `[dev]` in `fraiseql.toml`).
///
/// When enabled, injects default JWT claims for unauthenticated requests,
/// removing the need for a real OIDC/JWT setup during local development.
///
/// **MUST NOT** be used in production — the server forcibly disables dev mode
/// when `FRAISEQL_ENV=production`.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct DevConfig {
    /// Enable dev mode. Default: false.
    pub enabled: bool,
    /// Default claims injected when no `Authorization` header is present.
    ///
    /// Keys map to `SecurityContext` fields:
    /// - `"sub"` → `user_id`
    /// - `"tenant_id"` / `"org_id"` → `tenant_id`
    /// - `"roles"` → `roles` (JSON array of strings)
    /// - `"scopes"` / `"scope"` → `scopes` (space-delimited string or JSON array)
    /// - all other keys → `attributes`
    #[serde(default)]
    pub default_claims: HashMap<String, serde_json::Value>,
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

/// REST transport configuration (compiled from `[rest]` in `fraiseql.toml`).
///
/// Embedded in `CompiledSchema.rest_config` alongside `mcp_config`.
/// Follows the same dual-location pattern as `McpConfig`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct RestConfig {
    /// Whether REST transport is enabled.
    pub enabled: bool,
    /// Base path for REST endpoints (e.g., "/rest/v1").
    pub path: String,
    /// Require authentication for REST requests.
    pub require_auth: bool,
    /// Whitelist of resource names to expose (empty = all).
    pub include: Vec<String>,
    /// Blacklist of resource names to hide.
    pub exclude: Vec<String>,
    /// Response behavior for DELETE operations.
    pub delete_response: DeleteResponse,
    /// Maximum page size for list queries.
    pub max_page_size: u64,
    /// Default page size when not specified by client.
    pub default_page_size: u64,
    /// Whether to generate ETag headers for responses.
    pub etag: bool,
    /// Maximum allowed size in bytes for filter query parameters.
    pub max_filter_bytes: usize,
    /// Maximum nesting depth for resource embedding (default 3).
    pub max_embedding_depth: usize,
    /// Maximum number of rows a single bulk update/delete may affect.
    ///
    /// Acts as a server-side safety limit.  Clients may request a lower limit
    /// via `Prefer: max-affected=N`.  Default: 1000.
    pub max_bulk_affected: u64,
    /// Default `Cache-Control: max-age` for GET responses (seconds).
    ///
    /// Overridden per-query by `QueryDefinition.cache_ttl_seconds`.
    /// Set to 0 to disable caching. Default: 60.
    pub default_cache_ttl: u64,
    /// TTL for idempotency keys in seconds. Default: 86400 (24 hours).
    pub idempotency_ttl_seconds: u64,
    /// SSE heartbeat interval in seconds. Default: 30.
    #[serde(default = "default_sse_heartbeat_seconds")]
    pub sse_heartbeat_seconds: u64,
    /// CDN/shared-cache TTL in seconds. When set, appends `s-maxage={value}`
    /// to `Cache-Control` on public GET responses. `None` = no s-maxage directive.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cdn_max_age: Option<u64>,
    /// Batch size for NDJSON streaming responses. Each batch is fetched from the
    /// database and serialized to the client before fetching the next. Default: 500.
    #[serde(default = "default_ndjson_batch_size")]
    pub ndjson_batch_size: u64,
}

const fn default_ndjson_batch_size() -> u64 {
    500
}

const fn default_sse_heartbeat_seconds() -> u64 {
    30
}

impl Default for RestConfig {
    fn default() -> Self {
        Self {
            enabled:             false,
            path:                "/rest/v1".to_string(),
            require_auth:        true,
            include:             Vec::new(),
            exclude:             Vec::new(),
            delete_response:     DeleteResponse::NoContent,
            max_page_size:       100,
            default_page_size:   20,
            etag:                true,
            max_filter_bytes:    4096,
            max_embedding_depth:      DEFAULT_MAX_EMBEDDING_DEPTH,
            max_bulk_affected:        DEFAULT_MAX_BULK_AFFECTED,
            default_cache_ttl:        60,
            idempotency_ttl_seconds:  86_400,
            sse_heartbeat_seconds:    30,
            cdn_max_age:              None,
            ndjson_batch_size:        500,
        }
    }
}

/// gRPC transport configuration (compiled from `[grpc]` in `fraiseql.toml`).
///
/// Embedded in `CompiledSchema.grpc_config`. Controls the dedicated tonic gRPC
/// server that serves queries via row-shaped `vr_*` database views and protobuf
/// wire encoding.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct GrpcConfig {
    /// Whether gRPC transport is enabled.
    pub enabled: bool,
    /// Port for the gRPC server. Default: 50052.
    pub port: u16,
    /// Enable gRPC server reflection (for `grpcurl` discovery). Default: true.
    pub reflection: bool,
    /// Maximum inbound message size in bytes. Default: 4 MiB.
    pub max_message_size_bytes: usize,
    /// Path to the compiled `FileDescriptorSet` binary (`.binpb`).
    pub descriptor_path: String,
    /// Whitelist of type names to expose as gRPC services (empty = all).
    pub include_types: Vec<String>,
    /// Blacklist of type names to hide from gRPC services.
    pub exclude_types: Vec<String>,
    /// Batch size for server-streaming RPCs (list queries). Default: 500.
    pub stream_batch_size: u32,
}

impl Default for GrpcConfig {
    fn default() -> Self {
        Self {
            enabled:                false,
            port:                   50052,
            reflection:             true,
            max_message_size_bytes: 4 * 1024 * 1024,
            descriptor_path:        "proto/descriptor.binpb".to_string(),
            include_types:          Vec::new(),
            exclude_types:          Vec::new(),
            stream_batch_size:      500,
        }
    }
}

/// Response behavior for REST DELETE operations.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[non_exhaustive]
pub enum DeleteResponse {
    /// Return 204 No Content (default).
    #[default]
    NoContent,
    /// Return 200 with the deleted entity body.
    Entity,
}

/// Relationship between two types, derived from FK conventions or explicit annotation.
///
/// Used by the REST transport for nested resource embedding
/// (`?select=id,posts(id,title)`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RelationshipDef {
    /// Name of the relationship (e.g., "posts", "author").
    pub name: String,
    /// Target type name (e.g., "Post", "User").
    pub target_type: String,
    /// Foreign key column on the owning side (e.g., "fk_user").
    pub foreign_key: String,
    /// Referenced key column on the target side (e.g., "pk_user").
    pub referenced_key: String,
    /// Cardinality of the relationship.
    pub cardinality: Cardinality,
}

/// Cardinality of a relationship between types.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[non_exhaustive]
pub enum Cardinality {
    /// One parent has many children (embedded as array).
    OneToMany,
    /// Many children reference one parent (embedded as single object).
    ManyToOne,
    /// One-to-one association (embedded as single object or null).
    OneToOne,
}

/// Maximum number of rows a single bulk operation may affect (default 1000).
pub const DEFAULT_MAX_BULK_AFFECTED: u64 = 1_000;

/// Maximum nesting depth for resource embedding (default 3).
pub const DEFAULT_MAX_EMBEDDING_DEPTH: usize = 3;

/// Unique constraint columns for upsert conflict resolution.
///
/// Each `ConflictTarget` represents a unique constraint or unique index on a
/// table.  The REST transport uses these to determine which columns to check
/// for duplicates when `Prefer: resolution=merge-duplicates` is specified.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ConflictTarget {
    /// Constraint or index name (e.g., "uq_user_email").
    pub name: String,
    /// Column names that form the unique constraint.
    pub columns: Vec<String>,
}

/// WebSocket subscription configuration (compiled from `[subscriptions]` in `fraiseql.toml`).
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct SubscriptionsConfig {
    /// Maximum subscriptions per WebSocket connection (`None` = unlimited).
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
    /// URL to POST on WebSocket `connection_init` (fail-closed).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub on_connect:     Option<String>,
    /// URL to POST on WebSocket disconnect (fire-and-forget).
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

/// Source from which a session variable value is resolved at request time.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "source", rename_all = "snake_case")]
#[non_exhaustive]
pub enum SessionVariableSource {
    /// Extract from a JWT claim (e.g., `{ "source": "jwt", "claim": "tenant_id" }`).
    Jwt {
        /// JWT claim name (e.g., `"sub"`, `"tenant_id"`, or a custom claim).
        claim: String,
    },
    /// Extract from an HTTP request header (e.g., `Accept-Language`).
    Header {
        /// Header name (e.g., `"Accept-Language"`).
        name: String,
    },
}

/// A single session variable mapping.
///
/// Maps a PostgreSQL GUC variable name (e.g., `app.locale`) to a runtime source.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SessionVariableMapping {
    /// PostgreSQL GUC name (e.g., `"app.locale"`, `"app.tenant_id"`).
    ///
    /// Must start with `app.` or `fraiseql.` to avoid namespace collisions
    /// with built-in PostgreSQL settings.
    pub pg_name: String,

    /// Where to get the runtime value.
    #[serde(flatten)]
    pub source: SessionVariableSource,
}

/// Session variable configuration compiled from `[session_variables]` in `fraiseql.toml`.
///
/// Session variables are emitted as `SELECT set_config($1, $2, true)` before each
/// query/mutation. They are transaction-scoped (`SET LOCAL` semantics) and reset
/// automatically on commit/rollback.
///
/// PostgreSQL views and functions can read them via `current_setting('app.locale', true)`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SessionVariablesConfig {
    /// User-defined session variable mappings.
    #[serde(default)]
    pub variables: Vec<SessionVariableMapping>,

    /// Automatically inject `fraiseql.started_at` (current timestamp) before mutations.
    ///
    /// SQL functions can use `current_setting('fraiseql.started_at', true)::timestamptz`
    /// to compute elapsed time.
    #[serde(default = "default_inject_started_at")]
    pub inject_started_at: bool,
}

const fn default_inject_started_at() -> bool {
    true
}

impl Default for SessionVariablesConfig {
    fn default() -> Self {
        Self {
            variables:         Vec::new(),
            inject_started_at: true,
        }
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)] // Reason: test code, panics are acceptable

    use super::*;
    use crate::schema::compiled::schema::CompiledSchema;

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
    fn test_event_handler_synchronous_defaults_false() {
        let json = r#"{
            "name": "onOrderCreated",
            "event": "Order.created",
            "action": "webhook",
            "webhook_url": "https://example.com/hook"
        }"#;
        let handler: EventHandler = serde_json::from_str(json).unwrap();
        assert!(!handler.synchronous);
    }

    #[test]
    fn test_event_handler_synchronous_explicit_true() {
        let json = r#"{
            "name": "onOrderCreated",
            "event": "Order.created",
            "action": "webhook",
            "webhook_url": "https://example.com/hook",
            "synchronous": true
        }"#;
        let handler: EventHandler = serde_json::from_str(json).unwrap();
        assert!(handler.synchronous);
    }

    #[test]
    fn test_federation_entity_key_fields_default_to_id() {
        let json = r#"{"name": "Product"}"#;
        let entity: FederationEntity = serde_json::from_str(json).unwrap();
        assert_eq!(entity.name, "Product");
        assert_eq!(entity.key_fields, vec!["id".to_string()]);
    }

    #[test]
    fn test_federation_entity_key_fields_explicit_override() {
        let json = r#"{"name": "OrderLine", "key_fields": ["order_id", "line_id"]}"#;
        let entity: FederationEntity = serde_json::from_str(json).unwrap();
        assert_eq!(entity.key_fields, vec!["order_id", "line_id"]);
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

    // =========================================================================
    // RestConfig tests
    // =========================================================================

    #[test]
    fn test_rest_config_defaults() {
        let config = RestConfig::default();
        assert!(!config.enabled);
        assert_eq!(config.path, "/rest/v1");
        assert!(config.require_auth);
        assert!(config.include.is_empty());
        assert!(config.exclude.is_empty());
        assert_eq!(config.delete_response, DeleteResponse::NoContent);
        assert_eq!(config.max_page_size, 100);
        assert_eq!(config.default_page_size, 20);
        assert!(config.etag);
        assert_eq!(config.max_filter_bytes, 4096);
    }

    #[test]
    fn test_rest_config_roundtrip() {
        let config = RestConfig {
            enabled:                 true,
            path:                    "/api/v2".to_string(),
            require_auth:            false,
            include:                 vec!["users".to_string()],
            exclude:                 vec!["secrets".to_string()],
            delete_response:         DeleteResponse::Entity,
            max_page_size:           500,
            default_page_size:       50,
            etag:                    false,
            max_filter_bytes:        8192,
            max_embedding_depth:     5,
            max_bulk_affected:       500,
            default_cache_ttl:       120,
            idempotency_ttl_seconds: 3600,
            sse_heartbeat_seconds:  15,
            cdn_max_age:            Some(300),
            ndjson_batch_size:      1000,
        };
        let json = serde_json::to_string(&config).unwrap();
        let restored: RestConfig = serde_json::from_str(&json).unwrap();
        assert_eq!(config, restored);
    }

    #[test]
    fn test_rest_config_serde_defaults() {
        // Minimal JSON should fill in defaults
        let json = r#"{"enabled": true}"#;
        let config: RestConfig = serde_json::from_str(json).unwrap();
        assert!(config.enabled);
        assert_eq!(config.path, "/rest/v1");
        assert_eq!(config.max_page_size, 100);
    }

    #[test]
    fn test_delete_response_serialization() {
        assert_eq!(
            serde_json::to_string(&DeleteResponse::NoContent).unwrap(),
            r#""no_content""#
        );
        assert_eq!(
            serde_json::to_string(&DeleteResponse::Entity).unwrap(),
            r#""entity""#
        );
    }

    #[test]
    fn test_delete_response_deserialization() {
        let no_content: DeleteResponse = serde_json::from_str(r#""no_content""#).unwrap();
        assert_eq!(no_content, DeleteResponse::NoContent);

        let entity: DeleteResponse = serde_json::from_str(r#""entity""#).unwrap();
        assert_eq!(entity, DeleteResponse::Entity);
    }

    #[test]
    fn test_rest_config_none_skipped_in_compiled_schema() {
        let schema = CompiledSchema::new();
        let json = serde_json::to_string(&schema).unwrap();
        assert!(!json.contains("rest_config"));
    }

    #[test]
    fn test_rest_config_present_in_compiled_schema() {
        let mut schema = CompiledSchema::new();
        schema.rest_config = Some(RestConfig {
            enabled: true,
            ..RestConfig::default()
        });
        let json = serde_json::to_string(&schema).unwrap();
        assert!(json.contains("rest_config"));

        let restored: CompiledSchema = serde_json::from_str(&json).unwrap();
        assert!(restored.rest_config.is_some());
        assert!(restored.rest_config.unwrap().enabled);
    }

    #[test]
    fn test_rest_config_max_bulk_affected_default() {
        let config = RestConfig::default();
        assert_eq!(config.max_bulk_affected, DEFAULT_MAX_BULK_AFFECTED);
    }

    #[test]
    fn test_rest_config_max_bulk_affected_roundtrip() {
        let config = RestConfig {
            max_bulk_affected: 500,
            ..RestConfig::default()
        };
        let json = serde_json::to_string(&config).unwrap();
        let restored: RestConfig = serde_json::from_str(&json).unwrap();
        assert_eq!(restored.max_bulk_affected, 500);
    }

    #[test]
    fn test_conflict_target_serde() {
        let target = ConflictTarget {
            name: "uq_user_email".to_string(),
            columns: vec!["email".to_string()],
        };
        let json = serde_json::to_string(&target).unwrap();
        let restored: ConflictTarget = serde_json::from_str(&json).unwrap();
        assert_eq!(restored.name, "uq_user_email");
        assert_eq!(restored.columns, vec!["email"]);
    }

    #[test]
    fn test_conflict_target_multi_column() {
        let target = ConflictTarget {
            name: "uq_user_org_email".to_string(),
            columns: vec!["fk_org".to_string(), "email".to_string()],
        };
        let json = serde_json::to_string(&target).unwrap();
        let restored: ConflictTarget = serde_json::from_str(&json).unwrap();
        assert_eq!(restored.columns.len(), 2);
    }

    // ── Session variables config tests ──────────────────────────────────

    #[test]
    fn test_session_variables_config_default() {
        let config = SessionVariablesConfig::default();
        assert!(config.variables.is_empty());
        assert!(config.inject_started_at);
    }

    #[test]
    fn test_session_variable_jwt_source_roundtrip() {
        let mapping = SessionVariableMapping {
            pg_name: "app.tenant_id".to_string(),
            source:  SessionVariableSource::Jwt { claim: "tenant_id".to_string() },
        };
        let json = serde_json::to_string(&mapping).unwrap();
        let restored: SessionVariableMapping = serde_json::from_str(&json).unwrap();
        assert_eq!(restored.pg_name, "app.tenant_id");
        assert_eq!(
            restored.source,
            SessionVariableSource::Jwt { claim: "tenant_id".to_string() }
        );
    }

    #[test]
    fn test_session_variable_header_source_roundtrip() {
        let mapping = SessionVariableMapping {
            pg_name: "app.locale".to_string(),
            source:  SessionVariableSource::Header { name: "Accept-Language".to_string() },
        };
        let json = serde_json::to_string(&mapping).unwrap();
        let restored: SessionVariableMapping = serde_json::from_str(&json).unwrap();
        assert_eq!(restored.pg_name, "app.locale");
        assert_eq!(
            restored.source,
            SessionVariableSource::Header { name: "Accept-Language".to_string() }
        );
    }

    #[test]
    fn test_session_variables_config_in_compiled_schema() {
        let mut schema = CompiledSchema::new();
        schema.session_variables_config = Some(SessionVariablesConfig {
            variables: vec![
                SessionVariableMapping {
                    pg_name: "app.tenant_id".to_string(),
                    source:  SessionVariableSource::Jwt { claim: "tenant_id".to_string() },
                },
                SessionVariableMapping {
                    pg_name: "app.locale".to_string(),
                    source:  SessionVariableSource::Header { name: "Accept-Language".to_string() },
                },
            ],
            inject_started_at: true,
        });
        let json = serde_json::to_string(&schema).unwrap();
        assert!(json.contains("session_variables_config"));
        assert!(json.contains("app.tenant_id"));

        let restored: CompiledSchema = serde_json::from_str(&json).unwrap();
        let config = restored.session_variables_config.unwrap();
        assert_eq!(config.variables.len(), 2);
        assert!(config.inject_started_at);
    }

    #[test]
    fn test_session_variables_config_omitted_when_none() {
        let schema = CompiledSchema::new();
        let json = serde_json::to_string(&schema).unwrap();
        assert!(!json.contains("session_variables_config"));
    }

    // =========================================================================
    // GrpcConfig tests
    // =========================================================================

    #[test]
    fn test_grpc_config_defaults() {
        let config = GrpcConfig::default();
        assert!(!config.enabled);
        assert_eq!(config.port, 50052);
        assert!(config.reflection);
        assert_eq!(config.max_message_size_bytes, 4 * 1024 * 1024);
        assert_eq!(config.descriptor_path, "proto/descriptor.binpb");
        assert!(config.include_types.is_empty());
        assert!(config.exclude_types.is_empty());
    }

    #[test]
    fn test_grpc_config_roundtrip() {
        let config = GrpcConfig {
            enabled:                true,
            port:                   50053,
            reflection:             false,
            max_message_size_bytes: 8 * 1024 * 1024,
            descriptor_path:        "custom/desc.binpb".to_string(),
            include_types:          vec!["User".to_string()],
            exclude_types:          vec!["Secret".to_string()],
        };
        let json = serde_json::to_string(&config).unwrap();
        let restored: GrpcConfig = serde_json::from_str(&json).unwrap();
        assert_eq!(config, restored);
    }

    #[test]
    fn test_grpc_config_serde_defaults() {
        let json = r#"{"enabled": true}"#;
        let config: GrpcConfig = serde_json::from_str(json).unwrap();
        assert!(config.enabled);
        assert_eq!(config.port, 50052);
        assert!(config.reflection);
        assert_eq!(config.max_message_size_bytes, 4 * 1024 * 1024);
    }

    #[test]
    fn test_grpc_config_none_skipped_in_compiled_schema() {
        let schema = CompiledSchema::new();
        let json = serde_json::to_string(&schema).unwrap();
        assert!(!json.contains("grpc_config"));
    }

    #[test]
    fn test_grpc_config_present_in_compiled_schema() {
        let mut schema = CompiledSchema::new();
        schema.grpc_config = Some(GrpcConfig {
            enabled: true,
            ..GrpcConfig::default()
        });
        let json = serde_json::to_string(&schema).unwrap();
        assert!(json.contains("grpc_config"));

        let restored: CompiledSchema = serde_json::from_str(&json).unwrap();
        assert!(restored.grpc_config.is_some());
        let grpc = restored.grpc_config.unwrap();
        assert!(grpc.enabled);
        assert_eq!(grpc.port, 50052);
    }
}
