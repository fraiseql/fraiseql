//! In-memory usage counter store.
//!
//! Counters are keyed by `(tenant_id, period_yyyy_mm, entity_type)` and stored
//! as lock-free [`AtomicU64`] values inside a [`DashMap`].
//!
//! # Memory growth
//!
//! This is a **v1, unbounded** store: entries are never evicted. Growth is
//! proportional to the product of `#tenants × #periods × #entity_types`.
//! For a deployment with 100 tenants, 12 months retention, and 50 entity types
//! that is at most 60 000 entries — approximately 5 MB.  Eviction policies and
//! persistent storage are out of scope for v1.
//!
//! # Restarts
//!
//! With the default [`NoopBackend`] counters are in-memory only and reset to zero
//! on process restart. With a durable backend configured (`[usage]`), startup
//! loads the persisted counters and a periodic task flushes the **increments**
//! since the last flush — never the process-local totals, which would destroy the
//! stored value after a failed load and make multi-replica totals last-writer-wins
//! (#861). The aggregator is wired into `AppState` and exposed via
//! `GET /api/v1/admin/usage`.

use std::{
    collections::HashMap,
    sync::{
        Arc, OnceLock,
        atomic::{AtomicU64, Ordering},
    },
};

use dashmap::DashMap;
use serde::Serialize;

use super::events::MutationAuditEvent;

// ── Global aggregator ──────────────────────────────────────────────────────

static GLOBAL_USAGE_AGGREGATOR: OnceLock<Arc<UsageAggregator>> = OnceLock::new();

/// Return a reference to the process-wide [`UsageAggregator`].
///
/// Initialised on first call and shared for the lifetime of the process.
/// Both [`MutationAuditLayer`](super::layer::MutationAuditLayer) (tracing
/// subscriber) and the HTTP query endpoint use the same `Arc`, so counters
/// written by the layer are immediately visible to the endpoint.
///
/// [`MutationAuditLayer`]: crate::usage::layer::MutationAuditLayer
#[must_use]
pub fn global_aggregator() -> &'static Arc<UsageAggregator> {
    GLOBAL_USAGE_AGGREGATOR.get_or_init(|| Arc::new(UsageAggregator::new()))
}

// ── Period validation ──────────────────────────────────────────────────────

/// Validate a usage period string in `"YYYY-MM"` format.
///
/// Returns `true` when the period is exactly seven ASCII characters with a
/// `-` separator at index 4, a four-digit year, and a month in `01..=12`.
///
/// # Examples
///
/// ```
/// use fraiseql_server::usage::aggregator::validate_period;
///
/// assert!(validate_period("2026-04"));
/// assert!(!validate_period("2026-13")); // invalid month
/// assert!(!validate_period("2026"));    // missing month
/// assert!(!validate_period("26-04"));   // short year
/// ```
#[must_use]
pub fn validate_period(period: &str) -> bool {
    let bytes = period.as_bytes();
    if bytes.len() != 7 || bytes[4] != b'-' {
        return false;
    }
    let year_str = &period[..4];
    let month_str = &period[5..];
    if !year_str.bytes().all(|b| b.is_ascii_digit()) {
        return false;
    }
    if !month_str.bytes().all(|b| b.is_ascii_digit()) {
        return false;
    }
    let month: u8 = month_str.parse().unwrap_or(0);
    (1..=12).contains(&month)
}

// ── UsageSummary ───────────────────────────────────────────────────────────

/// Per-period mutation counts for a single tenant.
///
/// The `mutations` map has entity-type names as keys and the total mutation
/// count for that entity type in the queried period as values.
///
/// Serialises to:
/// ```json
/// { "mutations": { "User": 42, "Order": 7 } }
/// ```
#[non_exhaustive]
#[derive(Debug, Clone, Serialize)]
pub struct UsageSummary {
    /// Mutation counts keyed by entity type.
    pub mutations: HashMap<String, u64>,
}

// ── UsageAggregator ────────────────────────────────────────────────────────

