//! Security configuration types for compiled schemas.
//!
//! Contains role definitions, scope types, and injected parameter sources
//! that are compiled from `fraiseql.toml` into `schema.compiled.json`.

use std::collections::HashMap;

use serde::{Deserialize, Serialize};

use super::domain_types::{RoleName, Scope};

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

            // Handle wildcard patterns like "read:*" or "admin:*"
            if let Some(prefix) = scope.strip_suffix(":*") {
                return required_scope.starts_with(prefix) && required_scope.contains(':');
            }

            // Handle Type.* wildcard patterns like "read:User.*"
            if let Some(prefix) = scope.strip_suffix('*') {
                return required_scope.starts_with(prefix);
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
    /// The adapter issues `SET search_path TO tenant_{key}, public` on
    /// connection acquisition. `TenantExecutorFactory` provisions/drops
    /// schemas via DDL.
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

/// Security configuration from fraiseql.toml.
///
/// Contains role definitions and other security-related settings
/// that are compiled into schema.compiled.json.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct SecurityConfig {
    /// Role definitions mapping role names to their granted scopes.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub role_definitions: Vec<RoleDefinition>,

    /// Default role when none is specified.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub default_role: Option<String>,

    /// Whether this schema serves multiple tenants with data isolation via RLS.
    ///
    /// When `true` and caching is enabled, FraiseQL verifies that Row-Level Security
    /// is active on the database at startup. This prevents silent cross-tenant data
    /// leakage through the cache.
    ///
    /// Set `rls_enforcement` in `CacheConfig` to control whether a missing RLS check
    /// causes a startup failure or only emits a warning.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub multi_tenant: bool,

    /// Tenancy isolation configuration for multi-tenant deployments.
    ///
    /// When present and `mode != "none"`, the runtime enforces tenant isolation
    /// at the database level (row-based or schema-based).
    #[serde(default, skip_serializing_if = "is_default_tenancy")]
    pub tenancy: TenancyConfig,

    /// Additional security settings (rate limiting, audit logging, etc.)
    #[serde(flatten)]
    pub additional: HashMap<String, serde_json::Value>,
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

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)] // Reason: test code, panics are acceptable assertions

    use super::*;

    // ── TenancyMode ─────────────────────────────────────────────────────

    #[test]
    fn tenancy_mode_default_is_none() {
        assert_eq!(TenancyMode::default(), TenancyMode::None);
    }

    #[test]
    fn tenancy_mode_serde_round_trip() {
        for (mode, expected_str) in [
            (TenancyMode::None, "\"none\""),
            (TenancyMode::Row, "\"row\""),
            (TenancyMode::Schema, "\"schema\""),
        ] {
            let json = serde_json::to_string(&mode).unwrap();
            assert_eq!(json, expected_str, "serialization of {mode}");
            let back: TenancyMode = serde_json::from_str(&json).unwrap();
            assert_eq!(back, mode, "deserialization of {expected_str}");
        }
    }

    #[test]
    fn tenancy_mode_invalid_string_rejected() {
        let result: Result<TenancyMode, _> = serde_json::from_str("\"invalid\"");
        assert!(result.is_err(), "unknown variant must fail");
    }

    #[test]
    fn tenancy_mode_display() {
        assert_eq!(TenancyMode::None.to_string(), "none");
        assert_eq!(TenancyMode::Row.to_string(), "row");
        assert_eq!(TenancyMode::Schema.to_string(), "schema");
    }

    // ── TenancyConfig ───────────────────────────────────────────────────

    #[test]
    fn tenancy_config_default_values() {
        let config = TenancyConfig::default();
        assert_eq!(config.mode, TenancyMode::None);
        assert_eq!(config.tenant_claim, "tenant_id");
    }

    #[test]
    fn tenancy_config_serde_round_trip() {
        let config = TenancyConfig {
            mode:         TenancyMode::Row,
            tenant_claim: "org_id".to_string(),
        };
        let json = serde_json::to_string(&config).unwrap();
        let back: TenancyConfig = serde_json::from_str(&json).unwrap();
        assert_eq!(back, config);
    }

    #[test]
    fn tenancy_config_deserialize_from_compiled_json() {
        let json = r#"{"mode": "schema", "tenant_claim": "tenant_id"}"#;
        let config: TenancyConfig = serde_json::from_str(json).unwrap();
        assert_eq!(config.mode, TenancyMode::Schema);
        assert_eq!(config.tenant_claim, "tenant_id");
    }

    #[test]
    fn tenancy_config_defaults_when_empty() {
        let config: TenancyConfig = serde_json::from_str("{}").unwrap();
        assert_eq!(config.mode, TenancyMode::None);
        assert_eq!(config.tenant_claim, "tenant_id");
    }

    // ── SecurityConfig with tenancy ─────────────────────────────────────

    #[test]
    fn security_config_tenancy_defaults_to_none() {
        let config = SecurityConfig::default();
        assert_eq!(config.tenancy.mode, TenancyMode::None);
    }

    #[test]
    fn security_config_tenancy_skipped_when_default() {
        let config = SecurityConfig::default();
        let json = serde_json::to_string(&config).unwrap();
        // tenancy field should be absent when it's the default
        assert!(
            !json.contains("tenancy"),
            "default tenancy should be skipped in serialization"
        );
    }

    #[test]
    fn security_config_tenancy_present_when_non_default() {
        let config = SecurityConfig {
            tenancy: TenancyConfig {
                mode:         TenancyMode::Row,
                tenant_claim: "tenant_id".to_string(),
            },
            ..SecurityConfig::default()
        };
        let json = serde_json::to_string(&config).unwrap();
        assert!(json.contains("tenancy"), "non-default tenancy should be serialized");
        assert!(json.contains("\"row\""), "mode=row should appear in JSON");
    }

    #[test]
    fn security_config_with_tenancy_round_trip() {
        let config = SecurityConfig {
            tenancy: TenancyConfig {
                mode:         TenancyMode::Schema,
                tenant_claim: "org_id".to_string(),
            },
            ..SecurityConfig::default()
        };
        let json = serde_json::to_string(&config).unwrap();
        let back: SecurityConfig = serde_json::from_str(&json).unwrap();
        assert_eq!(back.tenancy.mode, TenancyMode::Schema);
        assert_eq!(back.tenancy.tenant_claim, "org_id");
    }
}
