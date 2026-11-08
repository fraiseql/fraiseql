//! Typed configuration structures for compiled schema.
//!
//! These types replace untyped `serde_json::Value` fields in `CompiledSchema`
//! to enable compile-time validation, IDE autocompletion, and clearer domain modeling.

use serde::{Deserialize, Serialize};

/// Federation configuration for Apollo Federation v2 support.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct FederationConfig {
    /// Enable Apollo federation.
    #[serde(default)]
    pub enabled:             bool,
    /// Federation specification version (e.g., "v2").
    #[serde(default)]
    pub version:             Option<String>,
    /// Subgraph service name (used in Apollo Studio).
    #[serde(default)]
    pub service_name:        Option<String>,
    /// Subgraph SDL URL (exposed at `/__subgraph_schema`).
    #[serde(default)]
    pub schema_url:          Option<String>,
    /// Federated entities defined in this subgraph.
    #[serde(default)]
    pub entities:            Vec<FederationEntity>,
    /// Non-entity object types that are `@shareable` across subgraphs (e.g. a shared
    /// `MutationError` value type). Unlike `entities`, these carry no `@key` and are
    /// not members of the `_Entity` union — they only receive a type-level
    /// `@shareable` directive so two subgraphs can both define the identical type
    /// without an Apollo Federation `INVALID_FIELD_SHARING` composition error.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub shareable_types:     Vec<String>,
    /// Fields hidden from the supergraph with `@inaccessible`, by object type name.
    /// They render with the directive in the `_service` SDL, are filtered out of
    /// introspection, and are never selected by `_entities`.
    #[serde(default, skip_serializing_if = "std::collections::BTreeMap::is_empty")]
    pub inaccessible_fields: std::collections::BTreeMap<String, Vec<String>>,
    /// Circuit breaker configuration for federation fan-out requests.
    #[serde(default)]
    pub circuit_breaker:     Option<CircuitBreakerConfig>,
}

/// Federated entity configuration.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct FederationEntity {
    /// Entity type name (e.g., "User", "Product").
    pub name:             String,
    /// Key fields that uniquely identify this entity.
    pub key_fields:       Vec<String>,
    /// This subgraph `extend`s an entity owned by another subgraph (renders
    /// `extend type Name @key(...)` in the federation SDL).
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub extends:          bool,
    /// Fields that are `@external` — owned by another subgraph and only referenced
    /// here (typically the `@key` field of an extended entity).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub external_fields:  Vec<String>,
    /// Fields that are `@shareable` — resolvable in more than one subgraph.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub shareable_fields: Vec<String>,
}

/// Circuit breaker configuration for federation entity resolution.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
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
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
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

/// Custom authorization rule.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
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
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
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
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct FieldAuthRule {
    /// Type name.
    pub type_name:  String,
    /// Field name.
    pub field_name: String,
    /// Policy to enforce.
    pub policy:     String,
}

/// Enterprise security features, compiled from `[security.enterprise]`.
///
/// One key, the one the runtime reads. Nine others (`rate_limiting_enabled`,
/// `auth_endpoint_max_requests`, `auth_endpoint_window_seconds`, `audit_log_backend`,
/// `audit_retention_days`, `error_sanitization`, `hide_implementation_details`,
/// `constant_time_comparison`, `pkce_enabled`) were accepted and read by nothing; the
/// sections that do the work are `[security.rate_limiting]`, `[security.error_sanitization]`
/// and `[security.pkce]`. `deny_unknown_fields` refuses them by name.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(default, deny_unknown_fields)]
pub struct EnterpriseSecurityConfig {
    /// Record mutations in the audit log (default: true).
    pub audit_logging_enabled: bool,
}

impl Default for EnterpriseSecurityConfig {
    fn default() -> Self {
        Self {
            audit_logging_enabled: true,
        }
    }
}

/// Observers/event system configuration.
///
/// Deliberately carries **no** handler definitions (#631): runtime observers
/// have exactly one source of truth — the `tb_observer` table and the admin
/// observer API. Compiled handler declarations (TOML `[[observers.handlers]]`,
/// SDK-authored `observers_config.handlers`) are rejected at compile time
/// rather than carried as decoration the runtime never reads.
/// `deny_unknown_fields` makes a hand-edited compiled schema that smuggles a
/// `handlers` array fail loudly at load instead of deserializing minus the key.
#[derive(Clone, Default, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ObserversConfig {
    /// Enable observers system.
    #[serde(default)]
    pub enabled:      bool,
    /// The transport the observer runtime reads change events from.
    #[serde(default)]
    pub backend:      ObserversBackend,
    /// NATS connection URL. `FRAISEQL_NATS_URL` overrides it at runtime.
    ///
    /// Carried as written, so it holds no credential: a URL with user info belongs in
    /// the variable `nats_url_env` names. At most one of the two is set.
    pub nats_url:     Option<String>,
    /// The environment variable holding the NATS URL, read by the server at startup.
    ///
    /// The compiled schema carries the variable's name, never its value.
    /// `FRAISEQL_NATS_URL`, when set, still takes precedence over the URL it names.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub nats_url_env: Option<String>,
}

