//! Distributed checkpoint leasing for multi-listener coordination.
//!
//! Three backends are available, chosen at construction time:
//!
//! | Backend | Coordination scope | Requires |
//! |---------|-------------------|----------|
//! | [`CheckpointLease::in_process`] | single process | nothing |
//! | [`CheckpointLease::postgres`] | multi-process | PostgreSQL (`postgres` feature) |
//! | `CheckpointLease::redis` | multi-process | Redis (`redis-lease` feature) |
//!
//! **In-process** is suitable for testing and single-node deployments.
//! **Postgres** uses a PostgreSQL *session advisory lock*: the lock lives as long as
//! the underlying database connection, so `renew()` is a no-op and the lease has no TTL.
//! **Redis** uses `SET NX EX` with Lua-guarded release and renewal; `renew()` extends
//! the TTL atomically.

use std::{sync::Arc, time::Instant};

use tokio::sync::Mutex;

use crate::error::{ObserverError, Result};

// ── In-process lease ──────────────────────────────────────────────────────────

/// Combined state for the in-process lease, protected by a single mutex.
///
/// Merging `holder` and `acquired_at` into one struct under one lock eliminates
/// the inconsistency window that existed when they were separate `Arc<Mutex<…>>`
/// fields: `time_remaining_ms()` previously read `acquired_at` without holding
/// `holder`, so a concurrent `release()` could clear `holder` while the caller
/// still observed a non-zero remaining time.
struct LeaseState {
    holder:      Option<String>,
    acquired_at: Option<Instant>,
}

struct InProcessLease {
    listener_id:       String,
    checkpoint_id:     i64,
    state:             Arc<Mutex<LeaseState>>,
    lease_duration_ms: u64,
}

impl InProcessLease {
    fn new(listener_id: String, checkpoint_id: i64, lease_duration_ms: u64) -> Self {
        Self {
            listener_id,
            checkpoint_id,
            state: Arc::new(Mutex::new(LeaseState {
                holder:      None,
                acquired_at: None,
            })),
            lease_duration_ms,
        }
    }

    async fn acquire(&self) -> Result<bool> {
        let mut state = self.state.lock().await;

        if let Some(ref current_holder) = state.holder {
            if let Some(acquired_time) = state.acquired_at {
                if acquired_time.elapsed().as_millis() < u128::from(self.lease_duration_ms) {
                    return Ok(current_holder == &self.listener_id);
                }
            }
        }

        state.holder = Some(self.listener_id.clone());
        state.acquired_at = Some(Instant::now());
        Ok(true)
    }

    async fn release(&self) -> Result<()> {
        let mut state = self.state.lock().await;
        if let Some(ref current_holder) = state.holder {
            if current_holder == &self.listener_id {
                state.holder = None;
                state.acquired_at = None;
                return Ok(());
            }
        }
        Err(ObserverError::InvalidConfig {
            message: format!(
                "Cannot release lease held by another listener: {:?}",
                state.holder.as_ref()
            ),
        })
    }

    async fn renew(&self) -> Result<bool> {
        let mut state = self.state.lock().await;
        if let Some(ref current_holder) = state.holder {
            if current_holder == &self.listener_id {
                state.acquired_at = Some(Instant::now());
                return Ok(true);
            }
        }
        Ok(false)
    }

    async fn is_valid(&self) -> Result<bool> {
        let state = self.state.lock().await;
        if let Some(ref current_holder) = state.holder {
            if current_holder == &self.listener_id {
                if let Some(acquired_time) = state.acquired_at {
                    return Ok(
                        acquired_time.elapsed().as_millis() < u128::from(self.lease_duration_ms)
                    );
                }
            }
        }
        Ok(false)
    }

    async fn get_holder(&self) -> Result<Option<String>> {
        Ok(self.state.lock().await.holder.clone())
    }

    async fn time_remaining_ms(&self) -> Result<u64> {
        let state = self.state.lock().await;
        if let Some(acquired_time) = state.acquired_at {
            // Consistent view: both holder and acquired_at are read under the same lock.
            let elapsed = acquired_time.elapsed().as_millis() as u64;
            if elapsed < self.lease_duration_ms {
                return Ok(self.lease_duration_ms - elapsed);
            }
            return Ok(0);
        }
        Ok(self.lease_duration_ms)
    }
}

