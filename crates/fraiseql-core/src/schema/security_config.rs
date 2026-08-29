//! Security configuration types for compiled schemas.
//!
//! Contains role definitions, scope types, and injected parameter sources
//! that are compiled from `fraiseql.toml` into `schema.compiled.json`.

use std::collections::HashMap;

use serde::{Deserialize, Serialize};

use super::domain_types::{RoleName, Scope};

#[cfg(test)]
mod tests;

/// Source from which an injected SQL parameter is resolved at runtime.
///
/// Injected parameters are not exposed in the GraphQL schema. They are
/// silently added to SQL queries and function calls, resolved from the
/// authenticated request context.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "source", content = "claim", rename_all = "snake_case")]
#[non_exhaustive]
pub enum InjectedParamSource {
    /// Extract a value from the JWT claims.
    ///
    /// Special aliases resolved before attribute lookup:
    /// - `"sub"` → `SecurityContext.user_id`
    /// - `"tenant_id"` / `"org_id"` → `SecurityContext.tenant_id`
    /// - any other name → `SecurityContext.attributes.get(name)`
    Jwt(String),
    /// Extract a DB-resolved enriched-identity field, read from the reserved
    /// `fraiseql.enriched.*` attribute namespace (#539).
    ///
    /// Unlike [`InjectedParamSource::Jwt`], there is **no** fallback to a raw
    /// claim or a well-known field: the namespace is forge-proof and a missing
    /// enriched field is a hard error, so a raw claim of the same name can never
    /// impersonate a DB-derived identity parameter. The wrapped value is the
    /// enriched field name, without the `fraiseql.enriched.` prefix.
    Enrichment(String),
}

/// Role definition for field-level RBAC.
///
/// Defines which GraphQL scopes a role grants access to.
/// Used by the runtime to determine which fields a user can access
/// based on their assigned roles.
///
/// # Example
///
/// ```json
/// {
///   "name": "admin",
///   "description": "Administrator with all scopes",
///   "scopes": ["admin:*"]
/// }
/// ```
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RoleDefinition {
    /// Role name (e.g., "admin", "user", "viewer").
    pub name: RoleName,

    /// Optional role description for documentation.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,

    /// List of scopes this role grants access to.
    /// Scopes follow the format: `action:resource` (e.g., "read:User.email", "admin:*")
    pub scopes: Vec<Scope>,
}

impl RoleDefinition {
    /// Create a new role definition.
    #[must_use]
    pub fn new(name: impl Into<String>, scopes: Vec<String>) -> Self {
        Self {
            name:        RoleName::new(name),
            description: None,
            scopes:      scopes.into_iter().map(Scope::new).collect(),
        }
    }

    /// Add a description to the role.
    #[must_use]
    pub fn with_description(mut self, description: String) -> Self {
        self.description = Some(description);
        self
    }

    /// Check if this role has a specific scope.
    ///
    /// Supports exact matching and wildcard patterns:
    /// - `read:User.email` matches exactly
    /// - `read:*` matches any scope starting with "read:"
    /// - `read:User.*` matches "read:User.email", "read:User.name", etc.
    /// - `admin:*` matches any admin scope
    #[must_use]
    pub fn has_scope(&self, required_scope: &str) -> bool {
        self.scopes.iter().any(|scope| {
            let scope = scope.as_str();
            if scope == "*" {
                return true; // Wildcard matches everything
            }

            if scope == required_scope {
                return true; // Exact match
            }

            // Handle wildcard patterns like "read:*" or "admin:*". The delimiter is
            // part of the match: `read:*` grants `read:…` only, never `readwrite:…`
            // scopes that merely share the string prefix (#784).
            if let Some(prefix) = scope.strip_suffix(":*") {
                return required_scope
                    .strip_prefix(prefix)
                    .is_some_and(|rest| rest.starts_with(':'));
            }

            // Handle Type.* wildcard patterns like "read:User.*". Only a prefix
            // ending on a `.`/`:` boundary is a wildcard grant; a bare prefix like
            // "read*" would otherwise also match "readwrite:…" (#784).
            if let Some(prefix) = scope.strip_suffix('*') {
                return (prefix.ends_with('.') || prefix.ends_with(':'))
                    && required_scope.starts_with(prefix);
            }

            false
        })
    }
}

