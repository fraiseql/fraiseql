//! `TenantExecutorRegistry` — per-tenant executor dispatch with lock-free reads.
//!
//! Maps tenant keys to individual `Executor<A>` instances, each holding its own
//! compiled schema and database adapter. Reads are lock-free via `ArcSwap`;
//! writes are serialized per-key via `DashMap`.

use std::sync::{
    Arc,
    atomic::{AtomicU8, Ordering},
};

use arc_swap::ArcSwap;
use dashmap::DashMap;
use fraiseql_core::{db::traits::DatabaseAdapter, runtime::Executor};
use fraiseql_error::FraiseQLError;

/// Tenant lifecycle status.
///
/// Stored as an `AtomicU8` in the registry for lock-free reads.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum TenantStatus {
    /// Tenant is operational — requests are served normally.
    Active = 0,
    /// Tenant is suspended — data requests return 503 with `Retry-After: 60`.
    Suspended = 1,
}

impl TenantStatus {
    const fn from_u8(v: u8) -> Self {
        match v {
            1 => Self::Suspended,
            _ => Self::Active,
        }
    }

    /// Returns the string label for this status.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Active => "active",
            Self::Suspended => "suspended",
        }
    }
}

/// A single tenant entry in the registry: executor + lifecycle status.
struct TenantEntry<A: DatabaseAdapter> {
    executor: Arc<ArcSwap<Executor<A>>>,
    status:   AtomicU8,
}

impl<A: DatabaseAdapter> TenantEntry<A> {
    fn new(executor: Arc<Executor<A>>) -> Self {
        Self {
            executor: Arc::new(ArcSwap::from(executor)),
            status:   AtomicU8::new(TenantStatus::Active as u8),
        }
    }

    fn status(&self) -> TenantStatus {
        TenantStatus::from_u8(self.status.load(Ordering::Relaxed))
    }

    fn set_status(&self, status: TenantStatus) {
        self.status.store(status as u8, Ordering::Relaxed);
    }
}

/// Default retry hint (seconds) when a suspended tenant is accessed.
const SUSPENDED_RETRY_AFTER_SECS: u64 = 60;

/// Registry mapping tenant keys to executors.
///
/// Each tenant gets its own `TenantEntry` holding an `ArcSwap<Executor<A>>` and
/// an `AtomicU8` status flag. Reads (`executor_for`) are wait-free; writes
/// (`upsert`, `remove`, `suspend`, `resume`) are serialized per-key by `DashMap`.
///
/// # Security invariant
///
/// When a tenant key is explicitly provided but not found in the registry,
/// `executor_for` returns `Err(FraiseQLError::Authorization)` — it does **not**
/// fall back to the default executor. Silent fallback on an explicit key would
/// serve the wrong tenant's data.
pub struct TenantExecutorRegistry<A: DatabaseAdapter> {
    /// Default executor used when no tenant key is provided (single-tenant compat).
    default: Arc<ArcSwap<Executor<A>>>,
    /// Per-tenant entries keyed by tenant identifier.
    tenants: DashMap<String, TenantEntry<A>>,
}

impl<A: DatabaseAdapter> TenantExecutorRegistry<A> {
    /// Create a new registry with the given default executor.
    #[must_use]
    pub fn new(default: Arc<ArcSwap<Executor<A>>>) -> Self {
        Self {
            default,
            tenants: DashMap::new(),
        }
    }

    /// Returns the executor for the given tenant key.
    ///
    /// - `None` → default executor (single-tenant compatibility)
    /// - `Some(key)` found + `Active` → tenant executor
    /// - `Some(key)` found + `Suspended` → `Err(ServiceUnavailable)`
    /// - `Some(key)` not found → `Err(Authorization)`
    ///
    /// # Errors
    ///
    /// Returns `FraiseQLError::Authorization` if the tenant key is explicit but
    /// not registered in the registry.
    /// Returns `FraiseQLError::ServiceUnavailable` if the tenant is suspended.
    pub fn executor_for(
        &self,
        tenant_key: Option<&str>,
    ) -> fraiseql_error::Result<arc_swap::Guard<Arc<Executor<A>>>> {
        match tenant_key {
            None => Ok(self.default.load()),
            Some(key) => {
                let entry = self.tenants.get(key).ok_or_else(|| {
                    FraiseQLError::unauthorized(format!("Tenant '{key}' is not registered"))
                })?;
                self.require_active(key, entry.value())?;
                Ok(entry.value().executor.load())
            },
        }
    }

