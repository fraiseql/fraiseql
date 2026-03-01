//! Complete TOML schema configuration supporting types, queries, mutations, federation, observers,
//! caching
//!
//! This module extends FraiseQLConfig to support the full TOML-based schema definition.

use std::{collections::BTreeMap, fmt, path::PathBuf};

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

use super::runtime::{DatabaseRuntimeConfig, ServerRuntimeConfig};
use super::expand_env_vars;

/// Domain-based schema organization
///
/// Automatically discovers schema files in domain directories:
/// ```toml
/// [schema.domain_discovery]
/// enabled = true
/// root_dir = "schema"
/// ```
///
/// Expects structure:
/// ```text
/// schema/
/// ├── auth/
/// │   ├── types.json
/// │   ├── queries.json
/// │   └── mutations.json
/// ├── products/
/// │   ├── types.json
/// │   ├── queries.json
/// │   └── mutations.json
/// ```
#[derive(Debug, Clone, Default, Deserialize, Serialize)]
#[serde(default, deny_unknown_fields)]
pub struct DomainDiscovery {
    /// Enable automatic domain discovery
    pub enabled:  bool,
    /// Root directory containing domains
    pub root_dir: String,
}

/// Represents a discovered domain
#[derive(Debug, Clone)]
pub struct Domain {
    /// Domain name (directory name)
    pub name: String,
    /// Path to domain root
    pub path: PathBuf,
}

impl DomainDiscovery {
    /// Discover all domains in root_dir
    pub fn resolve_domains(&self) -> Result<Vec<Domain>> {
        if !self.enabled {
            return Ok(Vec::new());
        }

        let root = PathBuf::from(&self.root_dir);
        if !root.is_dir() {
            anyhow::bail!("Domain discovery root not found: {}", self.root_dir);
        }

        let mut domains = Vec::new();

        for entry in std::fs::read_dir(&root)
            .context(format!("Failed to read domain root: {}", self.root_dir))?
        {
            let entry = entry.context("Failed to read directory entry")?;
            let path = entry.path();

            if path.is_dir() {
                let name = path
                    .file_name()
                    .and_then(|n| n.to_str())
                    .map(std::string::ToString::to_string)
                    .ok_or_else(|| anyhow::anyhow!("Invalid domain name: {}", path.display()))?;

                domains.push(Domain { name, path });
            }
        }

        // Sort for deterministic ordering
        domains.sort_by(|a, b| a.name.cmp(&b.name));

        Ok(domains)
    }
}

/// Schema includes for multi-file composition (glob patterns)
///
/// Supports glob patterns for flexible file inclusion:
/// ```toml
/// [schema.includes]
/// types = ["schema/types/**/*.json"]
/// queries = ["schema/queries/**/*.json"]
/// mutations = ["schema/mutations/**/*.json"]
/// ```
#[derive(Debug, Clone, Default, Deserialize, Serialize)]
#[serde(default, deny_unknown_fields)]
pub struct SchemaIncludes {
    /// Glob patterns for type files
    pub types:     Vec<String>,
    /// Glob patterns for query files
    pub queries:   Vec<String>,
    /// Glob patterns for mutation files
    pub mutations: Vec<String>,
}

impl SchemaIncludes {
    /// Check if any includes are specified
    pub fn is_empty(&self) -> bool {
        self.types.is_empty() && self.queries.is_empty() && self.mutations.is_empty()
    }

