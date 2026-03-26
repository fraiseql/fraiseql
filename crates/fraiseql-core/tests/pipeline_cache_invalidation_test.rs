#![allow(clippy::unwrap_used)] // Reason: test code, panics are acceptable

//! Pipeline 4 integration tests — cache invalidation.
//!
//! Drives the **cache invalidation pipeline** after a successful mutation:
//!
//!   `Executor::execute(mutation)` → success → `adapter.invalidate_views()`
//!   → cache entries for `invalidates_views` are evicted
//!
//! Also tests fact table version bumping:
//!   mutation success → `adapter.bump_fact_table_versions(invalidates_fact_tables)`
//!   → `FactTableVersionProvider` version counter incremented
//!
//! Tests use `CachedDatabaseAdapter` wrapping a plain mock so the actual
//! invalidation / version-bumping logic is exercised through the same code path
//! the production executor uses.

use std::{collections::HashMap, sync::Arc};

use async_trait::async_trait;
use chrono::Utc;
use fraiseql_core::{
    cache::{
        CacheConfig, CachedDatabaseAdapter, FactTableCacheConfig, FactTableVersionStrategy,
        QueryResultCache,
    },
    db::{
        traits::{DatabaseAdapter, MutationCapable},
        types::{DatabaseType, JsonbValue, PoolMetrics},
        where_clause::WhereClause,
    },
    error::Result,
    runtime::Executor,
    schema::{CompiledSchema, SqlProjectionHint},
    security::SecurityContext,
};
use serde_json::json;

// ---------------------------------------------------------------------------
// Inner mock adapter
// ---------------------------------------------------------------------------

struct InnerMockAdapter {
    mutation_row: HashMap<String, serde_json::Value>,
}

impl InnerMockAdapter {
    const fn with_row(mutation_row: HashMap<String, serde_json::Value>) -> Self {
        Self { mutation_row }
    }
}

// Reason: DatabaseAdapter is defined with #[async_trait]; all implementations must match
// its transformed method signatures to satisfy the trait contract
#[async_trait]
impl DatabaseAdapter for InnerMockAdapter {
    async fn execute_with_projection(
        &self,
        _view: &str,
        _projection: Option<&SqlProjectionHint>,
        _where_clause: Option<&WhereClause>,
        _limit: Option<u32>,
        _offset: Option<u32>,
    ) -> Result<Vec<JsonbValue>> {
        Ok(vec![])
    }

    async fn execute_where_query(
        &self,
        _view: &str,
        _where_clause: Option<&WhereClause>,
        _limit: Option<u32>,
        _offset: Option<u32>,
    ) -> Result<Vec<JsonbValue>> {
        Ok(vec![])
    }

    async fn health_check(&self) -> Result<()> {
        Ok(())
    }

    fn database_type(&self) -> DatabaseType {
        DatabaseType::PostgreSQL
    }

    fn pool_metrics(&self) -> PoolMetrics {
        PoolMetrics {
            total_connections:  1,
            active_connections: 0,
            idle_connections:   1,
            waiting_requests:   0,
        }
    }

    async fn execute_raw_query(
        &self,
        _sql: &str,
    ) -> Result<Vec<HashMap<String, serde_json::Value>>> {
        Ok(vec![])
    }

    async fn execute_parameterized_aggregate(
        &self,
        _sql: &str,
        _params: &[serde_json::Value],
    ) -> Result<Vec<HashMap<String, serde_json::Value>>> {
        Ok(vec![])
    }

    async fn execute_function_call(
        &self,
        function_name: &str,
        _args: &[serde_json::Value],
    ) -> Result<Vec<HashMap<String, serde_json::Value>>> {
        // When the executor calls `bump_tf_version` for fact table version bumping,
        // return a row with the new version number.
        if function_name == "bump_tf_version" {
            let mut row = HashMap::new();
            row.insert("version".to_string(), json!(2_i64));
            return Ok(vec![row]);
        }
        Ok(vec![self.mutation_row.clone()])
    }
}

impl MutationCapable for InnerMockAdapter {}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn order_success_row() -> HashMap<String, serde_json::Value> {
    let mut row = HashMap::new();
    row.insert("status".to_string(), json!("success"));
    row.insert("message".to_string(), json!("ok"));
    row.insert(
        "entity".to_string(),
        json!({"id": "order-1", "tenant_id": "t-1", "amount": "99.99", "status": "open"}),
    );
    row.insert("entity_type".to_string(), json!("Order"));
    row.insert("cascade".to_string(), serde_json::Value::Null);
    row.insert("metadata".to_string(), serde_json::Value::Null);
    row
}

fn admin_security_context() -> SecurityContext {
    SecurityContext {
        user_id:          "user-123".to_string(),
        tenant_id:        Some("tenant-456".to_string()),
        roles:            vec!["admin".to_string()],
        scopes:           vec![],
        attributes:       HashMap::new(),
        request_id:       "req-test".to_string(),
        ip_address:       None,
        authenticated_at: Utc::now(),
        expires_at:       Utc::now() + chrono::Duration::hours(1),
        issuer:           None,
        audience:         None,
    }
}

