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
