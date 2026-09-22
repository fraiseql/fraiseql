//! Row-Level Security (RLS) Policy Evaluation
//!
//! This module provides the [`RLSPolicy`] trait — the runtime seam that *would*
//! compose an RLS filter into a query's WHERE clause from the requesting
//! `SecurityContext`.
//!
//! # Status: not wired to a compiled-schema config (#612 item 4)
//!
//! **There is no compiled-schema declarative-authorization engine yet.**
//! `RuntimeConfig::from_compiled_schema` pins the operation- and field-authorizers
//! to `None`, and no code path constructs a [`CompiledRLSPolicy`] from a compiled
//! schema. The `[[security.rules]]` / `[[security.policies]]` / `[security.field_auth]`
//! TOML sections that a reader might expect to feed this trait are **rejected at
//! compile time** (`TomlSchema::reject_accepted_but_unconsumed_config`) rather than
//! silently accepted — declaring an authorization boundary the runtime does not
//! enforce is a false security claim, so the compiler fails loud and points here.
//!
//! This trait and its evaluators are therefore *infrastructure for the future
//! engine*, exercised by unit tests, not a shipping config surface. Building the
//! compiled-schema declarative-authz engine that populates it is tracked in
//! **[#626](https://github.com/fraiseql/fraiseql/issues/626)**.
//!
//! # Enforce authorization today
//!
//! Enforce row-level access at the **database layer** — PostgreSQL RLS policies
//! keyed on the session variables FraiseQL sets from the request identity
//! (`resolve_session_variables` →
//! `crates/fraiseql-core/src/runtime/executor/support/security.rs`). That path is
//! real and load-bearing; a compiled `[[security.rules]]` block is not.

use std::sync::Arc;

use serde::{Deserialize, Serialize};

use crate::{
    backend::WhereClause,
    error::{FraiseQLError, Result},
    security::SecurityContext,
    utils::clock::{Clock, SystemClock},
};

/// What a read is being performed against, for [`RLSPolicy::evaluate()`].
///
/// # Why this is a struct and not a name
///
/// `evaluate` used to take a single `&str` documented as `type_name`. No runtime
/// caller passed a type name: five passed the query name and two passed a table
/// name (#1359). A policy could not be written to satisfy all three, and a key
/// that matched nothing was not an error — `CompiledRLSPolicy` fell through to
/// its default rule, and with no default rule returned `Ok(None)`, meaning **no
/// filter at all**. A row-level security control that was configured, consulted,
/// and silently not in force.
///
/// Naming each part separately makes the question unambiguous, and changing the
/// signature breaks every implementation at compile time — which for a security
/// trait is the failure mode to prefer, because no deployment is silently rekeyed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RlsTarget<'a> {
    /// The root field being executed. Always known.
    pub query:     &'a str,
    /// The GraphQL type the read returns, when the caller knows it.
    ///
    /// `None` on the aggregate paths, which resolve a fact table rather than a
    /// type.
    pub type_name: Option<&'a str>,
    /// The physical table or view the read resolves to, when the caller knows it.
    ///
    /// `None` on the ordinary query paths, which resolve a SQL source later.
    pub table:     Option<&'a str>,
}

impl<'a> RlsTarget<'a> {
    /// A read of `query` returning `type_name`. The ordinary query paths.
    #[must_use]
    pub const fn query(query: &'a str, type_name: &'a str) -> Self {
        Self {
            query,
            type_name: Some(type_name),
            table: None,
        }
    }

    /// A read of `query` resolved against fact table `table`. The aggregate paths.
    #[must_use]
    pub const fn fact_table(query: &'a str, table: &'a str) -> Self {
        Self {
            query,
            type_name: None,
            table: Some(table),
        }
    }