// ---------------------------------------------------------------------------
// Mutation → cache invalidation
// ---------------------------------------------------------------------------

/// Pipeline 4: successful mutation invalidates listed views in the cache.
///
/// After `createOrder` succeeds, the `QueryResultCache` entries for views
/// listed in `invalidates_views` (`v_order_summary`, `v_order_items`) must be
/// evicted. Entries are pre-populated with the view name in their `accessed_views`
/// list so that `QueryResultCache::invalidate_views` can find and remove them.
///
/// The `cached_adapter` reference is retained as `Arc<CachedDatabaseAdapter<…>>`
/// so we can inspect the cache state via `cached_adapter.cache()` after execution.
#[tokio::test]
async fn mutation_invalidates_listed_views_in_cache() {
    let json = include_str!("../../../tests/fixtures/golden/05-security-inject-cache.json");
    let schema = CompiledSchema::from_json(json).expect("golden fixture must parse");

    // Verify fixture has invalidates_views
    let m = schema
        .find_mutation("createOrder")
        .expect("'createOrder' must be in golden fixture 05");
    assert!(
        !m.invalidates_views.is_empty(),
        "pre-condition: createOrder must have invalidates_views in fixture 05"
    );
    let views = m.invalidates_views.clone();

    // Build an enabled cache and pre-populate stale entries for each view.
    let cache = QueryResultCache::new(CacheConfig::enabled());
    for view in &views {
        let key = format!("pipeline4_stale:{view}");
        cache
            .put(
                key.clone(),
                vec![JsonbValue::new(json!({"stale": true}))],
                vec![view.clone()],
                None,
                None,
            )
            .expect("pre-population must succeed");
        assert!(
            cache.get(&key).unwrap().is_some(),
            "pre-condition: stale entry for '{view}' must exist before mutation"
        );
    }

    // Build the adapter with a concrete type so Executor can infer A: Sized.
    // We clone the Arc to retain access to the cache after the executor runs.
    let inner = InnerMockAdapter::with_row(order_success_row());
    let cached_adapter = Arc::new(CachedDatabaseAdapter::new(inner, cache, "test-v1".to_string()));

    // Executor<CachedDatabaseAdapter<InnerMockAdapter>> — concrete, Sized.
    let executor = Executor::new(schema, Arc::clone(&cached_adapter));

    let ctx = admin_security_context();
    let vars = serde_json::json!({"amount": "99.99"});
    executor
        .execute_with_security(r"mutation { createOrder { id } }", Some(&vars), &ctx)
        .await
        .expect("mutation must succeed");

    // After the mutation, all stale view entries must be gone
    for view in &views {
        let key = format!("pipeline4_stale:{view}");
        let entry = cached_adapter.cache().get(&key).expect("cache.get must not error");
        assert!(entry.is_none(), "mutation must have invalidated cache entry for view '{view}'");
    }
}

/// Pipeline 4: failed mutation does NOT invalidate cache entries.
///
/// When the database function returns a failure status (`failed:conflict`),
/// no data was written, so existing cache entries must remain intact.
#[tokio::test]
async fn failed_mutation_does_not_invalidate_cache() {
    let json = include_str!("../../../tests/fixtures/golden/05-security-inject-cache.json");
    let schema = CompiledSchema::from_json(json).expect("golden fixture must parse");

    let m = schema.find_mutation("createOrder").expect("createOrder must exist");
    let views = m.invalidates_views.clone();

    let cache = QueryResultCache::new(CacheConfig::enabled());
    for view in &views {
        let key = format!("pipeline4_valid:{view}");
        cache
            .put(
                key.clone(),
                vec![JsonbValue::new(json!({"valid": true}))],
                vec![view.clone()],
                None,
                None,
            )
            .expect("pre-population must succeed");
    }

    let mut failed_row = HashMap::new();
    failed_row.insert("status".to_string(), json!("failed:conflict"));
    failed_row.insert("message".to_string(), json!("already exists"));
    failed_row.insert("entity".to_string(), serde_json::Value::Null);
    failed_row.insert("entity_type".to_string(), serde_json::Value::Null);
    failed_row.insert("cascade".to_string(), serde_json::Value::Null);
    failed_row.insert(
        "metadata".to_string(),
        json!({"code": "DUPLICATE", "message": "already exists"}),
    );

    let inner = InnerMockAdapter::with_row(failed_row);
    let cached_adapter = Arc::new(CachedDatabaseAdapter::new(inner, cache, "test-v1".to_string()));

    let executor = Executor::new(schema, Arc::clone(&cached_adapter));
    let ctx = admin_security_context();

    // Execute — may return data or errors at the GraphQL level; either is fine
    let vars = serde_json::json!({"amount": "99.99"});
    let _ = executor
        .execute_with_security(r"mutation { createOrder { id } }", Some(&vars), &ctx)
        .await;

    // Regardless of GraphQL-level result, valid cache entries must remain intact
    for view in &views {
        let key = format!("pipeline4_valid:{view}");
        let entry = cached_adapter.cache().get(&key).expect("cache.get must not error");
        assert!(
            entry.is_some(),
            "failed mutation must NOT invalidate cache entry for view '{view}'"
        );
    }
}