/// Thread-safe, in-memory usage counter store with optional persistence backend.
///
/// Cheaply cloneable via [`Arc`] — all clones share the same underlying map.
///
/// ## Persistence
///
/// By default, the aggregator uses [`NoopBackend`] and counters are lost on
/// restart.  Pass a [`RedisBackend`] (or any [`UsageBackend`] impl) to
/// [`UsageAggregator::new_with_backend`] to enable durable storage.
///
/// ```rust,no_run
/// # use fraiseql_server::usage::aggregator::{UsageAggregator, NoopBackend};
/// # use std::sync::Arc;
/// let agg = UsageAggregator::new_with_backend(Arc::new(NoopBackend));
/// ```
///
/// # Example
///
/// ```
/// use std::sync::Arc;
/// use fraiseql_server::usage::aggregator::UsageAggregator;
/// use fraiseql_server::usage::events::MutationAuditEvent;
///
/// let agg = Arc::new(UsageAggregator::new());
/// let event = MutationAuditEvent::new("create_user", "User", "create", "acme", "2026-05");
/// agg.record(&event);
/// let summary = agg.query("acme", "2026-05");
/// assert_eq!(summary.mutations["User"], 1);
/// ```
pub struct UsageAggregator {
    /// Key: `(tenant_id, period_yyyy_mm, entity_type)`.
    counters: DashMap<(String, String, String), AtomicU64>,
    /// Optional persistence backend; defaults to [`NoopBackend`].
    ///
    /// Wrapped in `RwLock` so the backend can be swapped after initialization
    /// (e.g. to upgrade from `NoopBackend` to `PostgresBackend` once the DB pool
    /// is available at server startup, after the tracing subscriber has already
    /// taken a reference via [`global_aggregator`]).
    backend:  std::sync::RwLock<std::sync::Arc<dyn UsageBackend>>,
    /// Per-key high-water mark: how much of each counter the current backend has
    /// already been told about. `counters - flushed` is the delta a flush sends.
    flushed:  DashMap<(String, String, String), AtomicU64>,
    /// Whether the current backend's contents were read successfully.
    ///
    /// `false` between `set_backend` and a successful `load_from_backend`. While
    /// false the aggregator refuses to flush: a process that could not read the
    /// persisted counters must not write to them (#861).
    loaded:   std::sync::atomic::AtomicBool,
}

impl std::fmt::Debug for UsageAggregator {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("UsageAggregator")
            .field("entry_count", &self.counters.len())
            .finish_non_exhaustive()
    }
}

impl UsageAggregator {
    /// Create an empty aggregator with no persistence (in-memory only).
    #[must_use]
    pub fn new() -> Self {
        Self {
            counters: DashMap::new(),
            backend:  std::sync::RwLock::new(std::sync::Arc::new(NoopBackend)),
            flushed:  DashMap::new(),
            // The default backend holds nothing, so there is nothing that could
            // have failed to load and nothing a flush could destroy.
            loaded:   std::sync::atomic::AtomicBool::new(true),
        }
    }

    /// Create an empty aggregator backed by the given persistence backend.
    #[must_use]
    pub fn new_with_backend(backend: std::sync::Arc<dyn UsageBackend>) -> Self {
        Self {
            counters: DashMap::new(),
            backend:  std::sync::RwLock::new(backend),
            flushed:  DashMap::new(),
            // A durable backend that has not been read yet.
            loaded:   std::sync::atomic::AtomicBool::new(false),
        }
    }

    /// Replace the persistence backend at runtime.
    ///
    /// Called during server startup to upgrade from the default [`NoopBackend`]
    /// to a durable backend (e.g. [`PostgresBackend`]) once the database pool
    /// is available.  Any in-flight in-memory counters are preserved.
    ///
    /// # Panics
    ///
    /// Panics if the backend `RwLock` is poisoned (unrecoverable state).
    pub fn set_backend(&self, backend: std::sync::Arc<dyn UsageBackend>) {
        // A new backend has not been read yet, so flushing to it would be writing
        // over contents this process has never seen. `load_from_backend` re-arms.
        self.loaded.store(false, Ordering::Release);
        *self.backend.write().expect("backend lock poisoned") = backend;
    }

    /// Whether the current backend's persisted counters were read successfully.
    ///
    /// `false` disables flushing — see [`Self::flush_to_backend`].
    #[must_use]
    pub fn is_loaded(&self) -> bool {
        self.loaded.load(Ordering::Acquire)
    }

    /// Record one mutation audit event, incrementing the appropriate counter.
    ///
    /// This method is lock-free on the hot path: it uses [`AtomicU64::fetch_add`]
    /// after the initial shard lock in [`DashMap::entry`].
    pub fn record(&self, event: &MutationAuditEvent) {
        let key = (event.tenant_id.clone(), event.period.clone(), event.entity_type.clone());
        self.counters
            .entry(key)
            .or_insert_with(|| AtomicU64::new(0))
            .fetch_add(1, Ordering::Relaxed);
    }

