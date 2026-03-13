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

use std::sync::Arc;

use serde::{Deserialize, Serialize};

use crate::{
    db::WhereClause,
    error::{FraiseQLError, Result},
    security::SecurityContext,
    utils::clock::{Clock, SystemClock},
};

/// A WHERE clause that has been evaluated by an RLS policy.
///
/// This type is a compile-time guarantee that the WHERE clause was produced
/// by [`RLSPolicy::evaluate()`] rather than arbitrary user code.
///
/// `RlsWhereClause` can only be constructed within `fraiseql-core` via
/// [`RlsWhereClause::new()`], ensuring all instances originate from RLS evaluation.
///
/// # Invariant
///
/// Any value of this type was produced by an [`RLSPolicy`] implementation
/// invoked on a [`SecurityContext`], not by arbitrary caller code. This makes
/// it impossible to accidentally bypass RLS when composing cache keys or
/// building filtered queries.
///
/// # Example
///
/// ```no_run
/// // The executor receives an RlsWhereClause after evaluating the policy.
/// // It cannot construct one directly — that would be a compile error.
/// # use fraiseql_core::security::{RLSPolicy, DefaultRLSPolicy, SecurityContext};
/// let rls = DefaultRLSPolicy::new();
/// let rls_clause = rls.evaluate(&context, "Post").unwrap();
/// // rls_clause is Option<RlsWhereClause> — proven to have gone through RLS
/// ```
#[derive(Debug, Clone, PartialEq)]
pub struct RlsWhereClause {
    inner: WhereClause,
}

impl RlsWhereClause {
    /// Construct from an evaluated WHERE clause.
    ///
    /// `pub(crate)` — only RLS policy implementations within `fraiseql-core`
    /// may construct this type. External callers obtain instances through
    /// [`RLSPolicy::evaluate()`].
    pub(crate) const fn new(inner: WhereClause) -> Self {
        Self { inner }
    }

    /// Borrow the underlying WHERE clause.
    pub const fn as_where_clause(&self) -> &WhereClause {
        &self.inner
    }

    /// Consume this wrapper and return the underlying WHERE clause.
    pub fn into_where_clause(self) -> WhereClause {
        self.inner
    }
}

/// Cache entry for RLS policy decisions with TTL support
#[derive(Debug, Clone)]
struct CacheEntry {
    /// The cached RLS evaluation result
    result:     Option<WhereClause>,
    /// When this cache entry expires (Unix seconds)
    expires_at: u64,
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
    /// - `Ok(Some(clause))`: RLS filter to apply to query (wrapped in [`RlsWhereClause`])
    /// - `Ok(None)`: No RLS filter (full access)
    /// - `Err(e)`: Policy evaluation error (access denied)
    ///
    /// # Example
    ///
    /// ```no_run
    /// // Requires: a SecurityContext built from authenticated request metadata.
    /// // See: tests/integration/ for runnable examples.
    /// # use fraiseql_core::security::{RLSPolicy, DefaultRLSPolicy, SecurityContext};
    /// let rls = DefaultRLSPolicy::new();
    /// // filter is Some(RlsWhereClause) wrapping the evaluated WhereClause
    /// let filter = rls.evaluate(&context, "Post").unwrap();
    /// ```
    fn evaluate(
        &self,
        context: &SecurityContext,
        type_name: &str,
    ) -> Result<Option<RlsWhereClause>>;

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
    pub const fn with_single_tenant(mut self) -> Self {
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
    fn evaluate(
        &self,
        context: &SecurityContext,
        _type_name: &str,
    ) -> Result<Option<RlsWhereClause>> {
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

        // Combine all filters with AND and wrap in RlsWhereClause
        let clause = match filters.len() {
            0 => return Ok(None),
            1 => filters.into_iter().next().expect("len matched == 1"),
            _ => WhereClause::And(filters),
        };
        Ok(Some(RlsWhereClause::new(clause)))
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
    ) -> Result<Option<RlsWhereClause>> {
        Ok(None)
    }
}

/// Returns a production `SystemClock` wrapped in `Arc<dyn Clock>`.
/// Used as the serde `default` for [`CompiledRLSPolicy::clock`].
fn default_system_clock() -> Arc<dyn Clock> {
    Arc::new(SystemClock)
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
    /// Clock for cache-expiry checks. Injectable for deterministic testing.
    #[serde(skip, default = "default_system_clock")]
    clock:             Arc<dyn Clock>,
}

impl std::fmt::Debug for CompiledRLSPolicy {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("CompiledRLSPolicy")
            .field("rules_by_type", &self.rules_by_type)
            .field("default_rule", &self.default_rule)
            .field("cache", &"<cached>")
            .field("clock", &"<clock>")
            .finish()
    }
}

