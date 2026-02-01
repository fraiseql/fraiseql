//! Complete TOML schema configuration supporting types, queries, mutations, federation, observers, caching
//!
//! This module extends FraiseQLConfig to support the full TOML-based schema definition.

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

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
}

/// Schema metadata
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(default)]
pub struct SchemaMetadata {
    pub name: String,
    pub version: String,
    pub description: Option<String>,
    pub database_target: String, // postgresql, mysql, sqlite, sqlserver
}

impl Default for SchemaMetadata {
    fn default() -> Self {
        Self {
            name: "myapp".to_string(),
            version: "1.0.0".to_string(),
            description: None,
            database_target: "postgresql".to_string(),
        }
    }
}

/// Database configuration
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(default)]
pub struct DatabaseConfig {
    pub url: String,
    pub pool_size: u32,
    pub ssl_mode: String,
    pub timeout_seconds: u32,
}

impl Default for DatabaseConfig {
    fn default() -> Self {
        Self {
            url: "postgresql://localhost/mydb".to_string(),
            pool_size: 10,
            ssl_mode: "prefer".to_string(),
            timeout_seconds: 30,
        }
    }
}

/// Type definition in TOML
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(default)]
pub struct TypeDefinition {
    pub sql_source: String,
    pub description: Option<String>,
    pub fields: BTreeMap<String, FieldDefinition>,
}

impl Default for TypeDefinition {
    fn default() -> Self {
        Self {
            sql_source: "v_entity".to_string(),
            description: None,
            fields: BTreeMap::new(),
        }
    }
}

/// Field definition
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct FieldDefinition {
    #[serde(rename = "type")]
    pub field_type: String, // ID, String, Int, Boolean, DateTime, etc.
    #[serde(default)]
    pub nullable: bool,
    pub description: Option<String>,
}

/// Query definition in TOML
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(default)]
pub struct QueryDefinition {
    pub return_type: String,
    #[serde(default)]
    pub return_array: bool,
    pub sql_source: String,
    pub description: Option<String>,
    pub args: Vec<ArgumentDefinition>,
}

impl Default for QueryDefinition {
    fn default() -> Self {
        Self {
            return_type: "String".to_string(),
            return_array: false,
            sql_source: "v_entity".to_string(),
            description: None,
            args: vec![],
        }
    }
}

/// Mutation definition in TOML
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(default)]
pub struct MutationDefinition {
    pub return_type: String,
    pub sql_source: String,
    pub operation: String, // CREATE, UPDATE, DELETE
    pub description: Option<String>,
    pub args: Vec<ArgumentDefinition>,
}

impl Default for MutationDefinition {
    fn default() -> Self {
        Self {
            return_type: "String".to_string(),
            sql_source: "fn_operation".to_string(),
            operation: "CREATE".to_string(),
            description: None,
            args: vec![],
        }
    }
}

/// Argument definition
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ArgumentDefinition {
    pub name: String,
    #[serde(rename = "type")]
    pub arg_type: String,
    #[serde(default)]
    pub required: bool,
    pub default: Option<serde_json::Value>,
    pub description: Option<String>,
}

/// Federation configuration
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(default)]
pub struct FederationConfig {
    #[serde(default)]
    pub enabled: bool,
    pub apollo_version: Option<u32>,
    pub entities: Vec<FederationEntity>,
}

impl Default for FederationConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            apollo_version: Some(2),
            entities: vec![],
        }
    }
}

/// Federation entity
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct FederationEntity {
    pub name: String,
    pub key_fields: Vec<String>,
}

/// Security configuration
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(default)]
pub struct SecuritySettings {
    pub default_policy: Option<String>,
    pub rules: Vec<AuthorizationRule>,
    pub policies: Vec<AuthorizationPolicy>,
    pub field_auth: Vec<FieldAuthRule>,
    pub enterprise: EnterpriseSecurityConfig,
}

impl Default for SecuritySettings {
    fn default() -> Self {
        Self {
            default_policy: Some("authenticated".to_string()),
            rules: vec![],
            policies: vec![],
            field_auth: vec![],
            enterprise: EnterpriseSecurityConfig::default(),
        }
    }
}

/// Authorization rule (custom expressions)
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct AuthorizationRule {
    pub name: String,
    pub rule: String,
    pub description: Option<String>,
    #[serde(default)]
    pub cacheable: bool,
    pub cache_ttl_seconds: Option<u32>,
}

/// Authorization policy (RBAC/ABAC)
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct AuthorizationPolicy {
    pub name: String,
    #[serde(rename = "type")]
    pub policy_type: String, // RBAC, ABAC, CUSTOM, HYBRID
    pub rule: Option<String>,
    pub roles: Vec<String>,
    pub strategy: Option<String>, // ANY, ALL, EXACTLY
    pub attributes: Vec<String>,
    pub description: Option<String>,
    pub cache_ttl_seconds: Option<u32>,
}

