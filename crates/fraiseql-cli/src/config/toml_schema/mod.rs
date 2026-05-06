//! Complete TOML schema configuration supporting types, queries, mutations, federation, observers,
//! caching
//!
//! This module extends FraiseQLConfig to support the full TOML-based schema definition.

pub mod caching;
pub mod domain;
pub mod federation;
pub mod observability;
pub mod observers;
pub mod operations;
pub mod security;
pub mod server_settings;
pub mod subscriptions;
pub mod types;

use std::collections::BTreeMap;

use anyhow::{Context, Result};

/// Format "Did you mean?" suggestions from `suggest_similar` results.
fn format_suggestions(suggestions: Vec<&str>) -> String {
    if suggestions.is_empty() {
        String::new()
    } else {
        format!(". Did you mean: {}?", suggestions.join(", "))
    }
}
pub use caching::{AnalyticsConfig, AnalyticsQuery, CacheRule, CachingConfig};
pub use domain::{Domain, DomainDiscovery, ResolvedIncludes, SchemaIncludes};
pub use federation::{
    FederationCircuitBreakerConfig, FederationConfig, FederationEntity,
    PerDatabaseCircuitBreakerOverride,
};
use fraiseql_core::schema::{CrudNamingConfig, NamingConvention};
pub use observability::ObservabilityConfig;
pub use observers::{EventHandler, ObserversConfig};
pub use operations::{MutationDefinition, QueryDefaults, QueryDefinition, SchemaMetadata};
pub use security::{
    ApiKeySecurityConfig, AuthorizationPolicy, AuthorizationRule, CodeChallengeMethod,
    EncryptionAlgorithm, EnterpriseSecurityConfig, ErrorSanitizationTomlConfig, FieldAuthRule,
    KeySource, OidcClientConfig, PkceConfig, RateLimitingSecurityConfig, SecuritySettings,
    StateEncryptionConfig, StaticApiKeyEntry, TokenRevocationSecurityConfig, TrustedDocumentMode,
    TrustedDocumentsConfig,
};
use serde::{Deserialize, Serialize};
pub use server_settings::{DebugConfig, McpConfig, ValidationConfig};
pub use subscriptions::{SubscriptionHooksConfig, SubscriptionsConfig};
pub use types::{ArgumentDefinition, FieldDefinition, TypeDefinition};

use super::{
    expand_env_vars,
    runtime::{DatabaseRuntimeConfig, ServerRuntimeConfig},
};

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

    /// Naming convention for GraphQL operation names.
    ///
    /// `"preserve"` (default) keeps names as authored (snake_case from Python SDKs).
    /// `"camelCase"` converts operation names to standard GraphQL camelCase.
    #[serde(default)]
    pub naming_convention: NamingConvention,

    /// CRUD function naming config for automatic `sql_source` resolution.
    ///
    /// When set, mutations that omit `sql_source` have their PostgreSQL function
    /// name resolved at compile time using the configured template and the entity
    /// name derived from `return_type`.
    ///
    /// Example:
    /// ```toml
    /// [crud]
    /// function_schema = "app"
    /// function_naming = "trinity"
    /// ```
    #[serde(default)]
    pub crud: Option<CrudNamingConfig>,
}

impl TomlSchema {
    /// Load schema from TOML file
    ///
    /// # Errors
    ///
    /// Returns an error if the file cannot be read or cannot be parsed as a
    /// valid `TomlSchema`.
    pub fn from_file(path: &str) -> Result<Self> {
        let content =
            std::fs::read_to_string(path).context(format!("Failed to read TOML file: {path}"))?;
        Self::parse_toml(&content)
    }

    /// Parse schema from TOML string.
    ///
    /// Expands `${VAR}` environment variable placeholders before parsing.
    ///
    /// # Errors
    ///
    /// Returns an error if the TOML string cannot be deserialized into a
    /// `TomlSchema`.
    pub fn parse_toml(content: &str) -> Result<Self> {
        let expanded = expand_env_vars(content)?;
        toml::from_str(&expanded).context("Failed to parse TOML schema")
    }

    /// Validate schema
    ///
    /// # Errors
    ///
    /// Returns an error if any query or mutation references an undefined type,
    /// if a field auth rule references an undefined policy, if a federation
    /// entity references an undefined type, or if server/database/circuit-breaker
    /// configuration values are invalid.
    pub fn validate(&self) -> Result<()> {
        use fraiseql_core::runtime::suggest_similar;

        let type_names: Vec<&str> = self.types.keys().map(String::as_str).collect();

        // Validate that all query return types exist
        for (query_name, query_def) in &self.queries {
            if !self.types.contains_key(&query_def.return_type) {
                let hint = format_suggestions(suggest_similar(&query_def.return_type, &type_names));
                anyhow::bail!(
                    "Query '{query_name}' references undefined type '{}'{hint}",
                    query_def.return_type
                );
            }
        }

        // Validate that all mutation return types exist
        for (mut_name, mut_def) in &self.mutations {
            if !self.types.contains_key(&mut_def.return_type) {
                let hint = format_suggestions(suggest_similar(&mut_def.return_type, &type_names));
                anyhow::bail!(
                    "Mutation '{mut_name}' references undefined type '{}'{hint}",
                    mut_def.return_type
                );
            }
        }

        // Validate field auth rules reference existing policies
        for field_auth in &self.security.field_auth {
            let policy_exists = self.security.policies.iter().any(|p| p.name == field_auth.policy);
            if !policy_exists {
                let policy_names: Vec<&str> =
                    self.security.policies.iter().map(|p| p.name.as_str()).collect();
                let hint = format_suggestions(suggest_similar(&field_auth.policy, &policy_names));
                anyhow::bail!(
                    "Field auth references undefined policy '{}'{hint}",
                    field_auth.policy
                );
            }
        }

        // Validate federation entities reference existing types
        for entity in &self.federation.entities {
            if !self.types.contains_key(&entity.name) {
                let hint = format_suggestions(suggest_similar(&entity.name, &type_names));
                anyhow::bail!(
                    "Federation entity '{}' references undefined type{hint}",
                    entity.name
                );
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

#[cfg(test)]
mod tests;
