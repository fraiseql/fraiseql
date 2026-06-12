//! Token revocation — reject JWTs whose `jti` claim has been revoked.
//!
//! After JWT signature verification succeeds, the server checks the token's
//! `jti` (JWT ID) claim against a revocation store.  If the `jti` is present,
//! the token is rejected with 401.
//!
//! Two production backends: Redis (recommended) and PostgreSQL (fallback).
//! An in-memory backend is provided for testing and single-instance dev.
//!
//! Revoked JTIs expire automatically when the JWT's `exp` claim passes, keeping
//! the store bounded.

#[cfg(test)]
mod tests;

use std::sync::Arc;

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use dashmap::DashMap;
use serde::Deserialize;
use tracing::{debug, info, warn};

// ───────────────────────────────────────────────────────────────
// Configuration
// ───────────────────────────────────────────────────────────────

/// Token revocation configuration embedded in the compiled schema.
#[derive(Debug, Clone, Deserialize)]
pub struct TokenRevocationConfig {
    /// Whether token revocation is enabled.
    #[serde(default)]
    pub enabled: bool,

    /// Storage backend: `"redis"` or `"postgres"` or `"memory"`.
    #[serde(default = "default_backend")]
    pub backend: String,

    /// Reject JWTs that lack a `jti` claim when revocation is enabled.
    #[serde(default = "default_true")]
    pub require_jti: bool,

    /// If the revocation store is unreachable:
    /// - `false` (default): reject the request (fail-closed)
    /// - `true`: allow the request (fail-open)
    #[serde(default)]
    pub fail_open: bool,

    /// Redis URL (inherited from `[fraiseql.redis]` if not set here).
    pub redis_url: Option<String>,

    /// How long (seconds) a `revoke-all` epoch is retained.
    ///
    /// `revoke-all` records a per-user epoch (see [`RevocationStore::revoke_all_for_user`])
    /// rather than deleting individual tokens, so the entry must outlive every token that
    /// could have been issued before the revocation. Set this **above your maximum
    /// access-token lifetime**; once it expires a pre-revocation token would resume
    /// working (until its own `exp`). Default: 86400 (24h).
    #[serde(default = "default_revoke_all_ttl")]
    pub revoke_all_ttl_secs: u64,
}

fn default_backend() -> String {
    "memory".into()
}
const fn default_true() -> bool {
    true
}
const fn default_revoke_all_ttl() -> u64 {
    86_400
}

// ───────────────────────────────────────────────────────────────
// Trait
// ───────────────────────────────────────────────────────────────

/// Revocation store abstraction.
// Reason: used as dyn Trait (Arc<dyn RevocationStore>); async_trait ensures Send bounds and
// dyn-compatibility async_trait: dyn-dispatch required; remove when RTN + Send is stable (RFC 3425)
#[async_trait]
pub trait RevocationStore: Send + Sync {
    /// Check if a JTI has been revoked.
    async fn is_revoked(&self, jti: &str) -> Result<bool, RevocationError>;

    /// Revoke a single JTI.  `ttl_secs` is the remaining JWT lifetime —
    /// the store should auto-expire the entry after this duration.
    async fn revoke(&self, jti: &str, ttl_secs: u64) -> Result<(), RevocationError>;

    /// Revoke **all** of a user's tokens by recording a per-user *epoch*: every token
    /// for `sub` whose `iat` (issued-at) is at or before now is henceforth rejected by
    /// [`user_revoked_after`](Self::user_revoked_after).
    ///
    /// This is an epoch, not a row delete — it catches tokens that were never
    /// individually revoked (and tokens with no `jti`), which the previous sub-keyed
    /// delete could not. `ttl_secs` bounds how long the epoch is retained; it must
    /// exceed the maximum access-token lifetime so no pre-revocation token outlives it.
    async fn revoke_all_for_user(&self, sub: &str, ttl_secs: u64) -> Result<(), RevocationError>;

    /// Return the `revoke-all` epoch (unix seconds) currently in effect for `sub`, or
    /// `None` when the user has no active epoch. Tokens with `iat <= epoch` are revoked.
    async fn user_revoked_after(&self, sub: &str) -> Result<Option<i64>, RevocationError>;
}

/// Revocation store error.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum RevocationError {
    /// Backend is unreachable or returned an error.
    #[error("revocation store error: {0}")]
    Backend(String),
}