// ── PostgreSQL advisory lease ─────────────────────────────────────────────────

/// Distributed lease backed by a PostgreSQL *session* advisory lock.
///
/// The lock key is the `checkpoint_id` (`i8` / `bigint`). The lock is held for
/// as long as the underlying `PoolConnection` is alive; `release()` explicitly
/// calls `pg_advisory_unlock` before returning the connection to the pool.
///
/// Because advisory locks have no TTL, `renew()` is a no-op that returns
/// `true` while the lock is held, and `time_remaining_ms()` returns `u64::MAX`.
#[cfg(feature = "postgres")]
pub struct PostgresAdvisoryLease {
    pool:          sqlx::PgPool,
    listener_id:   String,
    checkpoint_id: i64,
    /// Holds the dedicated connection while the advisory lock is active.
    conn:          Arc<Mutex<Option<sqlx::pool::PoolConnection<sqlx::Postgres>>>>,
}

#[cfg(feature = "postgres")]
impl PostgresAdvisoryLease {
    /// Create a new Postgres-backed advisory lease.
    #[must_use]
    pub fn new(pool: sqlx::PgPool, listener_id: String, checkpoint_id: i64) -> Self {
        Self {
            pool,
            listener_id,
            checkpoint_id,
            conn: Arc::new(Mutex::new(None)),
        }
    }

    /// Attempt to acquire the session advisory lock.
    ///
    /// Returns `true` when the lock is now held (including if we already held it),
    /// `false` when another session holds the lock.
    ///
    /// # Errors
    ///
    /// Returns [`ObserverError::DatabaseError`] if acquiring a pool connection or
    /// executing `pg_try_advisory_lock` fails.
    pub async fn acquire(&self) -> Result<bool> {
        let mut conn_guard = self.conn.lock().await;

        // Already held — idempotent.
        if conn_guard.is_some() {
            return Ok(true);
        }

        let mut new_conn = self.pool.acquire().await.map_err(|e| ObserverError::DatabaseError {
            reason: format!("Failed to acquire connection for advisory lock: {e}"),
        })?;

        let acquired: bool = sqlx::query_scalar("SELECT pg_try_advisory_lock($1)")
            .bind(self.checkpoint_id)
            .fetch_one(&mut *new_conn)
            .await
            .map_err(|e| ObserverError::DatabaseError {
                reason: format!("pg_try_advisory_lock failed: {e}"),
            })?;

        if acquired {
            *conn_guard = Some(new_conn);
            // new_conn is moved into conn_guard; if not acquired it drops → back to pool.
        }

        Ok(acquired)
    }

    /// Release the advisory lock and return the connection to the pool.
    ///
    /// # Errors
    ///
    /// Returns [`ObserverError::DatabaseError`] if `pg_advisory_unlock` fails, or
    /// [`ObserverError::InvalidConfig`] if the lock is not currently held.
    pub async fn release(&self) -> Result<()> {
        let mut conn_guard = self.conn.lock().await;
        if let Some(mut conn) = conn_guard.take() {
            sqlx::query("SELECT pg_advisory_unlock($1)")
                .bind(self.checkpoint_id)
                .execute(&mut *conn)
                .await
                .map_err(|e| ObserverError::DatabaseError {
                    reason: format!("pg_advisory_unlock failed: {e}"),
                })?;
            // conn drops here → returned to pool.
            Ok(())
        } else {
            Err(ObserverError::InvalidConfig {
                message: "Cannot release advisory lock: not currently held".to_string(),
            })
        }
    }

    /// No-op: PostgreSQL session advisory locks have no TTL.
    ///
    /// Returns `true` while the lock is held, `false` otherwise.
    ///
    /// # Errors
    ///
    /// This function currently always returns `Ok`.
    pub async fn renew(&self) -> Result<bool> {
        Ok(self.conn.lock().await.is_some())
    }

    /// Returns `true` if we currently hold the advisory lock.
    ///
    /// # Errors
    ///
    /// This function currently always returns `Ok`.
    pub async fn is_valid(&self) -> Result<bool> {
        Ok(self.conn.lock().await.is_some())
    }

