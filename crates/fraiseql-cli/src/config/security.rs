//! Security configuration parsing from fraiseql.toml

use anyhow::Result;
use serde::{Deserialize, Serialize};

/// Audit logging configuration
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(default, deny_unknown_fields)]
pub struct AuditLoggingConfig {
    /// Enable audit logging.
    ///
    /// The only key with a consumer: it lowers onto
    /// `enterprise.audit_logging_enabled`, which the runtime reads. `log_level`,
    /// `include_sensitive_data`, `async_logging`, `buffer_size` and
    /// `flush_interval_secs` were accepted here and read by nothing, so they are
    /// gone and `deny_unknown_fields` now refuses them by name (#983).
    pub enabled: bool,
}

impl Default for AuditLoggingConfig {
    fn default() -> Self {
        Self { enabled: true }
    }
}

/// Error sanitization configuration
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(default, deny_unknown_fields)]
pub struct ErrorSanitizationConfig {
    /// Enable error sanitization.
    ///
    /// The only key with a consumer. `generic_messages`, `internal_logging`,
    /// `leak_sensitive_details` and `user_facing_format` reached no runtime shape,
    /// so they are gone and `deny_unknown_fields` refuses them by name (#983).
    /// `leak_sensitive_details = true` used to be refused here as "a security risk";
    /// it switched nothing, which is the more alarming half of that sentence.
    pub enabled: bool,
}

impl Default for ErrorSanitizationConfig {
    fn default() -> Self {
        Self { enabled: true }
    }
}

impl ErrorSanitizationConfig {
    /// Validate error sanitization configuration.
    ///
    /// # Errors
    ///
    /// Currently infallible; kept so the section keeps its place in
    /// [`SecurityConfig::validate`]'s sequence as keys are added.
    pub const fn validate(&self) -> Result<()> {
        Ok(())
    }
}

/// Rate limiting configuration
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(default, deny_unknown_fields)]
pub struct RateLimitConfig {
    /// Enable rate limiting
    pub enabled: bool,

    /// Max requests for auth start endpoint (per IP)
    pub auth_start_max_requests: u32,
    /// Time window for auth start in seconds
    pub auth_start_window_secs:  u64,

    /// Max requests for auth callback endpoint (per IP)
    pub auth_callback_max_requests: u32,
    /// Time window for auth callback in seconds
    pub auth_callback_window_secs:  u64,

    /// Max requests for auth refresh endpoint (per user)
    pub auth_refresh_max_requests: u32,
    /// Time window for auth refresh in seconds
    pub auth_refresh_window_secs:  u64,

    /// Max requests for auth logout endpoint (per user)
    pub auth_logout_max_requests: u32,
    /// Time window for auth logout in seconds
    pub auth_logout_window_secs:  u64,

    /// Max failed login attempts per IP.
    ///
    /// Emitted as the consumer's `failed_login_max_attempts`. The binary performs no
    /// first-factor login, so a value tuned away from the default is refused at boot
    /// in production (#356) — hence the default matching the consumer's, so an
    /// untouched config signals no intent and boots.
    pub failed_login_max_requests: u32,
    /// Time window for failed login tracking in seconds. See
    /// [`failed_login_max_requests`](Self::failed_login_max_requests).
    pub failed_login_window_secs:  u64,

    /// Global per-IP request rate (requests/second) for the GraphQL surface.
    pub requests_per_second: u32,
    /// Burst allowance above `requests_per_second`.
    pub burst_size:          u32,

    /// Trust `X-Real-IP` / `X-Forwarded-For` for the client IP.
    ///
    /// Only safe behind a proxy named in `trusted_proxy_cidrs`; the server refuses to
    /// boot in production if this is on and that list is empty (#618).
    pub trust_proxy_headers: bool,
    /// CIDR ranges trusted as proxies, e.g. `["10.0.0.0/8"]`.
    pub trusted_proxy_cidrs: Vec<String>,
}

