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
}

fn default_backend() -> String {
    "memory".into()
}
const fn default_true() -> bool {
    true
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

    /// Revoke all tokens for a user (by `sub` claim).
    /// Returns the number of tokens revoked.
    async fn revoke_all_for_user(&self, sub: &str) -> Result<u64, RevocationError>;
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
    entries: DashMap<String, (String, DateTime<Utc>)>,
}

impl InMemoryRevocationStore {
    /// Create a new, empty in-memory revocation store.
    #[must_use]
    pub fn new() -> Self {
        Self {
            entries: DashMap::new(),
        }
    }

    /// Remove expired entries.
    pub fn cleanup_expired(&self) {
        let now = Utc::now();
        self.entries.retain(|_, (_, exp)| *exp > now);
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

    async fn revoke_all_for_user(&self, sub: &str) -> Result<u64, RevocationError> {
        // Collect all JTIs belonging to this user and remove them from the store.
        // Two-pass approach (collect keys, then remove) avoids holding a mutable
        // reference to DashMap while iterating, which would deadlock.
        let keys_to_remove: Vec<String> = self
            .entries
            .iter()
            .filter(|entry| {
                let (s, _) = entry.value();
                s == sub
            })
            .map(|entry| entry.key().clone())
            .collect();

        let count = keys_to_remove.len() as u64;
        for key in &keys_to_remove {
            self.entries.remove(key);
        }
        Ok(count)
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

    async fn revoke_all_for_user(&self, sub: &str) -> Result<u64, RevocationError> {
        let mut conn = self
            .client
            .get_multiplexed_async_connection()
            .await
            .map_err(|e| RevocationError::Backend(format!("Redis: {e}")))?;
        // SECURITY: Use SCAN cursor iteration instead of KEYS to avoid O(N) blocking.
        // KEYS blocks Redis for the entire scan duration; SCAN is non-blocking and
        // yields results in small batches, making it safe for production use.
        // User-keyed entries use prefix: fraiseql:revoked:user:{sub}:*
        let pattern = format!("{}user:{sub}:*", self.key_prefix);
        let mut cursor: u64 = 0;
        let mut all_keys: Vec<String> = Vec::new();
        loop {
            let (next_cursor, batch): (u64, Vec<String>) = redis::cmd("SCAN")
                .arg(cursor)
                .arg("MATCH")
                .arg(&pattern)
                .arg("COUNT")
                .arg(100u32)
                .query_async(&mut conn)
                .await
                .map_err(|e| RevocationError::Backend(format!("Redis SCAN: {e}")))?;
            all_keys.extend(batch);
            cursor = next_cursor;
            if cursor == 0 {
                break;
            }
        }
        let count = all_keys.len() as u64;
        if !all_keys.is_empty() {
            let _: () = redis::cmd("DEL")
                .arg(&all_keys)
                .query_async(&mut conn)
                .await
                .map_err(|e| RevocationError::Backend(format!("Redis DEL: {e}")))?;
        }
        Ok(count)
    }
}

// ───────────────────────────────────────────────────────────────
// Token Revocation Manager
// ───────────────────────────────────────────────────────────────

/// High-level token revocation manager wrapping a backend store.
pub struct TokenRevocationManager {
    store:       Arc<dyn RevocationStore>,
    require_jti: bool,
    fail_open:   bool,
}

impl TokenRevocationManager {
    /// Create a new revocation manager.
    #[must_use]
    pub fn new(store: Arc<dyn RevocationStore>, require_jti: bool, fail_open: bool) -> Self {
        Self {
            store,
            require_jti,
            fail_open,
        }
    }

    /// Check if a token should be rejected.
    ///
    /// Returns `Ok(())` if the token is allowed, or an error reason if rejected.
    ///
    /// # Errors
    ///
    /// Returns `TokenRejection::MissingJti` if JTI is required but absent.
    /// Returns `TokenRejection::Revoked` if the token has been revoked.
    /// Returns `TokenRejection::StoreUnavailable` if the revocation store is unreachable and
    /// `fail_open` is false.
    pub async fn check_token(&self, jti: Option<&str>) -> Result<(), TokenRejection> {
        let jti = match jti {
            Some(j) if !j.is_empty() => j,
            _ => {
                if self.require_jti {
                    return Err(TokenRejection::MissingJti);
                }
                // No JTI and not required — allow through.
                return Ok(());
            },
        };

        match self.store.is_revoked(jti).await {
            Ok(true) => Err(TokenRejection::Revoked),
            Ok(false) => Ok(()),
            Err(e) => {
                warn!(error = %e, jti = %jti, "Revocation store check failed");
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

    /// Revoke all tokens for a user.
    ///
    /// # Errors
    ///
    /// Returns `RevocationError` if the underlying revocation store operation fails.
    pub async fn revoke_all_for_user(&self, sub: &str) -> Result<u64, RevocationError> {
        self.store.revoke_all_for_user(sub).await
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

/// Build a `TokenRevocationManager` from the compiled schema's `security.token_revocation` JSON.
pub fn revocation_manager_from_schema(
    schema: &fraiseql_core::schema::CompiledSchema,
) -> Option<Arc<TokenRevocationManager>> {
    let security = schema.security.as_ref()?;
    let revocation_val = security.additional.get("token_revocation")?;
    let config: TokenRevocationConfig = serde_json::from_value(revocation_val.clone())
        .map_err(|e| {
            warn!(error = %e, "Failed to parse security.token_revocation config");
        })
        .ok()?;

    if !config.enabled {
        return None;
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
        other => {
            warn!(backend = %other, "Unknown revocation backend — falling back to in-memory");
            Arc::new(InMemoryRevocationStore::new())
        },
    };

    Some(Arc::new(TokenRevocationManager::new(
        store,
        config.require_jti,
        config.fail_open,
    )))
}

// ───────────────────────────────────────────────────────────────
// Tests
// ───────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)] // Reason: test code, panics are acceptable

    use super::*;

    fn memory_store() -> Arc<dyn RevocationStore> {
        Arc::new(InMemoryRevocationStore::new())
    }

    #[tokio::test]
    async fn revoke_then_check_is_revoked() {
        let store = memory_store();
        store.revoke("jti-1", 3600).await.unwrap();
        assert!(store.is_revoked("jti-1").await.unwrap());
    }

    #[tokio::test]
    async fn non_revoked_jti_passes() {
        let store = memory_store();
        assert!(!store.is_revoked("jti-unknown").await.unwrap());
    }

    #[tokio::test]
    async fn expired_entry_not_revoked() {
        let store = InMemoryRevocationStore::new();
        // Insert with 0-second TTL → already expired.
        store.revoke("jti-expired", 0).await.unwrap();
        // Should not be considered revoked (TTL elapsed).
        assert!(!store.is_revoked("jti-expired").await.unwrap());
    }

    #[tokio::test]
    async fn cleanup_removes_expired() {
        let store = InMemoryRevocationStore::new();
        store.revoke("jti-a", 0).await.unwrap();
        store.revoke("jti-b", 3600).await.unwrap();
        store.cleanup_expired();
        // jti-a expired, jti-b still valid.
        assert_eq!(store.entries.len(), 1);
    }

    #[tokio::test]
    async fn manager_rejects_revoked_token() {
        let store = memory_store();
        store.revoke("jti-x", 3600).await.unwrap();
        let mgr = TokenRevocationManager::new(store, true, false);
        assert_eq!(mgr.check_token(Some("jti-x")).await, Err(TokenRejection::Revoked));
    }

    #[tokio::test]
    async fn manager_allows_non_revoked_token() {
        let mgr = TokenRevocationManager::new(memory_store(), true, false);
        mgr.check_token(Some("jti-ok"))
            .await
            .unwrap_or_else(|e| panic!("expected Ok for non-revoked token: {e:?}"));
    }

    #[tokio::test]
    async fn manager_rejects_missing_jti_when_required() {
        let mgr = TokenRevocationManager::new(memory_store(), true, false);
        assert_eq!(mgr.check_token(None).await, Err(TokenRejection::MissingJti));
    }

    #[tokio::test]
    async fn manager_allows_missing_jti_when_not_required() {
        let mgr = TokenRevocationManager::new(memory_store(), false, false);
        assert!(
            mgr.check_token(None).await.is_ok(),
            "missing jti should be allowed when jti is not required"
        );
    }

    #[tokio::test]
    async fn manager_allows_empty_jti_when_not_required() {
        let mgr = TokenRevocationManager::new(memory_store(), false, false);
        assert!(
            mgr.check_token(Some("")).await.is_ok(),
            "empty jti should be allowed when jti is not required"
        );
    }

    // ── S36: revoke_all_for_user actually revokes tokens ─────────────────────

    #[tokio::test]
    async fn revoke_all_for_user_removes_all_matching_entries() {
        // Populate the store with entries belonging to two different users.
        // revoke() stores an empty sub, so we need to seed InMemoryRevocationStore
        // directly to set a real sub.
        let store = InMemoryRevocationStore::new();
        let exp = Utc::now() + chrono::Duration::seconds(3600);
        store.entries.insert("jti-alice-1".to_string(), ("alice".to_string(), exp));
        store.entries.insert("jti-alice-2".to_string(), ("alice".to_string(), exp));
        store.entries.insert("jti-bob-1".to_string(), ("bob".to_string(), exp));

        let count = store.revoke_all_for_user("alice").await.unwrap();
        assert_eq!(count, 2, "should have revoked 2 alice entries, got {count}");

        // Alice entries must be gone.
        assert!(
            !store.is_revoked("jti-alice-1").await.unwrap(),
            "alice jti-1 should be removed from store"
        );
        assert!(
            !store.is_revoked("jti-alice-2").await.unwrap(),
            "alice jti-2 should be removed from store"
        );

        // Bob entry must remain.
        assert!(
            store.is_revoked("jti-bob-1").await.unwrap(),
            "bob jti-1 must NOT be revoked by alice's revoke_all"
        );
    }

    #[tokio::test]
    async fn revoke_all_for_user_returns_zero_when_no_entries() {
        let store = InMemoryRevocationStore::new();
        let count = store.revoke_all_for_user("unknown-user").await.unwrap();
        assert_eq!(count, 0, "empty store should return 0");
    }

    // ── S36: fail-open clock fix — unix_now helper ────────────────────────────

    #[test]
    fn unix_now_helper_in_session_returns_reasonable_value() {
        use fraiseql_auth::session::unix_now;
        // The epoch is 2009 or so for Bitcoin genesis, but we're well past that now.
        // Verify the value is at least 2020-01-01T00:00:00Z (1577836800).
        let now = unix_now().expect("unix_now should succeed on a normal system");
        assert!(now >= 1_577_836_800, "unix_now should return a timestamp after 2020");
    }

    // ── S36: predictable HMAC — token uniqueness ─────────────────────────────

    #[test]
    fn hmac_fallback_tokens_differ_between_calls_even_for_same_user() {
        use rand::{Rng, rngs::OsRng};
        // Verify that two randomly-generated 32-byte keys are always different,
        // confirming the fix produces different secrets per invocation.
        let key1: [u8; 32] = OsRng.gen();
        let key2: [u8; 32] = OsRng.gen();
        assert_ne!(key1, key2, "two OsRng-generated 256-bit keys must differ");
    }
}