/// Field-level authorization rule
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct FieldAuthRule {
    pub type_name: String,
    pub field_name: String,
    pub policy: String,
}

/// Enterprise security configuration
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(default)]
pub struct EnterpriseSecurityConfig {
    pub rate_limiting_enabled: bool,
    pub auth_endpoint_max_requests: u32,
    pub auth_endpoint_window_seconds: u64,
    pub audit_logging_enabled: bool,
    pub audit_log_backend: String,
    pub audit_retention_days: u32,
    pub error_sanitization: bool,
    pub hide_implementation_details: bool,
    pub constant_time_comparison: bool,
    pub pkce_enabled: bool,
}

impl Default for EnterpriseSecurityConfig {
    fn default() -> Self {
        Self {
            rate_limiting_enabled: true,
            auth_endpoint_max_requests: 100,
            auth_endpoint_window_seconds: 60,
            audit_logging_enabled: true,
            audit_log_backend: "postgresql".to_string(),
            audit_retention_days: 365,
            error_sanitization: true,
            hide_implementation_details: true,
            constant_time_comparison: true,
            pkce_enabled: true,
        }
    }
}

/// Observers/event system configuration
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(default)]
pub struct ObserversConfig {
    #[serde(default)]
    pub enabled: bool,
    pub backend: String, // redis, nats, postgresql, mysql, in-memory
    pub redis_url: Option<String>,
    pub handlers: Vec<EventHandler>,
}

impl Default for ObserversConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            backend: "redis".to_string(),
            redis_url: None,
            handlers: vec![],
        }
    }
}

/// Event handler configuration
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct EventHandler {
    pub name: String,
    pub event: String,
    pub action: String, // slack, email, sms, webhook, push, etc.
    pub webhook_url: Option<String>,
    pub retry_strategy: Option<String>,
    pub max_retries: Option<u32>,
    pub description: Option<String>,
}

/// Caching configuration
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(default)]
pub struct CachingConfig {
    #[serde(default)]
    pub enabled: bool,
    pub backend: String, // redis, memory, postgresql
    pub redis_url: Option<String>,
    pub rules: Vec<CacheRule>,
}

impl Default for CachingConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            backend: "redis".to_string(),
            redis_url: None,
            rules: vec![],
        }
    }
}

/// Cache invalidation rule
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct CacheRule {
    pub query: String,
    pub ttl_seconds: u32,
    pub invalidation_triggers: Vec<String>,
}

/// Analytics configuration
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(default)]
pub struct AnalyticsConfig {
    #[serde(default)]
    pub enabled: bool,
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
    pub name: String,
    pub sql_source: String,
    pub description: Option<String>,
}

/// Observability configuration
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(default)]
pub struct ObservabilityConfig {
    pub prometheus_enabled: bool,
    pub prometheus_port: u16,
    pub otel_enabled: bool,
    pub otel_exporter: String,
    pub otel_jaeger_endpoint: Option<String>,
    pub health_check_enabled: bool,
    pub health_check_interval_seconds: u32,
    pub log_level: String,
    pub log_format: String,
}

impl Default for ObservabilityConfig {
    fn default() -> Self {
        Self {
            prometheus_enabled: false,
            prometheus_port: 9090,
            otel_enabled: false,
            otel_exporter: "jaeger".to_string(),
            otel_jaeger_endpoint: None,
            health_check_enabled: true,
            health_check_interval_seconds: 30,
            log_level: "info".to_string(),
            log_format: "json".to_string(),
        }
    }
}

impl TomlSchema {
    /// Load schema from TOML file
    pub fn from_file(path: &str) -> Result<Self> {
        let content = std::fs::read_to_string(path)
            .context(format!("Failed to read TOML file: {path}"))?;
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
            let policy_exists = self.security.policies.iter()
                .any(|p| p.name == field_auth.policy);
            if !policy_exists {
                anyhow::bail!(
                    "Field auth references undefined policy '{}'",
                    field_auth.policy
                );
            }
        }

        // Validate federation entities reference existing types
        for entity in &self.federation.entities {
            if !self.types.contains_key(&entity.name) {
                anyhow::bail!(
                    "Federation entity '{}' references undefined type",
                    entity.name
                );
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
            let args: Vec<serde_json::Value> = query_def.args.iter().map(|arg| {
                serde_json::json!({
                    "name": arg.name,
                    "type": arg.arg_type,
                    "required": arg.required,
                    "default": arg.default,
                    "description": arg.description,
                })
            }).collect();

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