impl Default for RateLimitConfig {
    fn default() -> Self {
        Self {
            enabled:                    true,
            auth_start_max_requests:    100,
            auth_start_window_secs:     60,
            auth_callback_max_requests: 50,
            auth_callback_window_secs:  60,
            auth_refresh_max_requests:  10,
            auth_refresh_window_secs:   60,
            auth_logout_max_requests:   20,
            auth_logout_window_secs:    60,
            // Matching `DEFAULT_FAILED_LOGIN_MAX_ATTEMPTS` / `_LOCKOUT_SECS` in the
            // consumer, so an untouched config is recognisably untouched. The previous
            // 5 / 3600 read as deliberately tuned, and now that this section actually
            // reaches the runtime that would refuse every production boot (#356).
            failed_login_max_requests:  10,
            failed_login_window_secs:   900,
            requests_per_second:        100,
            burst_size:                 500,
            trust_proxy_headers:        false,
            trusted_proxy_cidrs:        Vec::new(),
        }
    }
}

impl RateLimitConfig {
    /// Validate rate limiting configuration
    ///
    /// # Errors
    ///
    /// Returns an error if any time-window is zero or if any `max_requests`
    /// value is zero (which would permanently block all requests).
    pub fn validate(&self) -> Result<()> {
        for (name, window) in &[
            ("auth_start_window_secs", self.auth_start_window_secs),
            ("auth_callback_window_secs", self.auth_callback_window_secs),
            ("auth_refresh_window_secs", self.auth_refresh_window_secs),
            ("auth_logout_window_secs", self.auth_logout_window_secs),
            ("failed_login_window_secs", self.failed_login_window_secs),
        ] {
            if *window == 0 {
                anyhow::bail!("{name} must be positive");
            }
        }
        for (name, max_req) in &[
            ("auth_start_max_requests", self.auth_start_max_requests),
            ("auth_callback_max_requests", self.auth_callback_max_requests),
            ("auth_refresh_max_requests", self.auth_refresh_max_requests),
            ("auth_logout_max_requests", self.auth_logout_max_requests),
            ("failed_login_max_requests", self.failed_login_max_requests),
        ] {
            if *max_req == 0 {
                anyhow::bail!(
                    "{name} must be at least 1; \
                     setting it to 0 blocks all requests permanently"
                );
            }
        }
        Ok(())
    }

    /// Convert to JSON representation for schema
    pub fn to_json(&self) -> serde_json::Value {
        // Flat, snake_case, one key per field of the consumer's
        // `RateLimitingSecurityConfig`. The nested camelCase shape below
        // (`authStart.maxRequests`) matched no reader anywhere, so a project
        // `fraiseql.toml` that configured rate limiting produced a section the server
        // could not use even after the section name was corrected (#893).
        serde_json::json!({
            "enabled": self.enabled,
            "requests_per_second": self.requests_per_second,
            "burst_size": self.burst_size,
            "trust_proxy_headers": self.trust_proxy_headers,
            "trusted_proxy_cidrs": self.trusted_proxy_cidrs,
            "auth_start_max_requests": self.auth_start_max_requests,
            "auth_start_window_secs": self.auth_start_window_secs,
            "auth_callback_max_requests": self.auth_callback_max_requests,
            "auth_callback_window_secs": self.auth_callback_window_secs,
            "auth_refresh_max_requests": self.auth_refresh_max_requests,
            "auth_refresh_window_secs": self.auth_refresh_window_secs,
            "auth_logout_max_requests": self.auth_logout_max_requests,
            "auth_logout_window_secs": self.auth_logout_window_secs,
            "failed_login_max_attempts": self.failed_login_max_requests,
            "failed_login_lockout_secs": self.failed_login_window_secs,
        })
    }
}

