//! Security context for runtime authorization
//!
//! This module provides the `SecurityContext` struct that flows through the executor,
//! carrying information about the authenticated user and their permissions.
//!
//! The security context is extracted from:
//! - JWT claims (`user_id` from 'sub', roles from 'roles', etc.)
//! - HTTP headers (`request_id`, `tenant_id`, etc.)
//! - Configuration (OAuth provider, scopes, etc.)
//!
//! # Architecture
//!
//! ```text
//! HTTP Request with Authorization header
//!     ↓
//! AuthMiddleware → AuthenticatedUser
//!     ↓
//! SecurityContext (created from AuthenticatedUser + request metadata)
//!     ↓
//! Executor (with context available for RLS policy evaluation)
//! ```
//!
//! # RLS Integration
//!
//! The `SecurityContext` is passed to `RLSPolicy::evaluate()` to determine what
//! rows a user can access. Policies are compiled into schema.compiled.json
//! and evaluated at runtime with the `SecurityContext`.

use std::collections::HashMap;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::{
    security::{ActorType, AuthenticatedUser, derive_actor},
    types::{TenantId, UserId},
};

/// Extract the user's roles from the (signature-verified) JWT custom claims.
///
/// Reads, in order, the scalar `role` claim, the `roles` array, and the
/// `fraiseql_roles` array, then sorts and de-duplicates. This mirrors the
/// claim names honoured by `fraiseql_auth::operation_rbac` so the GraphQL
/// `requires_role` gate and the observer-admin RBAC engine agree on what a
/// token grants. Without this, `SecurityContext.roles` was always empty and
/// every `requires_role` operation was unreachable over HTTP (#503).
fn roles_from_claims(extra_claims: &HashMap<String, serde_json::Value>) -> Vec<String> {
    let mut roles = Vec::new();

    if let Some(serde_json::Value::String(role)) = extra_claims.get("role") {
        roles.push(role.clone());
    }

    for key in ["roles", "fraiseql_roles"] {
        if let Some(serde_json::Value::Array(values)) = extra_claims.get(key) {
            roles.extend(values.iter().filter_map(|v| match v {
                serde_json::Value::String(role) => Some(role.clone()),
                _ => None,
            }));
        }
    }

    roles.sort();
    roles.dedup();
    roles
}

/// Security context for authorization evaluation.
///
/// Carries information about the authenticated user and their permissions
/// throughout the request lifecycle.
///
/// # Fields
///
/// - `user_id`: Unique identifier for the authenticated user (from JWT 'sub' claim)
/// - `roles`: User's roles (e.g., `["admin", "moderator"]`, from JWT 'roles' claim)
/// - `tenant_id`: Organization/tenant identifier for multi-tenant systems
/// - `scopes`: OAuth/permission scopes (e.g., `["read:user", "write:post"]`)
/// - `attributes`: Custom claims from JWT (e.g., department, region, tier)
/// - `request_id`: Correlation ID for audit logging and tracing
/// - `ip_address`: Client IP address for geolocation and fraud detection
/// - `authenticated_at`: When the JWT was issued
/// - `expires_at`: When the JWT expires
/// - `issuer`: Token issuer for multi-issuer systems
/// - `audience`: Token audience for validation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SecurityContext {
    /// User ID (from JWT 'sub' claim)
    pub user_id: UserId,

    /// User's roles (e.g., `["admin", "moderator"]`)
    ///
    /// Extracted from JWT 'roles' claim or derived from other claims.
    /// Used for role-based access control (RBAC) decisions.
    pub roles: Vec<String>,

    /// Tenant/organization ID (for multi-tenancy)
    ///
    /// When present, RLS policies can enforce tenant isolation.
    /// Extracted from JWT '`tenant_id`' or X-Tenant-Id header.
    pub tenant_id: Option<TenantId>,

    /// OAuth/permission scopes
    ///
    /// Format: `{action}:{resource}` or `{action}:{type}.{field}`
    /// Examples:
    /// - `read:user`
    /// - `write:post`
    /// - `read:User.email`
    /// - `admin:*`
    ///
    /// Extracted from JWT 'scope' claim.
    pub scopes: Vec<String>,

    /// Custom attributes from JWT claims
    ///
    /// Arbitrary key-value pairs from JWT payload.
    /// Examples: "department", "region", "tier", "country"
    ///
    /// Used by custom RLS policies that need domain-specific attributes.
    pub attributes: HashMap<String, serde_json::Value>,

    /// Request correlation ID for audit trails
    ///
    /// Extracted from X-Request-Id header or generated.
    /// Used for tracing and audit logging across services.
    pub request_id: String,

    /// Client IP address
    ///
    /// Extracted from X-Forwarded-For or connection socket.
    /// Used for geolocation and fraud detection in RLS policies.
    pub ip_address: Option<String>,

    /// When the JWT was issued
    pub authenticated_at: DateTime<Utc>,

    /// When the JWT expires
    pub expires_at: DateTime<Utc>,

    /// Token issuer (for multi-issuer systems)
    pub issuer: Option<String>,

    /// Token audience (for audience validation)
    pub audience: Option<String>,

    /// Normalised email address from the JWT `email` claim.
    ///
    /// Available as `jwt:email` in session variable mappings for RLS policies.
    pub email: Option<String>,

    /// Normalised display name from the JWT `name` claim.
    ///
    /// Available as `jwt:name` or `jwt:display_name` in session variable mappings.
    pub display_name: Option<String>,
}