/// Tenancy isolation mode for multi-tenant deployments.
///
/// Determines how tenant data is separated at the database level.
/// Configured via `[fraiseql.tenancy]` in `fraiseql.toml`.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[non_exhaustive]
pub enum TenancyMode {
    /// Single-tenant deployment, no isolation machinery.
    #[default]
    None,

    /// Row-level isolation: shared tables with `@tenant_id` column injection.
    ///
    /// The compiler validates that all queries/mutations referencing
    /// `@tenant_id`-annotated types have `inject_params` wired correctly.
    /// At runtime, `InjectedParamSource::Jwt` resolves the tenant claim
    /// and injects a WHERE clause.
    Row,

    /// Schema-level isolation: per-tenant PostgreSQL schemas.
    ///
    /// Each tenant's connection pool is *established* with
    /// `search_path = tenant_{key},public` (lowered into the PostgreSQL startup
    /// `options` parameter), so every connection the pool opens carries it —
    /// including ones opened later to grow the pool and replacements created
    /// after a backend disconnects. `TenantExecutorFactory` provisions the
    /// schema via DDL at registration and verifies the isolation took before
    /// returning an executor.
    ///
    /// It is deliberately **not** a `SET search_path` statement: that is
    /// session-scoped, so it configures one pooled connection and leaves the
    /// rest resolving against `public` (#809).
    Schema,
}

impl std::fmt::Display for TenancyMode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::None => write!(f, "none"),
            Self::Row => write!(f, "row"),
            Self::Schema => write!(f, "schema"),
        }
    }
}

/// Tenancy configuration for multi-tenant deployments.
///
/// Compiled from `[fraiseql.tenancy]` in `fraiseql.toml` into the
/// `security.tenancy` section of `schema.compiled.json`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TenancyConfig {
    /// Isolation strategy: `"none"`, `"row"`, or `"schema"`.
    #[serde(default)]
    pub mode: TenancyMode,

    /// JWT claim name that carries the tenant identifier.
    ///
    /// Defaults to `"tenant_id"`. Used by `InjectedParamSource::Jwt` to
    /// resolve the tenant at runtime, and by the compiler to validate
    /// `@tenant_id` annotations in row mode.
    #[serde(default = "default_tenant_claim")]
    pub tenant_claim: String,
}

fn default_tenant_claim() -> String {
    "tenant_id".to_string()
}

impl Default for TenancyConfig {
    fn default() -> Self {
        Self {
            mode:         TenancyMode::None,
            tenant_claim: default_tenant_claim(),
        }
    }
}

/// Returns `true` when tenancy config equals the default (mode=none, `claim=tenant_id`).
///
/// Used by serde to skip serializing the tenancy field when it carries no information.
fn is_default_tenancy(t: &TenancyConfig) -> bool {
    *t == TenancyConfig::default()
}

/// Declaration that this deployment relies on database Row-Level Security.
///
/// Compiled from `[security.rls]` in `fraiseql.toml`. FraiseQL does not author RLS
/// policies — they live in the database, keyed on the session variables the runtime
/// sets from the request identity — so the compiled schema can only carry the
/// operator's *declaration* that they exist. The server turns that declaration into
/// a checkable claim: when a multi-tenant schema declares RLS, boot verifies against
/// the live catalog that policies really are in force, and refuses to start if they
/// are not (#762).
///
/// Before this section existed, [`CompiledSchema::has_rls_configured`] counted
/// `security.additional["policies"]` — a key #612 made a hard compile error — so it
/// answered `false` for every schema any supported workflow could produce, and the
/// gates that depend on it were inert (#758).
///
/// [`CompiledSchema::has_rls_configured`]: crate::schema::CompiledSchema::has_rls_configured
// Deliberately not `Copy`: the serde `skip_serializing_if` predicate must take a
// reference, and a config struct that gains a second field would lose `Copy` anyway.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct RlsConfig {
    /// Whether database Row-Level Security is relied upon for data isolation.
    pub enabled: bool,
}