/// State encryption configuration
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(default, deny_unknown_fields)]
pub struct StateEncryptionConfig {
    /// Enable state encryption
    pub enabled:   bool,
    /// Encryption algorithm ("chacha20-poly1305")
    pub algorithm: String,
    // `key_rotation_enabled`, `nonce_size` and `key_size` were accepted here and
    // reached no runtime shape — nonce and key sizes are fixed by the algorithm, so
    // there was nothing for them to size (#983). `deny_unknown_fields` refuses them
    // by name now.
}

impl Default for StateEncryptionConfig {
    fn default() -> Self {
        Self {
            // Off unless the operator asks for it, matching
            // `fraiseql_core::schema::StateEncryptionConfig` (#1151). This field is not an
            // `Option` and the enclosing `SecurityConfig` carries `#[serde(default)]`, so an
            // absent `[security.state_encryption]` table lands here and is lowered verbatim
            // into the compiled artifact. Defaulting it on therefore enabled a control the
            // operator never chose, and the server refuses to boot when the key env var is
            // unset — a hard upgrade failure for every project that never opted in.
            enabled:   false,
            algorithm: "chacha20-poly1305".to_string(),
        }
    }
}

/// Supported encryption algorithms for `[security.state_encryption]`.
const SUPPORTED_ALGORITHMS: &[&str] = &["chacha20-poly1305", "aes-256-gcm"];

impl StateEncryptionConfig {
    /// Validate state encryption configuration
    ///
    /// # Errors
    ///
    /// Returns an error if `algorithm` is not a supported value, if `key_size`
    /// is not 16, 24, or 32 bytes, or if `nonce_size` is not 12 bytes.
    pub fn validate(&self) -> Result<()> {
        if !SUPPORTED_ALGORITHMS.contains(&self.algorithm.as_str()) {
            anyhow::bail!(
                "algorithm {:?} is not supported; must be one of: {}",
                self.algorithm,
                SUPPORTED_ALGORITHMS.join(", ")
            );
        }
        Ok(())
    }
}

/// Constant-time comparison configuration
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(default, deny_unknown_fields)]
pub struct ConstantTimeConfig {
    /// Enable constant-time comparisons
    pub enabled:                 bool,
    /// Apply constant-time comparison to JWT tokens
    pub apply_to_jwt:            bool,
    /// Apply constant-time comparison to session tokens
    pub apply_to_session_tokens: bool,
    /// Apply constant-time comparison to CSRF tokens
    pub apply_to_csrf_tokens:    bool,
    /// Apply constant-time comparison to refresh tokens
    pub apply_to_refresh_tokens: bool,
}

impl Default for ConstantTimeConfig {
    fn default() -> Self {
        Self {
            enabled:                 true,
            apply_to_jwt:            true,
            apply_to_session_tokens: true,
            apply_to_csrf_tokens:    true,
            apply_to_refresh_tokens: true,
        }
    }
}

/// Field-level RBAC role definition from fraiseql.toml
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct RoleDefinitionConfig {
    /// Role name identifier
    pub name:        String,
    /// Role description
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    /// Permission scopes assigned to this role
    pub scopes:      Vec<String>,
}

impl RoleDefinitionConfig {
    /// Lower into the type the runtime's `role_has_scope` consults.
    ///
    /// The single lowering shared by both security producers — `SecurityConfig::to_json`
    /// for a project `fraiseql.toml` and `schema::merger` for a TOML schema (`#897`).
    /// `role_has_scope` is the sole input to field-level access, so a grant that does not
    /// arrive here is a grant that does not exist; #757 is what a second hand-written copy
    /// of this mapping costs.
    #[must_use]
    pub fn to_runtime(&self) -> fraiseql_core::schema::RoleDefinition {
        fraiseql_core::schema::RoleDefinition {
            description: self.description.clone(),
            ..fraiseql_core::schema::RoleDefinition::new(self.name.clone(), self.scopes.clone())
        }
    }