    /// Returns our `listener_id` if we hold the lock, `None` otherwise.
    ///
    /// Note: advisory locks store no metadata in PostgreSQL, so the holder
    /// of an uncontested lock is not externally visible.
    ///
    /// # Errors
    ///
    /// This function currently always returns `Ok`.
    pub async fn get_holder(&self) -> Result<Option<String>> {
        if self.conn.lock().await.is_some() {
            Ok(Some(self.listener_id.clone()))
        } else {
            Ok(None)
        }
    }

    /// Returns `u64::MAX`: PostgreSQL advisory locks do not expire.
    ///
    /// # Errors
    ///
    /// This function currently always returns `Ok`.
    pub async fn time_remaining_ms(&self) -> Result<u64> {
        Ok(u64::MAX)
    }
}

// ── Redis advisory lease ──────────────────────────────────────────────────────

/// Distributed lease backed by Redis `SET NX EX`.
///
/// The key is `fraiseql:lease:{checkpoint_id}` and the value is the `listener_id`.
/// Lua scripts guard `release()` and `renew()` for atomicity.
#[cfg(feature = "redis-lease")]
pub struct RedisAdvisoryLease {
    conn:                redis::aio::ConnectionManager,
    listener_id:         String,
    checkpoint_id:       i64,
    lease_duration_secs: u64,
}

#[cfg(feature = "redis-lease")]
impl RedisAdvisoryLease {
    /// Create a new Redis-backed advisory lease.
    ///
    /// # Arguments
    ///
    /// * `conn` — shared Redis connection manager
    /// * `listener_id` — unique identifier for this listener instance
    /// * `checkpoint_id` — numeric key distinguishing the checkpoint being leased
    /// * `lease_duration_secs` — TTL; must be renewed before expiry via `renew()`
    #[must_use]
    pub fn new(
        conn: redis::aio::ConnectionManager,
        listener_id: String,
        checkpoint_id: i64,
        lease_duration_secs: u64,
    ) -> Self {
        Self {
            conn,
            listener_id,
            checkpoint_id,
            lease_duration_secs,
        }
    }

    fn redis_key(&self) -> String {
        format!("fraiseql:lease:{}", self.checkpoint_id)
    }

    /// Attempt to acquire the lease via `SET NX EX`.
    ///
    /// Returns `true` when the key was set (we now hold the lease),
    /// `false` when it already existed (another instance holds it).
    ///
    /// # Errors
    ///
    /// Propagates Redis connection or command errors.
    pub async fn acquire(&self) -> Result<bool> {
        let key = self.redis_key();
        // SET key value NX EX ttl → "OK" on success, nil on failure.
        let result: Option<String> = redis::cmd("SET")
            .arg(&key)
            .arg(&self.listener_id)
            .arg("NX")
            .arg("EX")
            .arg(self.lease_duration_secs)
            .query_async(&mut self.conn.clone())
            .await?;
        Ok(result.is_some())
    }

    /// Release the lease atomically via Lua.
    ///
    /// Only deletes the key when the stored value matches our `listener_id`.
    ///
    /// # Errors
    ///
    /// Propagates Redis connection or Lua script execution errors.
    pub async fn release(&self) -> Result<()> {
        let key = self.redis_key();
        // Lua: check value matches, then DEL atomically.
        let script = r"
            local val = redis.call('GET', KEYS[1])
            if val == ARGV[1] then
                return redis.call('DEL', KEYS[1])
            end
            return 0
        ";
        let _: i64 = redis::Script::new(script)
            .key(&key)
            .arg(&self.listener_id)
            .invoke_async(&mut self.conn.clone())
            .await?;
        Ok(())
    }

    /// Extend the lease TTL atomically via Lua.
    ///
    /// Returns `true` if the TTL was refreshed (we hold the lease),
    /// `false` if the key is missing or owned by another listener.
    ///
    /// # Errors
    ///
    /// Propagates Redis connection or Lua script execution errors.
    pub async fn renew(&self) -> Result<bool> {
        let key = self.redis_key();
        let script = r"
            local val = redis.call('GET', KEYS[1])
            if val == ARGV[1] then
                redis.call('EXPIRE', KEYS[1], ARGV[2])
                return 1
            end
            return 0
        ";
        let result: i64 = redis::Script::new(script)
            .key(&key)
            .arg(&self.listener_id)
            .arg(self.lease_duration_secs)
            .invoke_async(&mut self.conn.clone())
            .await?;
        Ok(result == 1)
    }