// ---------------------------------------------------------------------------
// Fact table version bumping
// ---------------------------------------------------------------------------

/// Pipeline 4: successful mutation bumps fact table version counters.
///
/// When `createOrder` succeeds with `VersionTable` strategy configured for
/// the fact tables in `invalidates_fact_tables`, the executor calls
/// `bump_fact_table_versions`. The inner mock returns `version=2` for
/// `bump_tf_version` calls, so the `FactTableVersionProvider` must record that.
#[tokio::test]
async fn mutation_bumps_fact_table_version_counter() {
    let json = include_str!("../../../tests/fixtures/golden/05-security-inject-cache.json");
    let schema = CompiledSchema::from_json(json).expect("golden fixture must parse");

    // Verify fixture has invalidates_fact_tables
    let m = schema
        .find_mutation("createOrder")
        .expect("'createOrder' must be in golden fixture 05");
    assert!(
        !m.invalidates_fact_tables.is_empty(),
        "pre-condition: createOrder must have invalidates_fact_tables in fixture 05"
    );
    let fact_tables = m.invalidates_fact_tables.clone();

    // Configure VersionTable strategy for each fact table
    let mut ft_config = FactTableCacheConfig::default();
    for table in &fact_tables {
        ft_config.set_strategy(table, FactTableVersionStrategy::VersionTable);
    }

    let inner = InnerMockAdapter::with_row(order_success_row());
    let cached_adapter = Arc::new(CachedDatabaseAdapter::with_fact_table_config(
        inner,
        QueryResultCache::new(CacheConfig::default()),
        "test-v1".to_string(),
        ft_config,
    ));

    // All versions must be None before the mutation
    for table in &fact_tables {
        assert!(
            cached_adapter.version_provider().get_cached_version(table).is_none(),
            "pre-condition: version for '{table}' must be None before mutation"
        );
    }

    let executor = Executor::new(schema, Arc::clone(&cached_adapter));
    let ctx = admin_security_context();
    let vars = serde_json::json!({"amount": "99.99"});

    executor
        .execute_with_security(r"mutation { createOrder { id } }", Some(&vars), &ctx)
        .await
        .expect("mutation must succeed");

    // After the mutation, versions must have been bumped (inner mock returns version=2
    // for bump_tf_version calls).
    for table in &fact_tables {
        let version = cached_adapter.version_provider().get_cached_version(table);
        assert_eq!(
            version,
            Some(2),
            "fact table '{table}' version must be bumped to 2 after successful mutation"
        );
    }
}

/// Pipeline 4: fact table version bump is skipped for `TimeBased` strategy.
///
/// When the strategy for a table is `TimeBased`, `bump_fact_table_versions` is a
/// no-op — no database call is made and the version cache stays empty.
#[tokio::test]
async fn mutation_skips_bump_for_time_based_strategy() {
    let json = include_str!("../../../tests/fixtures/golden/05-security-inject-cache.json");
    let schema = CompiledSchema::from_json(json).expect("golden fixture must parse");

    let m = schema.find_mutation("createOrder").expect("createOrder must exist");
    assert!(
        !m.invalidates_fact_tables.is_empty(),
        "pre-condition: createOrder must have invalidates_fact_tables"
    );

    // Use TimeBased strategy — no bump_tf_version call should be made
    let mut ft_config = FactTableCacheConfig::default();
    for table in &m.invalidates_fact_tables {
        ft_config.set_strategy(table, FactTableVersionStrategy::time_based(300));
    }
    let fact_tables = m.invalidates_fact_tables.clone();

    let inner = InnerMockAdapter::with_row(order_success_row());
    let cached_adapter = Arc::new(CachedDatabaseAdapter::with_fact_table_config(
        inner,
        QueryResultCache::new(CacheConfig::default()),
        "test-v1".to_string(),
        ft_config,
    ));

    let executor = Executor::new(schema, Arc::clone(&cached_adapter));
    let ctx = admin_security_context();

    // Must succeed without error
    let vars = serde_json::json!({"amount": "99.99"});
    executor
        .execute_with_security(r"mutation { createOrder { id } }", Some(&vars), &ctx)
        .await
        .expect("mutation with TimeBased fact-table strategy must succeed");

    // Version counter must remain None — TimeBased strategy does not bump
    for table in &fact_tables {
        assert!(
            cached_adapter.version_provider().get_cached_version(table).is_none(),
            "TimeBased strategy must not bump version for '{table}'"
        );
    }
}