    /// Returns `Ok(())` if the tenant is active, `Err(ServiceUnavailable)` if suspended.
    ///
    /// # Errors
    ///
    /// Returns `FraiseQLError::ServiceUnavailable` with a 60-second retry hint
    /// if the tenant status is `Suspended`.
    fn require_active(&self, key: &str, entry: &TenantEntry<A>) -> fraiseql_error::Result<()> {
        if entry.status() == TenantStatus::Suspended {
            return Err(FraiseQLError::ServiceUnavailable {
                message:     format!("Tenant '{key}' is suspended"),
                retry_after: Some(SUSPENDED_RETRY_AFTER_SECS),
            });
        }
        Ok(())
    }

    /// Returns the executor for a tenant regardless of its status.
    ///
    /// Used by admin endpoints that need to inspect tenant metadata even when
    /// the tenant is suspended. Does **not** check the status flag.
    ///
    /// # Errors
    ///
    /// Returns `FraiseQLError::Authorization` if the tenant key is not registered.
    pub fn executor_for_admin(
        &self,
        key: &str,
    ) -> fraiseql_error::Result<arc_swap::Guard<Arc<Executor<A>>>> {
        let entry = self.tenants.get(key).ok_or_else(|| {
            FraiseQLError::unauthorized(format!("Tenant '{key}' is not registered"))
        })?;
        Ok(entry.value().executor.load())
    }

    /// Register or update a tenant executor.
    ///
    /// Returns `true` if this was an insert (new tenant), `false` if it was an
    /// update (existing tenant). On update, the old executor is atomically swapped
    /// via `ArcSwap::store` — in-flight requests holding a guard to the previous
    /// executor continue undisturbed. Status is preserved on update.
    pub fn upsert(&self, key: impl Into<String>, executor: Arc<Executor<A>>) -> bool {
        let key = key.into();
        if let Some(existing) = self.tenants.get(&key) {
            existing.value().executor.store(executor);
            false
        } else {
            self.tenants.insert(key, TenantEntry::new(executor));
            true
        }
    }

    /// Remove a tenant from the registry.
    ///
    /// In-flight requests that already hold a guard to this tenant's executor
    /// continue using it until the guard is dropped (Arc semantics).
    ///
    /// # Errors
    ///
    /// Returns `FraiseQLError::NotFound` if the key is not registered.
    pub fn remove(&self, key: &str) -> fraiseql_error::Result<()> {
        self.tenants
            .remove(key)
            .map(|_| ())
            .ok_or_else(|| FraiseQLError::not_found("tenant", key))
    }

    /// Suspend a tenant — data requests will return 503 until resumed.
    ///
    /// No executor teardown occurs; the tenant's database connections remain open.
    ///
    /// # Errors
    ///
    /// Returns `FraiseQLError::NotFound` if the tenant key is not registered.
    pub fn suspend(&self, key: &str) -> fraiseql_error::Result<()> {
        let entry = self.tenants.get(key).ok_or_else(|| FraiseQLError::not_found("tenant", key))?;
        entry.value().set_status(TenantStatus::Suspended);
        Ok(())
    }

    /// Resume a suspended tenant — data requests are served normally again.
    ///
    /// # Errors
    ///
    /// Returns `FraiseQLError::NotFound` if the tenant key is not registered.
    pub fn resume(&self, key: &str) -> fraiseql_error::Result<()> {
        let entry = self.tenants.get(key).ok_or_else(|| FraiseQLError::not_found("tenant", key))?;
        entry.value().set_status(TenantStatus::Active);
        Ok(())
    }

    /// Returns the status of a tenant.
    ///
    /// # Errors
    ///
    /// Returns `FraiseQLError::NotFound` if the tenant key is not registered.
    pub fn tenant_status(&self, key: &str) -> fraiseql_error::Result<TenantStatus> {
        let entry = self.tenants.get(key).ok_or_else(|| FraiseQLError::not_found("tenant", key))?;
        Ok(entry.value().status())
    }

    /// List all registered tenant keys.
    #[must_use]
    pub fn tenant_keys(&self) -> Vec<String> {
        self.tenants.iter().map(|e| e.key().clone()).collect()
    }

    /// Number of registered tenants (excludes default).
    #[must_use]
    pub fn len(&self) -> usize {
        self.tenants.len()
    }

