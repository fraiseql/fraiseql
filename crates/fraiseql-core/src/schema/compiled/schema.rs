//! Compiled schema types - pure Rust, no authoring-language references.
//!
//! These types represent GraphQL schemas after compilation from authoring languages.
//! All data is owned by Rust - no foreign object references.
//!
//! # Schema Freeze Invariant
//!
//! After `CompiledSchema::from_json()`, the schema is frozen:
//! - All data is Rust-owned
//! - No authoring-language callbacks or object references
//! - Safe to use from any Tokio worker thread
//!
//! This enables the Axum server to handle requests without any
//! interaction with the authoring-language runtime.

use std::collections::HashMap;

use serde::{Deserialize, Serialize};

use super::{directive::DirectiveDefinition, mutation::MutationDefinition, query::QueryDefinition};
use crate::{
    compiler::fact_table::FactTableMetadata,
    schema::{
        config_types::{
            ChangelogConfig, DebugConfig, FederationConfig, GrpcConfig, McpConfig,
            NamingConvention, ObserversConfig, RestConfig, SessionVariablesConfig,
            SubscriptionsConfig, ValidationConfig,
        },
        graphql_type_defs::{
            EnumDefinition, InputObjectDefinition, InterfaceDefinition, TypeDefinition,
            UnionDefinition,
        },
        hierarchy::HierarchiesConfig,
        observer_types::ObserverDefinition,
        security_config::SecurityConfig,
        source_types::SourceDefinition,
        subscription_types::SubscriptionDefinition,
    },
    validation::CustomTypeRegistry,
};

/// Current schema format version.
///
/// Increment this constant when the compiled schema JSON format changes in a
/// backward-incompatible way so that startup rejects stale compiled schemas.
pub const CURRENT_SCHEMA_FORMAT_VERSION: u32 = 1;

/// A `@subscribable` declaration in the compiled schema (#366).
///
/// Maps a GraphQL type to the physical base table(s) whose **external** writes
/// (a raw `INSERT`/`UPDATE`/`DELETE` from psql / a migration / a third-party
/// tool) should be captured onto the Change Spine by the shipped fallback trigger
/// `core.fn_entity_change_log_capture`. The compiler aggregates one of these per
/// type carrying `@subscribable(tables=[...])`; the
/// [`generate_capture_trigger_ddl`](crate::schema::generate_capture_trigger_ddl)
/// generator turns them into per-table statement-level triggers that stamp
/// `object_type = entity_type` — the GraphQL type name the reader and the
/// subscription matcher key on, never the table name — so a captured external
/// write fans out through the existing poller with no table→type lookup.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SubscribableEntity {
    /// The GraphQL type name (e.g. `"Post"`) stamped as `object_type` on every
    /// captured change-log row.
    pub entity_type: String,

    /// The physical base table(s) backing `entity_type` (e.g. `["tb_post"]`,
    /// optionally schema-qualified `["public.tb_post"]`). A capture trigger is
    /// installed on each.
    pub tables: Vec<String>,

    /// Whether the capture triggers on this entity's tables also record the
    /// changed entity's **pre-image** (OLD) into `object_data_before` — the
    /// out-of-band parity for the per-mutation
    /// [`changelog_pre_image`](super::MutationDefinition::changelog_pre_image).
    ///
    /// The trigger always unifies `object_data` on the after-image (NEW)
    /// regardless of this flag; `pre_image` only adds the separate before-image
    /// column for opted-in tables, so audit-sensitive entities get an inline
    /// Debezium `{before, after}` even for raw external writes. Default `false`
    /// (opt in via `@subscribable(tables=[...], pre_image=True)`); an absent value
    /// is byte-identical to before this field existed, so it does not churn the
    /// codegen schema hash.
    #[serde(default, skip_serializing_if = "core::ops::Not::not")]
    pub pre_image: bool,
}