    /// Returns `true` if the Redis key exists and is owned by this listener.
    ///
    /// # Errors
    ///
    /// Propagates Redis connection or command errors.
    pub async fn is_valid(&self) -> Result<bool> {
        let key = self.redis_key();
        let holder: Option<String> =
            redis::cmd("GET").arg(&key).query_async(&mut self.conn.clone()).await?;
        Ok(holder.as_deref() == Some(self.listener_id.as_str()))
    }

    /// Returns the current holder's `listener_id`, or `None` if the key is absent.
    ///
    /// # Errors
    ///
    /// Propagates Redis connection or command errors.
    pub async fn get_holder(&self) -> Result<Option<String>> {
        let key = self.redis_key();
        let holder: Option<String> =
            redis::cmd("GET").arg(&key).query_async(&mut self.conn.clone()).await?;
        Ok(holder)
    }

    /// Returns remaining TTL in milliseconds (Redis `TTL` command × 1000).
    ///
    /// Returns `0` when the key is absent or has no TTL.
    ///
    /// # Errors
    ///
    /// Propagates Redis connection or command errors.
    pub async fn time_remaining_ms(&self) -> Result<u64> {
        let key = self.redis_key();
        let ttl_secs: i64 = redis::cmd("TTL").arg(&key).query_async(&mut self.conn.clone()).await?;
        if ttl_secs < 0 {
            Ok(0)
        } else {
            Ok(ttl_secs as u64 * 1_000)
        }
    }
}

// ── Private inner enum ────────────────────────────────────────────────────────

enum LeaseKind {
    InProcess(InProcessLease),
    #[cfg(feature = "postgres")]
    Postgres(PostgresAdvisoryLease),
    #[cfg(feature = "redis-lease")]
    Redis(RedisAdvisoryLease),
}

// ── Public struct ─────────────────────────────────────────────────────────────

/// Checkpoint lease for coordinating distributed listeners.
///
/// Use [`CheckpointLease::in_process`] for single-process deployments and tests,
/// [`CheckpointLease::postgres`] for multi-process coordination without Redis,
/// or `CheckpointLease::redis` (requires the `redis-lease` feature) for multi-process
/// coordination backed by Redis.
pub struct CheckpointLease(LeaseKind);

impl CheckpointLease {
    /// Create an in-process lease suitable for testing or single-node deployments.
    pub fn in_process(listener_id: String, checkpoint_id: i64, lease_duration_ms: u64) -> Self {
        Self(LeaseKind::InProcess(InProcessLease::new(
            listener_id,
            checkpoint_id,
            lease_duration_ms,
        )))
    }

    /// Construct an in-process lease (back-compat alias for [`Self::in_process`]).
    pub fn new(listener_id: String, checkpoint_id: i64, lease_duration_ms: u64) -> Self {
        Self::in_process(listener_id, checkpoint_id, lease_duration_ms)
    }

    /// Create a distributed lease backed by a PostgreSQL session advisory lock.
    #[cfg(feature = "postgres")]
    pub fn postgres(pool: sqlx::PgPool, listener_id: String, checkpoint_id: i64) -> Self {
        Self(LeaseKind::Postgres(PostgresAdvisoryLease::new(
            pool,
            listener_id,
            checkpoint_id,
        )))
    }

    /// Create a distributed lease backed by Redis `SET NX EX`.
    #[cfg(feature = "redis-lease")]
    pub fn redis(
        conn: redis::aio::ConnectionManager,
        listener_id: String,
        checkpoint_id: i64,
        lease_duration_secs: u64,
    ) -> Self {
        Self(LeaseKind::Redis(RedisAdvisoryLease::new(
            conn,
            listener_id,
            checkpoint_id,
            lease_duration_secs,
        )))
    }