    /// Reject a role that cannot grant anything.
    ///
    /// # Errors
    ///
    /// Returns an error if the name is empty or the role declares no scopes.
    pub fn validate(&self) -> Result<()> {
        if self.name.is_empty() {
            anyhow::bail!("Role name cannot be empty");
        }
        if self.scopes.is_empty() {
            anyhow::bail!("Role '{}' must have at least one scope", self.name);
        }
        Ok(())
    }
}

/// Tenancy isolation mode from fraiseql.toml.
///
/// Determines how tenant data is separated at the database level.
#[derive(Debug, Clone, Default, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum TenancyModeConfig {
    /// Single-tenant deployment, no isolation machinery.
    #[default]
    None,
    /// Row-level isolation via `@tenant_id` column injection.
    Row,
    /// Schema-level isolation via PostgreSQL schemas.
    Schema,
}

/// Tenancy configuration from `[fraiseql.tenancy]` in fraiseql.toml.
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(default, deny_unknown_fields)]
pub struct TenancyTomlConfig {
    /// Isolation strategy: `"none"`, `"row"`, or `"schema"`.
    pub mode:         TenancyModeConfig,
    /// JWT claim name that carries the tenant identifier.
    pub tenant_claim: String,
}

impl Default for TenancyTomlConfig {
    fn default() -> Self {
        Self {
            mode:         TenancyModeConfig::None,
            tenant_claim: "tenant_id".to_string(),
        }
    }
}

impl TenancyTomlConfig {
    /// Validate tenancy configuration.
    ///
    /// # Errors
    ///
    /// Returns an error if `tenant_claim` is empty when mode is not `none`.
    pub fn validate(&self) -> Result<()> {
        if !matches!(self.mode, TenancyModeConfig::None) && self.tenant_claim.is_empty() {
            anyhow::bail!("tenancy.tenant_claim must not be empty when mode is not 'none'");
        }
        Ok(())
    }

    /// Lower into the type the runtime deserializes.
    ///
    /// This is the **only** lowering of authored tenancy into the compiled shape, and
    /// both compile workflows go through it: `SecurityConfig::to_json` for a project
    /// `fraiseql.toml`, and `schema::merger` for a TOML schema (`#892`). A second
    /// hand-written copy is exactly how the seam produced `#757` — `to_json` emitted
    /// `tenantClaim`, which `TenancyConfig` does not declare, so a configured claim
    /// vanished into the untyped catch-all and the runtime silently reverted to its
    /// `"tenant_id"` default while compile-time `@tenant_id` validation read the
    /// camelCase key. Compiler and server then disagreed about which claim carries
    /// the tenant.
    ///
    /// Going through `TenancyConfig` makes them agree by construction: a rename on
    /// either side is a compile error here, not a silent default at runtime.
    #[must_use]
    pub fn to_runtime(&self) -> fraiseql_core::schema::TenancyConfig {
        fraiseql_core::schema::TenancyConfig {
            mode:         match self.mode {
                TenancyModeConfig::None => fraiseql_core::schema::TenancyMode::None,
                TenancyModeConfig::Row => fraiseql_core::schema::TenancyMode::Row,
                TenancyModeConfig::Schema => fraiseql_core::schema::TenancyMode::Schema,
            },
            tenant_claim: self.tenant_claim.clone(),
        }
    }

    /// Convert to JSON representation for compiled schema.
    pub fn to_json(&self) -> serde_json::Value {
        serde_json::to_value(self.to_runtime()).unwrap_or_else(|_| serde_json::json!({}))
    }
}