impl SecurityContext {
    /// Attribute key under which the delegated user's UUID is carried (string
    /// form), for an agent request acting on behalf of a human. Read back via
    /// [`acting_for`](Self::acting_for) (#390).
    pub const ACTING_FOR_ATTRIBUTE: &'static str = "fraiseql.acting_for";
    /// Attribute key under which the request's [`ActorType`] is carried (the
    /// `snake_case` token), derived at [`from_user`](Self::from_user) and read
    /// back via [`actor_type`](Self::actor_type) (#390).
    pub const ACTOR_TYPE_ATTRIBUTE: &'static str = "fraiseql.actor_type";
    /// Attribute key under which the originating request's full W3C trace context
    /// is carried (a JSON object), used to populate the change-log `trace_context`
    /// JSONB column (#375).
    pub const TRACE_CONTEXT_ATTRIBUTE: &'static str = "fraiseql.trace_context";
    /// Attribute key under which the originating request's W3C trace id is
    /// stamped. Set by the server's request pipeline from the inbound
    /// `traceparent` header; read back via [`trace_id`](Self::trace_id).
    pub const TRACE_ID_ATTRIBUTE: &'static str = "fraiseql.trace_id";

    /// Create a security context from an authenticated user and request metadata.
    ///
    /// # Arguments
    ///
    /// * `user` - Authenticated user from JWT validation
    /// * `request_id` - Correlation ID for this request
    ///
    /// # Example
    ///
    /// ```no_run
    /// // Requires: a live AuthenticatedUser from JWT validation.
    /// // See: tests/integration/ for runnable examples.
    /// # use fraiseql_core::security::SecurityContext;
    /// # use fraiseql_core::security::AuthenticatedUser;
    /// # let authenticated_user: AuthenticatedUser = panic!("example");
    /// let context = SecurityContext::from_user(&authenticated_user, "req-123".to_string());
    /// ```
    #[must_use]
    pub fn from_user(user: &AuthenticatedUser, request_id: String) -> Self {
        // Classify the actor from the (signature-verified) JWT material at
        // construction time, so every auth path (OIDC, HS256, gRPC, MCP) that
        // builds a context from a user gets it. The API-key path overrides to
        // ServiceAccount at its own construction site (#390).
        let (actor_type, acting_for) =
            derive_actor(user.user_id.as_str(), &user.scopes, &user.extra_claims);
        SecurityContext {
            user_id: user.user_id.clone(),
            roles: roles_from_claims(&user.extra_claims),
            tenant_id: None,
            scopes: user.scopes.clone(),
            attributes: HashMap::new(),
            request_id,
            ip_address: None,
            authenticated_at: Utc::now(),
            expires_at: user.expires_at,
            issuer: None,
            audience: None,
            email: user.email.clone(),
            display_name: user.display_name.clone(),
        }
        .with_actor_type(actor_type)
        .with_acting_for(acting_for)
    }

    /// Check if the user has a specific role.
    ///
    /// # Arguments
    ///
    /// * `role` - Role name to check (e.g., "admin", "moderator")
    ///
    /// # Returns
    ///
    /// `true` if the user has the specified role, `false` otherwise.
    #[must_use]
    pub fn has_role(&self, role: &str) -> bool {
        self.roles.iter().any(|r| r == role)
    }