// ───────────────────────────────────────────────────────────────
// In-memory backend
// ───────────────────────────────────────────────────────────────

/// In-memory revocation store for testing and single-instance dev.
pub struct InMemoryRevocationStore {
    /// Map of JTI → (sub, `expires_at`).
    pub(crate) entries:     DashMap<String, (String, DateTime<Utc>)>,
    /// Per-user `revoke-all` epochs: `sub` → (`revoked_after` unix seconds, entry expiry).
    pub(crate) user_epochs: DashMap<String, (i64, DateTime<Utc>)>,
}

impl InMemoryRevocationStore {
    /// Create a new, empty in-memory revocation store.
    #[must_use]
    pub fn new() -> Self {
        Self {
            entries:     DashMap::new(),
            user_epochs: DashMap::new(),
        }
    }

    /// Remove expired entries (single-JTI revocations and per-user epochs).
    pub fn cleanup_expired(&self) {
        let now = Utc::now();
        self.entries.retain(|_, (_, exp)| *exp > now);
        self.user_epochs.retain(|_, (_, exp)| *exp > now);
    }
}

impl Default for InMemoryRevocationStore {
    fn default() -> Self {
        Self::new()
    }
}

// Reason: RevocationStore is defined with #[async_trait]; all implementations must match
// its transformed method signatures to satisfy the trait contract
// async_trait: dyn-dispatch required; remove when RTN + Send is stable (RFC 3425)
#[async_trait]
impl RevocationStore for InMemoryRevocationStore {
    async fn is_revoked(&self, jti: &str) -> Result<bool, RevocationError> {
        if let Some(entry) = self.entries.get(jti) {
            let (_, expires_at) = entry.value();
            if *expires_at > Utc::now() {
                return Ok(true);
            }
            // Expired — remove lazily.
            drop(entry);
            self.entries.remove(jti);
        }
        Ok(false)
    }

    async fn revoke(&self, jti: &str, ttl_secs: u64) -> Result<(), RevocationError> {
        let expires_at = Utc::now() + chrono::Duration::seconds(ttl_secs.cast_signed());
        // We store an empty sub — single-JTI revocation doesn't need sub.
        self.entries.insert(jti.to_string(), (String::new(), expires_at));
        Ok(())
    }

    async fn revoke_all_for_user(&self, sub: &str, ttl_secs: u64) -> Result<(), RevocationError> {
        // Record a per-user epoch: every token for `sub` with iat <= now is revoked.
        // This catches tokens that were never individually revoked (and tokens with no
        // jti) — unlike the previous sub-keyed delete, which only ever matched the empty
        // sub written by `revoke`, so it removed nothing.
        let now = Utc::now();
        let expires_at = now + chrono::Duration::seconds(ttl_secs.cast_signed());
        self.user_epochs.insert(sub.to_string(), (now.timestamp(), expires_at));
        Ok(())
    }

    async fn user_revoked_after(&self, sub: &str) -> Result<Option<i64>, RevocationError> {
        if let Some(entry) = self.user_epochs.get(sub) {
            let (revoked_after, expires_at) = *entry.value();
            if expires_at > Utc::now() {
                return Ok(Some(revoked_after));
            }
            // Expired — remove lazily.
            drop(entry);
            self.user_epochs.remove(sub);
        }
        Ok(None)
    }
}

// ───────────────────────────────────────────────────────────────
// Redis backend (optional)
// ───────────────────────────────────────────────────────────────

/// Redis-backed JWT revocation store.
///
/// Stores revoked JTI claims in Redis with automatic TTL-based expiry.
/// Requires the `redis-rate-limiting` feature.
#[cfg(feature = "redis-rate-limiting")]
pub struct RedisRevocationStore {
    client:     redis::Client,
    key_prefix: String,
}

#[cfg(feature = "redis-rate-limiting")]
impl RedisRevocationStore {
    /// Create a new Redis-backed revocation store.
    ///
    /// # Errors
    ///
    /// Returns error if the Redis URL is invalid.
    pub fn new(redis_url: &str) -> Result<Self, RevocationError> {
        let client = redis::Client::open(redis_url)
            .map_err(|e| RevocationError::Backend(format!("Redis connection error: {e}")))?;
        Ok(Self {
            client,
            key_prefix: "fraiseql:revoked:".into(),
        })
    }
}