/// Complete security configuration from fraiseql.toml
#[derive(Debug, Clone, Default, Deserialize, Serialize)]
#[serde(default, deny_unknown_fields)]
pub struct SecurityConfig {
    /// Audit logging configuration
    #[serde(rename = "audit_logging")]
    pub audit_logging:      AuditLoggingConfig,
    /// Error sanitization configuration
    #[serde(rename = "error_sanitization")]
    pub error_sanitization: ErrorSanitizationConfig,
    /// Rate limiting configuration
    #[serde(rename = "rate_limiting")]
    pub rate_limiting:      RateLimitConfig,
    /// State encryption configuration
    #[serde(rename = "state_encryption")]
    pub state_encryption:   StateEncryptionConfig,
    /// Constant-time comparison configuration
    #[serde(rename = "constant_time")]
    pub constant_time:      ConstantTimeConfig,
    /// Field-level RBAC role definitions
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub role_definitions:   Vec<RoleDefinitionConfig>,
    /// Default role when user has no explicit role assignment
    #[serde(skip_serializing_if = "Option::is_none")]
    pub default_role:       Option<String>,
    /// Declare that this schema serves multiple tenants (`multi_tenant = true`).
    ///
    /// A non-`none` `[fraiseql.tenancy] mode` implies it. Activates the
    /// subscription tenant fail-closed gate and the cache+RLS boot gate — both of
    /// which named this key in their error messages while `deny_unknown_fields`
    /// made it a compile error to set (#758).
    pub multi_tenant:       bool,
    /// Declare that database Row-Level Security isolates this schema's data
    /// (`[fraiseql.security.rls] enabled = true`). Verified against the live
    /// catalog at boot.
    pub rls:                fraiseql_core::schema::RlsConfig,

    /// Machine service accounts, keyed by account name (#983).
    ///
    /// ```toml
    /// [fraiseql.security.service_accounts.reconciler]
    /// secret_env = "FRAISEQL_SA_RECONCILER_SECRET"
    /// roles      = ["reconciler"]
    /// scopes     = ["write:Invoice"]
    /// ```
    ///
    /// The server has consumed `security.service_accounts` since #977, but no
    /// authoring workflow could write it: only a hand-written `schema.json` could,
    /// so the feature was reachable only by accident. The secret is never inlined —
    /// `secret_env` names the environment variable holding it (ADR-0018).
    #[serde(skip_serializing_if = "std::collections::HashMap::is_empty")]
    pub service_accounts:
        std::collections::HashMap<String, fraiseql_core::schema::ServiceAccountConfig>,
}

impl SecurityConfig {
    /// Validate all security configurations
    ///
    /// # Errors
    ///
    /// Returns an error if any sub-configuration is invalid, if a role name is
    /// empty, or if a role definition contains no scopes.
    pub fn validate(&self) -> Result<()> {
        self.error_sanitization.validate()?;
        self.rate_limiting.validate()?;
        self.state_encryption.validate()?;
        self.reject_constant_time_section()?;

        for (name, account) in &self.service_accounts {
            if account.secret_env.trim().is_empty() {
                anyhow::bail!(
                    "service account '{name}' must set `secret_env` — the name of the \
                     environment variable holding its bearer secret. The secret itself is \
                     never written to the config."
                );
            }
            if account.roles.is_empty() && account.scopes.is_empty() {
                anyhow::bail!(
                    "service account '{name}' grants neither roles nor scopes, so it can do \
                     nothing. Give it a `run_as` ceiling, or remove it."
                );
            }
        }

        // Validate role definitions if present
        for role in &self.role_definitions {
            role.validate()?;
        }

        Ok(())
    }

    /// Refuse `[fraiseql.security.constant_time]`, which configures something that is
    /// not configurable.
    ///
    /// Constant-time comparison is applied unconditionally wherever a secret is
    /// compared, so there is nothing for these toggles to switch. They were emitted as
    /// `constantTime`, which no consumer read in either casing — and one of the keys
    /// inside was misspelled `applytoCsrfTokens`, which nothing noticed precisely
    /// because nothing read it (#893).
    ///
    /// Refused rather than dropped: an operator who writes `apply_to_jwt = false`
    /// should learn that the setting does not exist, not have it silently accepted.
    fn reject_constant_time_section(&self) -> Result<()> {
        let defaults = ConstantTimeConfig::default();
        let configured = self.constant_time.enabled != defaults.enabled
            || self.constant_time.apply_to_jwt != defaults.apply_to_jwt
            || self.constant_time.apply_to_session_tokens != defaults.apply_to_session_tokens
            || self.constant_time.apply_to_csrf_tokens != defaults.apply_to_csrf_tokens
            || self.constant_time.apply_to_refresh_tokens != defaults.apply_to_refresh_tokens;

        if configured {
            anyhow::bail!(
                "[security.constant_time] is accepted but not consumed: FraiseQL always compares \
                 tokens and secrets in constant time, and there is no code path that reads these \
                 toggles — so a `false` here would not disable anything, and a `true` does not \
                 enable anything that was off. Remove the section."
            );
        }
        Ok(())
    }

