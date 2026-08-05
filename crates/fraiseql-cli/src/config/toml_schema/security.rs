//! Security configuration types for `[security.*]` and `[auth]` TOML sections.

use fraiseql_core::security::oidc::MeEndpointConfig;
use serde::{Deserialize, Serialize};

/// Security configuration
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(default, deny_unknown_fields)]
pub struct SecuritySettings {
    /// Declare that this schema serves multiple tenants.
    ///
    /// Activates the subscription tenant fail-closed gate and the cache+RLS boot
    /// gate. Both gates named this key in their own error messages while
    /// `deny_unknown_fields` made it a compile error to set (#758).
    pub multi_tenant:           bool,
    /// Declare that database Row-Level Security isolates this schema's data.
    ///
    /// `[security.rls] enabled = true`. Verified against the live catalog at boot.
    pub rls:                    fraiseql_core::schema::RlsConfig,
    /// Default policy to apply if none specified
    pub default_policy:         Option<String>,
    /// Custom authorization rules
    pub rules:                  Vec<AuthorizationRule>,
    /// Authorization policies
    pub policies:               Vec<AuthorizationPolicy>,
    /// Field-level authorization rules
    pub field_auth:             Vec<FieldAuthRule>,
    /// Enterprise security configuration (legacy flags)
    pub enterprise:             EnterpriseSecurityConfig,
    /// Error sanitization — controls what detail clients see in error responses
    pub error_sanitization:     Option<ErrorSanitizationTomlConfig>,
    /// Rate limiting — per-endpoint request caps
    pub rate_limiting:          Option<RateLimitingSecurityConfig>,
    /// State encryption — AEAD encryption for OAuth state and PKCE blobs
    pub state_encryption:       Option<StateEncryptionConfig>,
    /// PKCE — Proof Key for Code Exchange for OAuth Authorization Code flows
    pub pkce:                   Option<PkceConfig>,
    /// API key authentication — static or database-backed key-based auth
    pub api_keys:               Option<ApiKeySecurityConfig>,
    /// Token revocation — reject JWTs by `jti` after revocation
    pub token_revocation:       Option<TokenRevocationSecurityConfig>,
    /// Trusted documents — query allowlist (strict or permissive mode)
    pub trusted_documents:      Option<TrustedDocumentsConfig>,
    /// Force persisted-operations-only enforcement (#379).
    ///
    /// When `true`, the runtime forces the trusted-document store into `strict` mode
    /// (reject any non-persisted operation) regardless of `[security.trusted_documents].mode`.
    /// Requires a configured trusted-documents manifest to have any effect.
    pub persisted_queries_only: bool,
    /// Operation cost budgets (#379): `per_request_max` (a hard per-operation
    /// ceiling the executor enforces on every transport) and
    /// `per_tenant_per_minute_default` (the rolling window seeded for tenants
    /// without their own budget). Shares the runtime's own type so the
    /// authoring and consuming shapes cannot drift.
    pub cost_budget:            Option<fraiseql_core::schema::CostBudgetConfig>,
}

impl Default for SecuritySettings {
    fn default() -> Self {
        Self {
            multi_tenant:           false,
            rls:                    fraiseql_core::schema::RlsConfig::default(),
            default_policy:         Some("authenticated".to_string()),
            rules:                  vec![],
            policies:               vec![],
            field_auth:             vec![],
            enterprise:             EnterpriseSecurityConfig::default(),
            error_sanitization:     None,
            rate_limiting:          None,
            state_encryption:       None,
            pkce:                   None,
            api_keys:               None,
            token_revocation:       None,
            trusted_documents:      None,
            persisted_queries_only: false,
            cost_budget:            None,
        }
    }
}

// The `[security.*]` section shapes are owned by `fraiseql_core::schema` (#977):
// the compiled schema's `SecurityConfig` carries them as typed fields, so the
// authoring (TOML), compiled and consuming (server) sides share one definition
// per subsystem and cannot drift. Re-exported here under the names this module
// historically used.
pub use fraiseql_core::schema::{
    ApiKeySecurityConfig, AuthorizationPolicy, AuthorizationRule, CodeChallengeMethod,
    EncryptionAlgorithm, EnterpriseSecurityConfig,
    ErrorSanitizationConfig as ErrorSanitizationTomlConfig, FieldAuthRule, KeySource,
    PkceSecurityConfig as PkceConfig, RateLimitingSecurityConfig, StateEncryptionConfig,
    StaticApiKeyEntry, TokenRevocationSecurityConfig, TrustedDocumentMode, TrustedDocumentsConfig,
};