    /// Check if the user has a specific scope.
    ///
    /// Supports wildcards: `admin:*` matches any admin scope.
    ///
    /// # Arguments
    ///
    /// * `scope` - Scope to check (e.g., "read:user", "write:post")
    ///
    /// # Returns
    ///
    /// `true` if the user has the specified scope, `false` otherwise.
    #[must_use]
    pub fn has_scope(&self, scope: &str) -> bool {
        self.scopes.iter().any(|s| {
            if s == scope {
                return true;
            }
            // Support wildcard matching: "admin:*" matches "admin:read"
            if s.ends_with(':') {
                scope.starts_with(s)
            } else if s.ends_with('*') {
                let prefix = &s[..s.len() - 1];
                scope.starts_with(prefix)
            } else {
                false
            }
        })
    }

    /// Get a custom attribute from the JWT claims.
    ///
    /// # Arguments
    ///
    /// * `key` - Attribute name
    ///
    /// # Returns
    ///
    /// The attribute value if present, `None` otherwise.
    #[must_use]
    pub fn get_attribute(&self, key: &str) -> Option<&serde_json::Value> {
        self.attributes.get(key)
    }

    /// The originating request's W3C trace id, when the server stamped one from
    /// the inbound `traceparent` header.
    ///
    /// Used to populate the change-log `trace_id` column so an outbox row links
    /// back to its distributed trace (#375); the #392 perf tooling surfaces it as
    /// the investigation handle. `None` when the request carried no trace context.
    #[must_use]
    pub fn trace_id(&self) -> Option<&str> {
        self.attributes
            .get(Self::TRACE_ID_ATTRIBUTE)
            .and_then(serde_json::Value::as_str)
    }

    /// Stamp the originating request's W3C trace id onto the context (carried in
    /// `attributes`). Read back via [`trace_id`](Self::trace_id).
    #[must_use]
    pub fn with_trace_id(mut self, trace_id: impl Into<String>) -> Self {
        self.attributes.insert(
            Self::TRACE_ID_ATTRIBUTE.to_string(),
            serde_json::Value::String(trace_id.into()),
        );
        self
    }

    /// The originating request's full W3C trace context (a JSON object), when the
    /// server stamped one from the inbound `traceparent`/`tracestate` headers.
    ///
    /// Used to populate the change-log `trace_context` JSONB column so an outbox
    /// row carries enough to re-propagate the distributed trace (#375). `None` when
    /// the request carried no valid trace context.
    #[must_use]
    pub fn trace_context(&self) -> Option<&serde_json::Value> {
        self.attributes.get(Self::TRACE_CONTEXT_ATTRIBUTE)
    }

    /// Stamp the originating request's full W3C trace context (a JSON object) onto
    /// the context (carried in `attributes`). Read back via
    /// [`trace_context`](Self::trace_context).
    #[must_use]
    pub fn with_trace_context(mut self, trace_context: serde_json::Value) -> Self {
        self.attributes.insert(Self::TRACE_CONTEXT_ATTRIBUTE.to_string(), trace_context);
        self
    }

    /// The request's actor classification (#390). Derived at
    /// [`from_user`](Self::from_user); [`ActorType::HumanUser`] when unset (e.g. a
    /// context built directly without derivation).
    #[must_use]
    pub fn actor_type(&self) -> ActorType {
        self.attributes
            .get(Self::ACTOR_TYPE_ATTRIBUTE)
            .and_then(serde_json::Value::as_str)
            .and_then(ActorType::from_token)
            .unwrap_or_default()
    }

    /// Stamp the request's [`ActorType`] onto the context (carried in
    /// `attributes`). Read back via [`actor_type`](Self::actor_type).
    #[must_use]
    pub fn with_actor_type(mut self, actor_type: ActorType) -> Self {
        self.attributes.insert(
            Self::ACTOR_TYPE_ATTRIBUTE.to_string(),
            serde_json::Value::String(actor_type.as_str().to_string()),
        );
        self
    }

