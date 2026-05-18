//! PKCE state store — RFC 7636 Proof Key for Code Exchange.
//!
//! Stores `(code_verifier, redirect_uri)` under a random internal key while
//! the OAuth2 authorization round-trip is in flight.  The token sent to the
//! OIDC provider in the `?state=` query parameter is either:
//! - the raw internal key (no encryption configured), or
//! - `encrypt(internal_key)` (when [`crate::state_encryption::StateEncryptionService`] is
//!   attached).
//!
//! State lifecycle:
//! - `create_state(redirect_uri)` → `internal_key = random 32 bytes (base64url)` → `outbound_token
//!   = encrypt(internal_key)` (or `internal_key` if no encryption) → `store.insert(internal_key,
//!   {verifier, redirect_uri, ttl})` → returns `(outbound_token, verifier)`
//! - `consume_state(outbound_token)` → `internal_key = decrypt(outbound_token)` (or
//!   `outbound_token` if no encryption) → `entry = store.remove(internal_key)?` (
//!   [`PkceError::StateNotFound`] if absent) → if `entry.elapsed > entry.ttl` →
//!   [`PkceError::StateExpired`] → returns `{verifier, redirect_uri}`
//!
//! Backends:
//! - **InMemory** — `DashMap`, single-process, per-replica
//! - **Redis** — distributed, multi-replica (requires the `redis-pkce` Cargo feature)

use std::{sync::Arc, time::Duration};

use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};
use dashmap::DashMap;
use rand::RngCore as _;
use sha2::{Digest, Sha256};
use thiserror::Error;

use crate::state_encryption::StateEncryptionService;

// ---------------------------------------------------------------------------
// Constants
// ---------------------------------------------------------------------------

/// Maximum number of in-flight PKCE state entries allowed in the in-memory
/// store at any one time.
///
/// This cap prevents the DashMap from growing without bound under load or
/// during a DoS attack where an adversary initiates many OAuth flows without
/// completing them. New inserts beyond this limit are rejected with
/// [`PkceError::StoreFull`].
///
/// 10 000 entries corresponds to approximately 10 000 concurrent in-flight
/// authorization requests, which is more than sufficient for any realistic
/// single-node deployment. Multi-replica deployments should use the Redis
/// backend instead.
const MAX_PKCE_ENTRIES: usize = 10_000;

// ---------------------------------------------------------------------------
// Error type
// ---------------------------------------------------------------------------

/// Errors returned by [`PkceStateStore::consume_state`] and
/// [`PkceStateStore::create_state`].
#[derive(Debug, Error)]
#[non_exhaustive]
pub enum PkceError {
    /// The state token was not found — either never issued, already consumed,
    /// or (when encryption is on) tampered/decryption failed.
    ///
    /// Clients receive the same message for unknown and tampered tokens to
    /// avoid leaking information about the store.
    #[error(
        "state not found — the authorization flow may have already been completed or the state is invalid"
    )]
    StateNotFound,

    /// The state token was found but its TTL has elapsed.
    ///
    /// Distinct from [`PkceError::StateNotFound`] so that clients can show
    /// a useful "please restart the authorization flow" message rather than
    /// a generic invalid-state error.
    #[error("state expired — please restart the authorization flow")]
    StateExpired,

    /// The in-memory store has reached `MAX_PKCE_ENTRIES` entries.
    ///
    /// This prevents unbounded memory growth under load or during a DoS
    /// attack. Callers should return HTTP 429 to the client and invite them
    /// to retry after a short delay.
    #[error("PKCE state store is full — too many concurrent authorization flows")]
    StoreFull,
}

// ---------------------------------------------------------------------------
// Public consumed-state value
// ---------------------------------------------------------------------------

/// The data recovered after consuming a valid PKCE state token.
#[derive(Debug)]
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
    /// Creation time as a Tokio instant so `tokio::time::pause()` +
    /// `tokio::time::advance()` can control TTL expiry in tests.
    created_at:   tokio::time::Instant,
    ttl:          Duration,
}

/// In-memory PKCE state store backed by a [`DashMap`].
///
/// State is per-process: lost on restart, not shared across replicas.
/// For multi-replica deployments, use `RedisPkceStateStore` instead
/// (requires the `redis-pkce` Cargo feature).
pub struct InMemoryPkceStateStore {
    state_ttl_secs: u64,
    entries:        DashMap<String, PkceEntry>,
    encryptor:      Option<Arc<StateEncryptionService>>,
    /// Maximum number of in-flight entries; defaults to `MAX_PKCE_ENTRIES`.
    /// Overridable in tests via [`InMemoryPkceStateStore::with_max_entries`].
    max_entries:    usize,
}