#[cfg(feature = "redis-rate-limiting")]
// Reason: RevocationStore is defined with #[async_trait]; all implementations must match
// its transformed method signatures to satisfy the trait contract
// async_trait: dyn-dispatch required; remove when RTN + Send is stable (RFC 3425)
#[async_trait]
impl RevocationStore for RedisRevocationStore {
    async fn is_revoked(&self, jti: &str) -> Result<bool, RevocationError> {
        use redis::AsyncCommands;
        let mut conn = self
            .client
            .get_multiplexed_async_connection()
            .await
            .map_err(|e| RevocationError::Backend(format!("Redis: {e}")))?;
        let key = format!("{}{jti}", self.key_prefix);
        let exists: bool = conn
            .exists(&key)
            .await
            .map_err(|e| RevocationError::Backend(format!("Redis EXISTS: {e}")))?;
        Ok(exists)
    }

    async fn revoke(&self, jti: &str, ttl_secs: u64) -> Result<(), RevocationError> {
        use redis::AsyncCommands;
        let mut conn = self
            .client
            .get_multiplexed_async_connection()
            .await
            .map_err(|e| RevocationError::Backend(format!("Redis: {e}")))?;
        let key = format!("{}{jti}", self.key_prefix);
        let _: () = conn
            .set_ex(&key, "1", ttl_secs)
            .await
            .map_err(|e| RevocationError::Backend(format!("Redis SET EX: {e}")))?;
        Ok(())
    }

    async fn revoke_all_for_user(&self, sub: &str, ttl_secs: u64) -> Result<(), RevocationError> {
        use redis::AsyncCommands;
        let mut conn = self
            .client
            .get_multiplexed_async_connection()
            .await
            .map_err(|e| RevocationError::Backend(format!("Redis: {e}")))?;
        // Record a per-user epoch under a single key (`…:user:{sub}`) with TTL. The old
        // implementation SCANned `…:user:{sub}:*`, a namespace `revoke` never wrote, so it
        // always matched nothing. The epoch is checked against each token's `iat`.
        let key = format!("{}user:{sub}", self.key_prefix);
        let now = Utc::now().timestamp();
        let _: () = conn
            .set_ex(&key, now, ttl_secs)
            .await
            .map_err(|e| RevocationError::Backend(format!("Redis SET EX: {e}")))?;
        Ok(())
    }

    async fn user_revoked_after(&self, sub: &str) -> Result<Option<i64>, RevocationError> {
        use redis::AsyncCommands;
        let mut conn = self
            .client
            .get_multiplexed_async_connection()
            .await
            .map_err(|e| RevocationError::Backend(format!("Redis: {e}")))?;
        let key = format!("{}user:{sub}", self.key_prefix);
        // Redis auto-expires the key after its TTL, so a present value is always live.
        let epoch: Option<i64> = conn
            .get(&key)
            .await
            .map_err(|e| RevocationError::Backend(format!("Redis GET: {e}")))?;
        Ok(epoch)
    }
}

// ───────────────────────────────────────────────────────────────
// PostgreSQL backend
// ───────────────────────────────────────────────────────────────

/// Maximum size of the dedicated pool used for token-revocation metadata.
/// Revocation is metadata-light (one row per revoked token), so a small pool is
/// sufficient and keeps startup cheap.
const REVOCATION_POOL_MAX: u32 = 5;

/// Idempotent DDL for the PostgreSQL revocation store.
///
/// `fraiseql_revoked_tokens` holds single-JTI revocations; `fraiseql_revoked_users`
/// holds per-user `revoke-all` epochs (`revoked_after` unix seconds, retained until
/// `expires_at`).
const REVOKED_TOKENS_SCHEMA_SQL: &str = "\
CREATE TABLE IF NOT EXISTS fraiseql_revoked_tokens (
    jti TEXT PRIMARY KEY,
    sub TEXT,
    expires_at TIMESTAMPTZ NOT NULL
);
CREATE INDEX IF NOT EXISTS idx_fraiseql_revoked_tokens_sub
    ON fraiseql_revoked_tokens (sub);
CREATE INDEX IF NOT EXISTS idx_fraiseql_revoked_tokens_expires
    ON fraiseql_revoked_tokens (expires_at);