/// Complete compiled schema - all type information for serving.
///
/// This is the central type that holds the entire GraphQL schema
/// after compilation from any supported authoring language.
///
/// # Example
///
/// ```
/// use fraiseql_core::schema::CompiledSchema;
///
/// let json = r#"{
///     "types": [],
///     "queries": [],
///     "mutations": [],
///     "subscriptions": []
/// }"#;
///
/// let schema = CompiledSchema::from_json(json, false).unwrap();
/// assert_eq!(schema.types.len(), 0);
/// ```
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct CompiledSchema {
    /// GraphQL object type definitions.
    #[serde(default)]
    pub types: Vec<TypeDefinition>,

    /// GraphQL enum type definitions.
    #[serde(default)]
    pub enums: Vec<EnumDefinition>,

    /// GraphQL input object type definitions.
    #[serde(default)]
    pub input_types: Vec<InputObjectDefinition>,

    /// GraphQL interface type definitions.
    #[serde(default)]
    pub interfaces: Vec<InterfaceDefinition>,

    /// GraphQL union type definitions.
    #[serde(default)]
    pub unions: Vec<UnionDefinition>,

    /// GraphQL query definitions.
    #[serde(default)]
    pub queries: Vec<QueryDefinition>,

    /// GraphQL mutation definitions.
    #[serde(default)]
    pub mutations: Vec<MutationDefinition>,

    /// GraphQL subscription definitions.
    #[serde(default)]
    pub subscriptions: Vec<SubscriptionDefinition>,

    /// Custom directive definitions.
    /// These are user-defined directives beyond the built-in @skip, @include, @deprecated.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub directives: Vec<DirectiveDefinition>,

    /// Fact table metadata (for analytics queries).
    /// Key: table name (e.g., `tf_sales`)
    #[serde(default, skip_serializing_if = "HashMap::is_empty")]
    pub fact_tables: HashMap<String, FactTableMetadata>,

    /// Observer definitions (database change event listeners).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub observers: Vec<ObserverDefinition>,

    /// Scheduled ingress source definitions (#573) — the dual of `observers`.
    ///
    /// Each runs its `function` on a cron schedule, pulling from an external system
    /// into the database via mutations with a durable cursor. Empty (and omitted
    /// from the compiled JSON) when no source is declared, so a schema that predates
    /// this field deserializes and re-serializes byte-for-byte unchanged.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub sources: Vec<SourceDefinition>,

    /// `@subscribable` declarations (#366): GraphQL types whose underlying
    /// table(s) get the shipped external-write capture trigger.
    ///
    /// Aggregated by the compiler from each type's `@subscribable(tables=[...])`
    /// annotation; consumed by
    /// [`generate_capture_trigger_ddl`](crate::schema::generate_capture_trigger_ddl)
    /// to emit per-table capture triggers. Empty (and omitted from the compiled
    /// JSON) when no type is subscribable — so a schema that predates this field
    /// deserializes and re-serializes byte-for-byte unchanged.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub subscribable: Vec<SubscribableEntity>,

    /// Per-operation `@cost(weight: N)` overrides (#379): root query/mutation name
    /// → manual cost weight, consulted by the runtime per-tenant cost-budget check
    /// (`estimate_query_cost`) so a top-level operation counts as exactly `N`
    /// instead of its walked subtree complexity. Aggregated by the compiler from
    /// each operation's `@cost` annotation. Empty (and omitted from the compiled
    /// JSON) when no operation carries `@cost` — so a schema that predates this
    /// field deserializes and re-serializes byte-for-byte unchanged.
    #[serde(default, skip_serializing_if = "HashMap::is_empty")]
    pub operation_cost_weights: HashMap<String, usize>,

    /// Federation metadata for Apollo Federation v2 support.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub federation: Option<FederationConfig>,

    /// Security configuration (from fraiseql.toml).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub security: Option<SecurityConfig>,

    /// PKCE OAuth-client configuration for server-side login (`[auth]` PKCE group).
    ///
    /// Carries the OIDC client identity (`client_id`, `client_secret_env`,
    /// `server_redirect_uri`) and the provider's `discovery_url`. The runtime
    /// resolves the `authorization_endpoint` / `token_endpoint` by fetching the
    /// discovery document **at boot** from `discovery_url` (#621) — the compiler
    /// stays hermetic and the discovery document is always fresh. `None` when the
    /// `[auth]` PKCE client group is absent.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub auth: Option<AuthClientConfig>,

    /// Observers/event system configuration (from fraiseql.toml).
    ///
    /// Contains backend connection settings (`redis_url`, `nats_url`, etc.) and
    /// event handler definitions compiled from the `[observers]` TOML section.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub observers_config: Option<ObserversConfig>,

    /// `WebSocket` subscription configuration (hooks, limits).
    /// Compiled from the `[subscriptions]` TOML section.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub subscriptions_config: Option<SubscriptionsConfig>,

    /// Query validation config (depth/complexity limits).
    /// Compiled from the `[validation]` TOML section.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub validation_config: Option<ValidationConfig>,

    /// Debug/development configuration.
    /// Compiled from the `[debug]` TOML section.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub debug_config: Option<DebugConfig>,

    /// MCP (Model Context Protocol) server configuration.
    /// Compiled from the `[mcp]` TOML section.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mcp_config: Option<McpConfig>,

    /// REST transport configuration.
    /// Compiled from the `[rest]` TOML section.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rest_config: Option<RestConfig>,

    /// gRPC transport configuration.
    /// Compiled from the `[grpc]` TOML section.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub grpc_config: Option<GrpcConfig>,

    /// Changelog GraphQL-exposure configuration.
    ///
    /// Compiled from the `[changelog]` TOML section. When present with
    /// `expose = true`, the compiler injects the `EntityChangeLog` /
    /// `TransportCheckpoint` types plus their cursor query, point-lookup query, and
    /// checkpoint upsert mutation. `None` when the block is absent (the default).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub changelog: Option<ChangelogConfig>,

    /// Session variable injection configuration.
    ///
    /// When populated, the executor calls PostgreSQL `set_config()` before each
    /// mutation, injecting per-request values (JWT claims, HTTP headers, literals)
    /// as transaction-scoped settings.  SQL functions read these via
    /// `current_setting('app.tenant_id', true)`.
    ///
    /// Compiled from the `[session_variables]` TOML section.
    #[serde(default)]
    pub session_variables: SessionVariablesConfig,

    /// Hierarchy definitions for ID-based ltree operators.
    ///
    /// Maps hierarchy names to `table`/`path_column` pairs. Compiled from the
    /// `[hierarchies]` TOML section. Used at runtime to resolve `HierarchyContext`
    /// for `descendantOfId` / `ancestorOfId` WHERE clause generation.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub hierarchies_config: Option<HierarchiesConfig>,

    /// Naming convention for GraphQL operation names.
    ///
    /// When set to `CamelCase`, operation names are converted from `snake_case`
    /// (e.g., `create_dns_server` → `createDnsServer`) in the introspection
    /// schema and lookup indexes. Compiled from `[fraiseql]` in `fraiseql.toml`.
    #[serde(default)]
    pub naming_convention: NamingConvention,

    /// Acronyms whose internal digit stays attached when resolving a GraphQL field
    /// name back to its `snake_case` JSONB key (e.g. `s3`, `ipv4`, `oauth2`). Added
    /// to the built-in defaults at boot via `fraiseql_db::utils::set_runtime_acronyms`.
    /// Skipped when empty so a schema with no project acronyms serializes byte-for-byte
    /// as before this field existed (no schema-hash churn; back-compat on load).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub naming_acronyms: Vec<String>,

    /// Schema format version emitted by the compiler.
    ///
    /// Used to detect runtime/compiler skew. If present and ≠ `CURRENT_SCHEMA_FORMAT_VERSION`,
    /// `validate_format_version()` returns an error.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub schema_format_version: Option<u32>,

    /// Raw GraphQL schema as string (for SDL generation).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub schema_sdl: Option<String>,

    /// Custom scalar type registry.
    ///
    /// Contains definitions for custom scalar types defined in the schema.
    /// Built during code generation from `IRScalar` definitions.
    /// Not serialized - populated at runtime from `ir.scalars`.
    #[serde(skip)]
    pub custom_scalars: CustomTypeRegistry,

    /// O(1) lookup index: query name → index into `self.queries`.
    /// Built at construction time by `build_indexes()`; not serialized.
    /// Populated automatically by `from_json()`; call `build_indexes()` after
    /// direct mutation of `self.queries`.
    #[serde(skip)]
    pub query_index: HashMap<String, usize>,

    /// O(1) lookup index: mutation name → index into `self.mutations`.
    /// Built at construction time by `build_indexes()`; not serialized.
    /// Populated automatically by `from_json()`; call `build_indexes()` after
    /// direct mutation of `self.mutations`.
    #[serde(skip)]
    pub mutation_index: HashMap<String, usize>,

    /// O(1) lookup index: subscription name → index into `self.subscriptions`.
    /// Built at construction time by `build_indexes()`; not serialized.
    /// Populated automatically by `from_json()`; call `build_indexes()` after
    /// direct mutation of `self.subscriptions`.
    #[serde(skip)]
    pub subscription_index: HashMap<String, usize>,

    /// The `where` keys of every type a nested predicate can descend into.
    ///
    /// A nested `where` level is adjudicated against the *target* type's keys —
    /// `{machine: {bogus: {eq: …}}}` is refused because `MachineWhereInput` has
    /// no `bogus`, rather than lowering to a JSON path that matches nothing.
    /// Built at construction time by `build_indexes()`; not serialized.
    #[serde(skip)]
    pub where_relation_fields: fraiseql_db::where_clause::RelationFieldMaps,
}

