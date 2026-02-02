//! Security context for runtime authorization
//!
//! This module provides the `SecurityContext` struct that flows through the executor,
//! carrying information about the authenticated user and their permissions.
//!
//! The security context is extracted from:
//! - JWT claims (user_id from 'sub', roles from 'roles', etc.)
//! - HTTP headers (request_id, tenant_id, etc.)
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
//! The SecurityContext is passed to RLSPolicy::evaluate() to determine what
//! rows a user can access. Policies are compiled into schema.compiled.json
//! and evaluated at runtime with the SecurityContext.

use std::collections::HashMap;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::security::AuthenticatedUser;

/// Security context for authorization evaluation.
///
/// Carries information about the authenticated user and their permissions
/// throughout the request lifecycle.
///
/// # Fields
///
/// - `user_id`: Unique identifier for the authenticated user (from JWT 'sub' claim)
/// - `roles`: User's roles (e.g., ["admin", "moderator"], from JWT 'roles' claim)
/// - `tenant_id`: Organization/tenant identifier for multi-tenant systems
/// - `scopes`: OAuth/permission scopes (e.g., ["read:user", "write:post"])
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
    pub user_id: String,

    /// User's roles (e.g., ["admin", "moderator"])
    ///
    /// Extracted from JWT 'roles' claim or derived from other claims.
    /// Used for role-based access control (RBAC) decisions.
    pub roles: Vec<String>,

    /// Tenant/organization ID (for multi-tenancy)
    ///
    /// When present, RLS policies can enforce tenant isolation.
    /// Extracted from JWT 'tenant_id' or X-Tenant-Id header.
    pub tenant_id: Option<String>,

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
}

impl SecurityContext {
    /// Create a security context from an authenticated user and request metadata.
    ///
    /// # Arguments
    ///
    /// * `user` - Authenticated user from JWT validation
    /// * `request_id` - Correlation ID for this request
    ///
    /// # Example
    ///
    /// ```ignore
    /// let context = SecurityContext::from_user(authenticated_user, "req-123")?;
    /// ```
    pub fn from_user(user: AuthenticatedUser, request_id: String) -> Self {
        SecurityContext {
            user_id: user.user_id.clone(),
            roles: vec![], // Will be populated from JWT claims
            tenant_id: None,
            scopes: user.scopes.clone(),
            attributes: HashMap::new(),
            request_id,
            ip_address: None,
            authenticated_at: Utc::now(),
            expires_at: user.expires_at,
            issuer: None,
            audience: None,
        }
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
            } else if s.ends_with("*") {
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
    /// `true` if tenant_id is present, `false` otherwise.
    #[must_use]
    pub fn is_multi_tenant(&self) -> bool {
        self.tenant_id.is_some()
    }

    /// Set or override a role (for testing or runtime role modification).
    pub fn with_role(mut self, role: String) -> Self {
        self.roles.push(role);
        self
    }

    /// Set or override scopes (for testing or runtime permission modification).
    pub fn with_scopes(mut self, scopes: Vec<String>) -> Self {
        self.scopes = scopes;
        self
    }

    /// Set tenant ID (for multi-tenancy).
    pub fn with_tenant(mut self, tenant_id: String) -> Self {
        self.tenant_id = Some(tenant_id);
        self
    }

    /// Set a custom attribute (for testing or runtime attribute addition).
    pub fn with_attribute(mut self, key: String, value: serde_json::Value) -> Self {
        self.attributes.insert(key, value);
        self
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_has_role() {
        let context = SecurityContext {
            user_id:          "user123".to_string(),
            roles:            vec!["admin".to_string(), "moderator".to_string()],
            tenant_id:        None,
            scopes:           vec![],
            attributes:       HashMap::new(),
            request_id:       "req-1".to_string(),
            ip_address:       None,
            authenticated_at: Utc::now(),
            expires_at:       Utc::now() + chrono::Duration::hours(1),
            issuer:           None,
            audience:         None,
        };

        assert!(context.has_role("admin"));
        assert!(context.has_role("moderator"));
        assert!(!context.has_role("superadmin"));
    }

    #[test]
    fn test_has_scope() {
        let context = SecurityContext {
            user_id:          "user123".to_string(),
            roles:            vec![],
            tenant_id:        None,
            scopes:           vec!["read:user".to_string(), "write:post".to_string()],
            attributes:       HashMap::new(),
            request_id:       "req-1".to_string(),
            ip_address:       None,
            authenticated_at: Utc::now(),
            expires_at:       Utc::now() + chrono::Duration::hours(1),
            issuer:           None,
            audience:         None,
        };

        assert!(context.has_scope("read:user"));
        assert!(context.has_scope("write:post"));
        assert!(!context.has_scope("admin:*"));
    }

    #[test]
    fn test_wildcard_scopes() {
        let context = SecurityContext {
            user_id:          "user123".to_string(),
            roles:            vec![],
            tenant_id:        None,
            scopes:           vec!["admin:*".to_string()],
            attributes:       HashMap::new(),
            request_id:       "req-1".to_string(),
            ip_address:       None,
            authenticated_at: Utc::now(),
            expires_at:       Utc::now() + chrono::Duration::hours(1),
            issuer:           None,
            audience:         None,
        };

        assert!(context.has_scope("admin:read"));
        assert!(context.has_scope("admin:write"));
        assert!(!context.has_scope("user:read"));
    }

    #[test]
    fn test_builder_pattern() {
        let now = Utc::now();
        let context = SecurityContext {
            user_id:          "user123".to_string(),
            roles:            vec![],
            tenant_id:        None,
            scopes:           vec![],
            attributes:       HashMap::new(),
            request_id:       "req-1".to_string(),
            ip_address:       None,
            authenticated_at: now,
            expires_at:       now + chrono::Duration::hours(1),
            issuer:           None,
            audience:         None,
        }
        .with_role("admin".to_string())
        .with_scopes(vec!["read:user".to_string()])
        .with_tenant("tenant-1".to_string());

        assert!(context.has_role("admin"));
        assert!(context.has_scope("read:user"));
        assert_eq!(context.tenant_id, Some("tenant-1".to_string()));
    }
}