/// OIDC configuration for `[auth]`.
///
/// This is the CLI's **compile-time view** of the `[auth]` block. The server
/// reads the same `[auth]` block independently into its own
/// [`OidcConfig`](fraiseql_core::security::OidcConfig) at runtime, so this
/// schema exists to *accept and structurally validate* the same block the
/// server consumes — not to lower it into the compiled schema (the compiled
/// schema carries no auth). Every JWT-validation field here must therefore stay
/// field-compatible with `OidcConfig`, or `deny_unknown_fields` would reject a
/// config the server accepts.
///
/// Two independent groups may be configured, together or separately:
///
/// - **JWT validation** — `issuer` **or** `jwks_uri`, plus `audience` and the rest of
///   `OidcConfig`'s JWT settings (`additional_audiences`, `allowed_algorithms`,
///   `jwks_cache_ttl_secs`, `clock_skew_secs`, `required`, `scope_claim`, `require_jti`,
///   `[auth.me]`). The server consumes these to validate incoming bearer tokens; the CLI accepts
///   them so the same file parses under both. Accepted and functional. `issuer` may be omitted for
///   identity providers whose access tokens carry no `iss` claim (e.g. self-hosted Hanko); in that
///   **issuer-less** mode `jwks_uri` must be pinned, since discovery cannot locate the JWKS
///   endpoint without an issuer.
/// - **PKCE OAuth client** (server-side login) — `discovery_url`, `client_id`, `client_secret_env`,
///   `server_redirect_uri`, configured all four together. **Not yet functional on the compiled path
///   (tracked in #621):** the compiled schema carries no `auth`/`auth_endpoints` for the server to
///   consume, so a complete client group is *rejected at compile time* rather than silently
///   accepted.
///
/// At least one group must be present; an empty `[auth]` is a load error. The client
/// secret itself must never appear here — `client_secret_env` names the environment
/// variable that holds it.
///
/// ```toml
/// [auth]
/// issuer   = "https://accounts.google.com"
/// audience = "my-api"
/// ```
///
/// Issuer-less (e.g. Hanko), pinning the JWKS endpoint:
///
/// ```toml
/// [auth]
/// jwks_uri = "https://hanko.example.com/.well-known/jwks.json"
/// audience = "my-relying-party-id"
/// ```
#[derive(Debug, Clone, Default, Deserialize, Serialize)]
#[serde(default, deny_unknown_fields)]
pub struct OidcClientConfig {
    /// OIDC issuer URL for JWT validation (e.g. `"https://accounts.google.com"`).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub issuer:   Option<String>,
    /// Expected `aud` claim for JWT validation. Required at runtime by the
    /// server's `OidcConfig` to prevent token-confusion attacks.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub audience: Option<String>,
    /// Pinned JWKS endpoint for JWT validation (skips OIDC discovery).
    ///
    /// Required for **issuer-less** identity providers — IdPs whose access
    /// tokens omit the `iss` claim (e.g. self-hosted Hanko). With `issuer`
    /// unset, discovery cannot locate the JWKS endpoint, so it must be pinned
    /// here. May also be set alongside `issuer` to skip discovery.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub jwks_uri: Option<String>,

    // JWT-validation fields below mirror the server's `OidcConfig` so a single
    // config file's `[auth]` block parses under both. They are accepted and
    // structurally validated here but consumed by the server (which re-reads the
    // same block); the CLI does not act on their values. `cli_auth_schema_...`
    // in tests pins that this list stays complete as `OidcConfig` evolves.
    /// Additional accepted `aud` values (union-compat with `OidcConfig`).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub additional_audiences: Vec<String>,
    /// Allowed JWT signing algorithms (union-compat; the server defaults to RS256).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub allowed_algorithms:   Vec<String>,
    /// JWKS cache TTL in seconds (union-compat with `OidcConfig`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub jwks_cache_ttl_secs:  Option<u64>,
    /// Clock-skew tolerance in seconds (union-compat with `OidcConfig`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub clock_skew_secs:      Option<u64>,
    /// Require authentication for all requests (union-compat with `OidcConfig`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub required:             Option<bool>,
    /// JWT scope-claim name (union-compat with `OidcConfig`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub scope_claim:          Option<String>,
    /// Require the `jti` claim on every token (union-compat with `OidcConfig`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub require_jti:          Option<bool>,
    /// `[auth.me]` session-identity endpoint config (union-compat with `OidcConfig`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub me:                   Option<MeEndpointConfig>,