    /// Whether the registry has no tenants (excludes default).
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.tenants.is_empty()
    }

    /// Get a reference to the default executor.
    #[must_use]
    pub fn default_executor(&self) -> arc_swap::Guard<Arc<Executor<A>>> {
        self.default.load()
    }

    /// Run a health check against a specific tenant's database adapter.
    ///
    /// # Errors
    ///
    /// Returns `FraiseQLError::NotFound` if the tenant key is not registered.
    /// Returns `FraiseQLError::Database` if the health check fails.
    pub async fn health_check(&self, key: &str) -> fraiseql_error::Result<()> {
        let entry = self.tenants.get(key).ok_or_else(|| FraiseQLError::not_found("tenant", key))?;
        let executor = entry.value().executor.load();
        executor.adapter().health_check().await
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)] // Reason: test code, panics acceptable
    #![allow(clippy::missing_panics_doc)] // Reason: test helpers
    #![allow(clippy::missing_errors_doc)] // Reason: test helpers
    #![allow(missing_docs)] // Reason: test code

    use std::sync::Arc;

    use arc_swap::ArcSwap;
    use async_trait::async_trait;
    use fraiseql_core::{
        db::{
            WhereClause,
            traits::DatabaseAdapter,
            types::{DatabaseType, JsonbValue, PoolMetrics},
        },
        error::Result as FraiseQLResult,
        runtime::Executor,
        schema::CompiledSchema,
    };

    use super::*;

    /// Minimal no-op database adapter for unit tests.
    #[derive(Debug, Clone)]
    struct StubAdapter {
        /// Label to distinguish adapters in tests.
        _label: &'static str,
    }

    impl StubAdapter {
        fn new(label: &'static str) -> Self {
            Self { _label: label }
        }
    }

    // Reason: async_trait required by DatabaseAdapter trait definition
    #[async_trait]
    impl DatabaseAdapter for StubAdapter {
        async fn execute_where_query(
            &self,
            _view: &str,
            _where_clause: Option<&WhereClause>,
            _limit: Option<u32>,
            _offset: Option<u32>,
            _order_by: Option<&[fraiseql_core::db::types::OrderByClause]>,
        ) -> FraiseQLResult<Vec<JsonbValue>> {
            Ok(vec![])
        }

        async fn execute_with_projection(
            &self,
            _view: &str,
            _projection: Option<&fraiseql_core::schema::SqlProjectionHint>,
            _where_clause: Option<&WhereClause>,
            _limit: Option<u32>,
            _offset: Option<u32>,
            _order_by: Option<&[fraiseql_core::db::types::OrderByClause]>,
        ) -> FraiseQLResult<Vec<JsonbValue>> {
            Ok(vec![])
        }

        fn database_type(&self) -> DatabaseType {
            DatabaseType::SQLite
        }

        async fn health_check(&self) -> FraiseQLResult<()> {
            Ok(())
        }

        fn pool_metrics(&self) -> PoolMetrics {
            PoolMetrics::default()
        }

        async fn execute_raw_query(
            &self,
            _sql: &str,
        ) -> FraiseQLResult<Vec<std::collections::HashMap<String, serde_json::Value>>> {
            Ok(vec![])
        }

        async fn execute_parameterized_aggregate(
            &self,
            _sql: &str,
            _params: &[serde_json::Value],
        ) -> FraiseQLResult<Vec<std::collections::HashMap<String, serde_json::Value>>> {
            Ok(vec![])
        }
    }

    fn default_executor() -> Arc<ArcSwap<Executor<StubAdapter>>> {
        let schema = CompiledSchema::default();
        let executor = Arc::new(Executor::new(schema, Arc::new(StubAdapter::new("default"))));
        Arc::new(ArcSwap::from(executor))
    }

    fn tenant_executor(label: &'static str) -> Arc<Executor<StubAdapter>> {
        let mut schema = CompiledSchema::default();
        schema
            .queries
            .push(fraiseql_core::schema::QueryDefinition::new("users", "User"));
        Arc::new(Executor::new(schema, Arc::new(StubAdapter::new(label))))
    }

    // ── Cycle 1: basic registry ──────────────────────────────────────────

    #[test]
    fn test_registry_returns_default_when_no_tenant() {
        let registry = TenantExecutorRegistry::new(default_executor());
        let exec = registry.executor_for(None);
        assert!(exec.is_ok());
        // Default schema has no queries
        assert_eq!(exec.unwrap().schema().queries.len(), 0);
    }

    #[test]
    fn test_registry_returns_tenant_executor() {
        let registry = TenantExecutorRegistry::new(default_executor());
        registry.upsert("tenant-abc", tenant_executor("abc"));
        let exec = registry.executor_for(Some("tenant-abc"));
        assert!(exec.is_ok());
        // Tenant executor has 1 query (added in tenant_executor())
        assert_eq!(exec.unwrap().schema().queries.len(), 1);
    }

    #[test]
    fn test_registry_falls_back_to_default_for_no_key() {
        let registry = TenantExecutorRegistry::new(default_executor());
        registry.upsert("tenant-abc", tenant_executor("abc"));
        let exec = registry.executor_for(None);
        assert!(exec.is_ok());
        // Must return default (0 queries), not tenant (1 query)
        assert_eq!(exec.unwrap().schema().queries.len(), 0);
    }

    #[test]
    fn test_registry_rejects_explicit_but_unregistered_key() {
        let registry = TenantExecutorRegistry::new(default_executor());
        let Err(err) = registry.executor_for(Some("unknown")) else {
            panic!("expected Err for unregistered key");
        };
        assert!(
            matches!(err, FraiseQLError::Authorization { .. }),
            "Expected Authorization error, got: {err:?}"
        );
    }

    #[test]
    fn test_registry_upsert_returns_true_on_insert() {
        let registry = TenantExecutorRegistry::new(default_executor());
        let was_insert = registry.upsert("tenant-abc", tenant_executor("abc"));
        assert!(was_insert);
    }

    #[test]
    fn test_registry_upsert_returns_false_on_update() {
        let registry = TenantExecutorRegistry::new(default_executor());
        registry.upsert("tenant-abc", tenant_executor("abc"));
        let was_insert = registry.upsert("tenant-abc", tenant_executor("abc-v2"));
        assert!(!was_insert);
    }

    #[test]
    fn test_registry_remove_existing() {
        let registry = TenantExecutorRegistry::new(default_executor());
        registry.upsert("tenant-abc", tenant_executor("abc"));
        assert_eq!(registry.len(), 1);
        assert!(registry.remove("tenant-abc").is_ok());
        assert_eq!(registry.len(), 0);
    }

    #[test]
    fn test_registry_remove_unknown_returns_error() {
        let registry = TenantExecutorRegistry::new(default_executor());
        let Err(err) = registry.remove("unknown") else {
            panic!("expected Err for unknown key");
        };
        assert!(
            matches!(err, FraiseQLError::NotFound { .. }),
            "Expected NotFound error, got: {err:?}"
        );
    }

    #[test]
    fn test_registry_tenant_keys() {
        let registry = TenantExecutorRegistry::new(default_executor());
        registry.upsert("tenant-abc", tenant_executor("abc"));
        registry.upsert("tenant-xyz", tenant_executor("xyz"));
        let mut keys = registry.tenant_keys();
        keys.sort();
        assert_eq!(keys, vec!["tenant-abc", "tenant-xyz"]);
    }

    #[test]
    fn test_registry_len_and_is_empty() {
        let registry = TenantExecutorRegistry::new(default_executor());
        assert!(registry.is_empty());
        assert_eq!(registry.len(), 0);
        registry.upsert("tenant-abc", tenant_executor("abc"));
        assert!(!registry.is_empty());
        assert_eq!(registry.len(), 1);
    }

    // ── Cycle 3: hot-reload via upsert ───────────────────────────────────

    #[test]
    fn test_registry_hot_reload_tenant() {
        let registry = TenantExecutorRegistry::new(default_executor());

        // Register tenant-abc with executor v1 (1 query)
        registry.upsert("tenant-abc", tenant_executor("abc-v1"));

        // Grab a guard simulating an in-flight request
        let guard_v1 = registry.executor_for(Some("tenant-abc")).unwrap();
        assert_eq!(guard_v1.schema().queries.len(), 1);

        // Hot-reload: upsert with executor v2 (2 queries)
        let mut schema_v2 = CompiledSchema::default();
        schema_v2
            .queries
            .push(fraiseql_core::schema::QueryDefinition::new("users", "User"));
        schema_v2
            .queries
            .push(fraiseql_core::schema::QueryDefinition::new("posts", "Post"));
        let executor_v2 = Arc::new(Executor::new(schema_v2, Arc::new(StubAdapter::new("abc-v2"))));
        registry.upsert("tenant-abc", executor_v2);

        // In-flight request on v1 still sees 1 query (guard holds Arc)
        assert_eq!(guard_v1.schema().queries.len(), 1);

        // New requests see v2 (2 queries)
        let guard_v2 = registry.executor_for(Some("tenant-abc")).unwrap();
        assert_eq!(guard_v2.schema().queries.len(), 2);
    }

    #[test]
    fn test_remove_tenant_in_flight_guard_survives() {
        let registry = TenantExecutorRegistry::new(default_executor());
        registry.upsert("tenant-abc", tenant_executor("abc"));

        // Grab a guard (simulating in-flight request)
        let guard = registry.executor_for(Some("tenant-abc")).unwrap();

        // Remove tenant
        let removed = registry.remove("tenant-abc");
        assert!(removed.is_ok());

        // Guard still works — Arc keeps executor alive
        assert_eq!(guard.schema().queries.len(), 1);

        // New requests to this tenant now fail
        let result = registry.executor_for(Some("tenant-abc"));
        assert!(result.is_err());
    }

    // ── Cycle 4: suspend/resume ─────────────────────────────────────────

    #[test]
    fn test_suspend_sets_status_to_suspended() {
        let registry = TenantExecutorRegistry::new(default_executor());
        registry.upsert("tenant-abc", tenant_executor("abc"));
        registry.suspend("tenant-abc").unwrap();
        assert_eq!(
            registry.tenant_status("tenant-abc").unwrap(),
            TenantStatus::Suspended
        );
    }

    #[test]
    fn test_suspended_tenant_returns_service_unavailable() {
        let registry = TenantExecutorRegistry::new(default_executor());
        registry.upsert("tenant-abc", tenant_executor("abc"));
        registry.suspend("tenant-abc").unwrap();

        let Err(err) = registry.executor_for(Some("tenant-abc")) else {
            panic!("expected Err for suspended tenant");
        };
        assert!(
            matches!(
                err,
                FraiseQLError::ServiceUnavailable {
                    retry_after: Some(60),
                    ..
                }
            ),
            "Expected ServiceUnavailable with retry_after=60, got: {err:?}"
        );
    }

    #[test]
    fn test_resume_restores_active_status() {
        let registry = TenantExecutorRegistry::new(default_executor());
        registry.upsert("tenant-abc", tenant_executor("abc"));

        registry.suspend("tenant-abc").unwrap();
        assert_eq!(
            registry.tenant_status("tenant-abc").unwrap(),
            TenantStatus::Suspended
        );

        registry.resume("tenant-abc").unwrap();
        assert_eq!(
            registry.tenant_status("tenant-abc").unwrap(),
            TenantStatus::Active
        );

        // Can access executor again
        let exec = registry.executor_for(Some("tenant-abc"));
        assert!(exec.is_ok());
    }

    #[test]
    fn test_new_tenant_starts_active() {
        let registry = TenantExecutorRegistry::new(default_executor());
        registry.upsert("tenant-abc", tenant_executor("abc"));
        assert_eq!(
            registry.tenant_status("tenant-abc").unwrap(),
            TenantStatus::Active
        );
    }

    #[test]
    fn test_upsert_preserves_status() {
        let registry = TenantExecutorRegistry::new(default_executor());
        registry.upsert("tenant-abc", tenant_executor("abc"));
        registry.suspend("tenant-abc").unwrap();

        // Update executor — status should stay Suspended
        registry.upsert("tenant-abc", tenant_executor("abc-v2"));
        assert_eq!(
            registry.tenant_status("tenant-abc").unwrap(),
            TenantStatus::Suspended
        );
    }

    #[test]
    fn test_suspend_unknown_tenant_returns_not_found() {
        let registry = TenantExecutorRegistry::<StubAdapter>::new(default_executor());
        let err = registry.suspend("unknown").unwrap_err();
        assert!(
            matches!(err, FraiseQLError::NotFound { .. }),
            "Expected NotFound, got: {err:?}"
        );
    }

    #[test]
    fn test_resume_unknown_tenant_returns_not_found() {
        let registry = TenantExecutorRegistry::<StubAdapter>::new(default_executor());
        let err = registry.resume("unknown").unwrap_err();
        assert!(
            matches!(err, FraiseQLError::NotFound { .. }),
            "Expected NotFound, got: {err:?}"
        );
    }

    #[test]
    fn test_tenant_status_as_str() {
        assert_eq!(TenantStatus::Active.as_str(), "active");
        assert_eq!(TenantStatus::Suspended.as_str(), "suspended");
    }
}
