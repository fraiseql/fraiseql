// PKCE state store — RFC 7636 Proof Key for Code Exchange
//
// Stores `(code_verifier, redirect_uri)` under a random internal key while
// the OAuth2 authorization round-trip is in flight.  The token sent to the
// OIDC provider in the `?state=` query parameter is either:
//   - the raw internal key (no encryption configured), or
//   - `encrypt(internal_key)` (when StateEncryptionService is attached).
//
// All state lives in a DashMap, making `consume_state` an atomic remove that
// prevents any form of state reuse.

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
// Internal store entry
// ---------------------------------------------------------------------------

struct PkceEntry {
    verifier:     String,
    redirect_uri: String,
    created_at:   Instant,
    ttl:          Duration,
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
// PkceStateStore
// ---------------------------------------------------------------------------

/// In-memory PKCE state store backed by a [`DashMap`].
///
/// # State lifecycle
///
/// ```text
/// create_state(redirect_uri)
///   → internal_key = random 32 bytes (base64url)
///   → outbound_token = encrypt(internal_key)  [or internal_key if no encryption]
///   → DashMap.insert(internal_key, {verifier, redirect_uri, now, ttl})
///   → return (outbound_token, verifier)
///
/// consume_state(outbound_token)
///   → internal_key = decrypt(outbound_token)  [or outbound_token if no encryption]
///   → (key, entry) = DashMap.remove(internal_key)?  [StateNotFound if absent]
///   → if entry.elapsed > entry.ttl → StateExpired
///   → return {verifier, redirect_uri}
/// ```
///
/// # Multi-instance limitation
///
/// State is per-process. Restarts and multi-replica deployments will
/// invalidate in-flight auth flows. Log a warning at startup when this
/// matters.
pub struct PkceStateStore {
    /// Seconds a state entry remains valid (from [`PkceConfig`]).
    pub state_ttl_secs: u64,
    entries:            DashMap<String, PkceEntry>,
    encryptor:          Option<Arc<StateEncryptionService>>,
}

impl PkceStateStore {
    /// Create a new store with the given TTL and optional encryption service.
    pub fn new(state_ttl_secs: u64, encryptor: Option<Arc<StateEncryptionService>>) -> Self {
        Self {
            state_ttl_secs,
            entries: DashMap::new(),
            encryptor,
        }
    }

    /// Generate an authorization-code verifier and reserve a state slot.
    ///
    /// Returns `(outbound_token, code_verifier)`:
    /// - `outbound_token` goes in the OIDC `?state=` query parameter.
    /// - `code_verifier` is used by the caller to compute `code_challenge`
    ///   (via [`Self::s256_challenge`]) and is stored here until callback.
    ///
    /// # Errors
    ///
    /// Propagates [`StateEncryptionService::encrypt`] failures (effectively
    /// never, as the cipher is initialized with a valid key).
    pub fn create_state(
        &self,
        redirect_uri: &str,
    ) -> Result<(String, String), anyhow::Error> {
        // ── code_verifier ────────────────────────────────────────────────
        // RFC 7636 §4.1: 43–128 chars, [A-Za-z0-9\-._~] character set.
        // 32 random bytes → 43-char URL_SAFE_NO_PAD base64 satisfies both.
        let mut verifier_bytes = [0u8; 32];
        OsRng.fill_bytes(&mut verifier_bytes);
        let verifier = URL_SAFE_NO_PAD.encode(verifier_bytes);

        // ── internal key ─────────────────────────────────────────────────
        // Separate from the verifier so that clients who see the outbound
        // token (which may be the raw key when encryption is off) still
        // cannot predict or derive the verifier.
        let mut key_bytes = [0u8; 32];
        OsRng.fill_bytes(&mut key_bytes);
        let internal_key = URL_SAFE_NO_PAD.encode(key_bytes);

        self.entries.insert(internal_key.clone(), PkceEntry {
            verifier:    verifier.clone(),
            redirect_uri: redirect_uri.to_owned(),
            created_at:  Instant::now(),
            ttl:         Duration::from_secs(self.state_ttl_secs),
        });

        // ── outbound token ───────────────────────────────────────────────
        let outbound_token = match &self.encryptor {
            Some(enc) => enc.encrypt(internal_key.as_bytes())?,
            None      => internal_key,
        };

        Ok((outbound_token, verifier))
    }