    /// Attempt to acquire the lease.
    ///
    /// Returns `true` when acquired (or already held), `false` when contested.
    ///
    /// # Errors
    ///
    /// Propagates errors from the underlying backend (`InProcess`, `Postgres`, or `Redis`).
    pub async fn acquire(&self) -> Result<bool> {
        match &self.0 {
            LeaseKind::InProcess(l) => l.acquire().await,
            #[cfg(feature = "postgres")]
            LeaseKind::Postgres(l) => l.acquire().await,
            #[cfg(feature = "redis-lease")]
            LeaseKind::Redis(l) => l.acquire().await,
        }
    }

    /// Release the lease.
    ///
    /// # Errors
    ///
    /// Propagates errors from the underlying backend (`InProcess`, `Postgres`, or `Redis`).
    pub async fn release(&self) -> Result<()> {
        match &self.0 {
            LeaseKind::InProcess(l) => l.release().await,
            #[cfg(feature = "postgres")]
            LeaseKind::Postgres(l) => l.release().await,
            #[cfg(feature = "redis-lease")]
            LeaseKind::Redis(l) => l.release().await,
        }
    }

    /// Renew (extend) the lease TTL.
    ///
    /// For Postgres advisory leases this is a no-op; returns `true` while held.
    ///
    /// # Errors
    ///
    /// Propagates errors from the underlying backend (`InProcess`, `Postgres`, or `Redis`).
    pub async fn renew(&self) -> Result<bool> {
        match &self.0 {
            LeaseKind::InProcess(l) => l.renew().await,
            #[cfg(feature = "postgres")]
            LeaseKind::Postgres(l) => l.renew().await,
            #[cfg(feature = "redis-lease")]
            LeaseKind::Redis(l) => l.renew().await,
        }
    }

    /// Returns `true` if the lease is currently held and (for timed backends) not expired.
    ///
    /// # Errors
    ///
    /// Propagates errors from the underlying backend (`InProcess`, `Postgres`, or `Redis`).
    pub async fn is_valid(&self) -> Result<bool> {
        match &self.0 {
            LeaseKind::InProcess(l) => l.is_valid().await,
            #[cfg(feature = "postgres")]
            LeaseKind::Postgres(l) => l.is_valid().await,
            #[cfg(feature = "redis-lease")]
            LeaseKind::Redis(l) => l.is_valid().await,
        }
    }

    /// Returns the listener ID that currently holds the lease, or `None`.
    ///
    /// # Errors
    ///
    /// Propagates errors from the underlying backend (`InProcess`, `Postgres`, or `Redis`).
    pub async fn get_holder(&self) -> Result<Option<String>> {
        match &self.0 {
            LeaseKind::InProcess(l) => l.get_holder().await,
            #[cfg(feature = "postgres")]
            LeaseKind::Postgres(l) => l.get_holder().await,
            #[cfg(feature = "redis-lease")]
            LeaseKind::Redis(l) => l.get_holder().await,
        }
    }

    /// Returns the remaining lease duration in milliseconds.
    ///
    /// For Postgres advisory leases returns `u64::MAX` (no expiry).
    ///
    /// # Errors
    ///
    /// Propagates errors from the underlying backend (`InProcess`, `Postgres`, or `Redis`).
    pub async fn time_remaining_ms(&self) -> Result<u64> {
        match &self.0 {
            LeaseKind::InProcess(l) => l.time_remaining_ms().await,
            #[cfg(feature = "postgres")]
            LeaseKind::Postgres(l) => l.time_remaining_ms().await,
            #[cfg(feature = "redis-lease")]
            LeaseKind::Redis(l) => l.time_remaining_ms().await,
        }
    }

    /// The checkpoint ID this lease guards.
    #[must_use]
    pub fn checkpoint_id(&self) -> i64 {
        match &self.0 {
            LeaseKind::InProcess(l) => l.checkpoint_id,
            #[cfg(feature = "postgres")]
            LeaseKind::Postgres(l) => l.checkpoint_id,
            #[cfg(feature = "redis-lease")]
            LeaseKind::Redis(l) => l.checkpoint_id,
        }
    }