    /// PKCE: OIDC provider discovery URL. **Not yet functional (#621).**
    #[serde(skip_serializing_if = "Option::is_none")]
    pub discovery_url:       Option<String>,
    /// PKCE: OAuth2 `client_id` registered with the provider. **Not yet functional (#621).**
    #[serde(skip_serializing_if = "Option::is_none")]
    pub client_id:           Option<String>,
    /// PKCE: name of the environment variable that holds the client secret.
    /// The secret itself must never appear in TOML or the compiled schema.
    /// **Not yet functional (#621).**
    #[serde(skip_serializing_if = "Option::is_none")]
    pub client_secret_env:   Option<String>,
    /// PKCE: the full URL of this server's `/auth/callback` endpoint.
    /// **Not yet functional (#621).**
    #[serde(skip_serializing_if = "Option::is_none")]
    pub server_redirect_uri: Option<String>,

    /// Social-login providers (`[auth.social.google]` / `[auth.social.github]`,
    /// #368). Deserializes directly into the **compiled** type so the authored
    /// and compiled shapes cannot drift; the merger embeds it verbatim under
    /// the compiled `auth.social` object. An unimplemented provider key
    /// (`apple`, `discord`, …) is an unknown-field load error, not a
    /// silently-ignored table.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub social: Option<fraiseql_core::schema::SocialAuthConfig>,

    /// First-party auth methods the server operates itself (`[auth.local]`,
    /// #367): email+password, email `OTP`/magic link, `TOTP` `MFA`, anonymous
    /// guest sessions. Deserializes directly into the compiled type so the
    /// authored and compiled shapes cannot drift.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub local: Option<fraiseql_core::schema::LocalAuthConfig>,
}