    /// Convert to JSON representation for schema.json.
    ///
    /// Built by constructing the typed `fraiseql_core::schema::SecurityConfig` and
    /// serializing it (#977): the compiled seam is `deny_unknown_fields`, so a key
    /// this path emitted under a name the consumer does not declare would fail the
    /// compile instead of landing in a catch-all nothing reads. That catch-all is
    /// how this path shipped camelCase sections (`auditLogging`, `constantTime`
    /// with a misspelled `applytoCsrfTokens`) that no consumer read in any casing
    /// (#757, #893).
    ///
    /// Mapping notes:
    /// - `[fraiseql.security.audit_logging] enabled` lowers onto `enterprise.audit_logging_enabled`
    ///   — the field the runtime actually reads. Its other keys (`log_level`, `buffer_size`, …)
    ///   have no consumer anywhere and are not emitted.
    /// - `[fraiseql.security.error_sanitization] enabled` lowers onto the compiled
    ///   `error_sanitization.enabled`; its other keys (`generic_messages`, `user_facing_format`, …)
    ///   likewise have no consumer and are not emitted.
    /// - `state_encryption` lowers `enabled` + `algorithm`; `key_rotation_enabled`, `nonce_size`
    ///   and `key_size` have no consumer (nonce/key sizes are fixed by the algorithm) and are not
    ///   emitted.
    pub fn to_json(&self) -> serde_json::Value {
        let role_definitions: Vec<fraiseql_core::schema::RoleDefinition> =
            self.role_definitions.iter().map(RoleDefinitionConfig::to_runtime).collect();

        let algorithm = if self.state_encryption.algorithm == "aes-256-gcm" {
            fraiseql_core::schema::EncryptionAlgorithm::Aes256Gcm
        } else {
            // `validate()` restricts the field to the two supported names.
            fraiseql_core::schema::EncryptionAlgorithm::Chacha20Poly1305
        };

        let runtime = fraiseql_core::schema::SecurityConfig {
            role_definitions,
            default_role: self.default_role.clone(),
            multi_tenant: self.multi_tenant,
            rls: self.rls.clone(),
            enterprise: Some(fraiseql_core::schema::EnterpriseSecurityConfig {
                audit_logging_enabled: self.audit_logging.enabled,
                error_sanitization: self.error_sanitization.enabled,
                rate_limiting_enabled: self.rate_limiting.enabled,
                ..Default::default()
            }),
            error_sanitization: Some(fraiseql_core::schema::ErrorSanitizationConfig {
                enabled: self.error_sanitization.enabled,
                ..Default::default()
            }),
            rate_limiting: Some(fraiseql_core::schema::RateLimitingSecurityConfig {
                enabled: self.rate_limiting.enabled,
                requests_per_second: self.rate_limiting.requests_per_second,
                burst_size: self.rate_limiting.burst_size,
                auth_start_max_requests: self.rate_limiting.auth_start_max_requests,
                auth_start_window_secs: self.rate_limiting.auth_start_window_secs,
                auth_callback_max_requests: self.rate_limiting.auth_callback_max_requests,
                auth_callback_window_secs: self.rate_limiting.auth_callback_window_secs,
                auth_refresh_max_requests: self.rate_limiting.auth_refresh_max_requests,
                auth_refresh_window_secs: self.rate_limiting.auth_refresh_window_secs,
                auth_logout_max_requests: self.rate_limiting.auth_logout_max_requests,
                auth_logout_window_secs: self.rate_limiting.auth_logout_window_secs,
                failed_login_max_attempts: self.rate_limiting.failed_login_max_requests,
                failed_login_lockout_secs: self.rate_limiting.failed_login_window_secs,
                requests_per_second_per_user: None,
                redis_url: None,
                trust_proxy_headers: self.rate_limiting.trust_proxy_headers,
                // Emitted even when empty: with `trust_proxy_headers = true`, an
                // explicit empty list is what the #618 boot guard refuses, while an
                // absent list means "trust every proxy" (warned). Collapsing empty
                // to absent would silently weaken the guard.
                trusted_proxy_cidrs: Some(self.rate_limiting.trusted_proxy_cidrs.clone()),
            }),
            state_encryption: Some(fraiseql_core::schema::StateEncryptionConfig {
                enabled: self.state_encryption.enabled,
                algorithm,
                ..Default::default()
            }),
            // #983: the server has read this since #977; this is the authoring side
            // it never had. Absent when empty so the compiled section stays quiet.
            service_accounts: (!self.service_accounts.is_empty())
                .then(|| self.service_accounts.clone()),
            ..fraiseql_core::schema::SecurityConfig::default()
        };
        serde_json::to_value(runtime).unwrap_or_else(|_| serde_json::json!({}))
    }
}