    /// Resolve glob patterns to actual file paths
    ///
    /// # Returns
    /// ResolvedIncludes with expanded file paths, or error if resolution fails
    pub fn resolve_globs(&self) -> Result<ResolvedIncludes> {
        use glob::glob as glob_pattern;

        let mut type_paths = Vec::new();
        let mut query_paths = Vec::new();
        let mut mutation_paths = Vec::new();

        // Resolve type globs
        for pattern in &self.types {
            for entry in glob_pattern(pattern)
                .context(format!("Invalid glob pattern for types: {pattern}"))?
            {
                match entry {
                    Ok(path) => type_paths.push(path),
                    Err(e) => {
                        anyhow::bail!("Error resolving type glob pattern '{pattern}': {e}");
                    },
                }
            }
        }

        // Resolve query globs
        for pattern in &self.queries {
            for entry in glob_pattern(pattern)
                .context(format!("Invalid glob pattern for queries: {pattern}"))?
            {
                match entry {
                    Ok(path) => query_paths.push(path),
                    Err(e) => {
                        anyhow::bail!("Error resolving query glob pattern '{pattern}': {e}");
                    },
                }
            }
        }

        // Resolve mutation globs
        for pattern in &self.mutations {
            for entry in glob_pattern(pattern)
                .context(format!("Invalid glob pattern for mutations: {pattern}"))?
            {
                match entry {
                    Ok(path) => mutation_paths.push(path),
                    Err(e) => {
                        anyhow::bail!("Error resolving mutation glob pattern '{pattern}': {e}");
                    },
                }
            }
        }

        // Sort for deterministic ordering
        type_paths.sort();
        query_paths.sort();
        mutation_paths.sort();

        // Remove duplicates
        type_paths.dedup();
        query_paths.dedup();
        mutation_paths.dedup();

        Ok(ResolvedIncludes {
            types:     type_paths,
            queries:   query_paths,
            mutations: mutation_paths,
        })
    }
}

/// Resolved glob patterns to actual file paths
#[derive(Debug, Clone)]
pub struct ResolvedIncludes {
    /// Resolved type file paths
    pub types:     Vec<PathBuf>,
    /// Resolved query file paths
    pub queries:   Vec<PathBuf>,
    /// Resolved mutation file paths
    pub mutations: Vec<PathBuf>,
}

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

/// Circuit breaker configuration for a specific federated database/service
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct PerDatabaseCircuitBreakerOverride {
    /// Database or service name matching a federation entity
    pub database:             String,
    /// Override: number of consecutive failures before opening (must be > 0)
    pub failure_threshold:    Option<u32>,
    /// Override: seconds to wait before attempting recovery (must be > 0)
    pub recovery_timeout_secs: Option<u64>,
    /// Override: successes required in half-open state to close the breaker (must be > 0)
    pub success_threshold:    Option<u32>,
}

/// Circuit breaker configuration for Apollo Federation fan-out requests
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(default, deny_unknown_fields)]
pub struct FederationCircuitBreakerConfig {
    /// Enable circuit breaker protection on federation fan-out
    pub enabled:              bool,
    /// Consecutive failures before the breaker opens (default: 5, must be > 0)
    pub failure_threshold:    u32,
    /// Seconds to wait before attempting a probe request (default: 30, must be > 0)
    pub recovery_timeout_secs: u64,
    /// Probe successes needed to transition from half-open to closed (default: 2, must be > 0)
    pub success_threshold:    u32,
    /// Per-database overrides (database name must match a defined federation entity)
    pub per_database:         Vec<PerDatabaseCircuitBreakerOverride>,
}

impl Default for FederationCircuitBreakerConfig {
    fn default() -> Self {
        Self {
            enabled:              true,
            failure_threshold:    5,
            recovery_timeout_secs: 30,
            success_threshold:    2,
            per_database:         vec![],
        }
    }
}

/// Federation configuration
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(default, deny_unknown_fields)]
pub struct FederationConfig {
    /// Enable Apollo federation
    #[serde(default)]
    pub enabled:         bool,
    /// Apollo federation version
    pub apollo_version:  Option<u32>,
    /// Federated entities
    pub entities:        Vec<FederationEntity>,
    /// Circuit breaker configuration for federation fan-out requests
    pub circuit_breaker: Option<FederationCircuitBreakerConfig>,
}

impl Default for FederationConfig {
    fn default() -> Self {
        Self {
            enabled:         false,
            apollo_version:  Some(2),
            entities:        vec![],
            circuit_breaker: None,
        }
    }
}