    /// The delegated user (the human a delegated agent acts for), when the request
    /// carried an RFC 8693 `act` claim (#390). `None` for a non-delegated request,
    /// or when the underlying subject was not UUID-shaped.
    #[must_use]
    pub fn acting_for(&self) -> Option<Uuid> {
        self.attributes
            .get(Self::ACTING_FOR_ATTRIBUTE)
            .and_then(serde_json::Value::as_str)
            .and_then(|s| Uuid::parse_str(s).ok())
    }

    /// Stamp the delegated user's UUID onto the context (carried in `attributes`).
    /// `None` removes any prior stamp. Read back via [`acting_for`](Self::acting_for).
    #[must_use]
    pub fn with_acting_for(mut self, acting_for: Option<Uuid>) -> Self {
        match acting_for {
            Some(uuid) => {
                self.attributes.insert(
                    Self::ACTING_FOR_ATTRIBUTE.to_string(),
                    serde_json::Value::String(uuid.to_string()),
                );
            },
            None => {
                self.attributes.remove(Self::ACTING_FOR_ATTRIBUTE);
            },
        }
        self
    }

    /// Check if the token has expired.
    ///
    /// # Returns
    ///
    /// `true` if the JWT has expired, `false` otherwise.
    #[must_use]
    pub fn is_expired(&self) -> bool {
        self.expires_at <= Utc::now()
    }

    /// Get time until expiry in seconds.
    ///
    /// # Returns
    ///
    /// Seconds until JWT expiry, negative if already expired.
    #[must_use]
    pub fn ttl_secs(&self) -> i64 {
        (self.expires_at - Utc::now()).num_seconds()
    }

    /// Check if the user is an admin.
    ///
    /// # Returns
    ///
    /// `true` if the user has the "admin" role, `false` otherwise.
    #[must_use]
    pub fn is_admin(&self) -> bool {
        self.has_role("admin")
    }

    /// Check if the context has a tenant ID (multi-tenancy enabled).
    ///
    /// # Returns
    ///
    /// `true` if `tenant_id` is present, `false` otherwise.
    #[must_use]
    pub const fn is_multi_tenant(&self) -> bool {
        self.tenant_id.is_some()
    }

    /// Set or override a role (for testing or runtime role modification).
    #[must_use]
    pub fn with_role(mut self, role: String) -> Self {
        self.roles.push(role);
        self
    }

    /// Set or override scopes (for testing or runtime permission modification).
    #[must_use]
    pub fn with_scopes(mut self, scopes: Vec<String>) -> Self {
        self.scopes = scopes;
        self
    }

    /// Set tenant ID (for multi-tenancy).
    pub fn with_tenant(mut self, tenant_id: impl Into<TenantId>) -> Self {
        self.tenant_id = Some(tenant_id.into());
        self
    }

    /// Set a custom attribute (for testing or runtime attribute addition).
    #[must_use]
    pub fn with_attribute(mut self, key: String, value: serde_json::Value) -> Self {
        self.attributes.insert(key, value);
        self
    }

    /// Check if user can access a field based on role definitions.
    ///
    /// Takes a required scope and checks if any of the user's roles grant that scope.
    ///
    /// # Arguments
    ///
    /// * `security_config` - Security config from compiled schema with role definitions
    /// * `required_scope` - Scope required to access the field (e.g., "read:User.email")
    ///
    /// # Returns
    ///
    /// `true` if user's roles grant the required scope, `false` otherwise.
    ///
    /// # Example
    ///
    /// ```no_run
    /// // Requires: a SecurityConfig from a compiled schema.
    /// // See: tests/integration/ for runnable examples.
    /// # use fraiseql_core::security::SecurityContext;
    /// # use fraiseql_core::schema::SecurityConfig;
    /// # let context: SecurityContext = panic!("example");
    /// # let config: SecurityConfig = panic!("example");
    /// let can_access = context.can_access_scope(&config, "read:User.email");
    /// ```
    #[must_use]
    pub fn can_access_scope(
        &self,
        security_config: &crate::schema::SecurityConfig,
        required_scope: &str,
    ) -> bool {
        // Check if any of user's roles grant this scope
        self.roles
            .iter()
            .any(|role_name| security_config.role_has_scope(role_name, required_scope))
    }
}

impl std::fmt::Display for SecurityContext {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "SecurityContext(user_id={}, roles={:?}, scopes={}, tenant={:?})",
            self.user_id,
            self.roles,
            self.scopes.len(),
            self.tenant_id
        )
    }
}