impl std::fmt::Debug for ObserversConfig {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ObserversConfig")
            .field("enabled", &self.enabled)
            .field("backend", &self.backend)
            .field("nats_url", &self.nats_url.as_deref().map(fraiseql_guard::redact::Url))
            .field("nats_url_env", &self.nats_url_env)
            .finish()
    }
}

/// The observer runtime's event transport: `"postgres"` (the default) or `"nats"`.
///
/// `"redis"` is refused with a pointer: the runtime has no Redis transport, and the
/// value used to compile into artifacts as the default without selecting anything.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, schemars::JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum ObserversBackend {
    /// PostgreSQL `LISTEN`/`NOTIFY` over the change log.
    #[default]
    Postgres,
    /// NATS `JetStream`.
    Nats,
}

impl<'de> Deserialize<'de> for ObserversBackend {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let name = String::deserialize(deserializer)?;
        match name.as_str() {
            "postgres" => Ok(Self::Postgres),
            "nats" => Ok(Self::Nats),
            "redis" => Err(serde::de::Error::custom(
                "observers backend \"redis\" names no transport: the observer runtime has no \
                 Redis transport. Set [observers] backend = \"postgres\" (the default) or \
                 \"nats\" in fraiseql.toml, and recompile",
            )),
            other => Err(serde::de::Error::custom(format!(
                "unknown observers backend `{other}`: expected \"postgres\" or \"nats\""
            ))),
        }
    }
}

/// Configuration for exposing the observer entity-change log as queryable GraphQL types.
///
/// When `expose = true`, the compiler injects two read-only object types
/// (`EntityChangeLog`, `TransportCheckpoint`), a cursor-paginated list query, a
/// point-lookup query, and a checkpoint upsert mutation, all backed by the views the
/// changelog migration installs. Sidecar consumers can then poll the changelog over the
/// same GraphQL endpoint they use for everything else — same auth, audit, rate limiting.
///
/// The generated operations are gated by `read_role` / `write_role` (RBAC). The views
/// read from `tb_entity_change_log` / `tb_transport_checkpoint`, which the observer
/// system installs (or which the operator supplies as a documented prerequisite).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(default, deny_unknown_fields)]
pub struct ChangelogConfig {
    /// Master switch. When `false`, no changelog types are registered (default).
    pub expose:        bool,
    /// PostgreSQL schema holding `tb_entity_change_log` and `tb_transport_checkpoint`.
    /// Default `"core"` (the observer install convention).
    pub schema:        String,
    /// Role required to query the changelog (`entity_change_logs`, `transport_checkpoint`).
    ///
    /// Matched against `SecurityContext.roles`. `None` disables the read gate (not
    /// recommended for production). Default `Some("changelog_reader")`.
    pub read_role:     Option<String>,
    /// Role required to upsert checkpoints (`upsert_transport_checkpoint`).
    ///
    /// Matched against `SecurityContext.roles`. `None` disables the write gate.
    /// Default `Some("changelog_writer")`.
    pub write_role:    Option<String>,
    /// Maximum rows a single `entity_change_logs` page should return.
    ///
    /// Enforced as a documented/runtime clamp rather than a GraphQL argument
    /// constraint (the argument system has no max). Default `1000`.
    pub max_limit:     u32,
    /// Whether the mutation executor writes the `tb_entity_change_log` outbox row
    /// in-transaction (the Change-Spine transactional outbox). Default `true`.
    ///
    /// Distinct from [`expose`](Self::expose), which gates the *read* path (the
    /// #149 GraphQL changelog types): `write_enabled` gates the *write* path. Set
    /// `false` to disable the outbox write **globally** — e.g. for an application
    /// that does not consume the Change Spine — so no mutation pays the write.
    /// Overridable at runtime by `FRAISEQL_CHANGELOG_ENABLED`; composes (AND) with
    /// the per-mutation
    /// [`MutationDefinition.changelog`](crate::schema::MutationDefinition) flag.
    pub write_enabled: bool,
}