/// Federation entity
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct FederationEntity {
    /// Entity name
    pub name:       String,
    /// Key fields for entity resolution
    pub key_fields: Vec<String>,
}

/// Security configuration
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(default, deny_unknown_fields)]
pub struct SecuritySettings {
    /// Default policy to apply if none specified
    pub default_policy:      Option<String>,
    /// Custom authorization rules
    pub rules:               Vec<AuthorizationRule>,
    /// Authorization policies
    pub policies:            Vec<AuthorizationPolicy>,
    /// Field-level authorization rules
    pub field_auth:          Vec<FieldAuthRule>,
    /// Enterprise security configuration (legacy flags)
    pub enterprise:          EnterpriseSecurityConfig,
    /// Error sanitization — controls what detail clients see in error responses
    pub error_sanitization:  Option<ErrorSanitizationTomlConfig>,
    /// Rate limiting — per-endpoint request caps
    pub rate_limiting:       Option<RateLimitingSecurityConfig>,
    /// State encryption — AEAD encryption for OAuth state and PKCE blobs
    pub state_encryption:    Option<StateEncryptionConfig>,
    /// PKCE — Proof Key for Code Exchange for OAuth Authorization Code flows
    pub pkce:                Option<PkceConfig>,
    /// API key authentication — static or database-backed key-based auth
    pub api_keys:            Option<ApiKeySecurityConfig>,
    /// Token revocation — reject JWTs by `jti` after revocation
    pub token_revocation:    Option<TokenRevocationSecurityConfig>,
}

impl Default for SecuritySettings {
    fn default() -> Self {
        Self {
            default_policy:     Some("authenticated".to_string()),
            rules:              vec![],
            policies:           vec![],
            field_auth:         vec![],
            enterprise:         EnterpriseSecurityConfig::default(),
            error_sanitization: None,
            rate_limiting:      None,
            state_encryption:   None,
            pkce:               None,
            api_keys:           None,
            token_revocation:   None,
        }
    }
}

/// Authorization rule (custom expressions)
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct AuthorizationRule {
    /// Rule name
    pub name:              String,
    /// Rule expression or condition
    pub rule:              String,
    /// Rule description
    pub description:       Option<String>,
    /// Whether rule result can be cached
    #[serde(default)]
    pub cacheable:         bool,
    /// Cache time-to-live in seconds
    pub cache_ttl_seconds: Option<u32>,
}

/// Authorization policy (RBAC/ABAC)
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct AuthorizationPolicy {
    /// Policy name
    pub name:              String,
    /// Policy type (RBAC, ABAC, CUSTOM, HYBRID)
    #[serde(rename = "type")]
    pub policy_type:       String,
    /// Optional rule expression
    pub rule:              Option<String>,
    /// Roles this policy applies to
    pub roles:             Vec<String>,
    /// Combination strategy (ANY, ALL, EXACTLY)
    pub strategy:          Option<String>,
    /// Attributes for attribute-based access control
    #[serde(default)]
    pub attributes:        Vec<String>,
    /// Policy description
    pub description:       Option<String>,
    /// Cache time-to-live in seconds
    pub cache_ttl_seconds: Option<u32>,
}

/// Field-level authorization rule
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct FieldAuthRule {
    /// Type name this rule applies to
    pub type_name:  String,
    /// Field name this rule applies to
    pub field_name: String,
    /// Policy to enforce
    pub policy:     String,
}

/// Enterprise security configuration
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(default, deny_unknown_fields)]
pub struct EnterpriseSecurityConfig {
    /// Enable rate limiting
    pub rate_limiting_enabled:        bool,
    /// Max requests per auth endpoint
    pub auth_endpoint_max_requests:   u32,
    /// Rate limit window in seconds
    pub auth_endpoint_window_seconds: u64,
    /// Enable audit logging
    pub audit_logging_enabled:        bool,
    /// Audit log backend service
    pub audit_log_backend:            String,
    /// Audit log retention in days
    pub audit_retention_days:         u32,
    /// Enable error sanitization
    pub error_sanitization:           bool,
    /// Hide implementation details in errors
    pub hide_implementation_details:  bool,
    /// Enable constant-time token comparison
    pub constant_time_comparison:     bool,
    /// Enable PKCE for OAuth flows
    pub pkce_enabled:                 bool,
}