CREATE TABLE IF NOT EXISTS fraiseql_revoked_users (
    sub TEXT PRIMARY KEY,
    revoked_after BIGINT NOT NULL,
    expires_at TIMESTAMPTZ NOT NULL
);
CREATE INDEX IF NOT EXISTS idx_fraiseql_revoked_users_expires
    ON fraiseql_revoked_users (expires_at);";

/// PostgreSQL-backed JWT revocation store.
///
/// Persists revoked `jti` claims in `fraiseql_revoked_tokens`, so revocations
/// survive a restart and are shared across replicas — unlike the in-memory
/// backend, which the server silently fell back to for `backend = "postgres"`
/// before this was implemented (#357). Each row carries an `expires_at` matching
/// the JWT's remaining lifetime; `is_revoked` ignores expired rows and
/// [`cleanup_expired`](Self::cleanup_expired) prunes them.
pub struct PostgresRevocationStore {
    pool: sqlx::PgPool,
}

impl PostgresRevocationStore {
    /// Create a Postgres revocation store, ensuring the backing table exists
    /// (idempotent DDL).
    ///
    /// # Errors
    ///
    /// Returns [`RevocationError::Backend`] if the schema cannot be created.
    pub async fn new(pool: sqlx::PgPool) -> Result<Self, RevocationError> {
        sqlx::raw_sql(REVOKED_TOKENS_SCHEMA_SQL)
            .execute(&pool)
            .await
            .map_err(|e| RevocationError::Backend(format!("schema creation failed: {e}")))?;
        Ok(Self { pool })
    }

    /// Delete expired revocation rows. Optional housekeeping; `is_revoked` already
    /// ignores expired entries, so this only reclaims space.
    ///
    /// # Errors
    ///
    /// Returns [`RevocationError::Backend`] if the delete fails.
    pub async fn cleanup_expired(&self) -> Result<u64, RevocationError> {
        let tokens = sqlx::query("DELETE FROM fraiseql_revoked_tokens WHERE expires_at <= NOW()")
            .execute(&self.pool)
            .await
            .map_err(|e| RevocationError::Backend(format!("cleanup failed: {e}")))?;
        let users = sqlx::query("DELETE FROM fraiseql_revoked_users WHERE expires_at <= NOW()")
            .execute(&self.pool)
            .await
            .map_err(|e| RevocationError::Backend(format!("cleanup failed: {e}")))?;
        Ok(tokens.rows_affected() + users.rows_affected())
    }
}

// Reason: RevocationStore is defined with #[async_trait]; all implementations must match
// its transformed method signatures to satisfy the trait contract
// async_trait: dyn-dispatch required; remove when RTN + Send is stable (RFC 3425)
#[async_trait]
impl RevocationStore for PostgresRevocationStore {
    async fn is_revoked(&self, jti: &str) -> Result<bool, RevocationError> {
        let revoked: bool = sqlx::query_scalar(
            "SELECT EXISTS (
                 SELECT 1 FROM fraiseql_revoked_tokens WHERE jti = $1 AND expires_at > NOW()
             )",
        )
        .bind(jti)
        .fetch_one(&self.pool)
        .await
        .map_err(|e| RevocationError::Backend(format!("is_revoked query failed: {e}")))?;
        Ok(revoked)
    }

    async fn revoke(&self, jti: &str, ttl_secs: u64) -> Result<(), RevocationError> {
        let expires_at = Utc::now() + chrono::Duration::seconds(ttl_secs.cast_signed());
        // Single-JTI revocation does not carry a `sub` (the trait signature has none);
        // it is recorded NULL, matching the in-memory backend.
        sqlx::query(
            "INSERT INTO fraiseql_revoked_tokens (jti, sub, expires_at)
             VALUES ($1, NULL, $2)
             ON CONFLICT (jti) DO UPDATE SET expires_at = EXCLUDED.expires_at",
        )
        .bind(jti)
        .bind(expires_at)
        .execute(&self.pool)
        .await
        .map_err(|e| RevocationError::Backend(format!("revoke insert failed: {e}")))?;
        Ok(())
    }

    async fn revoke_all_for_user(&self, sub: &str, ttl_secs: u64) -> Result<(), RevocationError> {
        // Upsert a per-user epoch. The previous `DELETE … WHERE sub = $1` removed nothing
        // because `revoke` records sub = NULL; this records "all of sub's tokens issued at
        // or before now are revoked", checked against each token's `iat`.
        let expires_at = Utc::now() + chrono::Duration::seconds(ttl_secs.cast_signed());
        sqlx::query(
            "INSERT INTO fraiseql_revoked_users (sub, revoked_after, expires_at)
             VALUES ($1, EXTRACT(EPOCH FROM NOW())::BIGINT, $2)
             ON CONFLICT (sub) DO UPDATE
                 SET revoked_after = EXTRACT(EPOCH FROM NOW())::BIGINT,
                     expires_at = EXCLUDED.expires_at",
        )
        .bind(sub)
        .bind(expires_at)
        .execute(&self.pool)
        .await
        .map_err(|e| RevocationError::Backend(format!("revoke_all_for_user failed: {e}")))?;
        Ok(())
    }

    async fn user_revoked_after(&self, sub: &str) -> Result<Option<i64>, RevocationError> {
        let epoch: Option<i64> = sqlx::query_scalar(
            "SELECT revoked_after FROM fraiseql_revoked_users
             WHERE sub = $1 AND expires_at > NOW()",
        )
        .bind(sub)
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| RevocationError::Backend(format!("user_revoked_after query failed: {e}")))?;
        Ok(epoch)
    }
}