impl Default for ChangelogConfig {
    fn default() -> Self {
        Self {
            expose:        false,
            schema:        "core".to_string(),
            read_role:     Some("changelog_reader".to_string()),
            write_role:    Some("changelog_writer".to_string()),
            max_limit:     1_000,
            write_enabled: true,
        }
    }
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

/// The query depth bound when `[validation] max_query_depth` is not declared.
///
/// A projector recurses as deep as the selection it follows, and the selection is the
/// client's document, so the depth that document may reach is decided here — once, for
/// the depth gate and the selection resolver alike. 11 is the depth GraphQL queries were
/// held to before there was a default, by the resolver's own fixed limit; it is kept so
/// that no deployment's accepted depth changes by default. Past about 49 a declared value
/// cannot bind on GraphQL: `graphql-parser` refuses a document nested past 50 brackets.
/// PostgreSQL 16 refuses a projection nested 2 044 `jsonb_build_object`s deep.
pub const DEFAULT_MAX_QUERY_DEPTH: u32 = 11;

/// Query validation limits (compiled from `[validation]` in `fraiseql.toml`).
// #1337: this one has TWO producers — `ServerConfig.validation` (TOML) and
// `CompiledSchema.validation_config` (the compiled artifact). Refusing unknown keys
// therefore makes an artifact carrying an unrecognised `validation` key a hard load
// failure rather than a silent drop. That is the intended direction: a compiled schema
// whose key does nothing is the same defect one layer down, and this project documents
// compiled-schema breaks rather than avoiding them.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(default, deny_unknown_fields)]
pub struct ValidationConfig {
    /// Maximum allowed query nesting depth; [`DEFAULT_MAX_QUERY_DEPTH`] when unset.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_query_depth:         Option<u32>,
    /// Maximum allowed query complexity score.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_query_complexity:    Option<u32>,
    /// Maximum bytes a single read may deliver, measured on the rows that come
    /// back.
    ///
    /// `max_query_depth` and `max_query_complexity` score a *document* — they are
    /// the guard for fields resolved at runtime, where the work grows with the
    /// shape of the request. They say nothing about a read served from a
    /// materialised view, which is one row read whatever its nesting depth, and
    /// whose cost is the bytes those rows weigh.
    ///
    /// That is the quantity this bounds, and it is the only one that means the
    /// same thing on every transport: a GraphQL document, a REST `?select=`, a
    /// gRPC column read and an NDJSON export all end in rows, and none of them
    /// can be scored for size before the database has answered. So this is
    /// enforced on the delivered rows rather than on the request — resolved once
    /// at the read chokepoint, charged wherever the rows arrive.
    ///
    /// A streamed read is cut at the frame that crosses the ceiling rather than
    /// being read to the end, so the bytes in the error are the running total at
    /// refusal, not the size of the whole answer.
    ///
    /// `None` leaves reads unbounded by size, which is the previous behaviour.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_response_bytes:      Option<u64>,
    /// Maximum number of rows a top-level `first`/`last`/`limit` argument may
    /// request, guarding against unbounded-pagination denial of service (#421).
    /// When unset, the runtime default (1000) applies; set to a large value to
    /// raise the ceiling. The server also honours the `FRAISEQL_MAX_PAGE_SIZE`
    /// environment variable as an override.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_page_size:           Option<u32>,
    /// The deepest offset a client may page to (#1306). `OFFSET n` reads `n` rows to
    /// discard them, so a deep offset page costs its depth however it is ordered; a
    /// relay connection pages by cursor at any depth. Unset (the default), no offset is
    /// refused; the server also honours `FRAISEQL_MAX_OFFSET` as an override.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_offset:              Option<u32>,
    /// The most (bucket, entity) cells an aggregate carrying a semi-additive measure forward
    /// may read (#1459); a larger request is refused before the measure is read. Unset, the
    /// runtime default applies. 0 is refused: it would refuse every such aggregate.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_semi_additive_cells: Option<u64>,
}

/// How an MCP server is reached.
#[derive(
    Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema,
)]
#[serde(rename_all = "lowercase")]
pub enum McpTransport {
    /// Over HTTP, at [`McpConfig::path`].
    #[default]
    Http,
    /// Over standard input and output, when the server is started as an MCP stdio server.
    Stdio,
    /// Both.
    Both,
}

impl std::fmt::Display for McpTransport {
    /// The spelling `[mcp] transport` takes.
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::Http => "http",
            Self::Stdio => "stdio",
            Self::Both => "both",
        })
    }
}

impl McpTransport {
    /// Whether the HTTP endpoint is mounted.
    #[must_use]
    pub const fn serves_http(self) -> bool {
        matches!(self, Self::Http | Self::Both)
    }
}

/// MCP (Model Context Protocol) server configuration: the `[mcp]` table of
/// `fraiseql.toml`, read by the compiler and compiled into the schema the server reads,
/// one type at both ends.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(default, deny_unknown_fields)]
pub struct McpConfig {
    /// Whether MCP is enabled.
    pub enabled:       bool,
    /// How the server is reached: `"http"`, `"stdio"`, or `"both"`.
    pub transport:     McpTransport,
    /// HTTP path for MCP endpoint (e.g., "/mcp").
    pub path:          String,
    /// Require authentication for MCP requests.
    pub require_auth:  bool,
    /// Whitelist of query/mutation names to expose (empty = all).
    pub include:       Vec<String>,
    /// Blacklist of query/mutation names to hide.
    pub exclude:       Vec<String>,
    /// **Read-only exposure**: when `true`, no mutation is ever exposed as an MCP
    /// tool, regardless of [`include`](Self::include)/[`exclude`](Self::exclude) —
    /// a fail-closed default for AI callers, so a mutation added later is not
    /// silently exposed. Wins over `include` listing a mutation.
    pub read_only:     bool,
    /// Keep per-thread working state across an agent's tool calls, in the
    /// `[session_state]` store (#967).
    ///
    /// Off by default, and opt-in rather than automatic for two reasons: it
    /// writes to a durable store on every authenticated tool call, and it makes
    /// one call's result depend on the calls before it — which is the point, and
    /// also a behaviour change no deployment should acquire by upgrading.
    ///
    /// Requires an authenticated caller and a configured `[session_state]`
    /// backend. With either missing, tool calls run exactly as they do today: an
    /// anonymous caller has no principal to scope a thread to, so there is
    /// nothing safe to key on.
    #[serde(default)]
    pub session_state: bool,
}