impl Default for EnterpriseSecurityConfig {
    fn default() -> Self {
        Self {
            rate_limiting_enabled:        true,
            auth_endpoint_max_requests:   100,
            auth_endpoint_window_seconds: 60,
            audit_logging_enabled:        true,
            audit_log_backend:            "postgresql".to_string(),
            audit_retention_days:         365,
            error_sanitization:           true,
            hide_implementation_details:  true,
            constant_time_comparison:     true,
            pkce_enabled:                 true,
        }
    }
}

/// Controls how much error detail is exposed to API clients.
/// When enabled, internal error messages, SQL, and stack traces are stripped.
///
/// Note: named `ErrorSanitizationTomlConfig` to avoid collision with the identically-named
/// struct in `config::security` which serves `FraiseQLConfig`.
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(default, deny_unknown_fields)]
pub struct ErrorSanitizationTomlConfig {
    /// Enable error sanitization (default: false — opt-in)
    pub enabled: bool,
    /// Strip stack traces, SQL fragments, file paths (default: true)
    #[serde(default = "default_true")]
    pub hide_implementation_details: bool,
    /// Replace raw database error messages with a generic message (default: true)
    #[serde(default = "default_true")]
    pub sanitize_database_errors: bool,
    /// Replacement message shown to clients when an internal error is sanitized
    pub custom_error_message: Option<String>,
}

impl Default for ErrorSanitizationTomlConfig {
    fn default() -> Self {
        Self {
            enabled:                     false,
            hide_implementation_details: true,
            sanitize_database_errors:    true,
            custom_error_message:        None,
        }
    }
}

/// Per-endpoint and global rate limiting configuration for `[security.rate_limiting]`.
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(default, deny_unknown_fields)]
pub struct RateLimitingSecurityConfig {
    /// Enable rate limiting
    pub enabled: bool,
    /// Global request rate cap (requests per second, per IP)
    pub requests_per_second: u32,
    /// Burst allowance above the steady-state rate
    pub burst_size: u32,
    /// Auth initiation endpoint — max requests per window
    pub auth_start_max_requests: u32,
    /// Auth initiation window in seconds
    pub auth_start_window_secs: u64,
    /// OAuth callback endpoint — max requests per window
    pub auth_callback_max_requests: u32,
    /// OAuth callback window in seconds
    pub auth_callback_window_secs: u64,
    /// Token refresh endpoint — max requests per window
    pub auth_refresh_max_requests: u32,
    /// Token refresh window in seconds
    pub auth_refresh_window_secs: u64,
    /// Logout endpoint — max requests per window
    pub auth_logout_max_requests: u32,
    /// Logout window in seconds
    pub auth_logout_window_secs: u64,
    /// Failed login attempts before lockout
    pub failed_login_max_attempts: u32,
    /// Duration of failed-login lockout in seconds
    pub failed_login_lockout_secs: u64,
    /// Per-authenticated-user request rate in requests/second.
    /// Defaults to 10× `requests_per_second` if not set.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub requests_per_second_per_user: Option<u32>,
    /// Redis URL for distributed rate limiting (optional — falls back to in-memory)
    pub redis_url: Option<String>,
    /// Trust `X-Real-IP` / `X-Forwarded-For` headers for client IP extraction.
    ///
    /// Set to `true` only when FraiseQL is deployed behind a trusted reverse proxy
    /// (e.g. nginx, Cloudflare, AWS ALB) that sets these headers.
    /// Enabling without a trusted proxy allows clients to spoof their IP address.
    #[serde(default)]
    pub trust_proxy_headers: bool,
}

