//! Row-level access constraint resolution.
//!
//! This module resolves which rows a user can access based on their roles
//! and configured row-level constraints. It follows the same patterns as
//! the permission resolver (caching, hierarchy traversal, tenant isolation).
//!
//! ## Architecture
//!
//! Row constraints define access rules at the table level:
//! - **Ownership**: User can only access rows where `field_name = user_id`
//! - **Tenant**: User can only access rows where `field_name = user_tenant_id`
//! - **Expression**: Custom SQL expressions (future: template evaluation)
//!
//! ## Query Flow
//!
//! 1. Check constraint cache (LRU hit: <0.1ms)
//! 2. Query `row_constraints` table for applicable rules
//! 3. Evaluate constraints based on user context
//! 4. Build WHERE clause fragment (e.g., `{tenant_id: {eq: user_tenant_id}}`)
//! 5. Cache result with TTL
//! 6. Return filter or None (no constraint = no WHERE filter)
//!
//! ## Performance
//!
//! | Scenario | Time |
//! |----------|------|
//! | Cached constraint | <0.1ms |
//! | Uncached (DB query) | <1ms |
//! | No constraint (admin role) | <0.05ms |
//!
//! ## Thread Safety
//!
//! The constraint resolver is thread-safe and can be shared via `Arc<>`:
//! - Uses shared `deadpool_postgres::Pool`
//! - Cache uses `Mutex<LruCache<>>`
//! - All methods are immutable (&self)

use super::{errors::Result, models::Role};
use deadpool_postgres::Pool;
use lru::LruCache;
use std::num::NonZeroUsize;
use std::sync::Mutex;
use std::time::{Duration, Instant};
use uuid::Uuid;

/// Row constraint types
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ConstraintType {
    /// User can only see rows where `field_name` = `user_id`
    Ownership,
    /// User can only see rows where `field_name` = `user_tenant_id`
    Tenant,
    /// Custom SQL expression (future implementation)
    Expression,
}

impl ConstraintType {
    /// Parse constraint type from string
    ///
    /// # Errors
    ///
    /// Returns an error if the string is not a valid constraint type.
    pub fn parse(s: &str) -> Result<Self> {
        match s {
            "ownership" => Ok(Self::Ownership),
            "tenant" => Ok(Self::Tenant),
            "expression" => Ok(Self::Expression),
            other => Err(super::errors::RbacError::ConfigError(format!(
                "Unknown constraint type: {other}"
            ))),
        }
    }
}

/// A row-level access constraint for a table/role pair
#[derive(Debug, Clone)]
pub struct RowConstraint {
    /// Table this constraint applies to
    pub table_name: String,
    /// Role this constraint applies to
    pub role_id: Uuid,
    /// Type of constraint
    pub constraint_type: ConstraintType,
    /// Field name (for ownership/tenant constraints)
    pub field_name: Option<String>,
    /// Expression (for custom expression constraints)
    pub expression: Option<String>,
}

impl RowConstraint {
    /// Build WHERE clause filter from this constraint and user context
    #[must_use]
    pub fn to_filter(&self, user_id: Uuid, user_tenant_id: Option<Uuid>) -> Option<RowFilter> {
        match self.constraint_type {
            ConstraintType::Ownership => {
                // User can only see rows where field_name = user_id
                let field = self.field_name.as_ref()?;
                Some(RowFilter {
                    field: field.clone(),
                    operator: "eq".to_string(),
                    value: user_id.to_string(),
                })
            }
            ConstraintType::Tenant => {
                // User can only see rows where field_name = user_tenant_id
                let field = self.field_name.as_ref()?;
                let tenant_id = user_tenant_id?;
                Some(RowFilter {
                    field: field.clone(),
                    operator: "eq".to_string(),
                    value: tenant_id.to_string(),
                })
            }
            ConstraintType::Expression => {
                // Expression constraints not yet implemented
                // TODO: Parse and evaluate expressions with template substitution
                None
            }
        }
    }
}

/// Row filter for WHERE clause injection
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RowFilter {
    /// Field name (e.g., `"tenant_id"`, `"owner_id"`)
    pub field: String,
    /// Operator (e.g., "eq", "neq", "in")
    pub operator: String,
    /// Value to match (e.g., UUID as string)
    pub value: String,
}

