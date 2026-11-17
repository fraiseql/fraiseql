//! Security configuration parsing from fraiseql.toml

use anyhow::Result;
use serde::{Deserialize, Serialize};

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
    /// Returns an error if `tenant_claim` is empty: it names the request's tenant in every
    /// mode (#1542).
    pub fn validate(&self) -> Result<()> {
        if self.tenant_claim.is_empty() {
            anyhow::bail!(
                "tenancy.tenant_claim must not be empty: it names the JWT claim that carries the \
                 request's tenant (default \"tenant_id\")"
            );
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
    /// `[fraiseql.security.enterprise]`: `audit_logging_enabled`, on unless written off.
    ///
    /// This, `error_sanitization`, `rate_limiting` and `state_encryption` are the TOML
    /// schema's sections, in the same compiled types, lowered by the same
    /// [`SecuritySettings::shared_sections`](super::toml_schema::security::SecuritySettings::shared_sections).
    pub enterprise:         fraiseql_core::schema::EnterpriseSecurityConfig,
    /// `[fraiseql.security.error_sanitization]`. Absent, the server decides at boot
    /// (sanitizing in production); written, it is on unless `enabled = false`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error_sanitization: Option<fraiseql_core::schema::ErrorSanitizationConfig>,
    /// `[fraiseql.security.rate_limiting]`. Absent, the global limit is off and the auth
    /// endpoint rules apply; written, it is on unless `enabled = false`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub rate_limiting:      Option<fraiseql_core::schema::RateLimitingSecurityConfig>,
    /// State encryption: the compiled type, read as a TOML schema reads it, so `key_env`
    /// names the key's variable here too. Off unless written (#1151): an absent table lowers
    /// the compiled default, which is disabled.
    #[serde(
        rename = "state_encryption",
        deserialize_with = "super::toml_schema::security::state_encryption"
    )]
    pub state_encryption:   fraiseql_core::schema::StateEncryptionConfig,
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

    /// PKCE for the OAuth Authorization Code flow (`[fraiseql.security.pkce]`).
    ///
    /// This and the access-control sections below are lowered by the same
    /// [`SecuritySettings::shared_sections`](super::toml_schema::security::SecuritySettings::shared_sections).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pkce:                   Option<fraiseql_core::schema::PkceSecurityConfig>,
    /// API key authentication (`[fraiseql.security.api_keys]`).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub api_keys:               Option<fraiseql_core::schema::ApiKeySecurityConfig>,
    /// Token revocation by `jti` (`[fraiseql.security.token_revocation]`).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub token_revocation:       Option<fraiseql_core::schema::TokenRevocationSecurityConfig>,
    /// The trusted-documents allowlist (`[fraiseql.security.trusted_documents]`).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub trusted_documents:      Option<fraiseql_core::schema::TrustedDocumentsConfig>,
    /// Refuse every operation that is not a trusted document
    /// (`[fraiseql.security] persisted_queries_only`).
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub persisted_queries_only: bool,
    /// Operation cost budgets (`[fraiseql.security.cost_budget]`).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cost_budget:            Option<fraiseql_core::schema::CostBudgetConfig>,
}

