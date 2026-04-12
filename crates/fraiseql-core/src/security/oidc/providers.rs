//! Per-provider OIDC configuration constructors (Auth0, Keycloak, Okta, etc.).

use serde::{Deserialize, Serialize};

use crate::security::errors::{Result, SecurityError};

// ============================================================================
// OIDC Configuration
// ============================================================================

/// Configuration for the `GET /auth/me` session-identity endpoint.
///
/// Exposed at `/auth/me` when enabled.  The endpoint reads the validated
/// JWT already in the request context and returns a JSON object containing
/// `sub`, `user_id` (alias for `sub`), `expires_at`, plus any extra claims
/// listed in `expose_claims` that are present in the token.
///
/// # Example (TOML)
///
/// ```toml
/// [auth.me]
/// enabled = true
/// expose_claims = ["email", "tenant_id", "https://myapp.com/role"]
/// ```
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct MeEndpointConfig {
    /// Enable the `GET /auth/me` endpoint.  Default: `false` (opt-in).
    #[serde(default)]
    pub enabled: bool,

    /// Raw JWT claim names to include in the response body, beyond the
    /// always-present `sub`, `user_id`, and `expires_at`.
    ///
    /// Names must match the raw claim key in the token (e.g. `"email"`,
    /// `"https://myapp.com/role"`).  Claims absent from the token are silently
    /// omitted.  The `user_id` alias for `sub` is always included and must
    /// **not** be listed here — listing `"user_id"` would silently return
    /// nothing because the JWT only carries `sub`.
    #[serde(default)]
    pub expose_claims: Vec<String>,
}

/// OIDC authentication configuration.
///
/// Configure this with your identity provider's issuer URL.
/// The validator will automatically discover JWKS endpoint.
///
/// **SECURITY CRITICAL**: You MUST configure the `audience` field to prevent
/// token confusion attacks. See the `audience` field documentation for details.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OidcConfig {
    /// Issuer URL (e.g., `https://your-tenant.auth0.com/`)
    ///
    /// Must match the `iss` claim in tokens exactly.
    /// Should include trailing slash if provider expects it.
    pub issuer: String,

    /// Expected audience claim (REQUIRED for security).
    ///
    /// **SECURITY CRITICAL**: This field is mandatory. Tokens must have this value in their `aud`
    /// claim. This prevents token confusion attacks where tokens intended for service A
    /// can be used for service B.
    ///
    /// For Auth0, this is typically your API identifier (e.g., `https://api.example.com`).
    /// For other providers, use a unique identifier that represents your application.
    ///
    /// Set at least one of:
    /// - `audience` (primary audience)
    /// - `additional_audiences` (secondary audiences)
    #[serde(default)]
    pub audience: Option<String>,

    /// Additional allowed audiences (optional).
    ///
    /// Some tokens may have multiple audiences. Add extras here.
    #[serde(default)]
    pub additional_audiences: Vec<String>,

    /// JWKS cache TTL in seconds.
    ///
    /// How long to cache the JWKS before refetching.
    /// Default: 300 (5 minutes) — short to prevent token cache poisoning.
    #[serde(default = "default_jwks_cache_ttl")]
    pub jwks_cache_ttl_secs: u64,

    /// Allowed token algorithms.
    ///
    /// Default: RS256 (most common for OIDC providers)
    #[serde(default = "default_algorithms")]
    pub allowed_algorithms: Vec<String>,

    /// Clock skew tolerance in seconds.
    ///
    /// Allow this many seconds of clock difference when
    /// validating exp/nbf/iat claims.
    /// Default: 60 seconds
    #[serde(default = "default_clock_skew")]
    pub clock_skew_secs: u64,

    /// Custom JWKS URI (optional).
    ///
    /// If set, skip OIDC discovery and use this URI directly.
    /// Useful for providers that don't support standard discovery.
    #[serde(default)]
    pub jwks_uri: Option<String>,

    /// Require authentication for all requests.
    ///
    /// If false, requests without tokens are allowed (anonymous access).
    /// Default: true
    #[serde(default = "default_required")]
    pub required: bool,

    /// Scope claim name.
    ///
    /// The claim containing user scopes/permissions.
    /// Default: "scope" (space-separated string)
    /// Some providers use "scp" or "permissions" (array)
    #[serde(default = "default_scope_claim")]
    pub scope_claim: String,

    /// Require the `jti` (JWT ID) claim on every validated token.
    ///
    /// When `true`, tokens without a `jti` are rejected with a validation error.
    /// When `false` (default), a missing `jti` is accepted but the token cannot
    /// be replay-checked.
    ///
    /// Set to `true` when you have a [`ReplayCache`] configured, so that every
    /// token is guaranteed to be uniquely identifiable.
    ///
    /// [`ReplayCache`]: crate::security::oidc::replay_cache::ReplayCache
    #[serde(default)]
    pub require_jti: bool,

    /// Configuration for the `GET /auth/me` session-identity endpoint.
    ///
    /// When present and `enabled = true`, mounts `GET /auth/me` behind
    /// OIDC authentication.  The endpoint reflects a configurable subset of
    /// the current session's JWT claims to the frontend.
    ///
    /// Default: `None` (endpoint not mounted).
    #[serde(default)]
    pub me: Option<MeEndpointConfig>,
}

pub(super) const fn default_jwks_cache_ttl() -> u64 {
    // SECURITY: Reduced from 3600s (1 hour) to 300s (5 minutes)
    // Prevents token cache poisoning by limiting revoked token window
    300
}