/// Returns `true` when the RLS declaration carries no information.
fn is_default_rls(r: &RlsConfig) -> bool {
    *r == RlsConfig::default()
}

/// Security configuration from fraiseql.toml.
///
/// Contains role definitions and other security-related settings
/// that are compiled into schema.compiled.json.
///
/// `deny_unknown_fields`: this object is the seam seven security subsystems are
/// configured through, and it used to carry a `#[serde(flatten)]` catch-all that
/// seven string lookups read — so `rate_limitting` or `token_revokation` in a
/// compiled schema landed in the catch-all, the lookup missed, and the subsystem
/// came up silently unconfigured (#977). A typo'd key must fail the load, not
/// disable the subsystem it names (the compiled-schema seam rule).
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SecurityConfig {
    /// Role definitions mapping role names to their granted scopes.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub role_definitions: Vec<RoleDefinition>,

    /// Default role when none is specified.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub default_role: Option<String>,

    /// Whether this schema serves multiple tenants with data isolation via RLS.
    ///
    /// Declared as `[security] multi_tenant = true`. A non-`none`
    /// [`tenancy.mode`](TenancyConfig::mode) implies it, so this flag is only needed
    /// by deployments that separate tenants without FraiseQL's own tenancy
    /// machinery — read [`CompiledSchema::is_multi_tenant`], never this field, when
    /// asking whether a deployment is multi-tenant.
    ///
    /// [`CompiledSchema::is_multi_tenant`]: crate::schema::CompiledSchema::is_multi_tenant
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub multi_tenant: bool,

    /// Declaration that database Row-Level Security isolates this schema's data.
    ///
    /// Compiled from `[security.rls]`. Consumed by the boot gate: a multi-tenant
    /// schema with caching enabled and no RLS declaration refuses to start, and one
    /// that *does* declare RLS has the declaration verified against the live
    /// database catalog.
    #[serde(default, skip_serializing_if = "is_default_rls")]
    pub rls: RlsConfig,

    /// Tenancy isolation configuration for multi-tenant deployments.
    ///
    /// When present and `mode != "none"`, the runtime enforces tenant isolation
    /// at the database level (row-based or schema-based).
    #[serde(default, skip_serializing_if = "is_default_tenancy")]
    pub tenancy: TenancyConfig,

    /// Operation cost budgets (#379), compiled from `[security.cost_budget]`.
    ///
    /// `per_request_max` is enforced by the **executor** (every transport that
    /// executes a GraphQL document), not only by the HTTP handler; the
    /// per-tenant default seeds the tenant registry's rolling windows.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cost_budget: Option<CostBudgetConfig>,

    /// Default authorization policy applied when an operation names none.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub default_policy: Option<String>,

    /// Custom authorization rules, compiled from `[[security.rules]]`.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub rules: Vec<super::config_types::AuthorizationRule>,

    /// Authorization policies (RBAC/ABAC), compiled from `[[security.policies]]`.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub policies: Vec<super::config_types::AuthorizationPolicy>,

    /// Field-level authorization rules, compiled from `[[security.field_auth]]`.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub field_auth: Vec<super::config_types::FieldAuthRule>,

    /// Enterprise security flags, compiled from `[security.enterprise]`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub enterprise: Option<super::config_types::EnterpriseSecurityConfig>,

    /// Error sanitization, compiled from `[security.error_sanitization]`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error_sanitization: Option<ErrorSanitizationConfig>,

    /// Rate limiting, compiled from `[security.rate_limiting]`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rate_limiting: Option<RateLimitingSecurityConfig>,

    /// OAuth state / PKCE blob encryption, compiled from
    /// `[security.state_encryption]`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub state_encryption: Option<StateEncryptionConfig>,

    /// PKCE for OAuth Authorization Code flows, compiled from `[security.pkce]`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pkce: Option<PkceSecurityConfig>,

    /// API key authentication, compiled from `[security.api_keys]`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub api_keys: Option<ApiKeySecurityConfig>,

    /// Token revocation, compiled from `[security.token_revocation]`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub token_revocation: Option<TokenRevocationSecurityConfig>,

    /// Trusted documents / persisted-query allowlist, compiled from
    /// `[security.trusted_documents]`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub trusted_documents: Option<TrustedDocumentsConfig>,

    /// Force persisted-operations-only enforcement (#379).
    ///
    /// When `true`, the runtime forces the trusted-document store into `strict`
    /// mode regardless of `trusted_documents.mode`. Requires a configured
    /// trusted-documents manifest to have any effect.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub persisted_queries_only: bool,

    /// Machine service accounts, keyed by account name (`security.service_accounts`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub service_accounts: Option<HashMap<String, ServiceAccountConfig>>,
}