    /// Consume a state token, atomically removing it from the store.
    ///
    /// Returns [`PkceError::StateNotFound`] for:
    /// - tokens that were never issued,
    /// - tokens that have already been consumed (one-time use), and
    /// - tokens that fail decryption (tampered or from a different key).
    ///
    /// Returns [`PkceError::StateExpired`] only when the token is
    /// cryptographically valid but its TTL has elapsed.
    pub fn consume_state(&self, outbound_token: &str) -> Result<ConsumedPkceState, PkceError> {
        // ── recover internal key ──────────────────────────────────────────
        let internal_key = match &self.encryptor {
            Some(enc) => {
                let bytes = enc
                    .decrypt(outbound_token)
                    .map_err(|_| PkceError::StateNotFound)?;
                // Decryption failure (tampered token) → StateNotFound.
                // Clients get no information about whether decryption or
                // the DashMap lookup failed.
                String::from_utf8(bytes).map_err(|_| PkceError::StateNotFound)?
            }
            None => outbound_token.to_owned(),
        };

        // ── atomic remove ────────────────────────────────────────────────
        let (_, entry) = self.entries.remove(&internal_key).ok_or(PkceError::StateNotFound)?;

        // ── TTL check ────────────────────────────────────────────────────
        if entry.created_at.elapsed() > entry.ttl {
            return Err(PkceError::StateExpired);
        }

        Ok(ConsumedPkceState {
            verifier:     entry.verifier,
            redirect_uri: entry.redirect_uri,
        })
    }

    /// Compute the S256 code challenge for a given verifier.
    ///
    /// Per RFC 7636 §4.2:
    /// `code_challenge = BASE64URL(SHA256(ASCII(code_verifier)))`
    /// (no padding).
    pub fn s256_challenge(verifier: &str) -> String {
        URL_SAFE_NO_PAD.encode(Sha256::digest(verifier.as_bytes()))
    }

    /// Remove all entries whose TTL has elapsed.
    ///
    /// Call from a background task on a fixed interval (e.g. every 5 minutes)
    /// to prevent unbounded memory growth.
    pub fn cleanup_expired(&self) {
        self.entries.retain(|_, e| e.created_at.elapsed() <= e.ttl);
    }

    /// Number of entries currently in the store.
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// Returns `true` when the store contains no entries.
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
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

    #[test]
    fn test_create_and_consume_roundtrip() {
        let store = store_no_enc(600);
        let (token, verifier) = store.create_state("https://app.example.com/cb").unwrap();
        let result = store.consume_state(&token).unwrap();
        assert_eq!(result.verifier, verifier);
        assert_eq!(result.redirect_uri, "https://app.example.com/cb");
    }

    #[test]
    fn test_consume_removes_entry_cannot_reuse() {
        let store = store_no_enc(600);
        let (token, _) = store.create_state("https://app.example.com/cb").unwrap();
        store.consume_state(&token).unwrap();
        assert!(
            matches!(store.consume_state(&token), Err(PkceError::StateNotFound)),
            "second consume must return StateNotFound"
        );
    }

    #[test]
    fn test_expired_state_returns_state_expired_not_not_found() {
        // TTL = 1 second; sleep 1.1s to ensure expiry before consume.
        let store = store_no_enc(1);
        let (token, _) = store.create_state("https://example.com").unwrap();
        std::thread::sleep(Duration::from_millis(1100));
        assert!(
            matches!(store.consume_state(&token), Err(PkceError::StateExpired)),
            "expired state must be StateExpired, not StateNotFound"
        );
    }

