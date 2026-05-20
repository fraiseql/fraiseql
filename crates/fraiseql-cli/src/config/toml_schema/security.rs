//! Security configuration types for `[security.*]` and `[auth]` TOML sections.

use std::fmt;

use serde::{Deserialize, Serialize};

/// Security configuration
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(default, deny_unknown_fields)]
pub struct SecuritySettings {
    /// Default policy to apply if none specified
    pub default_policy: Option<String>,
    /// Custom authorization rules
    pub rules: Vec<AuthorizationRule>,
    /// Authorization policies
    pub policies: Vec<AuthorizationPolicy>,
    /// Field-level authorization rules
    pub field_auth: Vec<FieldAuthRule>,
    /// Enterprise security configuration (legacy flags)
    pub enterprise: EnterpriseSecurityConfig,
    /// Error sanitization — controls what detail clients see in error responses
    pub error_sanitization: Option<ErrorSanitizationTomlConfig>,
    /// Rate limiting — per-endpoint request caps
    pub rate_limiting: Option<RateLimitingSecurityConfig>,
    /// State encryption — AEAD encryption for OAuth state and PKCE blobs
    pub state_encryption: Option<StateEncryptionConfig>,
    /// PKCE — Proof Key for Code Exchange for OAuth Authorization Code flows
    pub pkce: Option<PkceConfig>,
    /// API key authentication — static or database-backed key-based auth
    pub api_keys: Option<ApiKeySecurityConfig>,
    /// Token revocation — reject JWTs by `jti` after revocation
    pub token_revocation: Option<TokenRevocationSecurityConfig>,
    /// Trusted documents — query allowlist (strict or permissive mode)
    pub trusted_documents: Option<TrustedDocumentsConfig>,
}

impl Default for SecuritySettings {
    fn default() -> Self {
        Self {
            default_policy: Some("authenticated".to_string()),
            rules: vec![],
            policies: vec![],
            field_auth: vec![],
            enterprise: EnterpriseSecurityConfig::default(),
            error_sanitization: None,
            rate_limiting: None,
            state_encryption: None,
            pkce: None,
            api_keys: None,
            token_revocation: None,
            trusted_documents: None,
        }
    }
}

/// Authorization rule (custom expressions)
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct AuthorizationRule {
    /// Rule name
    pub name: String,
    /// Rule expression or condition
    pub rule: String,
    /// Rule description
    pub description: Option<String>,
    /// Whether rule result can be cached
    #[serde(default)]
    pub cacheable: bool,
    /// Cache time-to-live in seconds
    pub cache_ttl_seconds: Option<u32>,
}

/// Authorization policy (RBAC/ABAC)
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct AuthorizationPolicy {
    /// Policy name
    pub name: String,
    /// Policy type (RBAC, ABAC, CUSTOM, HYBRID)
    #[serde(rename = "type")]
    pub policy_type: String,
    /// Optional rule expression
    pub rule: Option<String>,
    /// Roles this policy applies to
    pub roles: Vec<String>,
    /// Combination strategy (ANY, ALL, EXACTLY)
    pub strategy: Option<String>,
    /// Attributes for attribute-based access control
    #[serde(default)]
    pub attributes: Vec<String>,
    /// Policy description
    pub description: Option<String>,
    /// Cache time-to-live in seconds
    pub cache_ttl_seconds: Option<u32>,
}

/// Field-level authorization rule
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct FieldAuthRule {
    /// Type name this rule applies to
    pub type_name: String,
    /// Field name this rule applies to
    pub field_name: String,
    /// Policy to enforce
    pub policy: String,
}

