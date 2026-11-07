//! Security configuration parsing from fraiseql.toml

use anyhow::Result;
use serde::{Deserialize, Serialize};

/// Field-level RBAC role definition from fraiseql.toml
#[derive(Debug, Clone, Deserialize, Serialize, schemars::JsonSchema)]
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
    /// The single lowering of a role, which `SecuritySettings::to_compiled` calls for
    /// every `fraiseql.toml`. `role_has_scope` is the sole input to field-level access, so a grant
    /// that does not arrive here is a grant that does not exist; #757 is what a second
    /// hand-written copy of this mapping costs.
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
#[derive(Debug, Clone, Default, Deserialize, Serialize, schemars::JsonSchema)]
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

/// Tenancy configuration from `[tenancy]` in fraiseql.toml.
#[derive(Debug, Clone, Deserialize, Serialize, schemars::JsonSchema)]
#[serde(default, deny_unknown_fields)]
pub struct TenancyTomlConfig {
    /// Isolation strategy: `"none"`, `"row"`, or `"schema"`.
    pub mode:         TenancyModeConfig,
    /// JWT claim name that carries the tenant identifier.
    pub tenant_claim: String,
    /// Types every tenant reads the same, under `mode = "row"`: every type a root returns
    /// carries a `@tenant_id` field or is listed here.
    pub shared_types: Vec<String>,
}

impl Default for TenancyTomlConfig {
    fn default() -> Self {
        Self {
            mode:         TenancyModeConfig::None,
            tenant_claim: "tenant_id".to_string(),
            shared_types: Vec::new(),
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
    /// This is the **only** lowering of authored tenancy into the compiled shape:
    /// `SecuritySettings::to_compiled` calls it for every `fraiseql.toml`. A second
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
            shared_types: self.shared_types.clone(),
        }
    }
}

#[cfg(test)]
mod security_seam_tests {
    #![allow(clippy::unwrap_used, clippy::expect_used)] // Reason: test code.

    use super::*;
    use crate::config::toml_schema::security::SecuritySettings;

    /// `settings` and a default `[tenancy]`, lowered and serialized.
    fn compiled(settings: &SecuritySettings) -> serde_json::Value {
        serde_json::to_value(settings.to_compiled(&TenancyTomlConfig::default())).unwrap()
    }

    /// Every section name `SecuritySettings::to_compiled` emits, and the consumer that reads it.
    ///
    /// Hand-written producer keys drift from the consumer struct's field names, which
    /// is the whole of #757 and #893. The list is asserted rather than described so a
    /// rename on either side has to come here.
    // A default config serializes exactly these sections: the typed
    // `SecurityConfig` (#977) skips fields carrying no information
    // (`multi_tenant = false`, a default `rls`, empty lists, `None`s, an unwritten
    // `error_sanitization` or `rate_limiting`), and `enterprise` carries
    // `audit_logging_enabled`, which defaults on.
    const EXPECTED_SECTIONS: &[&str] = &["enterprise"];

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
        let config: SecuritySettings = toml::from_str(toml).expect("the section must parse");
        config
            .validate_service_accounts()
            .expect("a service account with a secret_env and a ceiling is valid");

        let json = compiled(&config);
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
        let config: SecuritySettings = toml::from_str(toml).expect("parses");
        let err = config
            .validate_service_accounts()
            .expect_err("no roles and no scopes must be refused");
        assert!(
            err.to_string().contains("neither roles nor scopes"),
            "the refusal must say why; got: {err}"
        );
    }

    #[test]
    fn to_json_emits_exactly_the_sections_consumers_read() {
        let json = compiled(&SecuritySettings::default());
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
        let json = compiled(&SecuritySettings::default());
        for key in json.as_object().unwrap().keys() {
            assert!(
                !key.chars().any(char::is_uppercase),
                "section {key:?} is camelCase; the consumers and the other compile path \
                 (schema/merger.rs) both use snake_case (#893)"
            );
        }
    }

    /// `[security.rate_limiting]` written with no keys.
    fn written_rate_limiting() -> serde_json::Value {
        let config: SecuritySettings = toml::from_str("[rate_limiting]\n").expect("parses");
        compiled(&config)["rate_limiting"].clone()
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

    /// A project that writes no `[security.rate_limiting]` emits none, as a TOML
    /// schema does: the server then leaves the global limit off and applies its own auth
    /// endpoint budgets. Emitting a section here is what made the two files compile one
    /// untouched config to two postures.
    #[test]
    fn an_untouched_project_emits_no_rate_limiting_or_error_sanitization() {
        let json = compiled(&SecuritySettings::default());
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
}