    /// Return the usage summary for a tenant and period.
    ///
    /// Returns `UsageSummary { mutations: {} }` (never an error) when no events
    /// have been recorded for the given `(tenant_id, period)` pair.
    pub fn query(&self, tenant_id: &str, period: &str) -> UsageSummary {
        let mut mutations: HashMap<String, u64> = HashMap::new();
        for entry in &self.counters {
            let (t, p, e) = entry.key();
            if t == tenant_id && p == period {
                mutations.insert(e.clone(), entry.value().load(Ordering::Relaxed));
            }
        }
        UsageSummary { mutations }
    }

    /// Return the total number of distinct counter entries (for monitoring).
    #[must_use]
    pub fn entry_count(&self) -> usize {
        self.counters.len()
    }

    /// Send the counts recorded since the last successful flush to the backend.
    ///
    /// Sends **deltas**, not totals, and advances the per-key watermark only after
    /// the backend confirms the write — so a failed flush is retried on the next
    /// tick rather than lost, and N replicas sum rather than overwrite each other.
    ///
    /// A no-op when using the default [`NoopBackend`].
    ///
    /// # Errors
    ///
    /// Refuses with an error when the current backend's startup load failed: a
    /// process that could not read the persisted counters must not write to them
    /// (#861). Otherwise propagates errors from [`UsageBackend::flush_deltas`].
    ///
    /// # Panics
    ///
    /// Panics if the backend `RwLock` is poisoned (unrecoverable state).
    pub async fn flush_to_backend(&self) -> Result<(), String> {
        if !self.is_loaded() {
            return Err("refusing to flush usage counters: the startup load from the configured \
                 backend failed, so this process does not know the persisted totals. \
                 Restart once the store is reachable; counters accumulated meanwhile are \
                 in-memory only."
                .to_string());
        }

        // Snapshot totals and derive the delta against the watermark. Counts
        // recorded between here and the watermark advance below are simply picked
        // up by the next flush — the watermark is only ever moved to a value that
        // was actually sent.
        let mut deltas: HashMap<(String, String, String), u64> = HashMap::new();
        let mut sent: Vec<((String, String, String), u64)> = Vec::new();
        for entry in &self.counters {
            let total = entry.value().load(Ordering::Relaxed);
            let already =
                self.flushed.get(entry.key()).map_or(0, |w| w.value().load(Ordering::Relaxed));
            let delta = total.saturating_sub(already);
            if delta > 0 {
                deltas.insert(entry.key().clone(), delta);
                sent.push((entry.key().clone(), total));
            }
        }
        if deltas.is_empty() {
            return Ok(());
        }

        // Clone the Arc before awaiting so we don't hold the RwLock across await points.
        let backend = self.backend.read().expect("backend lock poisoned").clone();
        backend.flush_deltas(&deltas).await?;

        for (key, total) in sent {
            self.flushed
                .entry(key)
                .or_insert_with(|| AtomicU64::new(0))
                .store(total, Ordering::Relaxed);
        }
        Ok(())
    }

    /// Load persisted counters from the backend into the in-memory map.
    ///
    /// Existing in-memory counters are **merged** (not replaced): the loaded
    /// value is added to any in-flight in-memory count so that events recorded
    /// between the last flush and this load are not lost. The watermark is
    /// advanced by the same amount, so the next flush sends only what this
    /// process counted itself.
    ///
    /// On success the aggregator is armed for flushing; see
    /// [`Self::flush_to_backend`].
    ///
    /// # Errors
    ///
    /// Propagates errors from the underlying [`UsageBackend::load`]. On error the
    /// aggregator stays disarmed and will refuse to flush.
    ///
    /// # Panics
    ///
    /// Panics if the backend `RwLock` is poisoned (unrecoverable state).
    pub async fn load_from_backend(&self) -> Result<(), String> {
        // Clone the Arc before awaiting so we don't hold the RwLock across await points.
        let backend = self.backend.read().expect("backend lock poisoned").clone();
        let persisted = backend.load().await?;
        for (key, count) in persisted {
            self.counters
                .entry(key.clone())
                .or_insert_with(|| AtomicU64::new(0))
                .fetch_add(count, Ordering::Relaxed);
            // The store already holds this much, so it is not part of any delta.
            self.flushed
                .entry(key)
                .or_insert_with(|| AtomicU64::new(0))
                .fetch_add(count, Ordering::Relaxed);
        }
        self.loaded.store(true, Ordering::Release);
        Ok(())
    }
}

impl Default for UsageAggregator {
    fn default() -> Self {
        Self::new()
    }
}