    /// The listener ID that constructed this lease.
    #[must_use]
    pub fn listener_id(&self) -> &str {
        match &self.0 {
            LeaseKind::InProcess(l) => &l.listener_id,
            #[cfg(feature = "postgres")]
            LeaseKind::Postgres(l) => &l.listener_id,
            #[cfg(feature = "redis-lease")]
            LeaseKind::Redis(l) => &l.listener_id,
        }
    }
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[allow(clippy::unwrap_used)] // Reason: test code, panics are acceptable
#[cfg(test)]
mod tests {
    use super::*;

    // ── In-process lease ──────────────────────────────────────────────────

    #[tokio::test]
    async fn test_lease_acquisition() {
        let lease = CheckpointLease::in_process("listener-1".to_string(), 1000, 1000);

        let acquired = lease.acquire().await.unwrap();
        assert!(acquired);

        let holder = lease.get_holder().await.unwrap();
        assert_eq!(holder, Some("listener-1".to_string()));
    }

    #[tokio::test]
    async fn test_lease_release() {
        let lease = CheckpointLease::in_process("listener-1".to_string(), 1000, 1000);

        lease.acquire().await.unwrap();
        lease.release().await.unwrap();

        let holder = lease.get_holder().await.unwrap();
        assert_eq!(holder, None);
    }

    #[tokio::test]
    async fn test_lease_renewal() {
        let lease = CheckpointLease::in_process("listener-1".to_string(), 1000, 100);

        lease.acquire().await.unwrap();

        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
        let time_remaining_before = lease.time_remaining_ms().await.unwrap();

        lease.renew().await.unwrap();
        let time_remaining_after = lease.time_remaining_ms().await.unwrap();

        // After renewal, time remaining should be close to the original duration.
        assert!(time_remaining_after > time_remaining_before);
    }

    #[tokio::test]
    async fn test_lease_expiration() {
        let lease = CheckpointLease::in_process("listener-1".to_string(), 1000, 50);

        lease.acquire().await.unwrap();
        assert!(lease.is_valid().await.unwrap());

        tokio::time::sleep(std::time::Duration::from_millis(60)).await;
        assert!(!lease.is_valid().await.unwrap());
    }

    #[tokio::test]
    async fn test_lease_contested_acquisition() {
        let lease1 = CheckpointLease::in_process("listener-1".to_string(), 1000, 1000);
        let lease2 = CheckpointLease::in_process("listener-2".to_string(), 1000, 1000);

        assert!(lease1.acquire().await.unwrap());
        // Different in-process instances have separate state; each acquires independently.
        assert!(lease2.acquire().await.unwrap());

        assert_eq!(lease1.get_holder().await.unwrap(), Some("listener-1".to_string()));
        assert_eq!(lease2.get_holder().await.unwrap(), Some("listener-2".to_string()));
    }

    #[tokio::test]
    async fn test_lease_multiple_listeners() {
        let leases: Vec<_> = (0..3)
            .map(|i| {
                CheckpointLease::in_process(format!("listener-{i}"), 1000 + i64::from(i), 5000)
            })
            .collect();

        for lease in &leases {
            assert!(lease.acquire().await.unwrap());
        }

        for (i, lease) in leases.iter().enumerate() {
            assert_eq!(lease.get_holder().await.unwrap(), Some(format!("listener-{i}")));
        }
    }

    #[tokio::test]
    async fn test_lease_time_remaining() {
        let lease = CheckpointLease::in_process("listener-1".to_string(), 1000, 200);

        let initial_remaining = lease.time_remaining_ms().await.unwrap();
        assert_eq!(initial_remaining, 200);

        lease.acquire().await.unwrap();

        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
        let remaining_after_50ms = lease.time_remaining_ms().await.unwrap();

        assert!(remaining_after_50ms < 200);
        assert!(remaining_after_50ms >= 100);
    }

    #[tokio::test]
    async fn test_lease_idempotent_acquire() {
        let lease = CheckpointLease::in_process("listener-1".to_string(), 42, 5000);

        assert!(lease.acquire().await.unwrap());
        // Second acquire while still held by us → still true.
        assert!(lease.acquire().await.unwrap());
    }

    #[tokio::test]
    async fn test_checkpoint_id_accessor() {
        let lease = CheckpointLease::in_process("l".to_string(), 999, 1000);
        assert_eq!(lease.checkpoint_id(), 999);
    }