/// Controls how much error detail is exposed to API clients.
///
/// When enabled, internal error messages, SQL, and stack traces are stripped.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct ErrorSanitizationConfig {
    /// Enable error sanitization (default: false — opt-in).
    pub enabled:                     bool,
    /// Strip stack traces, SQL fragments, file paths (default: true).
    pub hide_implementation_details: bool,
    /// Replace raw database error messages with a generic message (default: true).
    pub sanitize_database_errors:    bool,
    /// Replacement message shown to clients when an internal error is sanitized.
    pub custom_error_message:        Option<String>,
}

impl Default for ErrorSanitizationConfig {
    fn default() -> Self {
        Self {
            enabled:                     false,
            hide_implementation_details: true,
            sanitize_database_errors:    true,
            custom_error_message:        None,
        }
    }
}

/// Default per-map bucket ceiling: ~20 `MiB` of tracking state at ~200 bytes a bucket.
///
/// Named rather than inlined because the server's `RateLimitConfig` must agree with it —
/// the two defaults living in two files with different values is what #977 fixed for the
/// rest of this section.
pub const DEFAULT_RATE_LIMIT_MAX_BUCKETS: usize = 100_000;

/// Per-endpoint and global rate limiting configuration for `[security.rate_limiting]`.
///
/// The single shape for this section: the CLI authors it from TOML and the
/// server consumes it from the compiled schema, so the two cannot drift (before
/// #977 each side kept its own copy — with different defaults).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct RateLimitingSecurityConfig {
    /// Enable rate limiting.
    pub enabled: bool,
    /// Global request rate cap (requests per second, per IP).
    pub requests_per_second: u32,
    /// Burst allowance above the steady-state rate.
    pub burst_size: u32,
    /// Auth initiation endpoint — max requests per window.
    pub auth_start_max_requests: u32,
    /// Auth initiation window in seconds.
    pub auth_start_window_secs: u64,
    /// OAuth callback endpoint — max requests per window.
    pub auth_callback_max_requests: u32,
    /// OAuth callback window in seconds.
    pub auth_callback_window_secs: u64,
    /// Token refresh endpoint — max requests per window.
    pub auth_refresh_max_requests: u32,
    /// Token refresh window in seconds.
    pub auth_refresh_window_secs: u64,
    /// Logout endpoint — max requests per window (#893).
    pub auth_logout_max_requests: u32,
    /// Logout window in seconds.
    pub auth_logout_window_secs: u64,
    /// Maximum failed first-factor login attempts before lockout.
    ///
    /// The off-the-shelf binary performs no first-factor login of its own, so it
    /// cannot enforce this; a value tuned away from the default is rejected at
    /// startup in production (#356) — see the server's `failed_login_lockout_check`.
    pub failed_login_max_attempts: u32,
    /// Lockout window in seconds after `failed_login_max_attempts` is exceeded.
    pub failed_login_lockout_secs: u64,
    /// Per-authenticated-user request rate in requests/second.
    /// Defaults to 10× `requests_per_second` if not set.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub requests_per_second_per_user: Option<u32>,
    /// Redis URL for distributed rate limiting.
    ///
    /// Optional: when unset, budgets are tracked in-memory, per process. When
    /// set, an unreachable Redis is a **boot error** in production (#770/#777
    /// class) — the server does not silently downgrade to per-process limits.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub redis_url: Option<String>,
    /// Trust `X-Real-IP` / `X-Forwarded-For` headers for client IP extraction.
    ///
    /// Set to `true` only when FraiseQL is deployed behind a trusted reverse
    /// proxy that sets these headers. Enabling without a trusted proxy allows
    /// clients to spoof their IP address.
    pub trust_proxy_headers: bool,
    /// CIDR ranges trusted as proxy IPs (e.g. `["10.0.0.0/8"]`).
    ///
    /// When set and `trust_proxy_headers = true`, `X-Forwarded-For` is only
    /// honoured when the direct connection IP falls within one of these ranges.
    /// When omitted with `trust_proxy_headers = true`, all proxy IPs are trusted
    /// (less secure — the server emits a startup warning).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub trusted_proxy_cidrs: Option<Vec<String>>,
    /// Maximum tracked buckets per map — a **memory ceiling, not a security control**.
    ///
    /// The limiter keeps one token bucket per key in each of its maps (per-IP,
    /// per-user, per-path-and-IP, per-tenant). This caps how many it holds at once,
    /// at roughly 200 bytes each: the default 100 000 is about 20 `MiB` per map.
    ///
    /// Reaching the cap evicts the least-recently-used of a sample rather than
    /// refusing the newcomer (#1080/#1143), so setting it low degrades *accuracy* —
    /// a client whose bucket was evicted starts again with a full one — and never
    /// availability. It is a knob to size to the host, not to tune for protection.
    pub max_buckets: usize,
}