/// Constraint cache entry with TTL
#[derive(Debug)]
struct CacheEntry {
    /// The cached constraint (None = no constraint applies)
    constraint: Option<RowConstraint>,
    /// When this entry expires
    expires_at: Instant,
}

/// Row constraint cache with LRU eviction and TTL expiry
#[derive(Debug)]
struct ConstraintCache {
    /// LRU cache storage
    cache: Mutex<LruCache<String, CacheEntry>>,
    /// TTL duration for cache entries
    ttl: Duration,
}

impl ConstraintCache {
    /// Create new constraint cache with given capacity
    fn new(capacity: usize) -> Self {
        #[allow(clippy::unwrap_used)]
        let capacity = NonZeroUsize::new(capacity).unwrap_or(NonZeroUsize::new(100).unwrap());
        Self {
            cache: Mutex::new(LruCache::new(capacity)),
            ttl: Duration::from_secs(300), // 5 minute TTL
        }
    }

    /// Get constraint from cache if available and not expired
    #[allow(clippy::option_option)]
    fn get(
        &self,
        user_id: Uuid,
        table_name: &str,
        tenant_id: Option<Uuid>,
    ) -> Option<Option<RowConstraint>> {
        let key = Self::cache_key(user_id, table_name, tenant_id);
        let mut cache = self
            .cache
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);

        if let Some(entry) = cache.get(&key) {
            // Check if entry has expired
            if Instant::now() < entry.expires_at {
                return Some(entry.constraint.clone());
            }
            // Entry expired, remove it
            cache.pop(&key);
        }
        None
    }

    /// Store constraint in cache
    fn set(
        &self,
        user_id: Uuid,
        table_name: &str,
        tenant_id: Option<Uuid>,
        constraint: Option<RowConstraint>,
    ) {
        let key = Self::cache_key(user_id, table_name, tenant_id);
        let mut cache = self
            .cache
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);

        cache.put(
            key,
            CacheEntry {
                constraint,
                expires_at: Instant::now() + self.ttl,
            },
        );
    }

    /// Invalidate all entries for a user
    fn invalidate_user(&self, _user_id: Uuid) {
        let mut cache = self
            .cache
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        // Since cache keys include user_id, we can't efficiently invalidate a single user
        // For now, we clear the entire cache on user role changes
        // TODO: Implement reverse index for user_id → cache keys
        cache.clear();
    }

    /// Clear entire cache
    fn clear(&self) {
        let mut cache = self
            .cache
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        cache.clear();
    }

    /// Build cache key
    fn cache_key(user_id: Uuid, table_name: &str, tenant_id: Option<Uuid>) -> String {
        tenant_id.map_or_else(
            || format!("{user_id}:{table_name}"),
            |tid| format!("{user_id}:{table_name}:{tid}"),
        )
    }
}

/// Row constraint resolver for query access filtering
#[derive(Debug)]
pub struct RowConstraintResolver {
    pool: Pool,
    cache: ConstraintCache,
}

impl RowConstraintResolver {
    /// Create new row constraint resolver with database pool and cache capacity
    #[must_use]
    pub fn new(pool: Pool, cache_capacity: usize) -> Self {
        Self {
            pool,
            cache: ConstraintCache::new(cache_capacity),
        }
    }

    /// Get row-level WHERE clause filter for a user on a table
    ///
    /// Returns:
    /// - `Ok(Some(filter))`: Filter should be applied (user has constraint)
    /// - `Ok(None)`: No filter applies (user has unrestricted access or is admin)
    ///
    /// # Errors
    ///
    /// Returns an error if:
    /// - Database query fails
    /// - User has invalid role
    pub async fn get_row_filters(
        &self,
        user_id: Uuid,
        table_name: &str,
        roles: &[Role],
        user_tenant_id: Option<Uuid>,
    ) -> Result<Option<RowFilter>> {
        // Try cache first
        if let Some(cached) = self.cache.get(user_id, table_name, user_tenant_id) {
            return Ok(cached.and_then(|c| c.to_filter(user_id, user_tenant_id)));
        }

        // Cache miss - query database
        let constraint = self
            .query_row_constraints(table_name, roles, user_tenant_id)
            .await?;

        // Cache the result (None is also cached)
        self.cache
            .set(user_id, table_name, user_tenant_id, constraint.clone());

        // Convert constraint to filter
        Ok(constraint.and_then(|c| c.to_filter(user_id, user_tenant_id)))
    }