// ───────────────────────────────────────────────────────────────
// Token Revocation Manager
// ───────────────────────────────────────────────────────────────

/// High-level token revocation manager wrapping a backend store.
pub struct TokenRevocationManager {
    store:               Arc<dyn RevocationStore>,
    require_jti:         bool,
    fail_open:           bool,
    revoke_all_ttl_secs: u64,
}

impl TokenRevocationManager {
    /// Create a new revocation manager.
    ///
    /// `revoke_all_ttl_secs` is how long a `revoke-all` epoch is retained (see
    /// [`TokenRevocationConfig::revoke_all_ttl_secs`]).
    #[must_use]
    pub fn new(
        store: Arc<dyn RevocationStore>,
        require_jti: bool,
        fail_open: bool,
        revoke_all_ttl_secs: u64,
    ) -> Self {
        Self {
            store,
            require_jti,
            fail_open,
            revoke_all_ttl_secs,
        }
    }

    /// Check if a token should be rejected, by single-JTI revocation **and** by the
    /// caller's `revoke-all` epoch.
    ///
    /// `jti`/`iat` are the token's claims; `sub` is the subject. The single-JTI check
    /// uses `jti`; the epoch check rejects when the user has an active `revoke-all` epoch
    /// and the token's `iat` is at or before it. A token with no `iat` cannot be
    /// epoch-checked, so the epoch is skipped for it (it can still be revoked by `jti`).
    ///
    /// Returns `Ok(())` if the token is allowed, or an error reason if rejected.
    ///
    /// # Errors
    ///
    /// Returns `TokenRejection::MissingJti` if JTI is required but absent.
    /// Returns `TokenRejection::Revoked` if the token's `jti` is revoked or its `iat`
    /// predates the user's `revoke-all` epoch.
    /// Returns `TokenRejection::StoreUnavailable` if the revocation store is unreachable and
    /// `fail_open` is false.
    pub async fn check_token(
        &self,
        jti: Option<&str>,
        sub: &str,
        iat: Option<i64>,
    ) -> Result<(), TokenRejection> {
        // 1. Single-JTI revocation.
        match jti {
            Some(j) if !j.is_empty() => match self.store.is_revoked(j).await {
                Ok(true) => return Err(TokenRejection::Revoked),
                Ok(false) => {},
                Err(e) => {
                    warn!(error = %e, jti = %j, "Revocation store check failed");
                    if self.fail_open {
                        debug!("fail_open=true — allowing request despite store error");
                        return Ok(());
                    }
                    return Err(TokenRejection::StoreUnavailable);
                },
            },
            _ => {
                if self.require_jti {
                    return Err(TokenRejection::MissingJti);
                }
                // No JTI and not required — fall through to the epoch check.
            },
        }

        // 2. Per-user `revoke-all` epoch (catches tokens never individually revoked).
        match self.store.user_revoked_after(sub).await {
            Ok(Some(epoch)) => {
                if iat.is_some_and(|issued| issued <= epoch) {
                    return Err(TokenRejection::Revoked);
                }
                Ok(())
            },
            Ok(None) => Ok(()),
            Err(e) => {
                warn!(error = %e, sub = %sub, "Revoke-all epoch check failed");
                if self.fail_open {
                    debug!("fail_open=true — allowing request despite store error");
                    Ok(())
                } else {
                    Err(TokenRejection::StoreUnavailable)
                }
            },
        }
    }

