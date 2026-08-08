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

    /// Tenant isolation strategy (`#892`).
    ///
    /// Mirrors `[fraiseql.tenancy]` in a project `fraiseql.toml`, minus the prefix, and
    /// lowers into the compiled schema's `security.tenancy` — the same place, read by the
    /// same runtime. Before this existed, `tenancy.mode` was unreachable from this
    /// authoring surface *and* from every `[domain_discovery]` project, because
    /// `commands::compile` deliberately skips the project config when the compile input is
    /// itself TOML (the two formats are not compatible). An operator wanting
    /// schema-per-tenant had no knob at all: `[tenancy]` was an unknown field, and
    /// `[fraiseql.tenancy]` was ignored.
    ///
    /// Shares `TenancyTomlConfig` with the project-config path so there is one authoring
    /// shape and one lowering ([`crate::config::security::TenancyTomlConfig::to_runtime`]),
    /// not two that can drift.
    #[serde(default)]
    pub tenancy: crate::config::security::TenancyTomlConfig,

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

    /// gRPC transport configuration.
    ///
    /// `CompiledSchema.grpc_config` gates the server's entire gRPC transport, and its own
    /// doc comment said it was "compiled from `[grpc]` in `fraiseql.toml`" — but no compile
    /// path could produce it, and `TomlSchema` is `deny_unknown_fields`, so following that
    /// documentation failed with "unknown field `grpc`". Removing the section compiled, and
    /// the server then silently never mounted gRPC: there was no supported way to turn on a
    /// shipped, e2e-tested transport (#780).
    ///
    /// Reuses the compiled type directly rather than defining a TOML mirror, so the two
    /// cannot drift the way the REST pair did.
    #[serde(default)]
    pub grpc: fraiseql_core::schema::GrpcConfig,

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

    /// Per-request PostgreSQL session variables resolved from the request identity.
    ///
    /// The runtime applies each mapping with `set_config(name, value, true)` before
    /// every query and mutation, so RLS policies can read it with
    /// `current_setting('app.tenant_id', true)`. This is the mechanism that makes
    /// database-layer tenant isolation work.
    ///
    /// ```toml
    /// [[session_variables.variables]]
    /// name = "app.tenant_id"
    /// source = "jwt"
    /// claim = "tenant_id"
    /// ```
    ///
    /// The compiled-schema field has documented itself as "compiled from the
    /// `[session_variables]` TOML section" since it was introduced; until #628 no
    /// such section existed in either TOML format, so the only way to declare the
    /// mapping was to hand-author `schema.json`.
    ///
    /// Deliberately the *same* type the runtime consumes: a CLI-side mirror struct
    /// is exactly the producer↔consumer seam that keeps dropping declarations.
    #[serde(default)]
    pub session_variables: fraiseql_core::schema::SessionVariablesConfig,
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

        // #1 [caching] (#623): `[[caching.rules]]` are lowered by the merger onto the
        // compiled per-query `cache_ttl_seconds` and per-mutation `invalidates_views`.
        // What is refused here is every configuration that would silently do nothing
        // or claim a backend that does not exist.
        if self.caching.backend != "memory" {
            anyhow::bail!(
                "[caching] backend = \"{}\" has no runtime counterpart: the result cache is \
                 in-process; only \"memory\" exists (there is no Redis-backed result cache \
                 anywhere in the runtime). Remove the `backend` key or set it to \"memory\".",
                self.caching.backend
            );
        }
        if self.caching.redis_url.is_some() {
            anyhow::bail!(
                "[caching] redis_url is set, but there is no Redis-backed result cache to \
                 connect it to. Remove the key. (Redis features elsewhere — APQ, rate \
                 limiting, PKCE — are separate subsystems with their own configuration.)"
            );
        }
        if !self.caching.enabled && !self.caching.rules.is_empty() {
            anyhow::bail!(
                "[caching] declares {} [[caching.rules]] but `enabled = false`, so none of \
                 them would take effect. Set `enabled = true` or remove the rules.",
                self.caching.rules.len()
            );
        }
        if self.caching.enabled && self.caching.rules.is_empty() {
            anyhow::bail!(
                "[caching] enabled = true with no [[caching.rules]] does nothing: runtime \
                 cache enablement is the server's `cache_enabled`, and per-query TTLs come \
                 from the rules. Add [[caching.rules]] or remove the section."
            );
        }

        // #2 [analytics] (#624): [[analytics.queries]] are lowered by the merger into
        // ordinary compiled queries. What is refused here is every shape that would
        // silently do nothing.
        if !self.analytics.enabled && !self.analytics.queries.is_empty() {
            anyhow::bail!(
                "[analytics] declares {} [[analytics.queries]] but `enabled = false`, so none \
                 of them would be compiled. Set `enabled = true` or remove the queries.",
                self.analytics.queries.len()
            );
        }
        if self.analytics.enabled && self.analytics.queries.is_empty() {
            anyhow::bail!(
                "[analytics] enabled = true with no [[analytics.queries]] does nothing. Add \
                 queries or remove the section."
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

        // #7 [security.api_keys] storage: `env` (static keys) and `postgres` (#627,
        // the server-side PgApiKeyStore) are implemented; anything else would
        // deserialize into an authenticator that authenticates nothing, silently.
        if let Some(api_keys) = &self.security.api_keys {
            if api_keys.storage != "env" && api_keys.storage != "postgres" {
                anyhow::bail!(
                    "[security.api_keys] storage = \"{}\" is not implemented: supported values \
                     are \"env\" (static hashed keys) and \"postgres\" (database-backed store, \
                     #627). Any other value would authenticate nothing.",
                    api_keys.storage
                );
            }
        }

        // #8 / #631 (decided): compiled handler declarations are not a runtime
        // concept. Runtime observers have exactly one source of truth — the
        // `tb_observer` table and the admin observer API — and the compiled
        // schema cannot even represent a `handlers` array any more. This bail
        // is permanent policy, not a stopgap awaiting a boot-time loader.
        // (`[observers] enabled` is still consumed as a changelog gate.)
        if !self.observers.handlers.is_empty() {
            anyhow::bail!(
                "[[observers.handlers]] is not supported: compiled TOML handlers are not a \
                 runtime concept (#631) — runtime observers come only from the `tb_observer` \
                 table and the admin observer API. Define observers in `tb_observer` / via \
                 `POST /api/observers` and remove the [[observers.handlers]] block."
            );
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

        // #892: the same validation the project-config path runs (`config::mod`'s
        // `self.fraiseql.tenancy.validate()`). An empty `tenant_claim` under a non-`none`
        // mode would compile to a tenancy config that resolves no tenant at all.
        self.tenancy.validate()?;

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
    ///
    /// # Panics
    ///
    /// Panics if serializing an
    /// [`IntermediateArgument`](crate::schema::intermediate::IntermediateArgument) fails. That
    /// struct holds only strings, bools and `serde_json::Value`s, none of which can
    /// error, so the panic is unreachable — it is an assertion rather than a failure mode. The
    /// arguments are serialized *through* the typed struct precisely so their wire keys cannot
    /// drift from the ones the consumer reads (#756).
    pub fn to_intermediate_schema(&self) -> serde_json::Value {
        let mut types_json = serde_json::Map::new();

        for (type_name, type_def) in &self.types {
            let mut fields_json = serde_json::Map::new();

            for (field_name, field_def) in &type_def.fields {
                let mut field_json = serde_json::json!({
                    "type": field_def.field_type,
                    "nullable": field_def.nullable,
                    "description": field_def.description,
                });
                // `vector_config` is emitted only when authored, matching the
                // intermediate-format key the converter reads (#386). Serialized
                // through the typed core struct so the wire shape cannot drift.
                if let Some(ref vector) = field_def.vector {
                    field_json["vector_config"] = serde_json::to_value(vector)
                        .expect("VectorConfig holds only plain enums and integers");
                }
                fields_json.insert(field_name.clone(), field_json);
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
                    // Serialized from the typed intermediate form so the wire keys are the
                    // ones the consumer reads. The hand-written `args`/`required` literal
                    // this replaces never bound (#756).
                    serde_json::to_value(crate::schema::intermediate::IntermediateArgument::from(
                        arg,
                    ))
                    .expect(
                        "IntermediateArgument holds only strings, bools and JSON values, \
                             so serializing it cannot fail",
                    )
                })
                .collect();

            queries_json.insert(
                query_name.clone(),
                serde_json::json!({
                    "name": query_name,
                    "return_type": query_def.return_type,
                    "returns_list": query_def.return_array,
                    "sql_source": query_def.sql_source,
                    "description": query_def.description,
                    "arguments": args,
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