// ── Persistence backend ────────────────────────────────────────────────────

/// Persistence backend for usage counters.
///
/// Implementations flush the aggregator's in-memory counters to a durable
/// store and reload them on startup. The default [`NoopBackend`] is a no-op
/// that preserves current in-memory-only behaviour.
#[async_trait::async_trait]
pub trait UsageBackend: Send + Sync {
    /// Add the given **deltas** to the backing store.
    ///
    /// The `deltas` map has the form
    /// `(tenant_id, period_yyyy_mm, entity_type) → increment-since-last-flush`, and
    /// an implementation must **add** each value to whatever is already stored.
    ///
    /// This was previously an absolute `flush` of the process-local totals, which
    /// was only coherent as a strict load-then-overwrite pair: if the startup load
    /// failed, the first tick wrote a fresh process's small count over the
    /// accumulated persisted total and destroyed it; and with more than one replica
    /// — the shipped Kubernetes manifests say `replicas: 3` — the replicas
    /// overwrote each other every interval, so totals were last-writer-wins rather
    /// than summed (#861). The method was renamed along with the semantics so no
    /// implementation could keep the old absolute write by accident.
    ///
    /// # Errors
    ///
    /// Returns an error if the backing store is unavailable or the write fails.
    async fn flush_deltas(
        &self,
        deltas: &std::collections::HashMap<(String, String, String), u64>,
    ) -> Result<(), String>;

    /// Load all persisted counters from the backing store.
    ///
    /// # Errors
    ///
    /// Returns an error if the backing store is unavailable or the read fails.
    async fn load(
        &self,
    ) -> Result<std::collections::HashMap<(String, String, String), u64>, String>;
}

/// No-op backend — counters are in-memory only, lost on restart.
///
/// This is the default when no persistence backend is configured.
#[derive(Debug, Default)]
pub struct NoopBackend;

// ── Redis backend ──────────────────────────────────────────────────────────

/// Redis-backed usage persistence.
///
/// Counters are stored as Redis hashes with the key pattern:
/// `fraiseql:usage:{tenant_id}:{period_yyyy_mm}` where each hash field is an
/// `entity_type` and the value is the cumulative mutation count.
///
/// Enable with the `redis-usage` Cargo feature.
#[cfg(feature = "redis-usage")]
#[derive(Debug, Clone)]
pub struct RedisBackend {
    client: ::redis::aio::ConnectionManager,
}

#[cfg(feature = "redis-usage")]
impl RedisBackend {
    /// Create a new Redis backend from an existing connection manager.
    #[must_use]
    pub const fn new(client: ::redis::aio::ConnectionManager) -> Self {
        Self { client }
    }

    fn redis_key(tenant_id: &str, period: &str) -> String {
        format!("fraiseql:usage:{tenant_id}:{period}")
    }
}

#[cfg(feature = "redis-usage")]
#[async_trait::async_trait]
impl UsageBackend for RedisBackend {
    async fn flush_deltas(
        &self,
        deltas: &std::collections::HashMap<(String, String, String), u64>,
    ) -> Result<(), String> {
        use ::redis::AsyncCommands as _;

        // `HINCRBY`, not `HSET`: the values are increments since the last flush.
        // The absolute `hset_multiple` this replaced made a shared Redis store —
        // the one backend whose whole point is being shared — last-writer-wins
        // across replicas (#861).
        let mut conn = self.client.clone();
        for ((tenant, period, entity), &delta) in deltas {
            if delta == 0 {
                continue;
            }
            let key = Self::redis_key(tenant, period);
            conn.hincr::<_, _, _, ()>(&key, entity.as_str(), delta)
                .await
                .map_err(|e| format!("Redis flush error: {e}"))?;
        }
        Ok(())
    }

    async fn load(
        &self,
    ) -> Result<std::collections::HashMap<(String, String, String), u64>, String> {
        use ::redis::AsyncCommands as _;

        let mut conn = self.client.clone();

        // SCAN for all keys matching fraiseql:usage:*
        let mut result = std::collections::HashMap::new();
        let keys: Vec<String> = conn
            .keys("fraiseql:usage:*")
            .await
            .map_err(|e| format!("Redis load scan error: {e}"))?;

        for key in &keys {
            // Key format: fraiseql:usage:{tenant}:{period}
            let parts: Vec<&str> = key.splitn(4, ':').collect();
            if parts.len() != 4 {
                continue;
            }
            let tenant = parts[2].to_owned();
            let period = parts[3].to_owned();

            let hash: std::collections::HashMap<String, u64> = conn
                .hgetall(key)
                .await
                .map_err(|e| format!("Redis load hgetall error for {key}: {e}"))?;

            for (entity, count) in hash {
                result.insert((tenant.clone(), period.clone(), entity), count);
            }
        }
        Ok(result)
    }
}