pub(super) fn default_algorithms() -> Vec<String> {
    vec!["RS256".to_string()]
}

/// Maximum clock skew tolerance enforced regardless of configuration.
/// Prevents accepting arbitrarily old expired tokens due to misconfiguration.
pub(super) const MAX_CLOCK_SKEW_SECS: u64 = 300;

pub(super) const fn default_clock_skew() -> u64 {
    60
}

pub(super) const fn default_required() -> bool {
    true
}

pub(super) fn default_scope_claim() -> String {
    "scope".to_string()
}

impl Default for OidcConfig {
    fn default() -> Self {
        Self {
            issuer:               String::new(),
            audience:             None,
            additional_audiences: Vec::new(),
            jwks_cache_ttl_secs:  default_jwks_cache_ttl(),
            allowed_algorithms:   default_algorithms(),
            clock_skew_secs:      default_clock_skew(),
            jwks_uri:             None,
            required:             default_required(),
            scope_claim:          default_scope_claim(),
            require_jti:          false,
            me:                   None,
        }
    }
}

impl OidcConfig {
    /// Create config for Auth0.
    ///
    /// # Arguments
    ///
    /// * `domain` - Your Auth0 domain (e.g., "your-tenant.auth0.com")
    /// * `audience` - Your API identifier
    #[must_use]
    pub fn auth0(domain: &str, audience: &str) -> Self {
        Self {
            issuer: format!("https://{domain}/"),
            audience: Some(audience.to_string()),
            ..Default::default()
        }
    }

    /// Create config for Keycloak.
    ///
    /// # Arguments
    ///
    /// * `base_url` - Keycloak server URL (e.g., `https://keycloak.example.com`)
    /// * `realm` - Realm name
    /// * `client_id` - Client ID (used as audience)
    #[must_use]
    pub fn keycloak(base_url: &str, realm: &str, client_id: &str) -> Self {
        Self {
            issuer: format!("{base_url}/realms/{realm}"),
            audience: Some(client_id.to_string()),
            ..Default::default()
        }
    }

    /// Create config for Okta.
    ///
    /// # Arguments
    ///
    /// * `domain` - Your Okta domain (e.g., "your-org.okta.com")
    /// * `audience` - Your API audience (often "<api://default>")
    #[must_use]
    pub fn okta(domain: &str, audience: &str) -> Self {
        Self {
            issuer: format!("https://{domain}"),
            audience: Some(audience.to_string()),
            ..Default::default()
        }
    }

    /// Create config for AWS Cognito.
    ///
    /// # Arguments
    ///
    /// * `region` - AWS region (e.g., "us-east-1")
    /// * `user_pool_id` - Cognito User Pool ID
    /// * `client_id` - App client ID (used as audience)
    #[must_use]
    pub fn cognito(region: &str, user_pool_id: &str, client_id: &str) -> Self {
        Self {
            issuer: format!("https://cognito-idp.{region}.amazonaws.com/{user_pool_id}"),
            audience: Some(client_id.to_string()),
            ..Default::default()
        }
    }

    /// Create config for Microsoft Entra ID (Azure AD).
    ///
    /// # Arguments
    ///
    /// * `tenant_id` - Azure AD tenant ID
    /// * `client_id` - Application (client) ID
    #[must_use]
    pub fn azure_ad(tenant_id: &str, client_id: &str) -> Self {
        Self {
            issuer: format!("https://login.microsoftonline.com/{tenant_id}/v2.0"),
            audience: Some(client_id.to_string()),
            ..Default::default()
        }
    }

    /// Create config for Google Identity.
    ///
    /// # Arguments
    ///
    /// * `client_id` - Google OAuth client ID
    #[must_use]
    pub fn google(client_id: &str) -> Self {
        Self {
            issuer: "https://accounts.google.com".to_string(),
            audience: Some(client_id.to_string()),
            ..Default::default()
        }
    }

    /// Validate the configuration.
    ///
    /// # Errors
    ///
    /// Returns `SecurityError::SecurityConfigError` if:
    /// - Issuer is empty
    /// - Issuer does not use HTTPS (except localhost/127.0.0.1)
    /// - Neither `audience` nor `additional_audiences` are configured
    /// - No algorithms are allowed
    pub fn validate(&self) -> Result<()> {
        if self.issuer.is_empty() {
            return Err(SecurityError::SecurityConfigError(
                "OIDC issuer URL is required".to_string(),
            ));
        }

        if !self.issuer.starts_with("https://")
            && !self.issuer.starts_with("http://localhost")
            && !self.issuer.starts_with("http://127.0.0.1")
        {
            return Err(SecurityError::SecurityConfigError(
                "OIDC issuer must use HTTPS (except localhost/127.0.0.1 for development)"
                    .to_string(),
            ));
        }

        // CRITICAL SECURITY FIX: Audience validation is now mandatory
        // This prevents token confusion attacks where tokens intended for service A
        // can be used for service B.
        if self.audience.is_none() && self.additional_audiences.is_empty() {
            return Err(SecurityError::SecurityConfigError(
                "OIDC audience is REQUIRED for security. Set 'audience' in auth config to your API identifier. \
                 This prevents token confusion attacks where tokens from one service can be used in another. \
                 Example: audience = \"https://api.example.com\" or audience = \"my-api-id\"".to_string(),
            ));
        }

        if self.allowed_algorithms.is_empty() {
            return Err(SecurityError::SecurityConfigError(
                "At least one algorithm must be allowed".to_string(),
            ));
        }

        Ok(())
    }
}