/// Enterprise security configuration
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(default, deny_unknown_fields)]
pub struct EnterpriseSecurityConfig {
    /// Enable rate limiting
    pub rate_limiting_enabled: bool,
    /// Max requests per auth endpoint
    pub auth_endpoint_max_requests: u32,
    /// Rate limit window in seconds
    pub auth_endpoint_window_seconds: u64,
    /// Enable audit logging
    pub audit_logging_enabled: bool,
    /// Audit log backend service
    pub audit_log_backend: String,
    /// Audit log retention in days
    pub audit_retention_days: u32,
    /// Enable error sanitization
    pub error_sanitization: bool,
    /// Hide implementation details in errors
    pub hide_implementation_details: bool,
    /// Enable constant-time token comparison
    pub constant_time_comparison: bool,
    /// Enable PKCE for OAuth flows
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
            enabled: false,
            hide_implementation_details: true,
            sanitize_database_errors: true,
            custom_error_message: None,
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
        }
    }
}

/// AEAD algorithm for OAuth state and PKCE state blobs.
#[derive(Debug, Clone, Default, Deserialize, Serialize, PartialEq, Eq)]
#[non_exhaustive]
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
            Self::Aes256Gcm => f.write_str("aes-256-gcm"),
        }
    }
}

/// Where the encryption key is sourced from.
#[derive(Debug, Clone, Default, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
#[non_exhaustive]
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
            enabled: false,
            algorithm: EncryptionAlgorithm::default(),
            key_source: KeySource::Env,
            key_env: Some("STATE_ENCRYPTION_KEY".to_string()),
        }
    }
}

/// PKCE code challenge method.
#[derive(Debug, Clone, Default, Deserialize, Serialize, PartialEq, Eq)]
#[non_exhaustive]
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
            enabled: false,
            code_challenge_method: CodeChallengeMethod::S256,
            state_ttl_secs: 600,
            redis_url: None,
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
            enabled: false,
            header: "X-API-Key".to_string(),
            hash_algorithm: "sha256".to_string(),
            storage: "env".to_string(),
            static_keys: vec![],
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

/// Trusted document mode.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "lowercase")]
#[non_exhaustive]
pub enum TrustedDocumentMode {
    /// Only documentId requests allowed; raw query strings rejected
    Strict,
    /// documentId requests use the manifest; raw queries fall through
    #[default]
    Permissive,
}

/// Trusted documents / query allowlist configuration.
///
/// ```toml
/// [security.trusted_documents]
/// enabled = true
/// mode = "strict"
/// manifest_path = "./trusted-documents.json"
/// reload_interval_secs = 0
/// ```
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(default, deny_unknown_fields)]
pub struct TrustedDocumentsConfig {
    /// Enable trusted documents
    pub enabled: bool,
    /// Enforcement mode: "strict" or "permissive"
    pub mode: TrustedDocumentMode,
    /// Path to the trusted documents manifest JSON file
    #[serde(skip_serializing_if = "Option::is_none")]
    pub manifest_path: Option<String>,
    /// URL to fetch the trusted documents manifest from at startup
    #[serde(skip_serializing_if = "Option::is_none")]
    pub manifest_url: Option<String>,
    /// Poll interval in seconds for hot-reloading the manifest (0 = no reload)
    #[serde(default)]
    pub reload_interval_secs: u64,
}

impl Default for TrustedDocumentsConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            mode: TrustedDocumentMode::Permissive,
            manifest_path: None,
            manifest_url: None,
            reload_interval_secs: 0,
        }
    }
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
    /// If revocation store is unreachable: `false` = reject (fail-closed), `true` = allow
    /// (fail-open)
    #[serde(default)]
    pub fail_open: bool,
    /// Redis URL for distributed revocation (optional — inherited from `[fraiseql.redis]` if
    /// absent)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub redis_url: Option<String>,
}

impl Default for TokenRevocationSecurityConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            backend: "memory".to_string(),
            require_jti: true,
            fail_open: false,
            redis_url: None,
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
    pub discovery_url: String,
    /// OAuth2 `client_id` registered with the provider.
    pub client_id: String,
    /// Name of the environment variable that holds the client secret.
    /// The secret itself must never appear in TOML or the compiled schema.
    pub client_secret_env: String,
    /// The full URL of this server's `/auth/callback` endpoint,
    /// e.g. `"https://api.example.com/auth/callback"`.
    pub server_redirect_uri: String,
}

fn default_true() -> bool {
    true
}
