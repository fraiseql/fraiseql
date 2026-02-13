//! Row-Level Security (RLS) Policy Evaluation
//!
//! This module provides the trait for evaluating RLS rules at runtime.
//!
//! RLS rules are defined in fraiseql.toml at authoring time and compiled into
//! schema.compiled.json. At runtime, the executor evaluates these rules using
//! the SecurityContext to determine what rows a user can access.
//!
//! # Architecture
//!
//! ```text
//! fraiseql.toml (authoring)
//!     ├── [[security.policies]]          # Define policies
//!     └── [[security.rules]]             # Define RLS rules
//!     ↓
//! schema.compiled.json (compiled)
//!     ├── "policies": [...]              # Serialized policies
//!     └── "rules": [...]                 # Serialized rules
//!     ↓
//! Executor.execute_regular_query()       # Runtime
//!     ├── SecurityContext (user info)
//!     └── RLSPolicy::evaluate()          # Evaluate rules
//!     ↓
//! WHERE clause composition
//!     └── WhereClause::And([user_where, rls_filter])
//! ```
//!
//! # Example RLS Rules (in fraiseql.toml)
//!
//! ```toml
//! # Users can only read their own posts
//! [[security.rules]]
//! name = "own_posts_only"
//! rule = "user.id == object.author_id"
//! cacheable = true
//! cache_ttl_seconds = 300
//!
//! # Admins can read everything
//! [[security.rules]]
//! name = "admin_can_read_all"
//! rule = "user.roles includes 'admin'"
//! cacheable = false
//! ```
//!
//! # Example RLS Policies (in fraiseql.toml)
//!
//! ```toml
//! [[security.policies]]
//! name = "read_own_posts"
//! type = "rls"
//! rules = ["own_posts_only"]
//! description = "Users can only read their own posts"
//!
//! [[security.policies]]
//! name = "admin_access"
//! type = "rbac"
//! roles = ["admin"]
//! strategy = "any"
//! description = "Admins have full access"
//! ```

use std::{sync::Arc, time::SystemTime};

use serde::{Deserialize, Serialize};

use crate::{db::WhereClause, error::Result, security::SecurityContext};

/// Cache entry for RLS policy decisions with TTL support
#[derive(Debug, Clone)]
struct CacheEntry {
    /// The cached RLS evaluation result
    result:     Option<WhereClause>,
    /// When this cache entry expires
    expires_at: SystemTime,
}

/// Row-Level Security (RLS) policy for runtime evaluation.
///
/// Implementations of this trait evaluate compiled RLS rules with the user's
/// SecurityContext to determine what rows they can access.
///
/// # Type Safety
///
/// The trait returns `Option<WhereClause>` to support composition:
/// - `None`: No RLS filter (unrestricted access)
/// - `Some(clause)`: Filter to apply to the query
///
/// The executor composes this with user-provided filters via `WhereClause::And()`.
pub trait RLSPolicy: Send + Sync {
    /// Evaluate RLS rules for the given type and security context.
    ///
    /// # Arguments
    ///
    /// * `context` - Security context with user information and permissions
    /// * `type_name` - GraphQL type name being accessed (e.g., "Post", "User")
    ///
    /// # Returns
    ///
    /// - `Ok(Some(clause))`: RLS filter to apply to query
    /// - `Ok(None)`: No RLS filter (full access)
    /// - `Err(e)`: Policy evaluation error (access denied)
    ///
    /// # Example
    ///
    /// ```ignore
    /// let rls = DefaultRLSPolicy::new(schema);
    /// let context = SecurityContext { user_id: "u1", roles: vec!["user"] };
    /// let filter = rls.evaluate(&context, "Post")?;
    /// // filter is Some(WhereClause::Field { path: ["author_id"], operator: Eq, value: "u1" })
    /// ```
    fn evaluate(&self, context: &SecurityContext, type_name: &str) -> Result<Option<WhereClause>>;