impl PartialEq for CompiledSchema {
    fn eq(&self, other: &Self) -> bool {
        // Compare all fields except custom_scalars (runtime state)
        self.schema_format_version == other.schema_format_version
            && self.types == other.types
            && self.enums == other.enums
            && self.input_types == other.input_types
            && self.interfaces == other.interfaces
            && self.unions == other.unions
            && self.queries == other.queries
            && self.mutations == other.mutations
            && self.subscriptions == other.subscriptions
            && self.directives == other.directives
            && self.fact_tables == other.fact_tables
            && self.observers == other.observers
            && self.sources == other.sources
            && self.subscribable == other.subscribable
            && self.federation == other.federation
            && self.security == other.security
            && self.auth == other.auth
            && self.observers_config == other.observers_config
            && self.subscriptions_config == other.subscriptions_config
            && self.validation_config == other.validation_config
            && self.debug_config == other.debug_config
            && self.mcp_config == other.mcp_config
            && self.changelog == other.changelog
            && self.naming_convention == other.naming_convention
            && self.naming_acronyms == other.naming_acronyms
            && self.schema_sdl == other.schema_sdl
    }
}

impl CompiledSchema {
    /// Create empty schema.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }
}

/// OAuth-client configuration compiled from the `[auth]` client groups.
///
/// Two independent groups, each optional (at least one is present — the CLI
/// refuses to compile an empty group set): the PKCE server-side login client
/// (#621, `[auth]`'s `discovery_url`/`client_id`/`client_secret_env`/
/// `server_redirect_uri` quadruple, lowered into [`pkce`](Self::pkce)) and the
/// social-login providers (#368, `[auth.social.*]`, lowered into
/// [`social`](Self::social)). Client secrets are **never** carried here — each
/// group names the environment variable the runtime reads its secret from.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AuthClientConfig {
    /// PKCE OAuth client for server-side login (`/auth/start`, `/auth/callback`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pkce:   Option<PkceClientConfig>,
    /// Social-login provider registry (`/auth/v1/authorize`, `/auth/v1/callback`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub social: Option<SocialAuthConfig>,
    /// First-party auth methods this server operates itself (`[auth.local]`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub local:  Option<LocalAuthConfig>,
}