impl Default for RateLimitingSecurityConfig {
    fn default() -> Self {
        Self {
            enabled:                      false,
            requests_per_second:          100,
            requests_per_second_per_user: None,
            burst_size:                   200,
            auth_start_max_requests:      5,
            auth_start_window_secs:       60,
            auth_callback_max_requests:   10,
            auth_callback_window_secs:    60,
            auth_refresh_max_requests:    20,
            auth_refresh_window_secs:     300,
            auth_logout_max_requests:     30,
            auth_logout_window_secs:      60,
            failed_login_max_attempts:    10,
            failed_login_lockout_secs:    900,
            redis_url:                    None,
            trust_proxy_headers:          false,
        }
    }
}

/// AEAD algorithm for OAuth state and PKCE state blobs.
#[derive(Debug, Clone, Default, Deserialize, Serialize, PartialEq, Eq)]
pub enum EncryptionAlgorithm {
    /// ChaCha20-Poly1305 (recommended — constant-time, software-friendly)
    #[default]
    #[serde(rename = "chacha20-poly1305")]
    Chacha20Poly1305,
    /// AES-256-GCM (hardware-accelerated on modern CPUs)
    #[serde(rename = "aes-256-gcm")]
    Aes256Gcm,
}

impl fmt::Display for EncryptionAlgorithm {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Chacha20Poly1305 => f.write_str("chacha20-poly1305"),
            Self::Aes256Gcm        => f.write_str("aes-256-gcm"),
        }
    }
}

/// Where the encryption key is sourced from.
#[derive(Debug, Clone, Default, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum KeySource {
    /// Read key from an environment variable
    #[default]
    Env,
}

/// AEAD encryption for OAuth state parameter and PKCE code challenges.
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(default, deny_unknown_fields)]
pub struct StateEncryptionConfig {
    /// Enable state encryption
    pub enabled: bool,
    /// AEAD algorithm to use
    pub algorithm: EncryptionAlgorithm,
    /// Where to source the encryption key
    pub key_source: KeySource,
    /// Environment variable holding the 32-byte hex-encoded key
    pub key_env: Option<String>,
}

impl Default for StateEncryptionConfig {
    fn default() -> Self {
        Self {
            enabled:    false,
            algorithm:  EncryptionAlgorithm::default(),
            key_source: KeySource::Env,
            key_env:    Some("STATE_ENCRYPTION_KEY".to_string()),
        }
    }
}

/// PKCE code challenge method.
#[derive(Debug, Clone, Default, Deserialize, Serialize, PartialEq, Eq)]
pub enum CodeChallengeMethod {
    /// SHA-256 (required in production)
    #[default]
    #[serde(rename = "S256")]
    S256,
    /// Plain (spec-allowed but insecure — warns at runtime)
    #[serde(rename = "plain")]
    Plain,
}

/// PKCE (Proof Key for Code Exchange) configuration.
/// Requires `state_encryption` to be enabled for secure state storage.
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(default, deny_unknown_fields)]
pub struct PkceConfig {
    /// Enable PKCE for OAuth Authorization Code flows
    pub enabled: bool,
    /// Code challenge method (`S256` recommended)
    pub code_challenge_method: CodeChallengeMethod,
    /// How long the PKCE state is valid before the auth flow expires (seconds)
    pub state_ttl_secs: u64,
    /// Redis URL for distributed PKCE state storage across multiple replicas.
    ///
    /// Required for multi-replica deployments (Kubernetes, ECS, fly.io with
    /// multiple instances). Without Redis, `/auth/start` and `/auth/callback`
    /// must hit the same replica.
    ///
    /// Requires the `redis-pkce` Cargo feature to be compiled in.
    /// Example: `"redis://localhost:6379"` or `"${REDIS_URL}"`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub redis_url: Option<String>,
}