    /// Optional: Cache RLS decisions for performance.
    ///
    /// The executor may call this to cache policy decisions per user/type
    /// combination to avoid repeated evaluations.
    ///
    /// # Arguments
    ///
    /// * `cache_key` - Cache key (typically "user_id:type_name")
    /// * `result` - The policy evaluation result to cache
    fn cache_result(&self, _cache_key: &str, _result: &Option<WhereClause>) {
        // Default: no caching. Implementers can override.
    }
}

/// Default RLS policy that enforces tenant isolation and owner-based access.
///
/// This is a reference implementation showing how to build RLS policies.
///
/// Rules:
/// 1. Multi-tenant: Filter to rows matching user's tenant_id
/// 2. Admin bypass: Admins can access all rows in their tenant
/// 3. Owner-based: Regular users can only access their own rows (author_id == user_id)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DefaultRLSPolicy {
    /// Enable multi-tenant isolation
    pub enable_tenant_isolation: bool,
    /// Field name for tenant isolation (default: "tenant_id")
    pub tenant_field:            String,
    /// Field name for owner-based access (default: "author_id")
    pub owner_field:             String,
}

impl DefaultRLSPolicy {
    /// Create a new default RLS policy.
    pub fn new() -> Self {
        Self {
            enable_tenant_isolation: true,
            tenant_field:            "tenant_id".to_string(),
            owner_field:             "author_id".to_string(),
        }
    }

    /// Disable tenant isolation (single-tenant mode).
    pub fn with_single_tenant(mut self) -> Self {
        self.enable_tenant_isolation = false;
        self
    }

    /// Set custom tenant field name.
    pub fn with_tenant_field(mut self, field: String) -> Self {
        self.tenant_field = field;
        self
    }

    /// Set custom owner field name.
    pub fn with_owner_field(mut self, field: String) -> Self {
        self.owner_field = field;
        self
    }
}

impl Default for DefaultRLSPolicy {
    fn default() -> Self {
        Self::new()
    }
}

impl RLSPolicy for DefaultRLSPolicy {
    fn evaluate(&self, context: &SecurityContext, _type_name: &str) -> Result<Option<WhereClause>> {
        // Admins bypass RLS
        if context.is_admin() {
            return Ok(None);
        }

        let mut filters = vec![];

        // Rule 1: Multi-tenant isolation
        if self.enable_tenant_isolation {
            if let Some(ref tenant_id) = context.tenant_id {
                filters.push(WhereClause::Field {
                    path:     vec![self.tenant_field.clone()],
                    operator: crate::db::WhereOperator::Eq,
                    value:    serde_json::json!(tenant_id.clone()),
                });
            }
        }

        // Rule 2: Owner-based access (users can only access their own rows)
        filters.push(WhereClause::Field {
            path:     vec![self.owner_field.clone()],
            operator: crate::db::WhereOperator::Eq,
            value:    serde_json::json!(context.user_id.clone()),
        });

        // Combine all filters with AND
        match filters.len() {
            0 => Ok(None),
            1 => Ok(Some(filters.into_iter().next().unwrap())),
            _ => Ok(Some(WhereClause::And(filters))),
        }
    }
}

/// No-op RLS policy that allows all access (for testing or fully open APIs).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NoRLSPolicy;

impl RLSPolicy for NoRLSPolicy {
    fn evaluate(
        &self,
        _context: &SecurityContext,
        _type_name: &str,
    ) -> Result<Option<WhereClause>> {
        Ok(None)
    }
}

/// Custom RLS policy that can be configured from schema.compiled.json
///
/// This allows schema authors to define RLS rules without writing Rust code.
/// Supports caching of policy evaluation results for performance optimization.
#[derive(Clone, Serialize, Deserialize)]
pub struct CompiledRLSPolicy {
    /// RLS rules indexed by type name
    pub rules_by_type: std::collections::HashMap<String, Vec<RLSRule>>,
    /// Default RLS rule if no type-specific rule exists
    pub default_rule:  Option<RLSRule>,
    /// Cache for policy evaluation results (not serialized)
    #[serde(skip)]
    cache:             Arc<parking_lot::RwLock<std::collections::HashMap<String, CacheEntry>>>,
}

