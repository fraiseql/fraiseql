// PKCE state store — RFC 7636 Proof Key for Code Exchange
//
// Stores `(code_verifier, redirect_uri)` under a random internal key while
// the OAuth2 authorization round-trip is in flight.  The token sent to the
// OIDC provider in the `?state=` query parameter is either:
//   - the raw internal key (no encryption configured), or
//   - `encrypt(internal_key)` (when StateEncryptionService is attached).
//
// State lifecycle:
//   create_state(redirect_uri)
//     → internal_key = random 32 bytes (base64url)
//     → outbound_token = encrypt(internal_key)  [or internal_key if no encryption]
//     → store.insert(internal_key, {verifier, redirect_uri, ttl})
//     → return (outbound_token, verifier)
//
//   consume_state(outbound_token)
//     → internal_key = decrypt(outbound_token)  [or outbound_token if no encryption]
//     → entry = store.remove(internal_key)?  [StateNotFound if absent]
//     → if entry.elapsed > entry.ttl → StateExpired
//     → return {verifier, redirect_uri}
//
// Backends:
//   InMemory — DashMap, single-process, per-replica
//   Redis    — distributed, multi-replica (requires `redis-pkce` Cargo feature)

use std::{
    sync::Arc,
    time::{Duration, Instant},
};

use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};
use dashmap::DashMap;
use rand::{RngCore, rngs::OsRng};
use sha2::{Digest, Sha256};
use thiserror::Error;

use crate::state_encryption::StateEncryptionService;

// ---------------------------------------------------------------------------
// Error type
// ---------------------------------------------------------------------------

/// Errors returned by [`PkceStateStore::consume_state`].
#[derive(Debug, Error)]
pub enum PkceError {
    /// The state token was not found — either never issued, already consumed,
    /// or (when encryption is on) tampered/decryption failed.
    ///
    /// Clients receive the same message for unknown and tampered tokens to
    /// avoid leaking information about the store.
    #[error("state not found — the authorization flow may have already been completed or the state is invalid")]
    StateNotFound,

    /// The state token was found but its TTL has elapsed.
    ///
    /// Distinct from [`PkceError::StateNotFound`] so that clients can show
    /// a useful "please restart the authorization flow" message rather than
    /// a generic invalid-state error.
    #[error("state expired — please restart the authorization flow")]
    StateExpired,
}

// ---------------------------------------------------------------------------
// Public consumed-state value
// ---------------------------------------------------------------------------

/// The data recovered after consuming a valid PKCE state token.
pub struct ConsumedPkceState {
    /// The `code_verifier` generated during `create_state`, needed for the
    /// PKCE code exchange at `/token`.
    pub verifier:     String,
    /// The `redirect_uri` the client specified at `/auth/start`.
    pub redirect_uri: String,
}

// ---------------------------------------------------------------------------
// InMemoryPkceStateStore
// ---------------------------------------------------------------------------

struct PkceEntry {
    verifier:     String,
    redirect_uri: String,
    created_at:   Instant,
    ttl:          Duration,
}

/// In-memory PKCE state store backed by a [`DashMap`].
///
/// State is per-process: lost on restart, not shared across replicas.
/// For multi-replica deployments, use [`PkceStateStore::new_redis`] instead
/// (requires the `redis-pkce` Cargo feature).
pub struct InMemoryPkceStateStore {
    state_ttl_secs: u64,
    entries:        DashMap<String, PkceEntry>,
    encryptor:      Option<Arc<StateEncryptionService>>,
}

impl InMemoryPkceStateStore {
    fn new(state_ttl_secs: u64, encryptor: Option<Arc<StateEncryptionService>>) -> Self {
        Self {
            state_ttl_secs,
            entries: DashMap::new(),
            encryptor,
        }
    }