    /// Revoke a single token by JTI.
    ///
    /// # Errors
    ///
    /// Returns `RevocationError` if the underlying revocation store operation fails.
    pub async fn revoke(&self, jti: &str, ttl_secs: u64) -> Result<(), RevocationError> {
        self.store.revoke(jti, ttl_secs).await
    }

    /// Revoke all of a user's tokens by recording a `revoke-all` epoch retained for
    /// the manager's configured `revoke_all_ttl_secs` (see
    /// [`RevocationStore::revoke_all_for_user`]).
    ///
    /// # Errors
    ///
    /// Returns `RevocationError` if the underlying revocation store operation fails.
    pub async fn revoke_all_for_user(&self, sub: &str) -> Result<(), RevocationError> {
        self.store.revoke_all_for_user(sub, self.revoke_all_ttl_secs).await
    }

    /// Return the `revoke-all` epoch currently in effect for `sub`, if any.
    ///
    /// # Errors
    ///
    /// Returns `RevocationError` if the underlying revocation store operation fails.
    pub async fn user_revoked_after(&self, sub: &str) -> Result<Option<i64>, RevocationError> {
        self.store.user_revoked_after(sub).await
    }

    /// Whether JTI is required.
    #[must_use]
    pub const fn require_jti(&self) -> bool {
        self.require_jti
    }
}

impl std::fmt::Debug for TokenRevocationManager {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("TokenRevocationManager")
            .field("require_jti", &self.require_jti)
            .field("fail_open", &self.fail_open)
            .finish_non_exhaustive()
    }
}

/// Why a token was rejected.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum TokenRejection {
    /// Token has been revoked.
    Revoked,
    /// Token lacks a `jti` claim and `require_jti` is enabled.
    MissingJti,
    /// Revocation store is unavailable and `fail_open` is false.
    StoreUnavailable,
}

// ───────────────────────────────────────────────────────────────
// Builder from compiled schema
// ───────────────────────────────────────────────────────────────

/// Build a `TokenRevocationManager` for the DB-agnostic backends (`memory`, `redis`)
/// from the compiled schema's `security.token_revocation` JSON.
///
/// The `postgres` backend is **deferred** here (returns `Ok(None)`) because it needs
/// a database connection; it is provisioned by [`build_postgres_revocation_manager`]
/// on the PostgreSQL runtime path and installed via `Server::with_revocation_manager`.
///
/// # Errors
///
/// Returns `ServerError::ConfigError` when the `token_revocation` JSON cannot be
/// parsed, or when `backend` is an unrecognised value — previously an unknown
/// backend silently fell back to in-memory, defeating the operator's intent (#357).
pub fn revocation_manager_from_schema(
    schema: &fraiseql_core::schema::CompiledSchema,
) -> crate::Result<Option<Arc<TokenRevocationManager>>> {
    let Some(security) = schema.security.as_ref() else {
        return Ok(None);
    };
    let Some(revocation_val) = security.additional.get("token_revocation") else {
        return Ok(None);
    };
    // The CLI compiler serialises an absent `[security.token_revocation]` as JSON `null`,
    // so a null value means "not configured" — treat it like an absent key rather than a
    // malformed config. A non-null value that fails to parse IS a genuine misconfig.
    if revocation_val.is_null() {
        return Ok(None);
    }
    let config: TokenRevocationConfig =
        serde_json::from_value(revocation_val.clone()).map_err(|e| {
            crate::ServerError::ConfigError(format!(
                "invalid security.token_revocation config: {e}"
            ))
        })?;

    if !config.enabled {
        return Ok(None);
    }

    let store: Arc<dyn RevocationStore> = match config.backend.as_str() {
        #[cfg(feature = "redis-rate-limiting")]
        "redis" => {
            let url = config.redis_url.as_deref().unwrap_or("redis://localhost:6379");
            match RedisRevocationStore::new(url) {
                Ok(s) => {
                    info!(backend = "redis", "Token revocation store initialized");
                    Arc::new(s)
                },
                Err(e) => {
                    warn!(error = %e, "Failed to init Redis revocation store — falling back to in-memory");
                    Arc::new(InMemoryRevocationStore::new())
                },
            }
        },
        #[cfg(not(feature = "redis-rate-limiting"))]
        "redis" => {
            warn!(
                "token_revocation.backend = \"redis\" but the `redis-rate-limiting` feature is \
                 not compiled in. Falling back to in-memory."
            );
            Arc::new(InMemoryRevocationStore::new())
        },
        "memory" | "env" => {
            info!(backend = "memory", "Token revocation store initialized (in-memory)");
            Arc::new(InMemoryRevocationStore::new())
        },
        "postgres" => {
            // Needs a database connection — provisioned by the PostgreSQL runtime path
            // (build_postgres_revocation_manager) and installed via with_revocation_manager.
            info!(
                backend = "postgres",
                "Token revocation backend = postgres; provisioned by the PostgreSQL runtime"
            );
            return Ok(None);
        },
        other => {
            return Err(crate::ServerError::ConfigError(format!(
                "unknown token_revocation backend {other:?}; \
                 expected \"memory\", \"redis\", or \"postgres\""
            )));
        },
    };

    Ok(Some(Arc::new(TokenRevocationManager::new(
        store,
        config.require_jti,
        config.fail_open,
        config.revoke_all_ttl_secs,
    ))))
}

