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
pub mod rest;
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
use fraiseql_core::schema::{ChangelogConfig, CrudNamingConfig, NamingConvention};
pub use observability::ObservabilityConfig;
pub use observers::{EventHandler, ObserversConfig};
pub use operations::{MutationDefinition, QueryDefaults, QueryDefinition, SchemaMetadata};
use rest::RestTomlConfig;
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

/// Default `naming_convention` for the TomlSchema compile path: `CamelCase`,
/// matching the JSON-schema compile path (#456). Note: the derived
/// [`Default`] for [`TomlSchema`] still yields the enum default (`Preserve`) for
/// this field; only deserialization (the real compile path, via
/// [`TomlSchema::parse_toml`]) applies this `camelCase` default.
fn default_naming_convention() -> NamingConvention {
    NamingConvention::CamelCase
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

    /// REST transport configuration.
    #[serde(default)]
    pub rest: RestTomlConfig,

    /// Changelog GraphQL-exposure configuration.
    ///
    /// When `[changelog] expose = true`, the compiler injects the observer
    /// entity-change-log (`EntityChangeLog` / `TransportCheckpoint`) types plus
    /// their cursor query, point-lookup query, and checkpoint upsert mutation.
    /// Requires `[observers]` to be enabled. Absent by default.
    #[serde(default)]
    pub changelog: Option<ChangelogConfig>,

    /// Naming convention for GraphQL operation names.
    ///
    /// Defaults to `"camelCase"` — the standard GraphQL surface (`snake_case` in
    /// the database, `camelCase` exposed to clients, with single-JSONB input-key
    /// recasing) — matching the JSON-schema (`fraiseql-cli compile schema.json`)
    /// compile path. This avoids the silent footgun where a TomlSchema-authored
    /// schema defaulted to `Preserve` and forwarded camelCase input keys verbatim
    /// to `snake_case` SQL functions (#456). Set `"preserve"` to keep names exactly
    /// as authored (`snake_case`).
    #[serde(default = "default_naming_convention")]
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

    /// Hierarchy definitions for ID-based ltree operators (`descendantOfId`, `ancestorOfId`).
    ///
    /// Maps a hierarchy name to its table and ltree path column. Used by the compiler
    /// to generate subquery-based ltree WHERE clauses that resolve an entity's ltree
    /// path from its UUID.
    ///
    /// Example:
    /// ```toml
    /// [hierarchies.category]
    /// table = "tb_category"
    /// path_column = "category_path"
    /// ```
    #[serde(default)]
    pub hierarchies: Option<std::collections::HashMap<String, HierarchyConfig>>,
}

/// Configuration for a single hierarchy used by ID-based ltree operators.
///
/// Defines the database table and ltree path column for a named hierarchy.
/// The `id` column is always `id` (UUID) per the trinity pattern — not configurable.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct HierarchyConfig {
    /// Database table containing the ltree column (e.g., `"tb_category"`).
    pub table: String,

    /// Name of the ltree column in the table (e.g., `"category_path"`).
    pub path_column: String,
}

impl HierarchyConfig {
    /// Validate that required fields are non-empty.
    ///
    /// # Errors
    ///
    /// Returns an error if `table` or `path_column` is empty.
    pub fn validate(&self) -> Result<()> {
        if self.table.is_empty() {
            anyhow::bail!("hierarchy table must not be empty");
        }
        if self.path_column.is_empty() {
            anyhow::bail!("hierarchy path_column must not be empty");
        }
        Ok(())
    }
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