impl Default for RateLimitingSecurityConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            requests_per_second: 100,
            requests_per_second_per_user: None,
            burst_size: 200,
            auth_start_max_requests: 5,
            auth_start_window_secs: 60,
            auth_callback_max_requests: 10,
            auth_callback_window_secs: 60,
            auth_refresh_max_requests: 20,
            auth_refresh_window_secs: 300,
            auth_logout_max_requests: 30,
            auth_logout_window_secs: 60,
            failed_login_max_attempts: 10,
            failed_login_lockout_secs: 900,
            redis_url: None,
            trust_proxy_headers: false,
            trusted_proxy_cidrs: None,
            max_buckets: DEFAULT_RATE_LIMIT_MAX_BUCKETS,
        }
    }
}

/// AEAD algorithm for OAuth state and PKCE state blobs.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[non_exhaustive]
pub enum EncryptionAlgorithm {
    /// ChaCha20-Poly1305 (recommended — constant-time, software-friendly).
    #[default]
    #[serde(rename = "chacha20-poly1305")]
    Chacha20Poly1305,
    /// AES-256-GCM (hardware-accelerated on modern CPUs).
    #[serde(rename = "aes-256-gcm")]
    Aes256Gcm,
}

impl std::fmt::Display for EncryptionAlgorithm {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Chacha20Poly1305 => f.write_str("chacha20-poly1305"),
            Self::Aes256Gcm => f.write_str("aes-256-gcm"),
        }
    }
}

/// Where the state-encryption key is sourced from.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
#[non_exhaustive]
pub enum KeySource {
    /// Read the key from an environment variable.
    #[default]
    Env,
}

