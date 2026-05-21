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
//! Counters are **in-memory only**; they reset to zero on process restart.
//! The aggregator is wired into `AppState` and exposed via `GET /api/v1/admin/usage`.

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
        }
    }

    /// Create an empty aggregator backed by the given persistence backend.
    #[must_use]
    pub fn new_with_backend(backend: std::sync::Arc<dyn UsageBackend>) -> Self {
        Self {
            counters: DashMap::new(),
            backend:  std::sync::RwLock::new(backend),
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
        *self.backend.write().expect("backend lock poisoned") = backend;
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

    /// Flush all current counters to the persistence backend.
    ///
    /// A no-op when using the default [`NoopBackend`].
    ///
    /// # Errors
    ///
    /// Propagates errors from the underlying [`UsageBackend::flush`].
    ///
    /// # Panics
    ///
    /// Panics if the backend `RwLock` is poisoned (unrecoverable state).
    pub async fn flush_to_backend(&self) -> Result<(), String> {
        let snapshot: HashMap<(String, String, String), u64> = self
            .counters
            .iter()
            .map(|entry| (entry.key().clone(), entry.value().load(Ordering::Relaxed)))
            .collect();
        // Clone the Arc before awaiting so we don't hold the RwLock across await points.
        let backend = self.backend.read().expect("backend lock poisoned").clone();
        backend.flush(&snapshot).await
    }

    /// Load persisted counters from the backend into the in-memory map.
    ///
    /// Existing in-memory counters are **merged** (not replaced): the loaded
    /// value is added to any in-flight in-memory count so that events recorded
    /// between the last flush and this load are not lost.
    ///
    /// # Errors
    ///
    /// Propagates errors from the underlying [`UsageBackend::load`].
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
                .entry(key)
                .or_insert_with(|| AtomicU64::new(0))
                .fetch_add(count, Ordering::Relaxed);
        }
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
    /// Flush all current counter values to the backing store.
    ///
    /// The `counters` map has the form `(tenant_id, period_yyyy_mm, entity_type) → count`.
    ///
    /// # Errors
    ///
    /// Returns an error if the backing store is unavailable or the write fails.
    async fn flush(
        &self,
        counters: &std::collections::HashMap<(String, String, String), u64>,
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
    async fn flush(
        &self,
        counters: &std::collections::HashMap<(String, String, String), u64>,
    ) -> Result<(), String> {
        use ::redis::AsyncCommands as _;

        // Group counters by (tenant, period) so we can HSET per Redis key
        let mut grouped: std::collections::HashMap<String, Vec<(&str, u64)>> =
            std::collections::HashMap::new();
        for ((tenant, period, entity), &count) in counters {
            let key = Self::redis_key(tenant, period);
            grouped.entry(key).or_default().push((entity.as_str(), count));
        }

        let mut conn = self.client.clone();
        for (key, fields) in &grouped {
            if !fields.is_empty() {
                conn.hset_multiple::<_, _, _, ()>(key, fields.as_slice())
                    .await
                    .map_err(|e| format!("Redis flush error: {e}"))?;
            }
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
    async fn flush(
        &self,
        _counters: &std::collections::HashMap<(String, String, String), u64>,
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
    async fn flush(
        &self,
        counters: &std::collections::HashMap<(String, String, String), u64>,
    ) -> Result<(), String> {
        if counters.is_empty() {
            return Ok(());
        }

        // UPSERT each counter — SET count = excluded.count so repeated flushes
        // of the same snapshot are idempotent (last writer wins per key).
        for ((tenant_id, period, entity_type), &count) in counters {
            sqlx::query(
                "INSERT INTO fraiseql_usage_counters
                    (tenant_id, period, entity_type, count, updated_at)
                 VALUES ($1, $2, $3, $4, NOW())
                 ON CONFLICT (tenant_id, period, entity_type)
                 DO UPDATE SET count = EXCLUDED.count, updated_at = NOW()",
            )
            .bind(tenant_id)
            .bind(period)
            .bind(entity_type)
            .bind(count.cast_signed())
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