impl InMemoryPkceStateStore {
    fn new(state_ttl_secs: u64, encryptor: Option<Arc<StateEncryptionService>>) -> Self {
        Self {
            state_ttl_secs,
            entries: DashMap::new(),
            encryptor,
            max_entries: MAX_PKCE_ENTRIES,
        }
    }

    /// Create a store with a custom entry cap — for testing only.
    #[cfg(test)]
    fn with_max_entries(
        state_ttl_secs: u64,
        encryptor: Option<Arc<StateEncryptionService>>,
        max_entries: usize,
    ) -> Self {
        Self {
            state_ttl_secs,
            entries: DashMap::new(),
            encryptor,
            max_entries,
        }
    }

    fn create_state_sync(&self, redirect_uri: &str) -> Result<(String, String), anyhow::Error> {
        // SECURITY: reject inserts when the map is at capacity to prevent
        // unbounded memory growth under DoS.  Callers translate this into
        // HTTP 429 and invite the client to retry.
        if self.entries.len() >= self.max_entries {
            return Err(PkceError::StoreFull.into());
        }

        // code_verifier — RFC 7636 §4.1: 43–128 chars, [A-Za-z0-9\-._~]
        let mut verifier_bytes = [0u8; 32];
        rand::rng().fill_bytes(&mut verifier_bytes);
        let verifier = URL_SAFE_NO_PAD.encode(verifier_bytes);

        // internal_key — separate from verifier so outbound token cannot reveal it
        let mut key_bytes = [0u8; 32];
        rand::rng().fill_bytes(&mut key_bytes);
        let internal_key = URL_SAFE_NO_PAD.encode(key_bytes);

        self.entries.insert(
            internal_key.clone(),
            PkceEntry {
                verifier:     verifier.clone(),
                redirect_uri: redirect_uri.to_owned(),
                created_at:   tokio::time::Instant::now(),
                ttl:          Duration::from_secs(self.state_ttl_secs),
            },
        );

        let outbound_token = match &self.encryptor {
            Some(enc) => enc.encrypt(internal_key.as_bytes())?,
            None => internal_key,
        };

        Ok((outbound_token, verifier))
    }

    /// Remove all expired entries.
    ///
    /// Call this from a background task on a fixed interval to reclaim memory.
    pub fn purge_expired(&self) {
        self.entries.retain(|_, e| e.created_at.elapsed() <= e.ttl);
    }

    fn consume_state_sync(&self, outbound_token: &str) -> Result<ConsumedPkceState, PkceError> {
        let internal_key = match &self.encryptor {
            Some(enc) => {
                let bytes = enc.decrypt(outbound_token).map_err(|_| PkceError::StateNotFound)?;
                String::from_utf8(bytes).map_err(|_| PkceError::StateNotFound)?
            },
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
        self.purge_expired();
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
pub static REDIS_PKCE_ERRORS: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

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
        Ok(Self {
            pool,
            state_ttl_secs,
            encryptor,
        })
    }

    async fn create_state_impl(
        &self,
        redirect_uri: &str,
    ) -> Result<(String, String), anyhow::Error> {
        // code_verifier (RFC 7636 §4.1)
        let mut verifier_bytes = [0u8; 32];
        rand::rng().fill_bytes(&mut verifier_bytes);
        let verifier = URL_SAFE_NO_PAD.encode(verifier_bytes);

        // Opaque internal key — separate from verifier
        let mut key_bytes = [0u8; 32];
        rand::rng().fill_bytes(&mut key_bytes);
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
            None => internal_key,
        };

        Ok((outbound_token, verifier))
    }