    fn create_state_sync(&self, redirect_uri: &str) -> Result<(String, String), anyhow::Error> {
        // code_verifier — RFC 7636 §4.1: 43–128 chars, [A-Za-z0-9\-._~]
        let mut verifier_bytes = [0u8; 32];
        OsRng.fill_bytes(&mut verifier_bytes);
        let verifier = URL_SAFE_NO_PAD.encode(verifier_bytes);

        // internal_key — separate from verifier so outbound token cannot reveal it
        let mut key_bytes = [0u8; 32];
        OsRng.fill_bytes(&mut key_bytes);
        let internal_key = URL_SAFE_NO_PAD.encode(key_bytes);

        self.entries.insert(internal_key.clone(), PkceEntry {
            verifier:     verifier.clone(),
            redirect_uri: redirect_uri.to_owned(),
            created_at:   Instant::now(),
            ttl:          Duration::from_secs(self.state_ttl_secs),
        });

        let outbound_token = match &self.encryptor {
            Some(enc) => enc.encrypt(internal_key.as_bytes())?,
            None      => internal_key,
        };

        Ok((outbound_token, verifier))
    }

    fn consume_state_sync(&self, outbound_token: &str) -> Result<ConsumedPkceState, PkceError> {
        let internal_key = match &self.encryptor {
            Some(enc) => {
                let bytes = enc
                    .decrypt(outbound_token)
                    .map_err(|_| PkceError::StateNotFound)?;
                String::from_utf8(bytes).map_err(|_| PkceError::StateNotFound)?
            }
            None => outbound_token.to_owned(),
        };

        let (_, entry) = self.entries.remove(&internal_key).ok_or(PkceError::StateNotFound)?;

        if entry.created_at.elapsed() > entry.ttl {
            return Err(PkceError::StateExpired);
        }

        Ok(ConsumedPkceState {
            verifier:     entry.verifier,
            redirect_uri: entry.redirect_uri,
        })
    }

    fn cleanup_expired_sync(&self) {
        self.entries.retain(|_, e| e.created_at.elapsed() <= e.ttl);
    }

    fn len_sync(&self) -> usize {
        self.entries.len()
    }
}

// ---------------------------------------------------------------------------
// Redis backend
// ---------------------------------------------------------------------------

/// Cumulative count of Redis PKCE store errors (unreachable Redis, etc.).
///
/// Exposed via `/metrics` as `fraiseql_pkce_redis_errors_total`.
#[cfg(feature = "redis-pkce")]
pub static REDIS_PKCE_ERRORS: std::sync::atomic::AtomicU64 =
    std::sync::atomic::AtomicU64::new(0);

/// Return the total number of Redis PKCE errors observed so far.
#[cfg(feature = "redis-pkce")]
pub fn redis_pkce_error_count_total() -> u64 {
    REDIS_PKCE_ERRORS.load(std::sync::atomic::Ordering::Relaxed)
}

/// Redis-backed PKCE state store for distributed, multi-replica deployments.
///
/// Stores PKCE state tokens in Redis with TTL, enabling auth flows to be
/// completed on any replica. Uses `GETDEL` for atomic one-shot consumption
/// — a state token cannot be reused even under concurrent requests.
///
/// Key format:   `fraiseql:pkce:{internal_key}`
/// Value format: `{"verifier":"...","redirect_uri":"..."}`
#[cfg(feature = "redis-pkce")]
pub struct RedisPkceStateStore {
    pool:           redis::aio::ConnectionManager,
    state_ttl_secs: u64,
    encryptor:      Option<Arc<StateEncryptionService>>,
}

#[cfg(feature = "redis-pkce")]
impl RedisPkceStateStore {
    /// Connect to Redis and prepare the PKCE state store.
    ///
    /// # Errors
    ///
    /// Returns an error if the URL is invalid or the initial connection fails.
    pub async fn new(
        url: &str,
        state_ttl_secs: u64,
        encryptor: Option<Arc<StateEncryptionService>>,
    ) -> Result<Self, redis::RedisError> {
        let client = redis::Client::open(url)?;
        let pool = redis::aio::ConnectionManager::new(client).await?;
        Ok(Self { pool, state_ttl_secs, encryptor })
    }

