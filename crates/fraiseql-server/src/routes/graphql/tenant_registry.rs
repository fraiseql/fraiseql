//! `TenantExecutorRegistry` — per-tenant executor dispatch with lock-free reads.
//!
//! Maps tenant keys to individual `Executor<A>` instances, each holding its own
//! compiled schema and database adapter. Reads are lock-free via `ArcSwap`;
//! writes are serialized per-key via `DashMap`.

use std::sync::{
    Arc,
    atomic::{AtomicBool, AtomicU8, Ordering},
};

use arc_swap::ArcSwap;
use dashmap::DashMap;
use fraiseql_core::{db::traits::DatabaseAdapter, runtime::Executor};
use fraiseql_error::FraiseQLError;
use serde::Deserialize;
use tokio::sync::Semaphore;

/// Tenant lifecycle status.
///
/// Stored as an `AtomicU8` in the registry for lock-free reads.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum TenantStatus {
    /// Tenant is operational — requests are served normally.
    Active    = 0,
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

/// Per-tenant quota configuration.
///
/// All fields are optional — `None` means unlimited (no enforcement).
#[derive(Debug, Clone, Default, Deserialize)]
pub struct TenantQuota {
    /// Maximum requests per second (token bucket rate).
    #[serde(default)]
    pub max_requests_per_sec: Option<u32>,
    /// Maximum concurrent in-flight requests (semaphore capacity).
    #[serde(default)]
    pub max_concurrent:       Option<u32>,
    /// Maximum storage in bytes (soft limit, checked periodically).
    #[serde(default)]
    pub max_storage_bytes:    Option<u64>,
}

/// A single tenant entry in the registry: executor + lifecycle status + quotas.
struct TenantEntry<A: DatabaseAdapter> {
    executor: Arc<ArcSwap<Executor<A>>>,
    status: AtomicU8,
    /// Concurrency semaphore — `None` when `max_concurrent` is unset.
    concurrency: Option<Arc<Semaphore>>,
    /// Soft quota exceeded flag (set by background task, blocks mutations).
    quota_exceeded: AtomicBool,
    /// Quota configuration (cloned from registration request).
    quota: TenantQuota,
}

impl<A: DatabaseAdapter> TenantEntry<A> {
    fn new(executor: Arc<Executor<A>>) -> Self {
        Self {
            executor: Arc::new(ArcSwap::from(executor)),
            status: AtomicU8::new(TenantStatus::Active as u8),
            concurrency: None,
            quota_exceeded: AtomicBool::new(false),
            quota: TenantQuota::default(),
        }
    }

    fn with_quota(mut self, quota: TenantQuota) -> Self {
        self.concurrency = quota.max_concurrent.map(|n| Arc::new(Semaphore::new(n as usize)));
        self.quota = quota;
        self
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
                message: format!("Tenant '{key}' is suspended"),
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

    /// Register or update a tenant executor with quota configuration.
    ///
    /// Like [`upsert`](Self::upsert), but also sets per-tenant quota limits.
    /// On insert, quotas are applied immediately. On update, the executor is
    /// swapped atomically; quotas are updated by removing and re-inserting
    /// the entry (status is preserved).
    pub fn upsert_with_quota(
        &self,
        key: impl Into<String>,
        executor: Arc<Executor<A>>,
        quota: TenantQuota,
    ) -> bool {
        let key = key.into();
        if let Some(existing) = self.tenants.get(&key) {
            // Preserve status across quota update
            let prev_status = existing.value().status();
            drop(existing);
            self.tenants.remove(&key);
            let entry = TenantEntry::new(executor).with_quota(quota);
            entry.set_status(prev_status);
            self.tenants.insert(key, entry);
            false
        } else {
            self.tenants.insert(key, TenantEntry::new(executor).with_quota(quota));
            true
        }
    }

    /// Try to acquire a concurrency permit for a tenant.
    ///
    /// Returns `Ok(Some(permit))` if a permit was acquired, `Ok(None)` if no
    /// concurrency limit is configured, or `Err(RateLimited)` if the limit is
    /// reached.
    ///
    /// The caller must hold the permit for the duration of the request.
    ///
    /// # Errors
    ///
    /// Returns `FraiseQLError::RateLimited` if all concurrency permits are in use.
    pub fn try_acquire_concurrency(
        &self,
        key: &str,
    ) -> fraiseql_error::Result<Option<tokio::sync::OwnedSemaphorePermit>> {
        let entry = self.tenants.get(key).ok_or_else(|| FraiseQLError::not_found("tenant", key))?;
        if let Some(ref sem) = entry.value().concurrency {
            match sem.clone().try_acquire_owned() {
                Ok(permit) => Ok(Some(permit)),
                Err(_) => Err(FraiseQLError::RateLimited {
                    message: format!(
                        "Tenant '{key}' concurrency limit reached (max {})",
                        entry.value().quota.max_concurrent.unwrap_or(0)
                    ),
                    retry_after_secs: 1,
                }),
            }
        } else {
            Ok(None)
        }
    }

    /// Returns `true` if the tenant's soft storage quota has been exceeded.
    ///
    /// When exceeded, mutations should be rejected (reads still allowed).
    #[must_use]
    pub fn is_quota_exceeded(&self, key: &str) -> bool {
        self.tenants
            .get(key)
            .is_some_and(|e| e.value().quota_exceeded.load(Ordering::Relaxed))
    }

    /// Set the quota-exceeded flag for a tenant (called by background task).
    pub fn set_quota_exceeded(&self, key: &str, exceeded: bool) {
        if let Some(entry) = self.tenants.get(key) {
            entry.value().quota_exceeded.store(exceeded, Ordering::Relaxed);
        }
    }

    /// Returns the quota configuration for a tenant.
    ///
    /// # Errors
    ///
    /// Returns `FraiseQLError::NotFound` if the tenant key is not registered.
    pub fn tenant_quota(&self, key: &str) -> fraiseql_error::Result<TenantQuota> {
        let entry = self.tenants.get(key).ok_or_else(|| FraiseQLError::not_found("tenant", key))?;
        Ok(entry.value().quota.clone())
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