/// `[auth.local]` — the auth methods `FraiseQL` operates itself, rather than
/// delegating to an `IdP` (#367).
///
/// Every enabled method mounts real `HTTP` routes and requires a database pool:
/// credentials, `MFA` enrollments, `OTP` budgets and sessions are all durable
/// state. A method enabled without what it needs refuses to boot rather than
/// mounting a flow that cannot complete.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LocalAuthConfig {
    /// Email + password sign-in: mounts `/auth/v1/password/{signup,login}` and
    /// the reset pair. Reset-link delivery additionally requires
    /// [`email_from`](Self::email_from).
    #[serde(default)]
    pub password: bool,

    /// Email `OTP` / magic-link sign-in: mounts `/auth/v1/otp` and
    /// `/auth/v1/verify`. Requires [`email_from`](Self::email_from) — a code
    /// nobody receives is not a login method.
    #[serde(default)]
    pub otp: bool,

    /// `TOTP` `MFA`: mounts `/auth/v1/mfa/{enroll,challenge,verify,unenroll}`,
    /// backed by the Postgres enrollment store (enrollments must survive a
    /// restart, or a deploy locks every user out of their own account).
    #[serde(default)]
    pub mfa: bool,

    /// Anonymous guest sessions: mounts `POST /auth/v1/signup`, which issues a
    /// session to any caller with no credentials at all. Off by default — this
    /// is a deliberate "anyone may hold a session" posture, not a convenience.
    #[serde(default)]
    pub anonymous: bool,

    /// Email verification for local-password accounts (#945): mounts
    /// `POST /auth/v1/email/verify/{start,confirm}`, both of which require an
    /// authenticated caller and act only on that caller's own account.
    ///
    /// Requires [`password`](Self::password) — verification proves the address a
    /// *local* identity claims, and an account without one has nothing to verify
    /// — plus [`email_from`](Self::email_from) and
    /// [`verification_url_template`](Self::verification_url_template).
    ///
    /// Confirming promotes the account's own `core.tb_user.email`, which is what
    /// lets a later trusted social sign-in for the same address link into it. It
    /// never merges two accounts: an address already verified elsewhere is
    /// refused.
    #[serde(default)]
    pub email_verification: bool,

    /// Service name shown in authenticator apps for `MFA` enrollment
    /// (`otpauth://…?issuer=`). Defaults to `"`FraiseQL`"`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mfa_issuer: Option<String>,

    /// The `[mailbox.<name>]` account whose `SMTP` half delivers `OTP` codes and
    /// password-reset links. Required whenever `otp` or `password` is enabled;
    /// naming a mailbox with no `[mailbox.<name>.smtp]` section refuses to boot.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub email_from: Option<String>,

    /// Template for the password-reset link, with `{token}` substituted for the
    /// opaque reset token — e.g. `https://app.example.com/reset?token={token}`.
    /// Required when `password` is enabled: the link points at the operator's
    /// front end, which `FraiseQL` cannot guess.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reset_url_template: Option<String>,

    /// Template for the `OTP` magic link, with `{code}` substituted. Optional:
    /// when absent the email carries the bare six-digit code.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub magic_link_template: Option<String>,

    /// Template for the email-verification link, with `{token}` substituted for
    /// the opaque verification token — e.g.
    /// `https://app.example.com/verify-email?token={token}`. Required when
    /// [`email_verification`](Self::email_verification) is enabled: the link
    /// points at the operator's front end, which `FraiseQL` cannot guess.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub verification_url_template: Option<String>,
}

