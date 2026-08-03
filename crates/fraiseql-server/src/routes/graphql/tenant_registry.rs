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
use serde::Deserialize;
use tokio::sync::Semaphore;

#[cfg(feature = "auth")]
use crate::auth::rate_limiting::{AuthRateLimitConfig, Clock, KeyedRateLimiter};

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

/// Type-erased view of tenant suspension status.
///
/// The subscription `WebSocket` path
/// ([`SubscriptionState`](crate::routes::subscriptions::SubscriptionState)) is not generic over the
/// database adapter, so it cannot hold a [`TenantExecutorRegistry<A>`] directly. This trait lets it
/// consult tenant suspension status through a trait object (M-tenant-ws-suspended).
pub trait TenantStatusSource: Send + Sync {
    /// Returns `true` if the named tenant is currently **suspended**.
    ///
    /// An unknown / unregistered tenant is reported as not suspended (active):
    /// unknown-tenant rejection is governed separately by the registry's
    /// `executor_for` authorization path, not by this status check.
    fn is_suspended(&self, tenant_key: &str) -> bool;
}

impl<A: DatabaseAdapter> TenantStatusSource for TenantExecutorRegistry<A> {
    fn is_suspended(&self, tenant_key: &str) -> bool {
        matches!(self.tenant_status(tenant_key), Ok(TenantStatus::Suspended))
    }
}

/// Per-tenant quota configuration.
///
/// All fields are optional — `None` means unlimited (no enforcement).
#[derive(Debug, Clone, Default, Deserialize)]
pub struct TenantQuota {
    /// Maximum requests per second (token bucket rate).
    #[serde(default)]
    pub max_requests_per_sec:       Option<u32>,
    /// Maximum concurrent in-flight requests (semaphore capacity).
    #[serde(default)]
    pub max_concurrent:             Option<u32>,
    /// Maximum storage in bytes — **advisory**, stored and reported, never
    /// enforced.
    ///
    /// The name carries the `_advisory` suffix because the previous one did not,
    /// and a field called `max_storage_bytes` reads as a boundary. There is no
    /// usage-metering path in FraiseQL: nothing measures per-tenant storage, so no
    /// request is ever refused on the basis of this value. It is retained as an
    /// operator annotation — an intent an external metering system can read back
    /// out of the admin API.
    ///
    /// Metering and enforcement are a subsystem, tracked at
    /// <https://github.com/fraiseql/fraiseql/issues/633>. When it lands, this field
    /// gets its unsuffixed name back.
    #[serde(default)]
    pub max_storage_bytes_advisory: Option<u64>,
    /// Maximum estimated cost of a single GraphQL operation (#379). `None` means
    /// no cost budget. A request whose `estimate_query_cost` exceeds this is
    /// rejected at the same chokepoint as the rate/concurrency quotas with
    /// `OPERATION_COST_EXCEEDED` — permanent for the operation, not retryable.
    #[serde(default)]
    pub cost_budget:                Option<usize>,
    /// Rolling per-minute cost budget (#379): the sum of estimated operation
    /// costs a tenant may spend per fixed 60-second window. `None` means no
    /// window budget. An exhausted window rejects with `COST_BUDGET_EXHAUSTED`
    /// (429 + `Retry-After` until the window resets).
    #[serde(default)]
    pub cost_budget_per_minute:     Option<usize>,
}

/// Fixed-window per-minute cost accumulator (#379).
///
/// The same window model as the per-second RPS gate one field up: a fixed
/// 60-second window keyed from the first charge, reset lazily on the first
/// charge after it elapses. Time is passed in as unix seconds so tests drive
/// rollover deterministically without a clock trait.
struct CostWindow {
    /// The per-window budget, in cost units.
    budget: u64,
    /// `(window_start_unix_secs, spent)` under one mutex so a charge is atomic.
    state:  std::sync::Mutex<(u64, u64)>,
}

/// Window length for [`CostWindow`], in seconds.
const COST_WINDOW_SECS: u64 = 60;

impl CostWindow {
    const fn new(budget: u64) -> Self {
        Self {
            budget,
            state: std::sync::Mutex::new((0, 0)),
        }
    }

    /// Charge `cost` against the window at time `now_secs`.
    ///
    /// # Errors
    ///
    /// Returns the seconds until the window resets when the charge would
    /// overspend the budget. The charge is not applied on rejection, so a
    /// cheaper follow-up operation can still fit in the remainder.
    fn try_charge(&self, now_secs: u64, cost: u64) -> Result<(), u64> {
        // Reason: a poisoned mutex means a panic mid-charge; failing closed by
        // propagating the panic is preferable to unbounded spend.
        #[allow(clippy::unwrap_used)]
        let mut state = self.state.lock().unwrap();
        if now_secs >= state.0 + COST_WINDOW_SECS {
            *state = (now_secs, 0);
        }
        if state.1.saturating_add(cost) > self.budget {
            return Err((state.0 + COST_WINDOW_SECS).saturating_sub(now_secs));
        }
        state.1 = state.1.saturating_add(cost);
        Ok(())
    }
}