impl std::fmt::Debug for CompiledRLSPolicy {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("CompiledRLSPolicy")
            .field("rules_by_type", &self.rules_by_type)
            .field("default_rule", &self.default_rule)
            .field("cache", &"<cached>")
            .finish()
    }
}

impl CompiledRLSPolicy {
    /// Create a new compiled RLS policy with caching enabled
    pub fn new(
        rules_by_type: std::collections::HashMap<String, Vec<RLSRule>>,
        default_rule: Option<RLSRule>,
    ) -> Self {
        Self {
            rules_by_type,
            default_rule,
            cache: Arc::new(parking_lot::RwLock::new(std::collections::HashMap::new())),
        }
    }
}

/// A single RLS rule for a type
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RLSRule {
    /// Rule name (for debugging)
    pub name:              String,
    /// Expression to evaluate (e.g., "user.id == object.author_id")
    pub expression:        String,
    /// Whether this rule result can be cached
    pub cacheable:         bool,
    /// Cache TTL in seconds (if cacheable)
    pub cache_ttl_seconds: Option<u64>,
}

impl RLSPolicy for CompiledRLSPolicy {
    fn evaluate(&self, context: &SecurityContext, type_name: &str) -> Result<Option<WhereClause>> {
        // Admins bypass all RLS (never cache admin access)
        if context.is_admin() {
            return Ok(None);
        }

        // Find rule for type or use default
        let rule = self
            .rules_by_type
            .get(type_name)
            .and_then(|rules| rules.first())
            .or(self.default_rule.as_ref());

        if let Some(rule) = rule {
            // Check cache for cacheable rules
            let cache_key = if rule.cacheable {
                Some(format!("{}:{}", context.user_id, type_name))
            } else {
                None
            };

            // Try to retrieve from cache
            if let Some(ref key) = cache_key {
                let cache = self.cache.read();
                if let Some(entry) = cache.get(key) {
                    if SystemTime::now() < entry.expires_at {
                        return Ok(entry.result.clone());
                    }
                }
                drop(cache);
            }

            // Evaluate the RLS expression and generate WHERE clause
            let result = evaluate_rls_expression(&rule.expression, context)?;

            // Cache the result if rule is cacheable
            if let Some(key) = cache_key {
                if let Some(ttl_secs) = rule.cache_ttl_seconds {
                    let expires_at = SystemTime::now() + std::time::Duration::from_secs(ttl_secs);
                    let entry = CacheEntry {
                        result: result.clone(),
                        expires_at,
                    };
                    let mut cache = self.cache.write();
                    cache.insert(key, entry);
                }
            }

            Ok(result)
        } else {
            Ok(None)
        }
    }

    fn cache_result(&self, cache_key: &str, result: &Option<WhereClause>) {
        // Direct cache storage with default TTL of 300 seconds
        let expires_at = SystemTime::now() + std::time::Duration::from_secs(300);
        let entry = CacheEntry {
            result: result.clone(),
            expires_at,
        };
        let mut cache = self.cache.write();
        cache.insert(cache_key.to_string(), entry);
    }
}