/// PKCE OAuth-client configuration compiled from the `[auth]` PKCE group (#621).
///
/// The client secret is **never** carried here — `client_secret_env` names the
/// environment variable the runtime reads it from. The `authorization_endpoint` /
/// `token_endpoint` are deliberately absent: the runtime resolves them at boot by
/// fetching `discovery_url`'s `.well-known/openid-configuration`, so the compiler
/// performs no network I/O and the endpoints cannot go stale in a cached schema.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PkceClientConfig {
    /// `OIDC` provider discovery base URL (e.g. `https://accounts.google.com`). The
    /// runtime appends `/.well-known/openid-configuration` at boot.
    pub discovery_url:       String,
    /// `OAuth2` `client_id` registered with the provider.
    pub client_id:           String,
    /// Name of the environment variable holding the client secret.
    pub client_secret_env:   String,
    /// This server's `/auth/callback` URL, registered with the provider.
    pub server_redirect_uri: String,
}

/// Social-login provider configuration compiled from `[auth.social]` (#368).
///
/// Each configured provider is auto-registered by the server and served through
/// the account-linking trust gate at `/auth/v1/authorize` / `/auth/v1/callback`.
/// Only implemented providers are typable: an `[auth.social.apple]` block is a
/// compile-time unknown-field error, not a silently-ignored table.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SocialAuthConfig {
    /// Allow-listed `redirect_uri` values for the post-login token hand-off
    /// (#427). Empty (the default) keeps the JSON-token response: the
    /// client-supplied `redirect_uri` is never used as a redirect target, so
    /// there is no open-redirect surface. Non-empty switches the callback to an
    /// implicit-style fragment redirect, refusing any URI not matched here.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub redirect_uri_allowlist: Vec<String>,

    /// `Google` `OIDC` social login.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub google: Option<GoogleSocialConfig>,

    /// `GitHub` `OAuth2` social login (non-`OIDC`; fixed well-known endpoints).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub github: Option<GitHubSocialConfig>,

    /// Sign in with `Apple`. Configuring it also mounts the `POST` variant of
    /// `/auth/v1/callback` that `Apple`'s `response_mode=form_post` requires.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub apple: Option<AppleSocialConfig>,

    /// `Discord` `OAuth2` social login (non-`OIDC`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub discord: Option<DiscordSocialConfig>,

    /// `Facebook` `OAuth2` social login (non-`OIDC`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub facebook: Option<FacebookSocialConfig>,
}

/// `[auth.social.google]` — `Google` `OIDC` social-login client (#368).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GoogleSocialConfig {
    /// `OAuth2` `client_id` from the `Google` Cloud Console.
    pub client_id:         String,
    /// Name of the environment variable holding the client secret.
    pub client_secret_env: String,
    /// This server's `/auth/v1/callback` URL, registered with `Google`.
    pub redirect_uri:      String,
    /// `OIDC` issuer override (default `https://accounts.google.com`). For
    /// `Google`-compatible stand-ins; the runtime's SSRF guards still apply.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub discovery_url:     Option<String>,
}