#[cfg(test)]
mod security_seam_tests {
    #![allow(clippy::unwrap_used, clippy::expect_used)] // Reason: test code.

    use super::*;

    /// Every section name `SecurityConfig::to_json` emits, and the consumer that reads it.
    ///
    /// Hand-written producer keys drift from the consumer struct's field names, which
    /// is the whole of #757 and #893. The list is asserted rather than described so a
    /// rename on either side has to come here.
    // A default config serializes exactly these sections: the typed
    // `SecurityConfig` (#977) skips fields carrying no information
    // (`multi_tenant = false`, a default `rls`, empty lists, `None`s), and
    // `audit_logging` lowers onto `enterprise.audit_logging_enabled` — the key
    // the runtime actually reads.
    const EXPECTED_SECTIONS: &[&str] = &[
        "enterprise",
        "error_sanitization",
        "rate_limiting",
        "state_encryption",
    ];

    /// #983 — the server has consumed `security.service_accounts` since #977, but no
    /// authoring workflow could write it: only a hand-written `schema.json` could, so
    /// the feature was reachable only by accident. This is the authoring side, and it
    /// must reach the compiled seam under the name the consumer declares.
    #[test]
    fn a_toml_service_account_reaches_the_compiled_seam() {
        let toml = r#"
[service_accounts.reconciler]
secret_env = "FRAISEQL_SA_RECONCILER_SECRET"
roles      = ["reconciler"]
scopes     = ["write:Invoice"]
"#;
        let config: SecurityConfig = toml::from_str(toml).expect("the section must parse");
        config
            .validate()
            .expect("a service account with a secret_env and a ceiling is valid");

        let json = config.to_json();
        let account = &json["service_accounts"]["reconciler"];
        assert_eq!(account["secret_env"], "FRAISEQL_SA_RECONCILER_SECRET");
        assert_eq!(account["roles"][0], "reconciler");
        assert_eq!(account["scopes"][0], "write:Invoice");

        // The compiled seam is `deny_unknown_fields`, so round-tripping through the
        // consumer's own type is what proves the key names match.
        let compiled: fraiseql_core::schema::SecurityConfig =
            serde_json::from_value(json).expect("the emitted block must load as the consumer type");
        let accounts = compiled.service_accounts.expect("service_accounts must survive");
        assert_eq!(accounts["reconciler"].secret_env, "FRAISEQL_SA_RECONCILER_SECRET");
    }