    async fn create_state_impl(
        &self,
        redirect_uri: &str,
    ) -> Result<(String, String), anyhow::Error> {
        // code_verifier (RFC 7636 §4.1)
        let mut verifier_bytes = [0u8; 32];
        OsRng.fill_bytes(&mut verifier_bytes);
        let verifier = URL_SAFE_NO_PAD.encode(verifier_bytes);

        // Opaque internal key — separate from verifier
        let mut key_bytes = [0u8; 32];
        OsRng.fill_bytes(&mut key_bytes);
        let internal_key = URL_SAFE_NO_PAD.encode(key_bytes);

        // Serialize state to JSON and store with TTL
        let redis_key = format!("fraiseql:pkce:{internal_key}");
        let value = serde_json::json!({
            "verifier":     verifier,
            "redirect_uri": redirect_uri,
        })
        .to_string();

        let mut conn = self.pool.clone();
        redis::cmd("SET")
            .arg(&redis_key)
            .arg(&value)
            .arg("EX")
            .arg(self.state_ttl_secs)
            .query_async::<()>(&mut conn)
            .await?;

        let outbound_token = match &self.encryptor {
            Some(enc) => enc.encrypt(internal_key.as_bytes())?,
            None      => internal_key,
        };

        Ok((outbound_token, verifier))
    }

    async fn consume_state_impl(
        &self,
        outbound_token: &str,
    ) -> Result<ConsumedPkceState, PkceError> {
        // Recover internal key from outbound token
        let internal_key = match &self.encryptor {
            Some(enc) => {
                let bytes = enc
                    .decrypt(outbound_token)
                    .map_err(|_| PkceError::StateNotFound)?;
                String::from_utf8(bytes).map_err(|_| PkceError::StateNotFound)?
            }
            None => outbound_token.to_owned(),
        };

        let redis_key = format!("fraiseql:pkce:{internal_key}");
        let mut conn = self.pool.clone();

        // GETDEL — atomically retrieve and delete in a single round-trip.
        // This guarantees one-shot consumption: no concurrent request can
        // reuse the same state token, even without application-level locking.
        let raw: Option<String> = redis::cmd("GETDEL")
            .arg(&redis_key)
            .query_async(&mut conn)
            .await
            .map_err(|_| PkceError::StateNotFound)?;

        let json = raw.ok_or(PkceError::StateNotFound)?;

        #[derive(serde::Deserialize)]
        struct StoredEntry {
            verifier:     String,
            redirect_uri: String,
        }

        let entry: StoredEntry =
            serde_json::from_str(&json).map_err(|_| PkceError::StateNotFound)?;

        // Note: TTL expiry is handled by Redis — expired entries are absent.
        // The Redis backend therefore never returns `PkceError::StateExpired`;
        // callers receive `StateNotFound` for both absent and expired tokens.
        Ok(ConsumedPkceState {
            verifier:     entry.verifier,
            redirect_uri: entry.redirect_uri,
        })
    }
}

// ---------------------------------------------------------------------------
// PkceStateStore — unified public interface
// ---------------------------------------------------------------------------

/// PKCE state store that dispatches to an in-memory or Redis backend.
///
/// # Backends
///
/// - **InMemory** (default): per-process DashMap. Safe for single-replica
///   deployments. State is lost on restart.
///
/// - **Redis** (requires `redis-pkce` Cargo feature): distributed, shared
///   across all replicas. Required for multi-instance Kubernetes / ECS / fly.io
///   deployments where `/auth/start` and `/auth/callback` may hit different nodes.
///
/// # Multi-replica requirement
///
/// Set `FRAISEQL_REQUIRE_REDIS=1` in your deployment environment to make
/// FraiseQL refuse to start without a Redis-backed PKCE store. This is the
/// recommended pattern for production Kubernetes deployments.
pub enum PkceStateStore {
    /// Single-node DashMap-backed store.
    InMemory(InMemoryPkceStateStore),
    /// Distributed Redis-backed store (requires `redis-pkce` Cargo feature).
    #[cfg(feature = "redis-pkce")]
    Redis(RedisPkceStateStore),
}

impl PkceStateStore {
    /// Create an in-memory PKCE state store (single-replica deployments).
    pub fn new(state_ttl_secs: u64, encryptor: Option<Arc<StateEncryptionService>>) -> Self {
        Self::InMemory(InMemoryPkceStateStore::new(state_ttl_secs, encryptor))
    }

    /// Create a Redis-backed distributed PKCE state store.
    ///
    /// # Errors
    ///
    /// Returns an error if the Redis URL is invalid or the connection fails.
    #[cfg(feature = "redis-pkce")]
    pub async fn new_redis(
        url: &str,
        state_ttl_secs: u64,
        encryptor: Option<Arc<StateEncryptionService>>,
    ) -> Result<Self, redis::RedisError> {
        let inner = RedisPkceStateStore::new(url, state_ttl_secs, encryptor).await?;
        Ok(Self::Redis(inner))
    }