/// `[auth.social.github]` — `GitHub` `OAuth2` social-login client (#368).
///
/// `GitHub` serves no `OIDC` discovery document; the endpoints are the fixed
/// well-known paths under [`base_url`](Self::base_url) /
/// [`api_base_url`](Self::api_base_url), overridable for `GitHub` Enterprise
/// Server deployments.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GitHubSocialConfig {
    /// `OAuth2` `client_id` of the `GitHub` OAuth app.
    pub client_id:         String,
    /// Name of the environment variable holding the client secret.
    pub client_secret_env: String,
    /// This server's `/auth/v1/callback` URL, registered with `GitHub`.
    pub redirect_uri:      String,
    /// Web base URL override (default `https://github.com`) — `GitHub` Enterprise
    /// Server. The runtime's SSRF guards still apply.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub base_url:          Option<String>,
    /// API base URL override (default `https://api.github.com`) — `GitHub`
    /// Enterprise Server (`https://HOSTNAME/api/v3`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub api_base_url:      Option<String>,
}

/// `[auth.social.apple]` — Sign in with `Apple` (#943).
///
/// `Apple` has no `client_secret_env`: its client secret is an ES256 assertion
/// the runtime mints from the `.p8` key over
/// ([`team_id`](Self::team_id), [`key_id`](Self::key_id),
/// [`client_id`](Self::client_id)) and re-mints before expiry. Supply the key
/// through exactly one of [`private_key_env`](Self::private_key_env) or
/// [`private_key_path`](Self::private_key_path) — naming both, or neither, is a
/// configuration error rather than a silent preference.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AppleSocialConfig {
    /// The services ID registered with `Apple` (its `OAuth2` `client_id`, and
    /// the `aud` of every `id_token`).
    pub client_id:        String,
    /// `Apple` developer team ID — the client-secret assertion's `iss`.
    pub team_id:          String,
    /// Key ID of the `.p8` signing key — the assertion header's `kid`.
    pub key_id:           String,
    /// Name of the environment variable holding the `.p8` private key, PEM text
    /// and all. Mutually exclusive with
    /// [`private_key_path`](Self::private_key_path).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub private_key_env:  Option<String>,
    /// Filesystem path to the `.p8` private key. Mutually exclusive with
    /// [`private_key_env`](Self::private_key_env).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub private_key_path: Option<String>,
    /// This server's `/auth/v1/callback` URL, registered with `Apple`.
    pub redirect_uri:     String,
    /// Base URL override (default `https://appleid.apple.com`). This is also
    /// the issuer every `id_token` must name, so the two move together. The
    /// runtime's SSRF guards still apply.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub base_url:         Option<String>,
}

/// `[auth.social.discord]` — `Discord` `OAuth2` social login (#944).
///
/// `Discord` reports `verified` on the user object, and the runtime honours it:
/// an unverified address keys on `(discord, id)` rather than on the email. That
/// check is why `discord` is in the default trusted-email set.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DiscordSocialConfig {
    /// `OAuth2` `client_id` of the `Discord` application.
    pub client_id:         String,
    /// Name of the environment variable holding the client secret.
    pub client_secret_env: String,
    /// This server's `/auth/v1/callback` URL, registered with `Discord`.
    pub redirect_uri:      String,
    /// Base URL override (default `https://discord.com`). The runtime's SSRF
    /// guards still apply.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub base_url:          Option<String>,
}

/// `[auth.social.facebook]` — `Facebook` `OAuth2` social login (#944).
///
/// `Facebook` publishes **no** email-verification signal, so every `Facebook`
/// identity keys on `(facebook, id)` and `facebook` is deliberately absent from
/// the default trusted-email set. The Graph API version is part of the request
/// path and `Meta` deprecates versions on a schedule, so it is configuration
/// rather than a constant.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FacebookSocialConfig {
    /// `OAuth2` `client_id` (the `Meta` app ID).
    pub client_id:         String,
    /// Name of the environment variable holding the client secret.
    pub client_secret_env: String,
    /// This server's `/auth/v1/callback` URL, registered with `Meta`.
    pub redirect_uri:      String,
    /// Graph API version segment (default `v21.0`), e.g. `v22.0`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub api_version:       Option<String>,
    /// Web base URL override (default `https://www.facebook.com`) — the
    /// authorization dialog. The runtime's SSRF guards still apply.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub base_url:          Option<String>,
    /// Graph API base URL override (default `https://graph.facebook.com`) — the
    /// token and profile endpoints. The runtime's SSRF guards still apply.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub graph_base_url:    Option<String>,
}