    /// A service account that grants nothing can do nothing — refused rather than
    /// compiled into a credential with no authority.
    #[test]
    fn a_service_account_with_no_ceiling_is_refused() {
        let toml = r#"
[service_accounts.ghost]
secret_env = "FRAISEQL_SA_GHOST_SECRET"
"#;
        let config: SecurityConfig = toml::from_str(toml).expect("parses");
        let err = config.validate().expect_err("no roles and no scopes must be refused");
        assert!(
            err.to_string().contains("neither roles nor scopes"),
            "the refusal must say why; got: {err}"
        );
    }

    #[test]
    fn to_json_emits_exactly_the_sections_consumers_read() {
        let json = SecurityConfig::default().to_json();
        let obj = json.as_object().expect("an object");

        let mut keys: Vec<&str> = obj.keys().map(String::as_str).collect();
        keys.sort_unstable();
        let mut expected: Vec<&str> = EXPECTED_SECTIONS.to_vec();
        expected.sort_unstable();

        assert_eq!(
            keys, expected,
            "the compiled security block must carry exactly the sections consumers read; a \
             camelCase spelling here reaches nothing (#893)"
        );
    }

    #[test]
    fn no_section_name_is_camel_case() {
        let json = SecurityConfig::default().to_json();
        for key in json.as_object().unwrap().keys() {
            assert!(
                !key.chars().any(char::is_uppercase),
                "section {key:?} is camelCase; the consumers and the other compile path \
                 (schema/merger.rs) both use snake_case (#893)"
            );
        }
    }

    #[test]
    fn rate_limiting_is_flat_and_carries_a_usable_budget() {
        let json = SecurityConfig::default().to_json();
        let rl = json.get("rate_limiting").expect("rate_limiting section").clone();

        // Flat, not the old nested `authStart: { maxRequests }` shape, which matched
        // no reader even once the section name was right.
        assert!(rl.get("authStart").is_none(), "the nested camelCase shape must be gone");
        assert_eq!(
            rl.get("auth_start_max_requests").and_then(serde_json::Value::as_u64),
            Some(100)
        );

        // The trap in #893: a section emitted without these two keys deserializes them
        // to zero on the consumer side, giving a limiter that denies every request.
        let rps = rl.get("requests_per_second").and_then(serde_json::Value::as_u64).unwrap();
        let burst = rl.get("burst_size").and_then(serde_json::Value::as_u64).unwrap();
        assert!(rps > 0, "a default project config must not compile to a zero-rps limiter");
        assert!(burst > 0, "a default project config must not compile to a zero-burst limiter");
    }

    #[test]
    fn failed_login_defaults_match_the_runtimes_so_an_untouched_config_boots() {
        // The runtime refuses to boot in production when these are tuned away from its
        // defaults (#356), because the binary has no first-factor login to enforce
        // them. An untouched project config must therefore emit exactly those defaults.
        let json = SecurityConfig::default().to_json();
        let rl = json.get("rate_limiting").unwrap();
        assert_eq!(
            rl.get("failed_login_max_attempts").and_then(serde_json::Value::as_u64),
            Some(10)
        );
        assert_eq!(
            rl.get("failed_login_lockout_secs").and_then(serde_json::Value::as_u64),
            Some(900)
        );
    }

    #[test]
    fn a_configured_constant_time_section_is_refused() {
        let config = SecurityConfig {
            constant_time: ConstantTimeConfig {
                apply_to_jwt: !ConstantTimeConfig::default().apply_to_jwt,
                ..ConstantTimeConfig::default()
            },
            ..SecurityConfig::default()
        };
        let err = config.validate().expect_err("an inert section must be refused, not ignored");
        assert!(err.to_string().contains("constant_time"), "{err}");
    }

    #[test]
    fn an_untouched_constant_time_section_still_validates() {
        SecurityConfig::default().validate().expect("defaults must validate");
    }
}