impl Default for McpConfig {
    fn default() -> Self {
        Self {
            enabled:       false,
            transport:     McpTransport::Http,
            path:          "/mcp".to_string(),
            require_auth:  true,
            include:       Vec::new(),
            exclude:       Vec::new(),
            read_only:     false,
            session_state: false,
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
    /// How the capture trigger reads each table it sits on, by the table's name as a type's
    /// `subscribable_tables` spells it (`[subscriptions.capture.<table>]`).
    #[serde(skip_serializing_if = "std::collections::BTreeMap::is_empty")]
    pub capture: std::collections::BTreeMap<String, CaptureTableConfig>,
    /// Open `WebSocket` connections one principal may hold at once, within its tenant.
    /// `None` (omitted) is the server's default.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_connections_per_principal: Option<ConnectionCap>,
    /// Open `WebSocket` connections the authenticated principals of one tenant may hold
    /// at once. `None` (omitted) is the server's default.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_connections_per_tenant: Option<ConnectionCap>,
    /// Open `WebSocket` connections with no principal, across the server. `None`
    /// (omitted) is the server's default.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_anonymous_connections: Option<ConnectionCap>,
    /// Open `WebSocket` connections with no principal from one client address. `None`
    /// (omitted) is the server's default.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_anonymous_connections_per_ip: Option<ConnectionCap>,
}

/// A bound on open `WebSocket` connections: a positive count, or explicitly none.
///
/// Written as a positive integer or the string `"unlimited"`. `0` is refused: it would
/// refuse every connection, which no deployment means by it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConnectionCap {
    /// At most this many open connections.
    Limit(std::num::NonZeroU32),
    /// No bound.
    Unlimited,
}

impl ConnectionCap {
    /// The bound, or `None` for [`ConnectionCap::Unlimited`].
    #[must_use]
    pub const fn limit(self) -> Option<u32> {
        match self {
            Self::Limit(n) => Some(n.get()),
            Self::Unlimited => None,
        }
    }
}

impl Serialize for ConnectionCap {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        match self {
            Self::Limit(n) => serializer.serialize_u32(n.get()),
            Self::Unlimited => serializer.serialize_str("unlimited"),
        }
    }
}

impl<'de> Deserialize<'de> for ConnectionCap {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        struct Visitor;

        impl serde::de::Visitor<'_> for Visitor {
            type Value = ConnectionCap;

            fn expecting(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                f.write_str("a positive number of connections, or \"unlimited\"")
            }

            fn visit_u64<E: serde::de::Error>(self, value: u64) -> Result<ConnectionCap, E> {
                let n = u32::try_from(value)
                    .map_err(|_| E::custom(format!("{value} connections is more than a u32")))?;
                std::num::NonZeroU32::new(n).map(ConnectionCap::Limit).ok_or_else(|| {
                    E::custom(
                        "0 connections would refuse every one; write \"unlimited\" for no bound",
                    )
                })
            }

            fn visit_i64<E: serde::de::Error>(self, value: i64) -> Result<ConnectionCap, E> {
                u64::try_from(value).map_or_else(
                    |_| Err(E::custom(format!("{value} is not a number of connections"))),
                    |v| self.visit_u64(v),
                )
            }

            fn visit_str<E: serde::de::Error>(self, value: &str) -> Result<ConnectionCap, E> {
                if value == "unlimited" {
                    Ok(ConnectionCap::Unlimited)
                } else {
                    Err(E::custom(format!(
                        "`{value}` is not a connection bound; write a positive number or \
                         \"unlimited\""
                    )))
                }
            }
        }

        deserializer.deserialize_any(Visitor)
    }
}

/// The two spellings a config writes, not the enum.
impl schemars::JsonSchema for ConnectionCap {
    fn schema_name() -> std::borrow::Cow<'static, str> {
        "ConnectionCap".into()
    }

    fn json_schema(_: &mut schemars::SchemaGenerator) -> schemars::Schema {
        schemars::json_schema!({
            "description": "A positive number of connections, or \"unlimited\".",
            "oneOf": [
                { "type": "integer", "format": "uint32", "minimum": 1 },
                { "type": "string", "const": "unlimited" },
            ],
        })
    }
}