    /// Returns `true` when backed by the in-memory DashMap store.
    ///
    /// Used by the `FRAISEQL_REQUIRE_REDIS` startup check.
    pub fn is_in_memory(&self) -> bool {
        matches!(self, Self::InMemory(_))
    }

    /// Generate an authorization-code verifier and reserve a state slot.
    ///
    /// Returns `(outbound_token, code_verifier)`:
    /// - `outbound_token` goes in the OIDC `?state=` query parameter.
    /// - `code_verifier` is passed to [`Self::s256_challenge`] and stored
    ///   until the callback arrives.
    ///
    /// # Errors
    ///
    /// Returns an error if encryption fails (effectively never with a valid
    /// key) or the Redis backend is unreachable.
    pub async fn create_state(&self, redirect_uri: &str) -> Result<(String, String), anyhow::Error> {
        match self {
            Self::InMemory(s) => s.create_state_sync(redirect_uri),
            #[cfg(feature = "redis-pkce")]
            Self::Redis(s)    => s.create_state_impl(redirect_uri).await,
        }
    }

    /// Consume a state token, atomically removing it from the store.
    ///
    /// Returns [`PkceError::StateNotFound`] for:
    /// - tokens that were never issued,
    /// - tokens that have already been consumed (one-time use), and
    /// - tokens that fail decryption (tampered or from a different key).
    ///
    /// Returns [`PkceError::StateExpired`] when the in-memory token is valid
    /// but its TTL has elapsed. The Redis backend returns `StateNotFound` for
    /// expired tokens (Redis TTL handles expiry).
    pub async fn consume_state(&self, outbound_token: &str) -> Result<ConsumedPkceState, PkceError> {
        match self {
            Self::InMemory(s) => s.consume_state_sync(outbound_token),
            #[cfg(feature = "redis-pkce")]
            Self::Redis(s)    => s.consume_state_impl(outbound_token).await,
        }
    }

    /// Compute the S256 code challenge for a given verifier.
    ///
    /// Per RFC 7636 §4.2:
    /// `code_challenge = BASE64URL(SHA256(ASCII(code_verifier)))`
    /// (no padding).
    pub fn s256_challenge(verifier: &str) -> String {
        URL_SAFE_NO_PAD.encode(Sha256::digest(verifier.as_bytes()))
    }

    /// Remove expired entries.
    ///
    /// No-op for the Redis backend — Redis TTL handles expiry automatically.
    /// Call from a background task on a fixed interval for the in-memory backend.
    pub async fn cleanup_expired(&self) {
        match self {
            Self::InMemory(s) => s.cleanup_expired_sync(),
            #[cfg(feature = "redis-pkce")]
            Self::Redis(_)    => {}, // Redis TTL handles expiry
        }
    }

    /// Number of entries currently in the store.
    ///
    /// Returns 0 for the Redis backend — Redis state is not enumerable locally.
    pub fn len(&self) -> usize {
        match self {
            Self::InMemory(s) => s.len_sync(),
            #[cfg(feature = "redis-pkce")]
            Self::Redis(_)    => 0,
        }
    }

    /// Returns `true` when the in-memory store contains no entries.
    ///
    /// Always returns `true` for the Redis backend.
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

// ---------------------------------------------------------------------------
// Unit tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use super::*;
    use crate::state_encryption::{EncryptionAlgorithm, StateEncryptionService};

    fn store_no_enc(ttl_secs: u64) -> PkceStateStore {
        PkceStateStore::new(ttl_secs, None)
    }

    fn enc_service() -> Arc<StateEncryptionService> {
        Arc::new(StateEncryptionService::from_raw_key(
            &[0u8; 32],
            EncryptionAlgorithm::Chacha20Poly1305,
        ))
    }

    // ── Core state machine ────────────────────────────────────────────────────

    #[tokio::test]
    async fn test_create_and_consume_roundtrip() {
        let store = store_no_enc(600);
        let (token, verifier) = store.create_state("https://app.example.com/cb").await.unwrap();
        let result = store.consume_state(&token).await.unwrap();
        assert_eq!(result.verifier, verifier);
        assert_eq!(result.redirect_uri, "https://app.example.com/cb");
    }