    #[test]
    fn test_unknown_token_returns_not_found() {
        let store = store_no_enc(600);
        assert!(matches!(
            store.consume_state("completely-unknown-token"),
            Err(PkceError::StateNotFound)
        ));
    }

    #[test]
    fn test_two_distinct_states_dont_interfere() {
        let store = store_no_enc(600);
        let (t1, v1) = store.create_state("https://a.example.com/cb").unwrap();
        let (t2, v2) = store.create_state("https://b.example.com/cb").unwrap();
        let r2 = store.consume_state(&t2).unwrap();
        let r1 = store.consume_state(&t1).unwrap();
        assert_eq!(r1.verifier, v1);
        assert_eq!(r2.verifier, v2);
    }

    // ── RFC 7636 compliance ───────────────────────────────────────────────────

    #[test]
    fn test_s256_challenge_matches_rfc7636_appendix_a() {
        // Test vector from RFC 7636 §Appendix A.
        let verifier  = "dBjftJeZ4CVP-mB92K27uhbUJU1p1r_wW1gFWFOEjXk";
        let expected  = "E9Melhoa2OwvFrEMTJguCHaoeK1t8URWbuGJSstw-cM";
        assert_eq!(PkceStateStore::s256_challenge(verifier), expected);
    }

    #[test]
    fn test_verifier_length_and_charset_are_rfc7636_compliant() {
        // RFC 7636 §4.1: 43–128 chars, no padding characters.
        let store = store_no_enc(600);
        let (_, verifier) = store.create_state("https://example.com").unwrap();
        assert!(
            verifier.len() >= 43 && verifier.len() <= 128,
            "verifier length {} is outside the 43–128 char range",
            verifier.len()
        );
        assert!(!verifier.contains('='), "verifier must not contain padding characters");
    }

    // ── Encryption integration ────────────────────────────────────────────────

    #[test]
    fn test_encrypted_token_is_longer_than_raw_internal_key() {
        // Raw internal keys are 43 chars (32 bytes base64url no-pad).
        // Encrypted tokens include a nonce prefix and auth tag → always longer.
        let store = PkceStateStore::new(600, Some(enc_service()));
        let (token, _) = store.create_state("https://app.example.com/cb").unwrap();
        assert!(
            token.len() > 43,
            "encrypted token (len={}) must be longer than a raw 32-byte key (43 chars)",
            token.len()
        );
    }

    #[test]
    fn test_encrypted_roundtrip_works_end_to_end() {
        let store = PkceStateStore::new(600, Some(enc_service()));
        let (token, verifier) = store.create_state("https://app.example.com/cb").unwrap();
        let result = store.consume_state(&token).unwrap();
        assert_eq!(result.verifier, verifier);
    }

    #[test]
    fn test_tampered_encrypted_token_returns_not_found() {
        let store = PkceStateStore::new(600, Some(enc_service()));
        store.create_state("https://app.example.com/cb").unwrap();
        // A completely fabricated token will fail decryption.
        let result = store.consume_state("aGVsbG8gd29ybGQ");
        assert!(
            matches!(result, Err(PkceError::StateNotFound)),
            "tampered token must yield StateNotFound, not an internal error"
        );
    }

    // ── Cleanup ───────────────────────────────────────────────────────────────

    #[test]
    fn test_cleanup_removes_expired_leaves_valid() {
        // 1-second TTL store: create one entry, let it expire, then cleanup.
        let store = store_no_enc(1);
        store.create_state("https://a.example.com").unwrap();
        std::thread::sleep(Duration::from_millis(1100));
        store.cleanup_expired();
        assert_eq!(store.len(), 0, "expired entry must be removed by cleanup");

        // A fresh entry is unaffected.
        let store2 = store_no_enc(600);
        store2.create_state("https://b.example.com").unwrap();
        store2.cleanup_expired();
        assert_eq!(store2.len(), 1, "unexpired entry must survive cleanup");
    }
}