    /// Query `tb_row_constraint` table for applicable constraints
    async fn query_row_constraints(
        &self,
        table_name: &str,
        roles: &[Role],
        _user_tenant_id: Option<Uuid>,
    ) -> Result<Option<RowConstraint>> {
        if roles.is_empty() {
            return Ok(None);
        }

        let sql = r"
            SELECT
                table_name,
                role_id,
                constraint_type,
                field_name,
                expression
            FROM tb_row_constraint
            WHERE table_name = $1
              AND role_id = ANY($2)
            ORDER BY constraint_type DESC
            LIMIT 1
        ";

        let client = self.pool.get().await?;
        let role_id_strings: Vec<String> = roles.iter().map(|r| r.id.to_string()).collect();

        let rows = client.query(sql, &[&table_name, &role_id_strings]).await?;

        if rows.is_empty() {
            return Ok(None);
        }

        let row = &rows[0];
        let constraint = RowConstraint {
            table_name: row.get(0),
            role_id: Uuid::parse_str(&row.get::<_, String>(1))?,
            constraint_type: ConstraintType::parse(&row.get::<_, String>(2))?,
            field_name: row.get(3),
            expression: row.get(4),
        };

        Ok(Some(constraint))
    }

    /// Invalidate cache for a user (called when roles change)
    pub fn invalidate_user(&self, user_id: Uuid) {
        self.cache.invalidate_user(user_id);
    }

    /// Clear entire cache
    pub fn clear_cache(&self) {
        self.cache.clear();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_constraint_type_parsing() {
        assert_eq!(
            ConstraintType::parse("ownership").unwrap(),
            ConstraintType::Ownership
        );
        assert_eq!(
            ConstraintType::parse("tenant").unwrap(),
            ConstraintType::Tenant
        );
        assert_eq!(
            ConstraintType::parse("expression").unwrap(),
            ConstraintType::Expression
        );
        assert!(ConstraintType::parse("invalid").is_err());
    }

    #[test]
    fn test_ownership_filter() {
        let constraint = RowConstraint {
            table_name: "documents".to_string(),
            role_id: Uuid::nil(),
            constraint_type: ConstraintType::Ownership,
            field_name: Some("owner_id".to_string()),
            expression: None,
        };

        let user_id = Uuid::nil();
        let filter = constraint.to_filter(user_id, None).unwrap();

        assert_eq!(filter.field, "owner_id");
        assert_eq!(filter.operator, "eq");
        assert_eq!(filter.value, user_id.to_string());
    }

    #[test]
    fn test_tenant_filter() {
        let constraint = RowConstraint {
            table_name: "documents".to_string(),
            role_id: Uuid::nil(),
            constraint_type: ConstraintType::Tenant,
            field_name: Some("tenant_id".to_string()),
            expression: None,
        };

        let user_id = Uuid::nil();
        let tenant_id = Uuid::new_v4();
        let filter = constraint.to_filter(user_id, Some(tenant_id)).unwrap();

        assert_eq!(filter.field, "tenant_id");
        assert_eq!(filter.operator, "eq");
        assert_eq!(filter.value, tenant_id.to_string());
    }

    #[test]
    fn test_cache_key_generation() {
        let _cache = ConstraintCache::new(100);
        let user_id = Uuid::nil();
        let table = "documents";
        let tenant_id = Uuid::new_v4();

        let key1 = ConstraintCache::cache_key(user_id, table, Some(tenant_id));
        let _key2 = ConstraintCache::cache_key(user_id, table, Some(tenant_id));
        let key3 = ConstraintCache::cache_key(user_id, table, None);
        assert_ne!(key1, key3);
    }
}