/// A single tenant entry in the registry: executor + lifecycle status + quotas.
struct TenantEntry<A: DatabaseAdapter> {
    executor:    Arc<ArcSwap<Executor<A>>>,
    status:      AtomicU8,
    /// Concurrency semaphore — `None` when `max_concurrent` is unset.
    concurrency: Option<Arc<Semaphore>>,
    /// Per-second request-rate limiter — `None` when `max_requests_per_sec` is
    /// unset. Built from `max_requests_per_sec` as a fixed one-second window.
    ///
    /// Gated on the `auth` feature because it reuses the audited
    /// [`KeyedRateLimiter`](crate::auth::rate_limiting::KeyedRateLimiter) from
    /// `fraiseql-auth`. In `--no-default-features` builds the limit is parsed
    /// but not enforced; [`with_quota`](TenantEntry::with_quota) logs a warning
    /// in that case so the gap is never silent.
    #[cfg(feature = "auth")]
    rps:         Option<Arc<KeyedRateLimiter>>,
    /// Rolling per-minute cost window — `None` when `cost_budget_per_minute`
    /// is unset (#379).
    cost_window: Option<CostWindow>,
    /// Quota configuration (cloned from registration request).
    quota:       TenantQuota,
}

impl<A: DatabaseAdapter> TenantEntry<A> {
    fn new(executor: Arc<Executor<A>>) -> Self {
        Self {
            executor: Arc::new(ArcSwap::from(executor)),
            status: AtomicU8::new(TenantStatus::Active as u8),
            concurrency: None,
            #[cfg(feature = "auth")]
            rps: None,
            cost_window: None,
            quota: TenantQuota::default(),
        }
    }

    fn with_quota(mut self, quota: TenantQuota) -> Self {
        self.concurrency = quota.max_concurrent.map(|n| Arc::new(Semaphore::new(n as usize)));
        #[cfg(feature = "auth")]
        {
            // Reuse the audited sliding-window limiter from `fraiseql-auth`,
            // configured as a fixed one-second window: at most
            // `max_requests_per_sec` requests are admitted per wall-clock second.
            self.rps = quota.max_requests_per_sec.map(|n| {
                Arc::new(KeyedRateLimiter::new(AuthRateLimitConfig {
                    enabled:      true,
                    max_requests: n,
                    window_secs:  1,
                }))
            });
        }
        #[cfg(not(feature = "auth"))]
        if quota.max_requests_per_sec.is_some() {
            tracing::warn!(
                "Tenant quota sets `max_requests_per_sec`, but per-second rate limiting requires \
                 the `auth` feature; the limit will NOT be enforced in this build."
            );
        }
        self.cost_window =
            quota.cost_budget_per_minute.map(|budget| CostWindow::new(budget as u64));
        self.quota = quota;
        self
    }