impl Default for PkceConfig {
    fn default() -> Self {
        Self {
            enabled:               false,
            code_challenge_method: CodeChallengeMethod::S256,
            state_ttl_secs:        600,
            redis_url:             None,
        }
    }
}

/// API key authentication configuration.
///
/// ```toml
/// [security.api_keys]
/// enabled = true
/// header = "X-API-Key"
/// hash_algorithm = "sha256"
/// storage = "env"
///
/// [[security.api_keys.static]]
/// key_hash = "sha256:abc123..."
/// scopes = ["read:*"]
/// name = "ci-readonly"
/// ```
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(default, deny_unknown_fields)]
pub struct ApiKeySecurityConfig {
    /// Enable API key authentication
    pub enabled: bool,
    /// HTTP header name to read the API key from
    pub header: String,
    /// Hash algorithm for key verification (`sha256`)
    pub hash_algorithm: String,
    /// Storage backend: `"env"` for static keys, `"postgres"` for DB-backed
    pub storage: String,
    /// Static API key entries (only for `storage = "env"`)
    #[serde(default, rename = "static")]
    pub static_keys: Vec<StaticApiKeyEntry>,
}

impl Default for ApiKeySecurityConfig {
    fn default() -> Self {
        Self {
            enabled:        false,
            header:         "X-API-Key".to_string(),
            hash_algorithm: "sha256".to_string(),
            storage:        "env".to_string(),
            static_keys:    vec![],
        }
    }
}

/// A single static API key entry.
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct StaticApiKeyEntry {
    /// Hex-encoded hash, optionally prefixed with algorithm (e.g. `sha256:abc...`)
    pub key_hash: String,
    /// Scopes granted by this key
    #[serde(default)]
    pub scopes: Vec<String>,
    /// Human-readable name for audit logging
    pub name: String,
}

/// Token revocation configuration.
///
/// ```toml
/// [security.token_revocation]
/// enabled = true
/// backend = "redis"
/// require_jti = true
/// fail_open = false
/// ```
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(default, deny_unknown_fields)]
pub struct TokenRevocationSecurityConfig {
    /// Enable token revocation
    pub enabled: bool,
    /// Backend: `"redis"`, `"postgres"`, or `"memory"`
    pub backend: String,
    /// Reject JWTs without a `jti` claim when revocation is enabled
    #[serde(default = "default_true")]
    pub require_jti: bool,
    /// If revocation store is unreachable: `false` = reject (fail-closed), `true` = allow (fail-open)
    #[serde(default)]
    pub fail_open: bool,
    /// Redis URL for distributed revocation (optional — inherited from `[fraiseql.redis]` if absent)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub redis_url: Option<String>,
}

impl Default for TokenRevocationSecurityConfig {
    fn default() -> Self {
        Self {
            enabled:     false,
            backend:     "memory".to_string(),
            require_jti: true,
            fail_open:   false,
            redis_url:   None,
        }
    }
}

/// OAuth2 client configuration for server-side PKCE flows.
///
/// The client secret is intentionally absent — use `client_secret_env` to
/// name the environment variable that holds the secret at runtime.
///
/// ```toml
/// [auth]
/// discovery_url       = "https://accounts.google.com"
/// client_id           = "my-fraiseql-client"
/// client_secret_env   = "OIDC_CLIENT_SECRET"
/// server_redirect_uri = "https://api.example.com/auth/callback"
/// ```
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct OidcClientConfig {
    /// OIDC provider discovery URL (e.g. `"https://accounts.google.com"`).
    /// Used to fetch `authorization_endpoint` and `token_endpoint` at compile time.
    pub discovery_url:       String,
    /// OAuth2 `client_id` registered with the provider.
    pub client_id:           String,
    /// Name of the environment variable that holds the client secret.
    /// The secret itself must never appear in TOML or the compiled schema.
    pub client_secret_env:   String,
    /// The full URL of this server's `/auth/callback` endpoint,
    /// e.g. `"https://api.example.com/auth/callback"`.
    pub server_redirect_uri: String,
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