/// How the capture trigger reads one table (`[subscriptions.capture.<table>]`).
///
/// ```toml
/// [subscriptions.capture.tb_ticket]
/// document_column = "data"
/// ```
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct CaptureTableConfig {
    /// The column the table keeps its entity document in. A captured write is stored as
    /// that document, as a mutation's is, and not as the table's row.
    pub document_column: String,
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

/// Naming convention for GraphQL operation names.
///
/// Controls how operation names (queries, mutations, subscriptions) are exposed
/// in the introspection schema and resolved at execution time.
///
/// Python/TypeScript SDKs emit `snake_case` operation names (e.g., `create_dns_server`).
/// Standard GraphQL convention is `camelCase` (`createDnsServer`). This setting
/// controls whether the compiler/runtime converts names automatically.
#[derive(
    Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema,
)]
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
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
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
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(default, deny_unknown_fields)]
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
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "camelCase", tag = "source")]
pub enum SessionVariableSource {
    /// Pull from a JWT claim (e.g. `"sub"`, `"tenant_id"`, or a custom claim).
    Jwt {
        /// JWT claim name to look up (e.g. `"sub"`, `"tenant_id"`).
        claim: String,
    },
    /// Pull from an HTTP request header (gRPC metadata on gRPC), read by the transport.
    ///
    /// A header is **client-controlled**: any caller can send any value. Never use one for
    /// row-level security or tenant scoping; use [`Jwt`](Self::Jwt) or
    /// [`Enrichment`](Self::Enrichment) for those.
    Header {
        /// HTTP header name (e.g. `"x-tenant-id"`).
        header: String,
    },
    /// A fixed literal value.
    Literal {
        /// The literal string value to inject.
        value: String,
    },
    /// Pull from a DB-resolved enriched-identity field, read from the reserved
    /// `fraiseql.enriched.*` attribute namespace (#539).
    ///
    /// Unlike [`SessionVariableSource::Jwt`], there is **no** fallback to a raw
    /// claim or a well-known field: the namespace is forge-proof (the extractor
    /// strips `fraiseql.` claims), and a missing enriched field is a hard error,
    /// never a silently-skipped/empty GUC. This is the construct that carries the
    /// fail-closed read-scoping guarantee.
    Enrichment {
        /// The enriched field name, without the `fraiseql.enriched.` prefix
        /// (e.g. `"actor_role"`).
        field: String,
    },
}

/// One session variable declaration.
///
/// The source is flattened, so a mapping is one flat table in both JSON and TOML:
///
/// ```toml
/// [[session_variables.variables]]
/// name = "app.tenant_id"
/// source = "jwt"
/// claim = "tenant_id"
/// ```
///
/// Flat is what lets `[session_variables]` be declared in `fraiseql.toml` against
/// this exact type, rather than through a CLI-side mirror struct — and a mirror
/// struct at a producer↔consumer seam is how declarations get silently dropped
/// (#806/#807/#758).
///
/// A key the mapping's source does not take is refused, the keys of another source
/// included: serde cannot deny unknown keys through a flatten, so it is read through
/// `FlatSessionVariable`, a private enum with one closed table per source.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(from = "FlatSessionVariable")]
pub struct SessionVariableMapping {
    /// The PostgreSQL setting name (e.g. `"app.tenant_id"`).
    pub name:   String,
    /// Where the value comes from.
    #[serde(flatten)]
    pub source: SessionVariableSource,
}

/// [`SessionVariableMapping`] as written: `name` and `source`, and the keys of that
/// source, nothing else.
///
/// The JSON Schema of [`SessionVariableMapping`] is this type's (`#[serde(from)]`), so an
/// editor closes each source's table as the parser does.
#[derive(Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "camelCase", tag = "source", deny_unknown_fields)]
enum FlatSessionVariable {
    /// Pull from a JWT claim.
    Jwt {
        /// The PostgreSQL setting name (e.g. `"app.tenant_id"`).
        name:  String,
        /// JWT claim name to look up (e.g. `"sub"`, `"tenant_id"`).
        claim: String,
    },
    /// Pull from an HTTP request header (gRPC metadata on gRPC). A header is
    /// client-controlled: never use one for row-level security or tenant scoping.
    Header {
        /// The PostgreSQL setting name (e.g. `"app.tenant_id"`).
        name:   String,
        /// HTTP header name (e.g. `"x-tenant-id"`).
        header: String,
    },
    /// A fixed literal value.
    Literal {
        /// The PostgreSQL setting name (e.g. `"app.tenant_id"`).
        name:  String,
        /// The literal string value to inject.
        value: String,
    },
    /// Pull from a field of the enriched identity the database supplies; a missing field
    /// is an error, never an empty setting.
    Enrichment {
        /// The PostgreSQL setting name (e.g. `"app.tenant_id"`).
        name:  String,
        /// The enriched field name, without the `fraiseql.enriched.` prefix (e.g.
        /// `"actor_role"`).
        field: String,
    },
}

impl From<FlatSessionVariable> for SessionVariableMapping {
    fn from(flat: FlatSessionVariable) -> Self {
        let (name, source) = match flat {
            FlatSessionVariable::Jwt { name, claim } => {
                (name, SessionVariableSource::Jwt { claim })
            },
            FlatSessionVariable::Header { name, header } => {
                (name, SessionVariableSource::Header { header })
            },
            FlatSessionVariable::Literal { name, value } => {
                (name, SessionVariableSource::Literal { value })
            },
            FlatSessionVariable::Enrichment { name, field } => {
                (name, SessionVariableSource::Enrichment { field })
            },
        };
        Self { name, source }
    }
}