    /// The keys this target may be looked up under, most specific first.
    ///
    /// Order is deliberate: the type name is the key the trait always documented,
    /// the table is what the aggregate paths keyed on after #795, and the query
    /// name is what every other caller keyed on in practice. Trying all three
    /// means an existing deployment keeps resolving whichever key it already used.
    pub fn keys(&self) -> impl Iterator<Item = &'a str> + '_ {
        self.type_name.into_iter().chain(self.table).chain(std::iter::once(self.query))
    }
}

/// What [`CompiledRLSPolicy`] does when no rule matches a target.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize, Default)]
pub enum UnmatchedTarget {
    /// Refuse the read with an [`Authorization`](fraiseql_error::FraiseQLError::Authorization)
    /// error naming the target and the keys tried.
    ///
    /// The default, because the alternative is a filter that silently does not apply.
    #[default]
    Deny,
    /// Return `Ok(None)` — no filter, full access.
    ///
    /// The pre-#1359 behaviour. Only choose this when unpolicied reads are
    /// genuinely intended to be unrestricted.
    Allow,
}

/// A WHERE clause that has been evaluated by an RLS policy.
///
/// This type is a compile-time guarantee that the WHERE clause was produced
/// by [`RLSPolicy::evaluate()`] rather than arbitrary user code.
///
/// `RlsWhereClause` can only be constructed within `fraiseql-core` via
/// `RlsWhereClause::new()`, ensuring all instances originate from RLS evaluation.
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
/// # use fraiseql_core::security::rls_policy::RlsTarget;
/// # let context: SecurityContext = panic!("example");
/// let rls = DefaultRLSPolicy::new();
/// let rls_clause = rls.evaluate(&context, &RlsTarget::query("posts", "Post")).unwrap();
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
    #[must_use]
    pub const fn as_where_clause(&self) -> &WhereClause {
        &self.inner
    }

    /// Consume this wrapper and return the underlying WHERE clause.
    #[must_use]
    pub fn into_where_clause(self) -> WhereClause {
        self.inner
    }
}

/// Cache entry for RLS policy decisions with TTL support
#[derive(Debug, Clone)]
pub(crate) struct CacheEntry {
    /// The cached RLS evaluation result
    pub(crate) result:     Option<WhereClause>,
    /// When this cache entry expires (Unix seconds)
    pub(crate) expires_at: u64,
}

/// Row-Level Security (RLS) policy for runtime evaluation.
///
/// Implementations of this trait evaluate compiled RLS rules with the user's
/// `SecurityContext` to determine what rows they can access.
///
/// # Type Safety
///
/// The trait returns `Option<WhereClause>` to support composition:
/// - `None`: No RLS filter (unrestricted access)
/// - `Some(clause)`: Filter to apply to the query
///
/// The executor composes this with user-provided filters via `WhereClause::And()`.
pub trait RLSPolicy: Send + Sync {
    /// Evaluate RLS rules for the given target and security context.
    ///
    /// # Arguments
    ///
    /// * `context` - Security context with user information and permissions
    /// * `target` - What is being read: see [`RlsTarget`]. It names the query, and the return type
    ///   or fact table where the caller knows them. It is a struct rather than a single name
    ///   because the callers disagreed about which name they were passing, and a mismatched key
    ///   filtered nothing (#1359).
    ///
    /// # Returns
    ///
    /// - `Ok(Some(clause))`: RLS filter to apply to query (wrapped in [`RlsWhereClause`])
    /// - `Ok(None)`: No RLS filter (full access)
    /// - `Err(e)`: Policy evaluation error (access denied)
    ///
    /// # Implementing this trait
    ///
    /// An implementation that recognises none of the target's names must not
    /// return `Ok(None)`: that is full access, and it is indistinguishable from a
    /// deliberate decision not to filter. Refuse instead, or opt in explicitly —
    /// [`CompiledRLSPolicy`] takes [`UnmatchedTarget`] for exactly this choice.
    ///
    /// # Example
    ///
    /// ```no_run
    /// // Requires: a SecurityContext built from authenticated request metadata.
    /// // See: tests/integration/ for runnable examples.
    /// # use fraiseql_core::security::{RLSPolicy, DefaultRLSPolicy, SecurityContext};
    /// # use fraiseql_core::security::rls_policy::RlsTarget;
    /// # let context: SecurityContext = panic!("example");
    /// let rls = DefaultRLSPolicy::new();
    /// // filter is Some(RlsWhereClause) wrapping the evaluated WhereClause
    /// let filter = rls.evaluate(&context, &RlsTarget::query("posts", "Post")).unwrap();
    /// ```
    ///
    /// # Errors
    ///
    /// Returns `FraiseQLError` if the RLS policy evaluation fails, or if the
    /// implementation refuses an unrecognised target.
    fn evaluate(
        &self,
        context: &SecurityContext,
        target: &RlsTarget<'_>,
    ) -> Result<Option<RlsWhereClause>>;