    #[tokio::test]
    async fn test_consume_removes_entry_cannot_reuse() {
        let store = store_no_enc(600);
        let (token, _) = store.create_state("https://app.example.com/cb").await.unwrap();
        store.consume_state(&token).await.unwrap();
        assert!(
            matches!(store.consume_state(&token).await, Err(PkceError::StateNotFound)),
            "second consume must return StateNotFound"
        );
    }

    #[tokio::test]
    async fn test_expired_state_returns_state_expired_not_not_found() {
        let store = store_no_enc(1);
        let (token, _) = store.create_state("https://example.com").await.unwrap();
        std::thread::sleep(Duration::from_millis(1100));
        assert!(
            matches!(store.consume_state(&token).await, Err(PkceError::StateExpired)),
            "expired state must be StateExpired, not StateNotFound"
        );
    }

    #[tokio::test]
    async fn test_unknown_token_returns_not_found() {
        let store = store_no_enc(600);
        assert!(matches!(
            store.consume_state("completely-unknown-token").await,
            Err(PkceError::StateNotFound)
        ));
    }

    #[tokio::test]
    async fn test_two_distinct_states_dont_interfere() {
        let store = store_no_enc(600);
        let (t1, v1) = store.create_state("https://a.example.com/cb").await.unwrap();
        let (t2, v2) = store.create_state("https://b.example.com/cb").await.unwrap();
        let r2 = store.consume_state(&t2).await.unwrap();
        let r1 = store.consume_state(&t1).await.unwrap();
        assert_eq!(r1.verifier, v1);
        assert_eq!(r2.verifier, v2);
    }

    // ── RFC 7636 compliance ───────────────────────────────────────────────────

    #[test]
    fn test_s256_challenge_matches_rfc7636_appendix_a() {
        let verifier = "dBjftJeZ4CVP-mB92K27uhbUJU1p1r_wW1gFWFOEjXk";
        let expected = "E9Melhoa2OwvFrEMTJguCHaoeK1t8URWbuGJSstw-cM";
        assert_eq!(PkceStateStore::s256_challenge(verifier), expected);
    }

    #[tokio::test]
    async fn test_verifier_length_and_charset_are_rfc7636_compliant() {
        let store = store_no_enc(600);
        let (_, verifier) = store.create_state("https://example.com").await.unwrap();
        assert!(
            verifier.len() >= 43 && verifier.len() <= 128,
            "verifier length {} is outside the 43–128 char range",
            verifier.len()
        );
        assert!(!verifier.contains('='), "verifier must not contain padding characters");
    }

    // ── Encryption integration ────────────────────────────────────────────────

    #[tokio::test]
    async fn test_encrypted_token_is_longer_than_raw_internal_key() {
        let store = PkceStateStore::new(600, Some(enc_service()));
        let (token, _) = store.create_state("https://app.example.com/cb").await.unwrap();
        assert!(
            token.len() > 43,
            "encrypted token (len={}) must be longer than a raw 32-byte key (43 chars)",
            token.len()
        );
    }

    #[tokio::test]
    async fn test_encrypted_roundtrip_works_end_to_end() {
        let store = PkceStateStore::new(600, Some(enc_service()));
        let (token, verifier) = store.create_state("https://app.example.com/cb").await.unwrap();
        let result = store.consume_state(&token).await.unwrap();
        assert_eq!(result.verifier, verifier);
    }

    #[tokio::test]
    async fn test_tampered_encrypted_token_returns_not_found() {
        let store = PkceStateStore::new(600, Some(enc_service()));
        store.create_state("https://app.example.com/cb").await.unwrap();
        let result = store.consume_state("aGVsbG8gd29ybGQ").await;
        assert!(
            matches!(result, Err(PkceError::StateNotFound)),
            "tampered token must yield StateNotFound, not an internal error"
        );
    }

    // ── is_in_memory ─────────────────────────────────────────────────────────

    #[test]
    fn test_is_in_memory_returns_true_for_in_memory_store() {
        let store = PkceStateStore::new(600, None);
        assert!(store.is_in_memory());
    }

    // ── Cleanup ───────────────────────────────────────────────────────────────