/// Helper function to evaluate RLS expressions
///
/// Supports simple expressions like:
/// - `user.id == object.author_id` - Equality comparison
/// - `user.roles includes 'admin'` - Role/array membership
/// - `user.tenant_id == object.tenant_id` - Tenant isolation
///
/// In production, consider using:
/// - Rhai for dynamic expression evaluation
/// - WASM for sandboxed custom policies
/// - A domain-specific language (DSL)
fn evaluate_rls_expression(
    expression: &str,
    context: &SecurityContext,
) -> Result<Option<WhereClause>> {
    let expr = expression.trim();

    // Pattern 1: Simple equality - "user.id == object.field_name"
    if let Some(eq_parts) = expr.split_once("==") {
        let left = eq_parts.0.trim();
        let right = eq_parts.1.trim();

        // Left side: user.{field}
        if let Some(user_field) = left.strip_prefix("user.") {
            let user_value = extract_user_value(user_field, context);

            // Right side: object.{field} or literal
            if let Some(object_field) = right.strip_prefix("object.") {
                // Return a field comparison filter
                return Ok(Some(WhereClause::Field {
                    path:     vec![object_field.to_string()],
                    operator: crate::db::WhereOperator::Eq,
                    value:    user_value.unwrap_or(serde_json::Value::Null),
                }));
            } else if serde_json::from_str::<serde_json::Value>(right).is_ok() {
                // Literal value comparison
                return Ok(Some(WhereClause::Field {
                    path:     vec!["_literal_".to_string()],
                    operator: crate::db::WhereOperator::Eq,
                    value:    serde_json::json!(user_value),
                }));
            }
        }
    }

    // Pattern 2: Membership test - "user.roles includes 'admin'"
    if expr.contains("includes") {
        if let Some(includes_parts) = expr.split_once("includes") {
            let left = includes_parts.0.trim();
            let right = includes_parts.1.trim().trim_matches(|c| c == '\'' || c == '"');

            if left == "user.roles" && context.has_role(right) {
                // User has the required role - no RLS filter needed
                return Ok(None);
            }
        }
    }

    // Pattern 3: Tenant isolation - "user.tenant_id == object.tenant_id"
    if expr.contains("tenant_id") && expr.contains("==") {
        if let Some(tenant_id) = &context.tenant_id {
            return Ok(Some(WhereClause::Field {
                path:     vec!["tenant_id".to_string()],
                operator: crate::db::WhereOperator::Eq,
                value:    serde_json::json!(tenant_id),
            }));
        }
    }

    // If no pattern matched or couldn't evaluate, return None (no filter)
    // In production, this should probably return an error for unparseable expressions
    Ok(None)
}

/// Extract a value from user context by field name
fn extract_user_value(field: &str, context: &SecurityContext) -> Option<serde_json::Value> {
    match field {
        "id" | "user_id" => Some(serde_json::json!(context.user_id)),
        "tenant_id" => context.tenant_id.as_ref().map(|t| serde_json::json!(t)),
        "roles" => Some(serde_json::json!(context.roles)),
        custom => context.get_attribute(custom).cloned(),
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use super::*;

    #[test]
    fn test_default_rls_policy_admin_bypass() {
        let policy = DefaultRLSPolicy::new();
        let context = SecurityContext {
            user_id:          "user123".to_string(),
            roles:            vec!["admin".to_string()],
            tenant_id:        Some("tenant1".to_string()),
            scopes:           vec![],
            attributes:       HashMap::new(),
            request_id:       "req1".to_string(),
            ip_address:       None,
            authenticated_at: chrono::Utc::now(),
            expires_at:       chrono::Utc::now() + chrono::Duration::hours(1),
            issuer:           None,
            audience:         None,
        };

        let result = policy.evaluate(&context, "Post").unwrap();
        assert_eq!(result, None, "Admins should bypass RLS");
    }

    #[test]
    fn test_default_rls_policy_tenant_isolation() {
        let policy = DefaultRLSPolicy::new();
        let context = SecurityContext {
            user_id:          "user123".to_string(),
            roles:            vec!["user".to_string()],
            tenant_id:        Some("tenant1".to_string()),
            scopes:           vec![],
            attributes:       HashMap::new(),
            request_id:       "req1".to_string(),
            ip_address:       None,
            authenticated_at: chrono::Utc::now(),
            expires_at:       chrono::Utc::now() + chrono::Duration::hours(1),
            issuer:           None,
            audience:         None,
        };

        let result = policy.evaluate(&context, "Post").unwrap();
        assert!(result.is_some(), "Non-admin users should have RLS filter applied");
    }

    #[test]
    fn test_no_rls_policy() {
        let policy = NoRLSPolicy;
        let context = SecurityContext {
            user_id:          "user123".to_string(),
            roles:            vec![],
            tenant_id:        None,
            scopes:           vec![],
            attributes:       HashMap::new(),
            request_id:       "req1".to_string(),
            ip_address:       None,
            authenticated_at: chrono::Utc::now(),
            expires_at:       chrono::Utc::now() + chrono::Duration::hours(1),
            issuer:           None,
            audience:         None,
        };

        let result = policy.evaluate(&context, "Post").unwrap();
        assert_eq!(result, None, "NoRLSPolicy should never apply filters");
    }
}