    /// Reject config sections the CLI accepts but no runtime consumes (#612).
    ///
    /// Each of these validated-then-did-nothing: the compiler embedded (or silently
    /// dropped) the section and the server never read it, so an operator who set it
    /// was misled. Per the fix-forward "honest-loud over silently-wrong" stance, each
    /// now fails at load with a pointer to the real mechanism or the tracking issue,
    /// rather than compiling a dishonest configuration. Mirrors the v2.7.0
    /// field-encryption precedent (refuse rather than run an unsupported config).
    ///
    /// # Errors
    ///
    /// Returns an error if any of `[security.rules]` / `[security.policies]` /
    /// `[security.field_auth]` (declared-but-unenforced authorization), `[caching]`,
    /// `[analytics]`, a non-default `[observability]`, or a non-`env`
    /// `[security.api_keys] storage` is present.
    ///
    /// Called from both [`Self::validate`] and the merger's `merge_values` so that no
    /// compile path can bypass it — the `--types` path (`merge_files`) deliberately
    /// skips the rest of `validate()` (queries may reference types from `types.json`),
    /// but these sections are self-contained and must be rejected there too.
    pub(crate) fn reject_accepted_but_unconsumed_config(&self) -> Result<()> {
        // #4 (security-shaped, highest-stakes): declared authorization the runtime does
        // not enforce. `RuntimeConfig::from_compiled_schema` pins the operation- and
        // field-authorizers to None, so any access boundary these blocks imply does not
        // exist. Fail loud rather than let a deployment believe it enforces authz.
        if !self.security.rules.is_empty()
            || !self.security.policies.is_empty()
            || !self.security.field_auth.is_empty()
        {
            anyhow::bail!(
                "[security.rules] / [security.policies] / [security.field_auth] declare \
                 authorization that the FraiseQL runtime does NOT enforce: the server pins \
                 the operation- and field-authorizers to None, so the access boundary these \
                 blocks imply does not exist. Remove the block(s). Enforce authorization at \
                 the database layer (RLS policies keyed on the session variables FraiseQL \
                 sets from the request identity) until a compiled-schema declarative \
                 authorization engine ships — tracked at \
                 https://github.com/fraiseql/fraiseql/issues/626."
            );
        }

        // #1 [caching]: never lowered into the compiled schema and never consumed;
        // server result caching is configured elsewhere. Reject a configured section.
        if self.caching.enabled || !self.caching.rules.is_empty() {
            anyhow::bail!(
                "[caching] is accepted but not consumed: the compiler does not lower it into \
                 the compiled schema and no runtime honors it, so `enabled` / \
                 `[[caching.rules]]` silently do nothing. Remove the [caching] section. \
                 Declarative per-rule result caching is tracked at \
                 https://github.com/fraiseql/fraiseql/issues/623."
            );
        }

        // #2 [analytics]: fully inert — never merged, never read.
        if self.analytics.enabled || !self.analytics.queries.is_empty() {
            anyhow::bail!(
                "[analytics] is accepted but fully inert: nothing in the compiler or runtime \
                 consumes it. Remove the [analytics] section. Analytics query definitions are \
                 tracked at https://github.com/fraiseql/fraiseql/issues/624."
            );
        }

        // #3 [observability]: inert on the compiled path — the real metrics/tracing config
        // lives in the server's runtime `[metrics]` / `[tracing]` sections.
        if self.observability != ObservabilityConfig::default() {
            anyhow::bail!(
                "[observability] is accepted but not consumed on the compiled path. Configure \
                 metrics under the server's [metrics] section and tracing under [tracing] in \
                 fraiseql.toml (logging via RUST_LOG / the server log settings), then remove \
                 [observability]. Alias-vs-remove rationale: \
                 https://github.com/fraiseql/fraiseql/issues/625."
            );
        }

        // #7 [security.api_keys] storage: only `env` (static keys) is implemented; the
        // server never reads `.storage`, so `postgres` authenticates nothing.
        if let Some(api_keys) = &self.security.api_keys {
            if api_keys.storage != "env" {
                anyhow::bail!(
                    "[security.api_keys] storage = \"{}\" is not implemented: the server only \
                     authenticates static `env` keys and never reads a postgres-backed key \
                     store, so this value authenticates nothing. Set storage = \"env\". A \
                     postgres-backed API-key store is tracked at \
                     https://github.com/fraiseql/fraiseql/issues/627.",
                    api_keys.storage
                );
            }
        }

        Ok(())
    }

    /// Validate schema
    ///
    /// # Errors
    ///
    /// Returns an error if any accepted-but-unconsumed config section is present
    /// (see `reject_accepted_but_unconsumed_config`), if any query or mutation
    /// references an undefined type, if a federation entity references an undefined
    /// type, or if server/database/circuit-breaker configuration values are invalid.
    pub fn validate(&self) -> Result<()> {
        use fraiseql_core::runtime::suggest_similar;

        self.reject_accepted_but_unconsumed_config()?;

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

        // Validate field hierarchy references exist in hierarchies config
        let hierarchy_names: std::collections::HashSet<&str> = self
            .hierarchies
            .as_ref()
            .map(|h| h.keys().map(String::as_str).collect())
            .unwrap_or_default();
        for (type_name, type_def) in &self.types {
            for (field_name, field_def) in &type_def.fields {
                if let Some(ref h_name) = field_def.hierarchy {
                    if !hierarchy_names.contains(h_name.as_str()) {
                        let hint = format_suggestions(suggest_similar(
                            h_name,
                            &hierarchy_names.iter().copied().collect::<Vec<_>>(),
                        ));
                        anyhow::bail!(
                            "Field '{type_name}.{field_name}' references undefined hierarchy \
                             '{h_name}'{hint}"
                        );
                    }
                }
            }
        }

        // Validate hierarchy configs have non-empty values
        if let Some(ref hierarchies) = self.hierarchies {
            for (name, config) in hierarchies {
                config
                    .validate()
                    .map_err(|e| anyhow::anyhow!("Invalid hierarchy config '{name}': {e}"))?;
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

        // Validate the [auth] block's group structure (#612 item 9): JWT group
        // (issuer/audience) is functional; a PKCE client group is all-four-or-none and
        // a complete one is rejected (not yet functional on the compiled path — #621).
        if let Some(auth) = &self.auth {
            auth.validate()?;
        }

        // Validate trusted_proxy_cidrs are parseable CIDR ranges (#609). The server
        // parses these into `ipnet::IpNet`; catching a bad value here surfaces the
        // error where the operator is authoring rather than at server boot.
        if let Some(rate_limiting) = &self.security.rate_limiting {
            if let Some(cidrs) = &rate_limiting.trusted_proxy_cidrs {
                for cidr in cidrs {
                    if cidr.parse::<ipnet::IpNet>().is_err() {
                        anyhow::bail!(
                            "[security.rate_limiting] trusted_proxy_cidrs contains an invalid \
                             CIDR range '{cidr}'. Use CIDR notation such as \"10.0.0.0/8\", or \
                             \"0.0.0.0/0\" to trust every proxy IP explicitly."
                        );
                    }
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