    /// Optional: Cache RLS decisions for performance.
    ///
    /// The executor may call this to cache policy decisions per user/type
    /// combination to avoid repeated evaluations.
    ///
    /// # Arguments
    ///
    /// * `cache_key` - Cache key (typically "`user_id:type_name`")
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
/// 1. Multi-tenant: Filter to rows matching user's `tenant_id`
/// 2. Admin bypass: Admins can access all rows in their tenant
/// 3. Owner-based: Regular users can only access their own rows (`author_id` == `user_id`)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DefaultRLSPolicy {
    /// Enable multi-tenant isolation
    pub enable_tenant_isolation: bool,
    /// Field name for tenant isolation (default: "`tenant_id`")
    pub tenant_field:            String,
    /// Field name for owner-based access (default: "`author_id`")
    pub owner_field:             String,
}

impl DefaultRLSPolicy {
    /// Create a new default RLS policy.
    #[must_use]
    pub fn new() -> Self {
        Self {
            enable_tenant_isolation: true,
            tenant_field:            "tenant_id".to_string(),
            owner_field:             "author_id".to_string(),
        }
    }

    /// Disable tenant isolation (single-tenant mode).
    #[must_use]
    pub const fn with_single_tenant(mut self) -> Self {
        self.enable_tenant_isolation = false;
        self
    }

    /// Set custom tenant field name.
    #[must_use]
    pub fn with_tenant_field(mut self, field: String) -> Self {
        self.tenant_field = field;
        self
    }

    /// Set custom owner field name.
    #[must_use]
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
        _target: &RlsTarget<'_>,
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
                    operator: crate::backend::WhereOperator::Eq,
                    value:    serde_json::json!(tenant_id.clone()),
                });
            }
        }

        // Rule 2: Owner-based access (users can only access their own rows)
        filters.push(WhereClause::Field {
            path:     vec![self.owner_field.clone()],
            operator: crate::backend::WhereOperator::Eq,
            value:    serde_json::json!(context.user_id.clone()),
        });

        // Combine all filters with AND and wrap in RlsWhereClause
        let clause = match filters.len() {
            0 => return Ok(None),
            // Reason: `filters.len() == 1` guarantees `.next()` yields `Some`
            1 => filters.into_iter().next().expect("len checked == 1"),
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
        _target: &RlsTarget<'_>,
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
    /// What to do when no rule matches and there is no default rule.
    ///
    /// Defaults to [`UnmatchedTarget::Deny`]. Before #1359 this case returned
    /// `Ok(None)` — no filter — so a policy keyed on names the callers never
    /// passed was consulted on every read and silently applied nothing.
    #[serde(default)]
    pub on_unmatched:  UnmatchedTarget,
    /// Cache for policy evaluation results (not serialized)
    #[serde(skip)]
    pub(crate) cache:  Arc<parking_lot::RwLock<std::collections::HashMap<String, CacheEntry>>>,
    /// Clock for cache-expiry checks. Injectable for deterministic testing.
    #[serde(skip, default = "default_system_clock")]
    clock:             Arc<dyn Clock>,
}