    #[tokio::test]
    async fn test_listener_id_accessor() {
        let lease = CheckpointLease::in_process("my-listener".to_string(), 1, 1000);
        assert_eq!(lease.listener_id(), "my-listener");
    }

    // ── PostgreSQL advisory lease (integration, requires real PG) ─────────

    #[cfg(feature = "postgres")]
    #[tokio::test]
    async fn test_postgres_advisory_acquire_release() {
        let Ok(url) = std::env::var("DATABASE_URL") else {
            eprintln!("Skipping: DATABASE_URL not set");
            return;
        };
        let pool = sqlx::PgPool::connect(&url).await.unwrap();

        let lease = CheckpointLease::postgres(pool, "pg-listener-1".to_string(), 99_999);

        assert!(lease.acquire().await.unwrap(), "first acquire should succeed");
        // Idempotent.
        assert!(lease.acquire().await.unwrap(), "second acquire should also succeed");
        assert!(lease.is_valid().await.unwrap());
        assert_eq!(lease.time_remaining_ms().await.unwrap(), u64::MAX);
        assert_eq!(lease.get_holder().await.unwrap(), Some("pg-listener-1".to_string()));

        lease.release().await.unwrap();
        assert!(!lease.is_valid().await.unwrap());
    }

    #[cfg(feature = "postgres")]
    #[tokio::test]
    async fn test_postgres_advisory_contention() {
        let Ok(url) = std::env::var("DATABASE_URL") else {
            eprintln!("Skipping: DATABASE_URL not set");
            return;
        };
        let pool = sqlx::PgPool::connect(&url).await.unwrap();

        let lease_a = CheckpointLease::postgres(pool.clone(), "pg-a".to_string(), 88_888);
        let lease_b = CheckpointLease::postgres(pool.clone(), "pg-b".to_string(), 88_888);

        // A acquires.
        assert!(lease_a.acquire().await.unwrap());
        // B cannot acquire the same key while A holds it.
        assert!(!lease_b.acquire().await.unwrap());

        lease_a.release().await.unwrap();
        // Now B can acquire.
        assert!(lease_b.acquire().await.unwrap());
        lease_b.release().await.unwrap();
    }

    // ── Redis advisory lease (integration, requires real Redis) ───────────

    #[cfg(feature = "redis-lease")]
    #[tokio::test]
    async fn test_redis_advisory_acquire_release() {
        let Ok(url) = std::env::var("REDIS_URL") else {
            eprintln!("Skipping: REDIS_URL not set");
            return;
        };
        let client = redis::Client::open(url).unwrap();
        let conn = redis::aio::ConnectionManager::new(client).await.unwrap();

        let lease = CheckpointLease::redis(conn, "redis-listener-1".to_string(), 77_777, 30);

        assert!(lease.acquire().await.unwrap());
        assert!(lease.is_valid().await.unwrap());
        assert_eq!(lease.get_holder().await.unwrap(), Some("redis-listener-1".to_string()));

        assert!(lease.renew().await.unwrap());
        let remaining = lease.time_remaining_ms().await.unwrap();
        assert!(remaining > 0 && remaining <= 30_000);

        lease.release().await.unwrap();
        assert!(!lease.is_valid().await.unwrap());
    }

    #[cfg(feature = "redis-lease")]
    #[tokio::test]
    async fn test_redis_advisory_contention() {
        let Ok(url) = std::env::var("REDIS_URL") else {
            eprintln!("Skipping: REDIS_URL not set");
            return;
        };
        let client = redis::Client::open(url).unwrap();
        let conn_a = redis::aio::ConnectionManager::new(client.clone()).await.unwrap();
        let conn_b = redis::aio::ConnectionManager::new(client).await.unwrap();

        let lease_a = CheckpointLease::redis(conn_a, "redis-a".to_string(), 66_666, 30);
        let lease_b = CheckpointLease::redis(conn_b, "redis-b".to_string(), 66_666, 30);

        assert!(lease_a.acquire().await.unwrap());
        assert!(!lease_b.acquire().await.unwrap(), "should be contested");

        lease_a.release().await.unwrap();
        assert!(lease_b.acquire().await.unwrap(), "should acquire after A released");
        lease_b.release().await.unwrap();
    }
}