impl CompiledRLSPolicy {
    /// Create a new compiled RLS policy with caching enabled.
    pub fn new(
        rules_by_type: std::collections::HashMap<String, Vec<RLSRule>>,
        default_rule: Option<RLSRule>,
    ) -> Self {
        Self::new_with_clock(rules_by_type, default_rule, Arc::new(SystemClock))
    }

    /// Create a compiled RLS policy with a custom clock for deterministic testing.
    pub fn new_with_clock(
        rules_by_type: std::collections::HashMap<String, Vec<RLSRule>>,
        default_rule: Option<RLSRule>,
        clock: Arc<dyn Clock>,
    ) -> Self {
        Self {
            rules_by_type,
            default_rule,
            cache: Arc::new(parking_lot::RwLock::new(std::collections::HashMap::new())),
            clock,
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
    fn evaluate(
        &self,
        context: &SecurityContext,
        type_name: &str,
    ) -> Result<Option<RlsWhereClause>> {
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

            // Try to retrieve from cache (CacheEntry stores raw WhereClause internally)
            if let Some(ref key) = cache_key {
                let cache = self.cache.read();
                if let Some(entry) = cache.get(key) {
                    if self.clock.now_secs() < entry.expires_at {
                        // Re-wrap: the cached clause originated from RLS evaluation
                        return Ok(entry.result.clone().map(RlsWhereClause::new));
                    }
                }
                drop(cache);
            }

            // Evaluate the RLS expression and generate WHERE clause
            let result: Option<WhereClause> =
                evaluate_rls_expression(&rule.expression, context)?;

            // Cache the raw WhereClause for reuse
            if let Some(key) = cache_key {
                if let Some(ttl_secs) = rule.cache_ttl_seconds {
                    let expires_at = self.clock.now_secs() + ttl_secs;
                    let entry = CacheEntry {
                        result: result.clone(),
                        expires_at,
                    };
                    let mut cache = self.cache.write();
                    cache.insert(key, entry);
                }
            }

            Ok(result.map(RlsWhereClause::new))
        } else {
            Ok(None)
        }
    }

    fn cache_result(&self, cache_key: &str, result: &Option<WhereClause>) {
        // Direct cache storage with default TTL of 300 seconds
        let expires_at = self.clock.now_secs() + 300;
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

    // Unrecognised expression: fail closed to prevent silent cross-tenant access.
    Err(FraiseQLError::Validation {
        message: format!("Unrecognised RLS expression: '{expr}'"),
        path:    None,
    })
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
    #![allow(clippy::unwrap_used)] // Reason: test code, panics are acceptable

    use std::collections::HashMap;

    use super::*;

    // ── helpers ──────────────────────────────────────────────────────────────

    fn make_context(user_id: &str, roles: Vec<&str>, tenant_id: Option<&str>) -> SecurityContext {
        SecurityContext {
            user_id:          user_id.to_string(),
            roles:            roles.into_iter().map(String::from).collect(),
            tenant_id:        tenant_id.map(String::from),
            scopes:           vec![],
            attributes:       HashMap::new(),
            request_id:       "req1".to_string(),
            ip_address:       None,
            authenticated_at: chrono::Utc::now(),
            expires_at:       chrono::Utc::now() + chrono::Duration::hours(1),
            issuer:           None,
            audience:         None,
        }
    }

    fn cacheable_owner_rule() -> RLSRule {
        RLSRule {
            name:              "owner_only".to_string(),
            expression:        "user.id == object.author_id".to_string(),
            cacheable:         true,
            cache_ttl_seconds: Some(300),
        }
    }

    fn policy_with_rule(rule: RLSRule) -> CompiledRLSPolicy {
        let mut rules_by_type = std::collections::HashMap::new();
        rules_by_type.insert("Post".to_string(), vec![rule]);
        CompiledRLSPolicy::new(rules_by_type, None)
    }

    fn policy_with_rule_and_clock(
        rule: RLSRule,
        clock: std::sync::Arc<dyn crate::utils::clock::Clock>,
    ) -> CompiledRLSPolicy {
        let mut rules_by_type = std::collections::HashMap::new();
        rules_by_type.insert("Post".to_string(), vec![rule]);
        CompiledRLSPolicy::new_with_clock(rules_by_type, None, clock)
    }

    // ── DefaultRLSPolicy ─────────────────────────────────────────────────────

    #[test]
    fn test_with_tenant_field_sets_field_name() {
        // Kills mutation: with_tenant_field → Default::default() (line 225).
        // Default::default() returns a policy with tenant_field = "tenant_id";
        // after with_tenant_field("org_id") it must be "org_id".
        let policy = DefaultRLSPolicy::new().with_tenant_field("org_id".to_string());
        assert_eq!(policy.tenant_field, "org_id", "with_tenant_field must update tenant_field");

        // Verify the custom field name appears in the generated WHERE clause
        let context = make_context("user1", vec!["viewer"], Some("org42"));
        let result = policy.evaluate(&context, "Post").unwrap().unwrap();
        let sql = format!("{:?}", result.into_where_clause());
        assert!(sql.contains("org_id"), "custom tenant field must appear in WHERE clause: {sql}");
        assert!(!sql.contains("\"tenant_id\""), "default field name must not appear: {sql}");
    }

    #[test]
    fn test_with_owner_field_sets_field_name() {
        // Kills mutation: with_owner_field → Default::default() (line 231).
        // Default::default() returns a policy with owner_field = "author_id".
        let policy = DefaultRLSPolicy::new().with_owner_field("creator_id".to_string());
        assert_eq!(policy.owner_field, "creator_id", "with_owner_field must update owner_field");

        // Verify the custom field appears in the generated WHERE clause
        let context = make_context("user1", vec!["viewer"], None);
        let result = policy.evaluate(&context, "Post").unwrap().unwrap();
        let sql = format!("{:?}", result.into_where_clause());
        assert!(sql.contains("creator_id"), "custom owner field must appear in WHERE clause: {sql}");
        assert!(!sql.contains("author_id"), "default field name must not appear: {sql}");
    }

    #[test]
    fn test_default_rls_policy_admin_bypass() {
        let policy = DefaultRLSPolicy::new();
        let context = make_context("user123", vec!["admin"], Some("tenant1"));
        let result = policy.evaluate(&context, "Post").unwrap();
        assert_eq!(result, None, "Admins should bypass RLS");
    }

    #[test]
    fn test_default_rls_policy_tenant_isolation() {
        let policy = DefaultRLSPolicy::new();
        let context = make_context("user123", vec!["user"], Some("tenant1"));
        let result = policy.evaluate(&context, "Post").unwrap();
        assert!(result.is_some(), "Non-admin users should have RLS filter applied");
    }

    #[test]
    fn test_no_rls_policy() {
        let policy = NoRLSPolicy;
        let context = make_context("user123", vec![], None);
        let result = policy.evaluate(&context, "Post").unwrap();
        assert_eq!(result, None, "NoRLSPolicy should never apply filters");
    }

    // ── Cache TTL correctness (kills lines 399, 414) ─────────────────────────

    #[test]
    fn test_compiled_rls_cache_entry_has_correct_ttl() {
        // Verifies: expires_at = clock.now_secs() + ttl_secs  (line 414)
        // Mutation `+ → -` would give expires_at already in the past; `+ → *` gives huge value.
        use crate::utils::clock::ManualClock;
        let clock = std::sync::Arc::new(ManualClock::new());
        let t0 = clock.now_secs();
        // Move clock into policy (t0 already captured — clock not needed afterwards)
        let policy = policy_with_rule_and_clock(cacheable_owner_rule(), clock);
        let context = make_context("user1", vec!["viewer"], Some("t1"));

        // First evaluation populates cache
        policy.evaluate(&context, "Post").unwrap();

        let cache = policy.cache.read();
        let entry = cache.get("user1:Post").expect("cache should be populated after first evaluate");
        assert_eq!(entry.expires_at, t0 + 300, "expires_at must be now_secs + ttl_secs (300)");
    }

    #[test]
    fn test_compiled_rls_cache_hit_does_not_refresh_expiry() {
        // Verifies: when now < expires_at, cache is read and expiry is NOT updated (line 399: <).
        // Mutation `< → ==` would cause cache miss at any time ≠ expires_at.
        // Mutation `< → >` would cause hit AFTER expiry (backward).
        use crate::utils::clock::ManualClock;
        let clock = std::sync::Arc::new(ManualClock::new());
        let t0 = clock.now_secs();

        let policy = policy_with_rule_and_clock(cacheable_owner_rule(), clock.clone());
        let context = make_context("user1", vec!["viewer"], Some("t1"));

        // Populate cache at T
        policy.evaluate(&context, "Post").unwrap();
        let first_expires_at = policy.cache.read().get("user1:Post").unwrap().expires_at;
        assert_eq!(first_expires_at, t0 + 300);

        // Advance to 1 second before expiry — still within TTL
        clock.advance(std::time::Duration::from_secs(299));

        // Should hit cache, NOT re-calculate expiry
        policy.evaluate(&context, "Post").unwrap();
        let second_expires_at = policy.cache.read().get("user1:Post").unwrap().expires_at;
        assert_eq!(
            second_expires_at, first_expires_at,
            "Cache hit must not update expires_at (would indicate a miss + re-cache)"
        );
    }

    #[test]
    fn test_compiled_rls_cache_miss_after_expiry_refreshes_entry() {
        // Verifies: when now >= expires_at, cache is NOT used and entry is refreshed (line 399: <).
        // Mutation `< → >` would use the stale entry (no refresh), so expires_at stays at T+300.
        use crate::utils::clock::ManualClock;
        let clock = std::sync::Arc::new(ManualClock::new());
        let t0 = clock.now_secs();

        let policy = policy_with_rule_and_clock(cacheable_owner_rule(), clock.clone());
        let context = make_context("user1", vec!["viewer"], Some("t1"));

        // Populate cache at T → expires_at = T+300
        policy.evaluate(&context, "Post").unwrap();

        // Advance 301 seconds — clearly past expiry
        clock.advance(std::time::Duration::from_secs(301));

        // Cache miss: re-evaluates and re-caches with new expiry = (T+301)+300 = T+601
        policy.evaluate(&context, "Post").unwrap();
        let new_expires = policy.cache.read().get("user1:Post").unwrap().expires_at;
        assert_eq!(
            new_expires,
            t0 + 601,
            "After TTL expiry, cache must be refreshed with updated expires_at"
        );
    }

    #[test]
    fn test_compiled_rls_cache_expires_exactly_at_ttl_boundary() {
        // Verifies: at exactly expires_at (now == expires_at), cache is expired (line 399: <).
        // Mutation `< → <=` would treat the exact boundary as still valid (off-by-one).
        use crate::utils::clock::ManualClock;
        let clock = std::sync::Arc::new(ManualClock::new());
        let t0 = clock.now_secs();

        let policy = policy_with_rule_and_clock(cacheable_owner_rule(), clock.clone());
        let context = make_context("user1", vec!["viewer"], Some("t1"));

        // Populate cache at T → expires_at = T+300
        policy.evaluate(&context, "Post").unwrap();

        // Advance to EXACTLY the expiry second
        clock.advance(std::time::Duration::from_secs(300));
        assert_eq!(clock.now_secs(), t0 + 300);

        // At exactly expires_at, the entry must be considered expired (now < expires_at is false)
        policy.evaluate(&context, "Post").unwrap();
        let refreshed_expires = policy.cache.read().get("user1:Post").unwrap().expires_at;
        assert_eq!(
            refreshed_expires,
            t0 + 600,
            "At exact TTL boundary, cache must expire and refresh (< not <=)"
        );
    }

    // ── cache_result (kills line 432) ─────────────────────────────────────────

    #[test]
    fn test_cache_result_stores_with_300s_default_ttl() {
        // Verifies: cache_result uses 300s TTL (line 432: +).
        // Mutation `+ → -` gives expires_at already past; `+ → *` gives huge value.
        // Mutation "delete entire method" would leave cache empty.
        use crate::utils::clock::ManualClock;
        let clock = std::sync::Arc::new(ManualClock::new());
        let t0 = clock.now_secs();

        let policy = CompiledRLSPolicy::new_with_clock(
            std::collections::HashMap::new(),
            None,
            clock,
        );

        let result = Some(WhereClause::Field {
            path:     vec!["author_id".to_string()],
            operator: crate::db::WhereOperator::Eq,
            value:    serde_json::json!("user_x"),
        });

        policy.cache_result("user_x:Post", &result);

        let cache = policy.cache.read();
        let entry = cache.get("user_x:Post").expect("cache_result must insert entry");
        assert_eq!(entry.expires_at, t0 + 300, "cache_result must use 300s TTL");
        assert_eq!(entry.result, result, "cache_result must store the provided result");
    }

    #[test]
    fn test_cache_result_stores_none_result() {
        // Verifies cache_result stores None (bypass) results correctly.
        use crate::utils::clock::ManualClock;
        let policy = CompiledRLSPolicy::new_with_clock(
            std::collections::HashMap::new(),
            None,
            std::sync::Arc::new(ManualClock::new()),
        );

        policy.cache_result("user1:Post", &None);

        let cache = policy.cache.read();
        let entry = cache.get("user1:Post").expect("cache_result must store even None result");
        assert!(entry.result.is_none(), "cached None result must remain None");
    }

    // ── extract_user_value (kills line 518) ──────────────────────────────────

    #[test]
    fn test_extract_user_value_id_field() {
        // Mutation `→ None` gives None; `→ Some(Default)` gives Some(Null). Both fail.
        let ctx = make_context("user_abc_123", vec![], None);
        assert_eq!(
            extract_user_value("id", &ctx),
            Some(serde_json::json!("user_abc_123")),
            "'id' must return the actual user_id"
        );
    }

    #[test]
    fn test_extract_user_value_user_id_alias() {
        let ctx = make_context("user_abc_123", vec![], None);
        assert_eq!(
            extract_user_value("user_id", &ctx),
            Some(serde_json::json!("user_abc_123")),
            "'user_id' must return the same user_id as 'id'"
        );
    }

    #[test]
    fn test_extract_user_value_tenant_id_present() {
        let ctx = make_context("u1", vec![], Some("tenant_xyz"));
        assert_eq!(
            extract_user_value("tenant_id", &ctx),
            Some(serde_json::json!("tenant_xyz")),
            "'tenant_id' must return the tenant id when present"
        );
    }

    #[test]
    fn test_extract_user_value_tenant_id_absent() {
        let ctx = make_context("u1", vec![], None);
        assert_eq!(
            extract_user_value("tenant_id", &ctx),
            None,
            "'tenant_id' must return None when absent, not Some(null)"
        );
    }

    #[test]
    fn test_extract_user_value_roles_field() {
        let ctx = make_context("u1", vec!["editor", "viewer"], None);
        assert_eq!(
            extract_user_value("roles", &ctx),
            Some(serde_json::json!(["editor", "viewer"])),
            "'roles' must return the full roles array"
        );
    }

    #[test]
    fn test_extract_user_value_custom_attribute() {
        let mut attrs = HashMap::new();
        attrs.insert("department".to_string(), serde_json::json!("engineering"));
        let ctx = SecurityContext {
            user_id:          "u1".to_string(),
            roles:            vec![],
            tenant_id:        None,
            scopes:           vec![],
            attributes:       attrs,
            request_id:       "r1".to_string(),
            ip_address:       None,
            authenticated_at: chrono::Utc::now(),
            expires_at:       chrono::Utc::now() + chrono::Duration::hours(1),
            issuer:           None,
            audience:         None,
        };
        assert_eq!(
            extract_user_value("department", &ctx),
            Some(serde_json::json!("engineering")),
            "Custom attribute must be returned by name"
        );
    }

    #[test]
    fn test_extract_user_value_unknown_returns_none() {
        let ctx = make_context("u1", vec![], None);
        assert_eq!(
            extract_user_value("nonexistent_field", &ctx),
            None,
            "Unknown field must return None, not Some(null)"
        );
    }

    // ── extract_user_value integration: user_id flows to WHERE clause ─────────

    #[test]
    fn test_user_id_propagated_to_rls_where_clause() {
        // Ensures extract_user_value("id") result reaches the generated WhereClause.
        // Kills mutations → None and → Some(Default) on line 518.
        let policy = policy_with_rule(RLSRule {
            name:              "owner_only".to_string(),
            expression:        "user.id == object.author_id".to_string(),
            cacheable:         false,
            cache_ttl_seconds: None,
        });

        let context = make_context("specific_user_42", vec!["viewer"], None);
        let result = policy.evaluate(&context, "Post").unwrap();

        let clause = result
            .expect("non-admin user must receive an RLS filter")
            .into_where_clause();
        match clause {
            WhereClause::Field { value, .. } => {
                assert_eq!(
                    value,
                    serde_json::json!("specific_user_42"),
                    "RLS WhereClause must embed the actual user_id, not null"
                );
            }
            other => panic!("Expected Field clause, got {other:?}"),
        }
    }
}