#[async_trait::async_trait]
impl UsageBackend for NoopBackend {
    async fn flush_deltas(
        &self,
        _deltas: &std::collections::HashMap<(String, String, String), u64>,
    ) -> Result<(), String> {
        Ok(())
    }

    async fn load(
        &self,
    ) -> Result<std::collections::HashMap<(String, String, String), u64>, String> {
        Ok(std::collections::HashMap::new())
    }
}

// ── PostgreSQL backend ─────────────────────────────────────────────────────

/// PostgreSQL-backed usage persistence.
///
/// Counters are stored in a `fraiseql_usage_counters` table using UPSERT
/// semantics. The schema is created automatically on [`PostgresBackend::new`]
/// if it does not already exist.
///
/// The table schema is:
///
/// ```sql
/// CREATE TABLE fraiseql_usage_counters (
///     tenant_id   TEXT NOT NULL,
///     period      TEXT NOT NULL,
///     entity_type TEXT NOT NULL,
///     count       BIGINT NOT NULL DEFAULT 0,
///     updated_at  TIMESTAMPTZ NOT NULL DEFAULT NOW(),
///     PRIMARY KEY (tenant_id, period, entity_type)
/// );
/// ```
#[derive(Debug, Clone)]
pub struct PostgresBackend {
    pool: sqlx::PgPool,
}

impl PostgresBackend {
    /// Create a new PostgreSQL backend, ensuring the schema exists.
    ///
    /// # Errors
    ///
    /// Returns an error if the schema migration fails.
    pub async fn new(pool: sqlx::PgPool) -> Result<Self, String> {
        sqlx::query(
            "CREATE TABLE IF NOT EXISTS fraiseql_usage_counters (
                tenant_id   TEXT NOT NULL,
                period      TEXT NOT NULL,
                entity_type TEXT NOT NULL,
                count       BIGINT NOT NULL DEFAULT 0,
                updated_at  TIMESTAMPTZ NOT NULL DEFAULT NOW(),
                PRIMARY KEY (tenant_id, period, entity_type)
            )",
        )
        .execute(&pool)
        .await
        .map_err(|e| format!("PostgresBackend schema migration failed: {e}"))?;

        Ok(Self { pool })
    }
}

#[async_trait::async_trait]
impl UsageBackend for PostgresBackend {
    async fn flush_deltas(
        &self,
        deltas: &std::collections::HashMap<(String, String, String), u64>,
    ) -> Result<(), String> {
        if deltas.is_empty() {
            return Ok(());
        }

        // Additive UPSERT. `count = fraiseql_usage_counters.count + EXCLUDED.count`
        // is correct under both hazards the absolute write got wrong: a process
        // whose startup load failed adds only what it counted itself, and N
        // replicas each add their own interval instead of overwriting each other
        // (#861). The aggregator only advances its watermark after this returns
        // Ok, so a failed flush is retried rather than lost.
        for ((tenant_id, period, entity_type), &delta) in deltas {
            sqlx::query(
                "INSERT INTO fraiseql_usage_counters
                    (tenant_id, period, entity_type, count, updated_at)
                 VALUES ($1, $2, $3, $4, NOW())
                 ON CONFLICT (tenant_id, period, entity_type)
                 DO UPDATE SET count = fraiseql_usage_counters.count + EXCLUDED.count,
                               updated_at = NOW()",
            )
            .bind(tenant_id)
            .bind(period)
            .bind(entity_type)
            .bind(delta.cast_signed())
            .execute(&self.pool)
            .await
            .map_err(|e| format!("PostgresBackend flush error: {e}"))?;
        }
        Ok(())
    }

    async fn load(
        &self,
    ) -> Result<std::collections::HashMap<(String, String, String), u64>, String> {
        let rows: Vec<(String, String, String, i64)> = sqlx::query_as(
            "SELECT tenant_id, period, entity_type, count
             FROM fraiseql_usage_counters",
        )
        .fetch_all(&self.pool)
        .await
        .map_err(|e| format!("PostgresBackend load error: {e}"))?;

        let result = rows
            .into_iter()
            .map(|(tenant_id, period, entity_type, count)| {
                ((tenant_id, period, entity_type), count.max(0).cast_unsigned())
            })
            .collect();

        Ok(result)
    }
}