/// Top-level session variables configuration in the compiled schema.
///
/// When populated, the executor calls `set_config()` before each query and
/// mutation to inject per-request values (JWT claims, HTTP headers, or literals)
/// as PostgreSQL transaction-scoped settings.  SQL functions and RLS policies can
/// then read these via `current_setting('app.tenant_id', true)`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct SessionVariablesConfig {
    /// Per-request session variable mappings.
    #[serde(default)]
    pub variables:         Vec<SessionVariableMapping>,
    /// Inject the built-in `fraiseql.started_at` timestamp before every mutation (and
    /// only mutations: no read consults it).
    #[serde(default = "session_default_true")]
    pub inject_started_at: bool,
}

/// One default whether or not `[session_variables]` is written: the timestamp is on.
/// A derived `Default` used to say `false` while the field's serde default said `true`,
/// so declaring an unrelated session variable switched the mutation timestamp on.
impl Default for SessionVariablesConfig {
    fn default() -> Self {
        Self {
            variables:         Vec::new(),
            inject_started_at: session_default_true(),
        }
    }
}

const fn session_default_true() -> bool {
    true
}

/// How DELETE endpoints report success.
#[derive(
    Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema,
)]
#[non_exhaustive]
pub enum DeleteResponse {
    /// Return `204 No Content` (default).
    #[default]
    NoContent,
    /// Return `200` with the deleted entity in the body.
    Entity,
}

/// Relationship cardinality between REST resources.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[non_exhaustive]
pub enum Cardinality {
    /// Parent has many children (array embed).
    OneToMany,
    /// Child points to one parent (object embed).
    ManyToOne,
    /// Exactly one related resource (object or null).
    OneToOne,
}

impl Cardinality {
    /// Whether an embed of this cardinality answers with **one object** rather than a
    /// collection.
    ///
    /// The two object cardinalities are the ones whose join key must identify at most one
    /// target row, because there is no second row for the answer to carry; see
    /// [`relationship_violations`](crate::schema::CompiledSchema::relationship_violations).
    ///
    /// Stated here for the reason [`Relationship::parent_join_column`] is: a caller
    /// keeping its own copy of which variants are "to-one" would classify a future variant
    /// silently, and the two opinions would drift.
    #[must_use]
    #[allow(clippy::match_like_matches_macro)] // Reason: an exhaustive match makes a new
    // variant a compile error at this site, which `matches!` would absorb into `false` —
    // the same tripwire `parent_join_column` relies on and `SubSelect::split` documents.
    pub const fn is_to_one(self) -> bool {
        match self {
            Self::ManyToOne | Self::OneToOne => true,
            Self::OneToMany => false,
        }
    }
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
    /// The SQL columns the database proved this relationship joins on (#1587), filled by
    /// `fraiseql compile --database` from `tviews.registry.references`. With it, a composed
    /// read correlates on those columns (an index serves them) instead of the stored keys.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub native_join:    Option<NativeJoin>,
}

/// The SQL columns of a relationship, proven by a foreign key (#1587).
///
/// `foreign_key` on the child's view, `referenced_key` on the parent's, both of one type.
/// Proven for those two views only: a level reading another relation (a query's own view)
/// keeps the stored keys.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct NativeJoin {
    /// The child view's column holding the parent's key (`fk_post`).
    pub foreign_key:    String,
    /// The parent view's column it references (`pk_post`).
    pub referenced_key: String,
    /// The child's view, as the schema names it (`app.tv_comment`).
    pub child_view:     String,
    /// The parent's view (`app.tv_post`).
    pub parent_view:    String,
}

impl Relationship {
    /// The native columns an embed of this relationship correlates on, as `(target view's
    /// column, declaring view's column)`: the mirror of
    /// [`target_join_column`](Self::target_join_column) and
    /// [`parent_join_column`](Self::parent_join_column) for [`native_join`](Self::native_join).
    #[must_use]
    pub fn native_correlation(&self) -> Option<(&str, &str)> {
        let join = self.native_join.as_ref()?;
        let (foreign, referenced) = (join.foreign_key.as_str(), join.referenced_key.as_str());
        Some(match self.cardinality {
            Cardinality::ManyToOne | Cardinality::OneToOne => (referenced, foreign),
            Cardinality::OneToMany => (foreign, referenced),
        })
    }

    /// The column an embed of this relationship reads off the **declaring** type's row.
    ///
    /// `OneToMany` hangs the foreign key on the child, so the declaring side is its
    /// `referenced_key` — conventionally `id`, which a client selects anyway, which is
    /// why #1230 hid here for so long. `ManyToOne`/`OneToOne` hold the key themselves,
    /// so the declaring side is the `foreign_key` — the one column a client asking for
    /// `author` has no reason to select.
    ///
    /// Stated here rather than in the REST executor because the compile-time and
    /// load-time checks that a join key names a declared field (#1266) have to ask the
    /// same question the executor asks; a check with its own copy of this table would
    /// accept exactly the schemas the executor cannot follow.
    #[must_use]
    pub const fn parent_join_column(&self) -> &String {
        match self.cardinality {
            Cardinality::ManyToOne | Cardinality::OneToOne => &self.foreign_key,
            Cardinality::OneToMany => &self.referenced_key,
        }
    }