/// AEAD encryption for the OAuth state parameter and PKCE code challenges.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct StateEncryptionConfig {
    /// Enable state encryption.
    pub enabled:    bool,
    /// AEAD algorithm to use.
    pub algorithm:  EncryptionAlgorithm,
    /// Where to source the encryption key.
    pub key_source: KeySource,
    /// Environment variable holding the 32-byte hex-encoded key.
    pub key_env:    Option<String>,
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
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[non_exhaustive]
pub enum CodeChallengeMethod {
    /// SHA-256 (required in production).
    #[default]
    #[serde(rename = "S256")]
    S256,
    /// Plain (spec-allowed but insecure — warns at runtime).
    #[serde(rename = "plain")]
    Plain,
}

impl CodeChallengeMethod {
    /// The method name as it appears on the wire (`S256` / `plain`).
    #[must_use]
    pub const fn as_str(&self) -> &'static str {
        match self {
            Self::S256 => "S256",
            Self::Plain => "plain",
        }
    }
}

/// PKCE (Proof Key for Code Exchange) configuration for `[security.pkce]`.
///
/// Requires `state_encryption` to be enabled for secure state storage.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct PkceSecurityConfig {
    /// Enable PKCE for OAuth Authorization Code flows.
    pub enabled:               bool,
    /// Code challenge method (`S256` recommended).
    pub code_challenge_method: CodeChallengeMethod,
    /// How long the PKCE state is valid before the auth flow expires (seconds).
    pub state_ttl_secs:        u64,
    /// Redis URL for distributed PKCE state storage across multiple replicas.
    ///
    /// Required for multi-replica deployments; without Redis, `/auth/start` and
    /// `/auth/callback` must hit the same replica. Requires the `redis-pkce`
    /// Cargo feature.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub redis_url:             Option<String>,
}

impl Default for PkceSecurityConfig {
    fn default() -> Self {
        Self {
            enabled:               false,
            code_challenge_method: CodeChallengeMethod::S256,
            state_ttl_secs:        600,
            redis_url:             None,
        }
    }
}

/// API key authentication configuration for `[security.api_keys]`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct ApiKeySecurityConfig {
    /// Enable API key authentication.
    pub enabled:        bool,
    /// HTTP header name to read the API key from.
    pub header:         String,
    /// Hash algorithm for key verification (`sha256`).
    pub hash_algorithm: String,
    /// Storage backend: `"env"` for static keys, `"postgres"` for DB-backed.
    pub storage:        String,
    /// Static API key entries (only for `storage = "env"`).
    #[serde(rename = "static")]
    pub static_keys:    Vec<StaticApiKeyEntry>,
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
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct StaticApiKeyEntry {
    /// Hex-encoded hash, optionally prefixed with the algorithm (`sha256:…`).
    pub key_hash: String,
    /// OAuth-style scopes granted by this key.
    #[serde(default)]
    pub scopes:   Vec<String>,
    /// Human-readable name (for audit logging).
    pub name:     String,
}

/// Trusted document enforcement mode.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
#[non_exhaustive]
pub enum TrustedDocumentMode {
    /// Only documentId requests allowed; raw query strings rejected.
    Strict,
    /// documentId requests use the manifest; raw queries fall through.
    #[default]
    Permissive,
}

/// Trusted documents / query allowlist configuration for
/// `[security.trusted_documents]`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct TrustedDocumentsConfig {
    /// Enable trusted documents.
    pub enabled:              bool,
    /// Enforcement mode: `strict` or `permissive`.
    pub mode:                 TrustedDocumentMode,
    /// Path to the trusted documents manifest JSON file.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub manifest_path:        Option<String>,
    /// URL to fetch the trusted documents manifest from at startup.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub manifest_url:         Option<String>,
    /// Poll interval in seconds for hot-reloading the manifest (0 = no reload).
    pub reload_interval_secs: u64,
}

impl Default for TrustedDocumentsConfig {
    fn default() -> Self {
        Self {
            enabled:              false,
            mode:                 TrustedDocumentMode::Permissive,
            manifest_path:        None,
            manifest_url:         None,
            reload_interval_secs: 0,
        }
    }
}

