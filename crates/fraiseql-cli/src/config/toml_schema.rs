//! Complete TOML schema configuration supporting types, queries, mutations, federation, observers,
//! caching
//!
//! This module extends FraiseQLConfig to support the full TOML-based schema definition.

use std::{collections::BTreeMap, path::PathBuf};

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

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
#[serde(default)]
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
                    .map(|s| s.to_string())
                    .ok_or_else(|| anyhow::anyhow!("Invalid domain name: {:?}", path))?;

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
#[serde(default)]
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
                        anyhow::bail!("Error resolving type glob pattern '{}': {}", pattern, e);
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
                        anyhow::bail!("Error resolving query glob pattern '{}': {}", pattern, e);
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
                        anyhow::bail!("Error resolving mutation glob pattern '{}': {}", pattern, e);
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

/// Complete TOML schema configuration
#[derive(Debug, Clone, Default, Deserialize, Serialize)]
#[serde(default)]
pub struct TomlSchema {
    /// Schema metadata
    #[serde(rename = "schema")]
    pub schema: SchemaMetadata,

    /// Database configuration
    #[serde(rename = "database")]
    pub database: DatabaseConfig,

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
}

/// Schema metadata
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(default)]
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

/// Database configuration
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(default)]
pub struct DatabaseConfig {
    /// Database connection URL
    pub url:             String,
    /// Connection pool size
    pub pool_size:       u32,
    /// SSL mode (disable, allow, prefer, require)
    pub ssl_mode:        String,
    /// Connection timeout in seconds
    pub timeout_seconds: u32,
}

impl Default for DatabaseConfig {
    fn default() -> Self {
        Self {
            url:             "postgresql://localhost/mydb".to_string(),
            pool_size:       10,
            ssl_mode:        "prefer".to_string(),
            timeout_seconds: 30,
        }
    }
}

/// Type definition in TOML
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(default)]
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
#[serde(default)]
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
#[serde(default)]
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

/// Federation configuration
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(default)]
pub struct FederationConfig {
    /// Enable Apollo federation
    #[serde(default)]
    pub enabled:        bool,
    /// Apollo federation version
    pub apollo_version: Option<u32>,
    /// Federated entities
    pub entities:       Vec<FederationEntity>,
}

impl Default for FederationConfig {
    fn default() -> Self {
        Self {
            enabled:        false,
            apollo_version: Some(2),
            entities:       vec![],
        }
    }
}

/// Federation entity
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct FederationEntity {
    /// Entity name
    pub name:       String,
    /// Key fields for entity resolution
    pub key_fields: Vec<String>,
}

/// Security configuration
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(default)]
pub struct SecuritySettings {
    /// Default policy to apply if none specified
    pub default_policy: Option<String>,
    /// Custom authorization rules
    pub rules:          Vec<AuthorizationRule>,
    /// Authorization policies
    pub policies:       Vec<AuthorizationPolicy>,
    /// Field-level authorization rules
    pub field_auth:     Vec<FieldAuthRule>,
    /// Enterprise security configuration
    pub enterprise:     EnterpriseSecurityConfig,
}

impl Default for SecuritySettings {
    fn default() -> Self {
        Self {
            default_policy: Some("authenticated".to_string()),
            rules:          vec![],
            policies:       vec![],
            field_auth:     vec![],
            enterprise:     EnterpriseSecurityConfig::default(),
        }
    }
}

/// Authorization rule (custom expressions)
#[derive(Debug, Clone, Deserialize, Serialize)]
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
#[serde(default)]
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

/// Observers/event system configuration
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(default)]
pub struct ObserversConfig {
    /// Enable observers system
    #[serde(default)]
    pub enabled:   bool,
    /// Backend service (redis, nats, postgresql, mysql, in-memory)
    pub backend:   String,
    /// Redis connection URL
    pub redis_url: Option<String>,
    /// Event handlers
    pub handlers:  Vec<EventHandler>,
}

impl Default for ObserversConfig {
    fn default() -> Self {
        Self {
            enabled:   false,
            backend:   "redis".to_string(),
            redis_url: None,
            handlers:  vec![],
        }
    }
}

/// Event handler configuration
#[derive(Debug, Clone, Deserialize, Serialize)]
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
#[serde(default)]
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
pub struct CacheRule {
    /// Query pattern to cache
    pub query:                 String,
    /// Time-to-live in seconds
    pub ttl_seconds:           u32,
    /// Events that trigger cache invalidation
    pub invalidation_triggers: Vec<String>,
}

/// Analytics configuration
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(default)]
pub struct AnalyticsConfig {
    /// Enable analytics
    #[serde(default)]
    pub enabled: bool,
    /// Analytics queries
    pub queries: Vec<AnalyticsQuery>,
}

impl Default for AnalyticsConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            queries: vec![],
        }
    }
}

/// Analytics query definition
#[derive(Debug, Clone, Deserialize, Serialize)]
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
#[serde(default)]
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
        Self::from_str(&content)
    }

    /// Parse schema from TOML string
    pub fn from_str(content: &str) -> Result<Self> {
        toml::from_str(content).context("Failed to parse TOML schema")
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
        let schema = TomlSchema::from_str(toml).expect("Failed to parse");
        assert_eq!(schema.schema.name, "myapp");
        assert!(schema.types.contains_key("User"));
    }

    #[test]
    fn test_validate_schema() {
        let schema = TomlSchema::default();
        assert!(schema.validate().is_ok());
    }
}