    async fn consume_state_impl(
        &self,
        outbound_token: &str,
    ) -> Result<ConsumedPkceState, PkceError> {
        #[derive(serde::Deserialize)]
        struct StoredEntry {
            verifier:     String,
            redirect_uri: String,
        }

        // Recover internal key from outbound token
        let internal_key = match &self.encryptor {
            Some(enc) => {
                let bytes = enc.decrypt(outbound_token).map_err(|_| PkceError::StateNotFound)?;
                String::from_utf8(bytes).map_err(|_| PkceError::StateNotFound)?
            },
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
/// - **InMemory** (default): per-process DashMap. Safe for single-replica deployments. State is
///   lost on restart.
///
/// - **Redis** (requires `redis-pkce` Cargo feature): distributed, shared across all replicas.
///   Required for multi-instance Kubernetes / ECS / fly.io deployments where `/auth/start` and
///   `/auth/callback` may hit different nodes.
///
/// # Multi-replica requirement
///
/// Set `FRAISEQL_REQUIRE_REDIS=1` in your deployment environment to make
/// FraiseQL refuse to start without a Redis-backed PKCE store. This is the
/// recommended pattern for production Kubernetes deployments.
#[non_exhaustive]
pub enum PkceStateStore {
    /// Single-node DashMap-backed store.
    InMemory(InMemoryPkceStateStore),
    /// Distributed Redis-backed store (requires `redis-pkce` Cargo feature).
    #[cfg(feature = "redis-pkce")]
    Redis(RedisPkceStateStore),
}

impl PkceStateStore {
    /// Create an in-memory PKCE state store (single-replica deployments).
    #[must_use] 
    pub fn new(state_ttl_secs: u64, encryptor: Option<Arc<StateEncryptionService>>) -> Self {
        Self::InMemory(InMemoryPkceStateStore::new(state_ttl_secs, encryptor))
    }

    /// Create an in-memory PKCE state store with a custom entry cap.
    ///
    /// For testing only — use [`Self::new`] in production.
    #[cfg(test)]
    pub(crate) fn new_capped(
        state_ttl_secs: u64,
        encryptor: Option<Arc<StateEncryptionService>>,
        max_entries: usize,
    ) -> Self {
        Self::InMemory(InMemoryPkceStateStore::with_max_entries(
            state_ttl_secs,
            encryptor,
            max_entries,
        ))
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
    #[must_use] 
    pub const fn is_in_memory(&self) -> bool {
        matches!(self, Self::InMemory(_))
    }

    /// Generate an authorization-code verifier and reserve a state slot.
    ///
    /// Returns `(outbound_token, code_verifier)`:
    /// - `outbound_token` goes in the OIDC `?state=` query parameter.
    /// - `code_verifier` is passed to [`Self::s256_challenge`] and stored until the callback
    ///   arrives.
    ///
    /// # Errors
    ///
    /// Returns an error if encryption fails (effectively never with a valid
    /// key) or the Redis backend is unreachable.
    pub async fn create_state(
        &self,
        redirect_uri: &str,
    ) -> Result<(String, String), anyhow::Error> {
        match self {
            Self::InMemory(s) => s.create_state_sync(redirect_uri),
            #[cfg(feature = "redis-pkce")]
            Self::Redis(s) => s.create_state_impl(redirect_uri).await,
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
    ///
    /// # Errors
    ///
    /// Returns `PkceError::StateNotFound` if the token is unknown, already consumed,
    /// or fails decryption. Returns `PkceError::StateExpired` if the token's TTL has elapsed.
    pub async fn consume_state(
        &self,
        outbound_token: &str,
    ) -> Result<ConsumedPkceState, PkceError> {
        match self {
            Self::InMemory(s) => s.consume_state_sync(outbound_token),
            #[cfg(feature = "redis-pkce")]
            Self::Redis(s) => s.consume_state_impl(outbound_token).await,
        }
    }

    /// Compute the S256 code challenge for a given verifier.
    ///
    /// Per RFC 7636 §4.2:
    /// `code_challenge = BASE64URL(SHA256(ASCII(code_verifier)))`
    /// (no padding).
    #[must_use] 
    pub fn s256_challenge(verifier: &str) -> String {
        URL_SAFE_NO_PAD.encode(Sha256::digest(verifier.as_bytes()))
    }

    /// Remove expired entries from the in-memory store.
    ///
    /// No-op for the Redis backend — Redis TTL handles expiry automatically.
    /// Call from a background task on a fixed interval for the in-memory
    /// backend to reclaim memory and free capacity below `MAX_PKCE_ENTRIES`.
    pub async fn cleanup_expired(&self) {
        match self {
            Self::InMemory(s) => s.cleanup_expired_sync(),
            #[cfg(feature = "redis-pkce")]
            Self::Redis(_) => {}, // Redis TTL handles expiry
        }
    }

    /// Synchronously remove all expired entries from the in-memory store.
    ///
    /// Identical to [`Self::cleanup_expired`] but callable from synchronous
    /// contexts (e.g. maintenance hooks, benchmarks). No-op for the Redis
    /// backend.
    pub fn purge_expired(&self) {
        match self {
            Self::InMemory(s) => s.purge_expired(),
            #[cfg(feature = "redis-pkce")]
            Self::Redis(_) => {}, // Redis TTL handles expiry
        }
    }

    /// Number of entries currently in the store.
    ///
    /// Returns 0 for the Redis backend — Redis state is not enumerable locally.
    #[must_use] 
    pub fn len(&self) -> usize {
        match self {
            Self::InMemory(s) => s.len_sync(),
            #[cfg(feature = "redis-pkce")]
            Self::Redis(_) => 0,
        }
    }

    /// Returns `true` when the in-memory store contains no entries.
    ///
    /// Always returns `true` for the Redis backend.
    #[must_use] 
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

// ---------------------------------------------------------------------------
// Unit tests
// ---------------------------------------------------------------------------