    /// The column the join predicate filters on the **target** row — the mirror of
    /// [`parent_join_column`](Self::parent_join_column), stated once so the two can
    /// never drift apart.
    #[must_use]
    pub const fn target_join_column(&self) -> &String {
        match self.cardinality {
            Cardinality::ManyToOne | Cardinality::OneToOne => &self.referenced_key,
            Cardinality::OneToMany => &self.foreign_key,
        }
    }
}

/// REST transport configuration (compiled from `[rest]` in `fraiseql.toml`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
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
    /// The page each parent row gets of an embedded relationship when the request names
    /// none (`?select=orders(id)` without `?orders.limit=`).
    ///
    /// Distinct from [`max_page_size`](Self::max_page_size), which is the ceiling a client
    /// may ask up to, and applied at most at that ceiling. An embedded level is charged
    /// its page for every parent row, so defaulting it to the ceiling charged an unpaged
    /// embed as if every parent had a full page of related rows.
    pub default_embed_page_size: u64,
    /// Batch size for NDJSON streaming responses.
    pub ndjson_batch_size:       u64,
    /// Maximum affected rows for bulk PATCH/DELETE.
    pub max_bulk_affected:       u64,
    /// Maximum byte length for `?filter=` JSON values.
    pub max_filter_bytes:        u64,
    /// How DELETE endpoints report success.
    pub delete_response:         DeleteResponse,
    /// Default result cache TTL in seconds (0 = no caching): the `max-age` of a GET's
    /// `Cache-Control`. An authenticated read (any credential) is `private`; only an
    /// anonymous read is `public`, and only a public one carries `s-maxage`.
    pub default_cache_ttl:       u64,
    /// CDN `s-maxage` value in seconds for `Cache-Control` headers (`None` = omit).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cdn_max_age:             Option<u64>,
    /// Whether REST endpoints require authentication by default.
    pub require_auth:            bool,
    /// SSE heartbeat interval in seconds.
    pub sse_heartbeat_seconds:   u64,
    /// How many delivered events a `Last-Event-ID` resume may reach back over
    /// (#1310).
    ///
    /// A reconnecting client names the last event it received, and the stream replays
    /// everything delivered since. A resume point far behind the head is therefore a
    /// read proportional to how far behind it is, on the busiest table in the
    /// deployment — so a request that would reach back further than this is refused
    /// (`413 RESUME_TOO_FAR_BEHIND`) before a single frame is emitted, rather than
    /// served in part. The client can then reconnect without the header and knowingly
    /// start from now.
    ///
    /// Counted across every entity type, because that is what the walk through the
    /// dispatch ledger costs, whatever share of it one resource turns out to be.
    ///
    /// `0` means **no bound** — every resume is served however far back it reaches.
    /// That is the permissive setting, and worth naming as such: on a stream reachable
    /// without a credential it hands an anonymous client an unbounded read.
    pub sse_max_replay_events:   u64,
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
            default_embed_page_size: 50,
            ndjson_batch_size:       500,
            max_bulk_affected:       10_000,
            max_filter_bytes:        4_096,
            delete_response:         DeleteResponse::NoContent,
            default_cache_ttl:       0,
            cdn_max_age:             None,
            require_auth:            false,
            sse_heartbeat_seconds:   30,
            sse_max_replay_events:   10_000,
            max_embedding_depth:     3,
            include:                 Vec::new(),
            exclude:                 Vec::new(),
            etag:                    true,
            idempotency_ttl_seconds: 300,
        }
    }
}

/// Keys `[grpc]` no longer takes.
///
/// Neither restricted the service: every method of the
/// descriptor set was served whatever they said. Each is refused by name, wherever a
/// `[grpc]` section or a `grpc_config` copy of it is read, with [`grpc_removed_key_refusal`].
pub const GRPC_REMOVED_KEYS: [&str; 2] = ["include_types", "exclude_types"];

/// The refusal of `[grpc] key`, one of [`GRPC_REMOVED_KEYS`].
#[must_use]
pub fn grpc_removed_key_refusal(key: &str) -> String {
    format!(
        "`[grpc] {key}` was removed: it never restricted the service. The descriptor set \
         decides which methods are served: leave the type's methods out of the `.proto` you \
         build with `protoc --descriptor_set_out`, or gate the operation with \
         `requires_role`. Remove the key and recompile."
    )
}

/// gRPC transport configuration (compiled from `[grpc]` in `fraiseql.toml`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(default, deny_unknown_fields)]
pub struct GrpcConfig {
    /// Whether the gRPC transport is enabled.
    pub enabled:           bool,
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
            descriptor_path:   String::new(),
            reflection:        true,
            stream_batch_size: 500,
        }
    }
}