/// Build a PostgreSQL-backed `TokenRevocationManager` from the compiled schema's
/// `security.token_revocation` config, connecting a dedicated metadata pool from
/// `database_url`.
///
/// Returns `Ok(None)` when token revocation is disabled or the backend is not
/// `"postgres"` (the `memory`/`redis` backends are built on the generic construction
/// path by [`revocation_manager_from_schema`]). Call this on the PostgreSQL runtime
/// path and install the result with `Server::with_revocation_manager`.
///
/// # Errors
///
/// Returns an error message when the `token_revocation` config is invalid, the
/// database cannot be reached, or the backing table cannot be created.
pub async fn build_postgres_revocation_manager(
    database_url: &str,
    schema: &fraiseql_core::schema::CompiledSchema,
) -> std::result::Result<Option<Arc<TokenRevocationManager>>, String> {
    let Some(security) = schema.security.as_ref() else {
        return Ok(None);
    };
    let Some(revocation_val) = security.additional.get("token_revocation") else {
        return Ok(None);
    };
    // A null value means the section is absent (see revocation_manager_from_schema).
    if revocation_val.is_null() {
        return Ok(None);
    }
    let config: TokenRevocationConfig = serde_json::from_value(revocation_val.clone())
        .map_err(|e| format!("invalid security.token_revocation config: {e}"))?;

    if !config.enabled || config.backend != "postgres" {
        return Ok(None);
    }

    let pool = sqlx::postgres::PgPoolOptions::new()
        .max_connections(REVOCATION_POOL_MAX)
        .connect(database_url)
        .await
        .map_err(|e| format!("token revocation: failed to connect to PostgreSQL: {e}"))?;

    let store = PostgresRevocationStore::new(pool)
        .await
        .map_err(|e| format!("token revocation: {e}"))?;

    info!(backend = "postgres", "Token revocation store initialized (PostgreSQL)");
    Ok(Some(Arc::new(TokenRevocationManager::new(
        Arc::new(store),
        config.require_jti,
        config.fail_open,
        config.revoke_all_ttl_secs,
    ))))
}

/// Returns `true` when token revocation is enabled with the `postgres` backend.
///
/// Non-PostgreSQL runtime paths use this to warn that the backend is unavailable:
/// the binary cannot connect a PostgreSQL pool from, e.g., a MySQL `database_url`,
/// so `revocation_manager_from_schema` defers the backend and nothing builds it.
#[must_use]
pub fn revocation_backend_is_postgres(schema: &fraiseql_core::schema::CompiledSchema) -> bool {
    schema
        .security
        .as_ref()
        .and_then(|s| s.additional.get("token_revocation"))
        .and_then(|v| serde_json::from_value::<TokenRevocationConfig>(v.clone()).ok())
        .is_some_and(|c| c.enabled && c.backend == "postgres")
}