impl std::fmt::Debug for CompiledRLSPolicy {
    #[cfg_attr(test, mutants::skip)]
    // Reason: diagnostic-only impl — outputs "<cached>" and "<clock>" placeholder
    // strings that no test asserts on; mutations to these literals cannot be killed.
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("CompiledRLSPolicy")
            .field("rules_by_type", &self.rules_by_type)
            .field("default_rule", &self.default_rule)
            .field("on_unmatched", &self.on_unmatched)
            .field("cache", &"<cached>")
            .field("clock", &"<clock>")
            .finish()
    }
}

impl CompiledRLSPolicy {
    /// Create a new compiled RLS policy with caching enabled.
    ///
    /// Unmatched targets are refused ([`UnmatchedTarget::Deny`]). Use
    /// [`Self::with_unmatched`] to opt into the pre-#1359 behaviour of treating
    /// them as unrestricted.
    #[must_use]
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
            on_unmatched: UnmatchedTarget::Deny,
            cache: Arc::new(parking_lot::RwLock::new(std::collections::HashMap::new())),
            clock,
        }
    }

    /// Choose what happens when no rule matches a target and no default rule is set.
    #[must_use]
    pub const fn with_unmatched(mut self, on_unmatched: UnmatchedTarget) -> Self {
        self.on_unmatched = on_unmatched;
        self
    }
}

/// A single RLS rule for a type
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RLSRule {
    /// Rule name (for debugging)
    pub name:              String,
    /// Expression to evaluate (e.g., "user.id == `object.author_id`")
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
        target: &RlsTarget<'_>,
    ) -> Result<Option<RlsWhereClause>> {
        // Admins bypass all RLS (never cache admin access)
        if context.is_admin() {
            return Ok(None);
        }

        // Try every name the target carries, most specific first, so a policy keyed
        // the way any existing deployment keyed it still resolves. `matched_key` is
        // kept because it is what the cache is keyed on: caching under a different
        // name than the one that selected the rule would serve one type's filter to
        // another.
        let matched = target.keys().find_map(|key| {
            self.rules_by_type.get(key).and_then(|r| r.first()).map(|rule| (key, rule))
        });

        let (matched_key, rule) = match matched {
            Some(found) => found,
            None => match self.default_rule.as_ref() {
                Some(rule) => (target.query, rule),
                // No rule and no default. Returning Ok(None) here is what made a
                // mis-keyed policy invisible (#1359): full access, no error, no log.
                None => {
                    return match self.on_unmatched {
                        UnmatchedTarget::Allow => Ok(None),
                        UnmatchedTarget::Deny => Err(FraiseQLError::Authorization {
                            message:  format!(
                                "no RLS rule matches this read, and the policy has no default \
                                 rule; refusing rather than reading unfiltered (tried {})",
                                target.keys().collect::<Vec<_>>().join(", ")
                            ),
                            action:   Some("read".to_string()),
                            resource: Some(target.query.to_string()),
                        }),
                    };
                },
            },
        };

        {
            let type_name = matched_key;
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
            let result: Option<WhereClause> = evaluate_rls_expression(&rule.expression, context)?;

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
                    operator: crate::backend::WhereOperator::Eq,
                    value:    user_value.unwrap_or(serde_json::Value::Null),
                }));
            } else if serde_json::from_str::<serde_json::Value>(right).is_ok() {
                // Literal value comparison
                return Ok(Some(WhereClause::Field {
                    path:     vec!["_literal_".to_string()],
                    operator: crate::backend::WhereOperator::Eq,
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
                operator: crate::backend::WhereOperator::Eq,
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
pub(crate) fn extract_user_value(
    field: &str,
    context: &SecurityContext,
) -> Option<serde_json::Value> {
    match field {
        "id" | "user_id" => Some(serde_json::json!(context.user_id)),
        "tenant_id" => context.tenant_id.as_ref().map(|t| serde_json::json!(t)),
        "roles" => Some(serde_json::json!(context.roles)),
        custom => context.get_attribute(custom).cloned(),
    }
}