#[cfg(test)]
mod changelog_config_tests {
    #![allow(clippy::unwrap_used)] // Reason: test code, panics are acceptable

    use super::*;

    #[test]
    fn changelog_defaults_to_disabled_with_secure_roles() {
        let cfg = ChangelogConfig::default();
        assert!(!cfg.expose, "exposure must be opt-in");
        assert_eq!(cfg.schema, "core");
        assert_eq!(cfg.read_role.as_deref(), Some("changelog_reader"));
        assert_eq!(cfg.write_role.as_deref(), Some("changelog_writer"));
        assert_eq!(cfg.max_limit, 1_000);
    }

    #[test]
    fn changelog_partial_toml_fills_defaults() {
        // A block that only flips `expose` must inherit every other default,
        // including the secure-by-default RBAC roles.
        let cfg: ChangelogConfig = serde_json::from_str(r#"{"expose": true}"#).unwrap();
        assert!(cfg.expose);
        assert_eq!(cfg.schema, "core");
        assert_eq!(cfg.read_role.as_deref(), Some("changelog_reader"));
        assert_eq!(cfg.max_limit, 1_000);
    }

    #[test]
    fn changelog_explicit_null_role_disables_gate() {
        let cfg: ChangelogConfig =
            serde_json::from_str(r#"{"expose": true, "read_role": null}"#).unwrap();
        assert!(cfg.read_role.is_none(), "null read_role disables the read gate");
    }

    #[test]
    fn changelog_round_trips_through_json() {
        let cfg = ChangelogConfig {
            expose:        true,
            schema:        "audit".to_string(),
            read_role:     Some("ops".to_string()),
            write_role:    None,
            max_limit:     250,
            write_enabled: false,
        };
        let json = serde_json::to_string(&cfg).unwrap();
        let back: ChangelogConfig = serde_json::from_str(&json).unwrap();
        assert_eq!(cfg, back);
    }
}

#[cfg(test)]
mod native_join_tests {
    use super::*;

    fn relationship(cardinality: Cardinality) -> Relationship {
        Relationship {
            name: "r".to_string(),
            target_type: "T".to_string(),
            cardinality,
            foreign_key: "post_id".to_string(),
            referenced_key: "id".to_string(),
            native_join: Some(NativeJoin {
                foreign_key:    "fk_post".to_string(),
                referenced_key: "pk_post".to_string(),
                child_view:     "tv_comment".to_string(),
                parent_view:    "tv_post".to_string(),
            }),
        }
    }

    #[test]
    fn a_list_of_children_correlates_their_foreign_key_to_the_parents_key() {
        assert_eq!(
            relationship(Cardinality::OneToMany).native_correlation(),
            Some(("fk_post", "pk_post"))
        );
    }

    #[test]
    fn a_parent_embedded_in_its_child_correlates_its_key_to_the_childs_foreign_key() {
        for cardinality in [Cardinality::ManyToOne, Cardinality::OneToOne] {
            assert_eq!(
                relationship(cardinality).native_correlation(),
                Some(("pk_post", "fk_post"))
            );
        }
    }

    #[test]
    fn without_a_proof_there_is_none_and_the_schema_does_not_say_so() {
        let mut unproven = relationship(Cardinality::OneToMany);
        unproven.native_join = None;
        assert_eq!(unproven.native_correlation(), None);
        let json = serde_json::to_value(&unproven).unwrap_or_default();
        assert!(json.get("native_join").is_none(), "{json}");
    }
}

#[cfg(test)]
mod session_variable_tests {
    #![allow(clippy::unwrap_used)] // Reason: test code, panics are acceptable

    use super::*;

    fn every_source() -> [SessionVariableSource; 4] {
        [
            SessionVariableSource::Jwt {
                claim: "sub".to_owned(),
            },
            SessionVariableSource::Header {
                header: "x-region".to_owned(),
            },
            SessionVariableSource::Literal {
                value: "eu".to_owned(),
            },
            SessionVariableSource::Enrichment {
                field: "actor_role".to_owned(),
            },
        ]
    }

    /// What the compiler writes is what the server reads back, for every source.
    #[test]
    fn every_source_round_trips_through_the_compiled_json() {
        for source in every_source() {
            let mapping = SessionVariableMapping {
                name: "app.x".to_owned(),
                source,
            };
            let json = serde_json::to_value(&mapping).unwrap();
            let back: SessionVariableMapping = serde_json::from_value(json.clone()).unwrap();
            assert_eq!(back, mapping, "{json}");
        }
    }

    /// A key beside the ones the compiler writes is refused, not dropped.
    #[test]
    fn a_key_the_mapping_does_not_write_is_refused() {
        for source in every_source() {
            let mut json = serde_json::to_value(SessionVariableMapping {
                name: "app.x".to_owned(),
                source,
            })
            .unwrap();
            json["not_a_key"] = serde_json::json!(1);
            let refused = serde_json::from_value::<SessionVariableMapping>(json.clone());
            let message = refused.unwrap_err().to_string();
            assert!(message.contains("not_a_key"), "{json}: {message}");
        }
    }
}