/// Token revocation configuration for `[security.token_revocation]`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct TokenRevocationSecurityConfig {
    /// Enable token revocation.
    pub enabled:             bool,
    /// Storage backend: `"redis"`, `"postgres"`, or `"memory"`.
    pub backend:             String,
    /// Reject JWTs without a `jti` claim when revocation is enabled.
    pub require_jti:         bool,
    /// If the revocation store is unreachable: `false` = reject (fail-closed),
    /// `true` = allow (fail-open).
    pub fail_open:           bool,
    /// Redis URL for distributed revocation (inherited from `[fraiseql.redis]`
    /// if absent).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub redis_url:           Option<String>,
    /// How long (seconds) a `revoke-all` epoch is retained.
    ///
    /// `revoke-all` records a per-user epoch rather than deleting individual
    /// tokens, so the entry must outlive every token that could have been issued
    /// before the revocation: set this **above your maximum access-token
    /// lifetime**. Once it expires a pre-revocation token would resume working
    /// (until its own `exp`). Default: 86400 (24h).
    pub revoke_all_ttl_secs: u64,
}

impl Default for TokenRevocationSecurityConfig {
    fn default() -> Self {
        Self {
            enabled:             false,
            backend:             "memory".to_string(),
            require_jti:         true,
            fail_open:           false,
            redis_url:           None,
            revoke_all_ttl_secs: 86_400,
        }
    }
}

/// A machine service account (`security.service_accounts.<name>`).
///
/// The bearer secret is **never** inlined — `secret_env` names the environment
/// variable that holds it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ServiceAccountConfig {
    /// Name of the environment variable holding the plaintext bearer secret.
    pub secret_env:      String,
    /// The `run_as` ceiling — roles granted. Empty ⇒ no role authority.
    #[serde(default)]
    pub roles:           Vec<String>,
    /// The `run_as` ceiling — scopes granted. Empty ⇒ no scope authority.
    #[serde(default)]
    pub scopes:          Vec<String>,
    /// Optional tenant pin. Omitted ⇒ global / NULL tenant.
    #[serde(default)]
    pub tenant:          Option<String>,
    /// Optional server-injected `fraiseql.enriched.*` fields, the **only**
    /// sanctioned deviation from uniform enrichment (ADR-0016 decision 6 /
    /// ADR-0018 decision 5) — for a daemon with no natural actor row.
    /// Server-injected, never token-asserted.
    #[serde(default)]
    pub static_enriched: HashMap<String, serde_json::Value>,
}

/// Compiled `[security.cost_budget]` (#379).
///
/// `deny_unknown_fields`: a typo'd budget key must fail the load, not silently
/// leave the budget unenforced (the compiled-schema seam rule).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(deny_unknown_fields)]
pub struct CostBudgetConfig {
    /// Hard ceiling on a single operation's estimated cost, enforced in the
    /// executor's dispatch for **every** transport. `None` = no ceiling.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub per_request_max: Option<u64>,

    /// Default rolling per-minute cost budget for registered tenants that do
    /// not set their own `cost_budget_per_minute`. `None` = no default.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub per_tenant_per_minute_default: Option<u64>,
}

impl SecurityConfig {
    /// Create a new empty security configuration.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Add a role definition.
    pub fn add_role(&mut self, role: RoleDefinition) {
        self.role_definitions.push(role);
    }

    /// Find a role definition by name.
    #[must_use]
    pub fn find_role(&self, name: &str) -> Option<&RoleDefinition> {
        self.role_definitions.iter().find(|r| r.name == name)
    }

    /// Get all scopes granted to a role.
    #[must_use]
    pub fn get_role_scopes(&self, role_name: &str) -> Vec<String> {
        self.find_role(role_name)
            .map(|role| role.scopes.iter().map(|s| s.to_string()).collect::<Vec<String>>())
            .unwrap_or_default()
    }

    /// Check if a role has a specific scope.
    #[must_use]
    pub fn role_has_scope(&self, role_name: &str, scope: &str) -> bool {
        self.find_role(role_name).is_some_and(|role| role.has_scope(scope))
    }
}