    /// Seed the per-minute cost window from the schema-wide default (#379) when
    /// the tenant did not set its own `cost_budget_per_minute`. An explicit
    /// per-tenant budget always wins.
    fn with_default_minute_budget(mut self, default: Option<u64>) -> Self {
        if self.cost_window.is_none() {
            self.cost_window = default.map(CostWindow::new);
        }
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
    default:               Arc<ArcSwap<Executor<A>>>,
    /// Per-tenant entries keyed by tenant identifier.
    tenants:               DashMap<String, TenantEntry<A>>,
    /// Compiled `[security.cost_budget] per_tenant_per_minute_default` (#379),
    /// read once from the default executor's schema at construction. Seeds a
    /// cost window for every registered tenant that does not set its own
    /// `cost_budget_per_minute`.
    default_minute_budget: Option<u64>,
}

impl<A: DatabaseAdapter> TenantExecutorRegistry<A> {
    /// Create a new registry with the given default executor.
    ///
    /// Reads the compiled schema's `[security.cost_budget]
    /// per_tenant_per_minute_default` here, in the constructor, so a second
    /// construction site cannot forget it.
    #[must_use]
    pub fn new(default: Arc<ArcSwap<Executor<A>>>) -> Self {
        let default_minute_budget = default
            .load()
            .schema()
            .security
            .as_ref()
            .and_then(|s| s.cost_budget.as_ref())
            .and_then(|c| c.per_tenant_per_minute_default);
        Self {
            default,
            tenants: DashMap::new(),
            default_minute_budget,
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
            self.tenants.insert(
                key,
                TenantEntry::new(executor).with_default_minute_budget(self.default_minute_budget),
            );
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
            let entry = TenantEntry::new(executor)
                .with_quota(quota)
                .with_default_minute_budget(self.default_minute_budget);
            entry.set_status(prev_status);
            self.tenants.insert(key, entry);
            false
        } else {
            self.tenants.insert(
                key,
                TenantEntry::new(executor)
                    .with_quota(quota)
                    .with_default_minute_budget(self.default_minute_budget),
            );
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
                    message:          format!(
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

    /// Try to admit a request under the tenant's per-second rate limit.
    ///
    /// Returns `Ok(())` when the request is within the configured
    /// `max_requests_per_sec` for the current one-second window, or when no
    /// per-second limit is configured. Like
    /// [`try_acquire_concurrency`](Self::try_acquire_concurrency), this is only
    /// meaningful for an explicitly-keyed, registered tenant — the default
    /// (`None`-key) executor is unlimited and its key must not be passed here.
    ///
    /// Unlike the concurrency permit, nothing is returned to hold: the limiter
    /// is a counter, so admission is recorded immediately and there is no guard
    /// to release.
    ///
    /// # Errors
    ///
    /// Returns [`FraiseQLError::NotFound`] if the tenant key is not registered,
    /// or [`FraiseQLError::RateLimited`] if the current one-second window is
    /// exhausted.
    #[cfg(feature = "auth")]
    pub fn try_acquire_rps(&self, key: &str) -> fraiseql_error::Result<()> {
        let entry = self.tenants.get(key).ok_or_else(|| FraiseQLError::not_found("tenant", key))?;
        match entry.value().rps {
            Some(ref limiter) => rps_gate_check(
                limiter.as_ref(),
                key,
                entry.value().quota.max_requests_per_sec.unwrap_or(0),
            ),
            None => Ok(()),
        }
    }

    /// Returns `true` if the tenant has any cost budget configured — the
    /// per-request ceiling or the rolling per-minute window (#379).
    ///
    /// Lets the caller skip the (otherwise wasted) cost estimation + query re-parse
    /// for tenants with no budget of either kind.
    #[must_use]
    pub fn has_cost_budget(&self, key: &str) -> bool {
        self.tenants.get(key).is_some_and(|e| {
            // The window covers both the explicit per-tenant budget and the
            // schema-default-seeded one.
            e.value().quota.cost_budget.is_some() || e.value().cost_window.is_some()
        })
    }

    /// Reject a request whose estimated `cost` exceeds the tenant's per-operation
    /// cost budget (#379).
    ///
    /// Returns `Ok(())` when within budget or when no budget is configured. Like
    /// the rate/concurrency quotas, this is only meaningful for an explicitly-keyed,
    /// registered tenant.
    ///
    /// # Errors
    ///
    /// Returns [`FraiseQLError::NotFound`] if the tenant key is not registered, or
    /// [`FraiseQLError::CostExceeded`] when `cost` exceeds the configured budget
    /// (`retry_after_secs: None` — a per-request ceiling is permanent for the
    /// operation, so it must not surface as a retryable rate limit).
    pub fn check_cost_budget(&self, key: &str, cost: usize) -> fraiseql_error::Result<()> {
        let entry = self.tenants.get(key).ok_or_else(|| FraiseQLError::not_found("tenant", key))?;
        match entry.value().quota.cost_budget {
            Some(budget) if cost > budget => Err(FraiseQLError::CostExceeded {
                message:          format!(
                    "Tenant '{key}' operation cost {cost} exceeds the per-request cost budget of \
                     {budget}"
                ),
                cost:             cost as u64,
                limit:            budget as u64,
                retry_after_secs: None,
            }),
            _ => Ok(()),
        }
    }

    /// Charge `cost` against the tenant's rolling per-minute budget (#379).
    ///
    /// A no-op for a tenant with no `cost_budget_per_minute`. The window is a
    /// fixed 60 seconds from its first charge, reset lazily; a rejected charge
    /// is not applied, so a cheaper follow-up can still fit the remainder.
    ///
    /// # Errors
    ///
    /// Returns [`FraiseQLError::NotFound`] if the tenant key is not registered,
    /// or [`FraiseQLError::CostExceeded`] with `retry_after_secs: Some(_)` when
    /// the window budget is spent — retryable once the window resets.
    pub fn charge_cost_window(&self, key: &str, cost: usize) -> fraiseql_error::Result<()> {
        let entry = self.tenants.get(key).ok_or_else(|| FraiseQLError::not_found("tenant", key))?;
        let Some(ref window) = entry.value().cost_window else {
            return Ok(());
        };
        let now_secs = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |d| d.as_secs());
        window.try_charge(now_secs, cost as u64).map_err(|retry_after_secs| {
            FraiseQLError::CostExceeded {
                message:          format!(
                    "Tenant '{key}' per-minute cost budget of {budget} is exhausted; retry in \
                     {retry_after_secs}s",
                    budget = window.budget
                ),
                cost:             cost as u64,
                limit:            window.budget,
                retry_after_secs: Some(retry_after_secs),
            }
        })
    }

    // `is_quota_exceeded` / `set_quota_exceeded` are gone. They were a public pair
    // with no production caller on either side: nothing measured storage, so nothing
    // ever set the flag, and reading it always answered `false`. A quota API that is
    // permanently off is worse than no quota API — it reads as an enforced boundary
    // to anyone who greps for one. The metering subsystem that would drive it is
    // tracked at <https://github.com/fraiseql/fraiseql/issues/633> and re-adds the
    // seam when it lands.

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

/// Map an exhausted per-tenant request-rate window onto a
/// [`FraiseQLError::RateLimited`] carrying a one-second retry hint.
///
/// Generic over the limiter's [`Clock`](crate::auth::rate_limiting::Clock) so
/// unit tests can drive window rollover deterministically with a mock clock,
/// while production uses the default system clock.
#[cfg(feature = "auth")]
fn rps_gate_check<C: Clock>(
    limiter: &KeyedRateLimiter<C>,
    key: &str,
    max_per_sec: u32,
) -> fraiseql_error::Result<()> {
    limiter.check(key).map_err(|_| FraiseQLError::RateLimited {
        message:          format!(
            "Tenant '{key}' request-rate limit reached (max {max_per_sec} req/s)"
        ),
        retry_after_secs: 1,
    })
}

#[cfg(test)]
mod cost_window_tests {
    #![allow(clippy::unwrap_used, clippy::panic)] // Reason: test code.

    use super::{COST_WINDOW_SECS, CostWindow};

    /// The fixed window admits charges up to the budget, rejects the
    /// overspending charge without applying it, and reports the time to reset.
    #[test]
    fn admits_to_budget_then_rejects_without_charging() {
        let w = CostWindow::new(1_000);
        assert!(w.try_charge(1_000, 600).is_ok());
        assert!(w.try_charge(1_010, 400).is_ok());
        // Overspend rejected with the remaining window time…
        assert_eq!(w.try_charge(1_030, 1), Err(30));
        // …and NOT charged: a charge that still fits is admitted after.
        assert!(w.try_charge(1_030, 0).is_ok());
    }

    /// The window resets `COST_WINDOW_SECS` after its first charge, not on a
    /// sliding basis.
    #[test]
    fn window_resets_after_sixty_seconds() {
        let w = CostWindow::new(100);
        assert!(w.try_charge(1_000, 100).is_ok());
        assert_eq!(w.try_charge(1_059, 1), Err(1), "still inside the window");
        assert!(
            w.try_charge(1_000 + COST_WINDOW_SECS, 100).is_ok(),
            "a fresh window admits a full budget"
        );
    }
}

#[cfg(all(test, feature = "auth"))]
mod rps_tests {
    #![allow(clippy::unwrap_used, clippy::panic)] // Reason: test code.

    use std::sync::{
        Arc,
        atomic::{AtomicU64, Ordering},
    };

    use fraiseql_error::FraiseQLError;

    use super::rps_gate_check;
    use crate::auth::rate_limiting::{AuthRateLimitConfig, KeyedRateLimiter};

    const fn one_second_window(max: u32) -> AuthRateLimitConfig {
        AuthRateLimitConfig {
            enabled:      true,
            max_requests: max,
            window_secs:  1,
        }
    }

    #[test]
    fn admits_up_to_limit_then_rejects_within_window() {
        // Frozen clock → every call lands in the same one-second window.
        let now = Arc::new(AtomicU64::new(1_000));
        let clock = move || now.load(Ordering::Relaxed);
        let limiter = KeyedRateLimiter::with_clock(one_second_window(2), clock);

        // 2 req/s configured → the first two are admitted...
        assert!(rps_gate_check(&limiter, "acme", 2).is_ok());
        assert!(rps_gate_check(&limiter, "acme", 2).is_ok());
        // ...the third in the same second is rejected as RateLimited.
        assert!(matches!(
            rps_gate_check(&limiter, "acme", 2),
            Err(FraiseQLError::RateLimited { .. })
        ));
    }

    #[test]
    fn window_resets_on_next_second() {
        let now = Arc::new(AtomicU64::new(1_000));
        let clock = {
            let n = now.clone();
            move || n.load(Ordering::Relaxed)
        };
        let limiter = KeyedRateLimiter::with_clock(one_second_window(1), clock);

        assert!(rps_gate_check(&limiter, "acme", 1).is_ok());
        assert!(rps_gate_check(&limiter, "acme", 1).is_err()); // window exhausted

        now.store(1_001, Ordering::Relaxed); // advance one second
        assert!(rps_gate_check(&limiter, "acme", 1).is_ok(), "window must reset");
    }
}