    #[tokio::test]
    async fn test_cleanup_removes_expired_leaves_valid() {
        let store = store_no_enc(1);
        store.create_state("https://a.example.com").await.unwrap();
        std::thread::sleep(Duration::from_millis(1100));
        store.cleanup_expired().await;
        assert_eq!(store.len(), 0, "expired entry must be removed by cleanup");

        let store2 = store_no_enc(600);
        store2.create_state("https://b.example.com").await.unwrap();
        store2.cleanup_expired().await;
        assert_eq!(store2.len(), 1, "unexpired entry must survive cleanup");
    }

    // ── Redis integration tests ───────────────────────────────────────────────
    // Require a live Redis instance.  Run with:
    //   REDIS_URL=redis://localhost:6379 cargo test -p fraiseql-auth \
    //     --features redis-pkce -- redis_pkce --ignored

    #[cfg(feature = "redis-pkce")]
    #[tokio::test]
    #[ignore = "requires Redis — set REDIS_URL=redis://localhost:6379"]
    async fn test_redis_pkce_create_and_consume_roundtrip() {
        let url = std::env::var("REDIS_URL")
            .unwrap_or_else(|_| "redis://localhost:6379".to_string());
        let store = PkceStateStore::new_redis(&url, 300, None)
            .await
            .expect("Redis connection failed");

        let (token, verifier) = store.create_state("https://example.com/cb").await.unwrap();
        let consumed = store.consume_state(&token).await.unwrap();
        assert_eq!(consumed.verifier, verifier);
        assert_eq!(consumed.redirect_uri, "https://example.com/cb");
    }

    #[cfg(feature = "redis-pkce")]
    #[tokio::test]
    #[ignore = "requires Redis — set REDIS_URL=redis://localhost:6379"]
    async fn test_redis_pkce_one_shot_consumption() {
        let url = std::env::var("REDIS_URL")
            .unwrap_or_else(|_| "redis://localhost:6379".to_string());
        let store = PkceStateStore::new_redis(&url, 300, None)
            .await
            .expect("Redis connection failed");

        let (token, _) = store.create_state("https://example.com/cb").await.unwrap();
        store.consume_state(&token).await.unwrap();

        let second = store.consume_state(&token).await;
        assert!(
            matches!(second, Err(PkceError::StateNotFound)),
            "second consume must return StateNotFound — GETDEL guarantees one-shot"
        );
    }

    #[cfg(feature = "redis-pkce")]
    #[tokio::test]
    #[ignore = "requires Redis — set REDIS_URL=redis://localhost:6379"]
    async fn test_redis_pkce_two_instances_share_state() {
        let url = std::env::var("REDIS_URL")
            .unwrap_or_else(|_| "redis://localhost:6379".to_string());

        // Simulate two server replicas sharing the same Redis instance
        let store_a = PkceStateStore::new_redis(&url, 300, None)
            .await
            .expect("Redis connection failed");
        let store_b = PkceStateStore::new_redis(&url, 300, None)
            .await
            .expect("Redis connection failed");

        // Replica A handles /auth/start
        let (token, verifier) = store_a.create_state("https://example.com/cb").await.unwrap();

        // Replica B handles /auth/callback — must be able to consume the state
        let consumed = store_b.consume_state(&token).await.unwrap();
        assert_eq!(
            consumed.verifier, verifier,
            "cross-replica consumption must succeed with shared Redis"
        );
    }

    #[cfg(feature = "redis-pkce")]
    #[tokio::test]
    #[ignore = "requires Redis — set REDIS_URL=redis://localhost:6379"]
    async fn test_redis_pkce_tampered_token_rejected() {
        let url = std::env::var("REDIS_URL")
            .unwrap_or_else(|_| "redis://localhost:6379".to_string());
        let enc = Some(Arc::new(StateEncryptionService::from_raw_key(
            &[0u8; 32],
            EncryptionAlgorithm::Chacha20Poly1305,
        )));
        let store = PkceStateStore::new_redis(&url, 300, enc)
            .await
            .expect("Redis connection failed");

        store.create_state("https://example.com/cb").await.unwrap();

        let result = store.consume_state("completely-fabricated-token").await;
        assert!(
            matches!(result, Err(PkceError::StateNotFound)),
            "tampered token must be rejected"
        );
    }
}