impl OidcClientConfig {
    /// Validate the `[auth]` group structure (#612 item 9).
    ///
    /// The JWT group (`issuer` or a pinned `jwks_uri`, plus optional `audience`) is
    /// accepted and functional. The PKCE client group is all-four-or-none, and a
    /// *complete* client group is rejected — it is not yet functional on the compiled
    /// path (#621), so it is refused rather than silently accepted. At least one group
    /// must be present.
    ///
    /// # Errors
    ///
    /// Returns an error naming the specific problem: an incomplete client group (with the
    /// missing fields), a complete-but-unsupported client group, `audience` without a JWT
    /// group (neither `issuer` nor `jwks_uri`), or an empty `[auth]` block.
    pub fn validate(&self) -> anyhow::Result<()> {
        let client_fields = [
            ("discovery_url", self.discovery_url.is_some()),
            ("client_id", self.client_id.is_some()),
            ("client_secret_env", self.client_secret_env.is_some()),
            ("server_redirect_uri", self.server_redirect_uri.is_some()),
        ];
        let client_set = client_fields.iter().filter(|(_, set)| *set).count();
        let has_client_group = client_set > 0;
        let has_social_group = self.social.is_some();
        let has_local_group = self.local.is_some();
        // A JWT-validation group exists if either `issuer` (used for discovery
        // and `iss` validation) or a pinned `jwks_uri` (issuer-less mode, for
        // IdPs that omit `iss`) is set. Mirrors the server's `OidcConfig`.
        let has_jwt_group = self.issuer.is_some() || self.jwks_uri.is_some();

        if has_client_group && client_set < client_fields.len() {
            let missing: Vec<&str> =
                client_fields.iter().filter(|(_, set)| !*set).map(|(name, _)| *name).collect();
            anyhow::bail!(
                "[auth] PKCE OAuth-client config is incomplete: discovery_url, client_id, \
                 client_secret_env, and server_redirect_uri must be set together (missing: {}).",
                missing.join(", ")
            );
        }

        // #621: a complete PKCE client group is now functional on the compiled
        // path — the merger emits the compiled `auth` object, and the runtime
        // resolves the OIDC discovery document at boot from `discovery_url`
        // (fetch-at-boot, so the compiler stays hermetic). It no longer bails.
        // (`has_client_group` with `client_set == 4` is accepted.)

        if self.audience.is_some() && !has_jwt_group {
            anyhow::bail!(
                "[auth] audience is set but neither issuer nor jwks_uri is configured. JWT \
                 validation needs an issuer (used for discovery and `iss` validation) or a pinned \
                 jwks_uri (for IdPs whose tokens omit `iss`, e.g. Hanko) — add one, or remove \
                 audience."
            );
        }

        // #368: [auth.social] must name at least one provider, and every named
        // provider's fields must be non-empty — an empty block or blank
        // client_id would compile into a registry that can never serve a login.
        if let Some(social) = &self.social {
            if social.google.is_none() && social.github.is_none() {
                anyhow::bail!(
                    "[auth.social] is configured but names no provider. Add an \
                     [auth.social.google] or [auth.social.github] block (Apple/Discord/Facebook \
                     are not implemented and are refused as unknown fields)."
                );
            }
            let providers = [
                (
                    "google",
                    social.google.as_ref().map(|g| {
                        [
                            ("client_id", &g.client_id),
                            ("client_secret_env", &g.client_secret_env),
                            ("redirect_uri", &g.redirect_uri),
                        ]
                    }),
                ),
                (
                    "github",
                    social.github.as_ref().map(|g| {
                        [
                            ("client_id", &g.client_id),
                            ("client_secret_env", &g.client_secret_env),
                            ("redirect_uri", &g.redirect_uri),
                        ]
                    }),
                ),
            ];
            for (provider, fields) in providers {
                for (field, value) in fields.into_iter().flatten() {
                    if value.trim().is_empty() {
                        anyhow::bail!("[auth.social.{provider}] {field} must not be empty.");
                    }
                }
            }
        }

        // #367: [auth.local] must enable at least one method, and every method
        // that sends mail or issues links must have what it needs to do so.
        // Refusing at compile time keeps the operator's feedback loop at the
        // file they are editing rather than at server boot.
        if let Some(local) = &self.local {
            if !local.password && !local.otp && !local.mfa && !local.anonymous {
                anyhow::bail!(
                    "[auth.local] is configured but enables no method. Set at least one of \
                     password / otp / mfa / anonymous, or remove the block."
                );
            }
            if (local.otp || local.password) && local.email_from.is_none() {
                anyhow::bail!(
                    "[auth.local] enables {} but sets no email_from. Both flows deliver mail \
                     (OTP codes, reset links); name the [mailbox.<name>] account whose SMTP \
                     half should send them.",
                    if local.otp { "otp" } else { "password" }
                );
            }
            if local.password && local.reset_url_template.is_none() {
                anyhow::bail!(
                    "[auth.local] password = true needs reset_url_template — the reset link \
                     points at your front end, which FraiseQL cannot guess. Example: \
                     reset_url_template = \"https://app.example.com/reset?token={{token}}\""
                );
            }
            for (field, template, placeholder) in [
                ("reset_url_template", local.reset_url_template.as_ref(), "{token}"),
                ("magic_link_template", local.magic_link_template.as_ref(), "{code}"),
            ] {
                if let Some(t) = template {
                    if !t.contains(placeholder) {
                        anyhow::bail!(
                            "[auth.local] {field} must contain the {placeholder} placeholder, \
                             or every link it builds is the same dead link: {t}"
                        );
                    }
                }
            }
        }

        // A PKCE client group alone (no JWT group) is a valid configuration:
        // server-side OAuth login without also validating bearer JWTs. Only an
        // entirely empty block is refused.
        if !has_jwt_group && !has_client_group && !has_social_group && !has_local_group {
            anyhow::bail!(
                "[auth] is empty. Configure JWT validation with issuer (or jwks_uri for IdPs \
                 whose tokens omit `iss`, e.g. Hanko), a PKCE OAuth-client group \
                 (discovery_url, client_id, client_secret_env, server_redirect_uri), or \
                 [auth.social.*] providers. An empty [auth] block does nothing."
            );
        }

        Ok(())
    }
}