impl SecurityConfig {
    /// Validate all security configurations
    ///
    /// # Errors
    ///
    /// Returns an error if any sub-configuration is invalid, if a role name is
    /// empty, or if a role definition contains no scopes.
    pub fn validate(&self) -> Result<()> {
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
    /// Every section the TOML schema also takes (`enterprise`, `error_sanitization`,
    /// `rate_limiting`, `state_encryption` and the access-control sections) is in the compiled
    /// type and lowered by the TOML schema's own lowering, so both files compile it alike.
    pub fn to_json(&self) -> serde_json::Value {
        let role_definitions: Vec<fraiseql_core::schema::RoleDefinition> =
            self.role_definitions.iter().map(RoleDefinitionConfig::to_runtime).collect();

        let super::toml_schema::security::SharedSections {
            enterprise,
            error_sanitization,
            rate_limiting,
            state_encryption,
            pkce,
            api_keys,
            token_revocation,
            trusted_documents,
            persisted_queries_only,
            cost_budget,
        } = self.toml_schema_settings().shared_sections();

        let runtime = fraiseql_core::schema::SecurityConfig {
            role_definitions,
            default_role: self.default_role.clone(),
            multi_tenant: self.multi_tenant,
            rls: self.rls.clone(),
            enterprise: Some(enterprise),
            error_sanitization,
            rate_limiting,
            state_encryption,
            pkce,
            api_keys,
            token_revocation,
            trusted_documents,
            persisted_queries_only,
            cost_budget,
            // #983: the server has read this since #977; this is the authoring side
            // it never had. Absent when empty so the compiled section stays quiet.
            service_accounts: (!self.service_accounts.is_empty())
                .then(|| self.service_accounts.clone()),
            ..fraiseql_core::schema::SecurityConfig::default()
        };
        serde_json::to_value(runtime).unwrap_or_else(|_| serde_json::json!({}))
    }

    /// The sections this table shares with a TOML schema's `[security]`, as that table: what
    /// the TOML schema's lowering and checks read.
    #[must_use]
    pub fn toml_schema_settings(&self) -> super::toml_schema::security::SecuritySettings {
        super::toml_schema::security::SecuritySettings {
            enterprise: self.enterprise.clone(),
            error_sanitization: self.error_sanitization.clone(),
            rate_limiting: self.rate_limiting.clone(),
            state_encryption: Some(self.state_encryption.clone()),
            pkce: self.pkce.clone(),
            api_keys: self.api_keys.clone(),
            token_revocation: self.token_revocation.clone(),
            trusted_documents: self.trusted_documents.clone(),
            persisted_queries_only: self.persisted_queries_only,
            cost_budget: self.cost_budget.clone(),
            ..Default::default()
        }
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
    // (`multi_tenant = false`, a default `rls`, empty lists, `None`s, an unwritten
    // `error_sanitization` or `rate_limiting`), and `enterprise` carries
    // `audit_logging_enabled`, which defaults on.
    const EXPECTED_SECTIONS: &[&str] = &["enterprise", "state_encryption"];

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

    /// `[fraiseql.security.rate_limiting]` written with no keys.
    fn written_rate_limiting() -> serde_json::Value {
        let config: SecurityConfig = toml::from_str("[rate_limiting]\n").expect("parses");
        config.to_json()["rate_limiting"].clone()
    }

    #[test]
    fn rate_limiting_is_flat_and_carries_a_usable_budget() {
        let rl = written_rate_limiting();

        // Flat, not the old nested `authStart: { maxRequests }` shape, which matched
        // no reader even once the section name was right.
        assert!(rl.get("authStart").is_none(), "the nested camelCase shape must be gone");
        assert_eq!(
            rl.get("auth_start_max_requests").and_then(serde_json::Value::as_u64),
            Some(u64::from(
                fraiseql_core::schema::RateLimitingSecurityConfig::DEFAULT.auth_start_max_requests
            ))
        );

        // The trap in #893: a section emitted without these two keys deserializes them
        // to zero on the consumer side, giving a limiter that denies every request.
        let rps = rl.get("requests_per_second").and_then(serde_json::Value::as_u64).unwrap();
        let burst = rl.get("burst_size").and_then(serde_json::Value::as_u64).unwrap();
        assert!(rps > 0, "a written section must not compile to a zero-rps limiter");
        assert!(burst > 0, "a written section must not compile to a zero-burst limiter");
    }

    /// A project that writes no `[fraiseql.security.rate_limiting]` emits none, as a TOML
    /// schema does: the server then leaves the global limit off and applies its own auth
    /// endpoint budgets. Emitting a section here is what made the two files compile one
    /// untouched config to two postures.
    #[test]
    fn an_untouched_project_emits_no_rate_limiting_or_error_sanitization() {
        let json = SecurityConfig::default().to_json();
        assert!(json.get("rate_limiting").is_none(), "{json}");
        assert!(json.get("error_sanitization").is_none(), "{json}");
    }

    #[test]
    fn failed_login_defaults_match_the_runtimes_so_a_written_section_boots() {
        // The runtime refuses to boot in production when these are tuned away from its
        // defaults (#356), because the binary has no first-factor login to enforce
        // them. A section that does not write them must therefore emit exactly those.
        let rl = written_rate_limiting();
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
