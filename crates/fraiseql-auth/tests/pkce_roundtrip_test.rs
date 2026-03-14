//! Integration tests for the PKCE state store lifecycle.
//!
//! These tests cover the full `create_state` → `consume_state` → reject-reuse cycle
//! using the public [`PkceStateStore`] API, with and without [`StateEncryptionService`].
//!
//! They complement the unit tests in `pkce.rs` by exercising the public interface
//! from outside the crate — the way downstream users (e.g. `fraiseql-server`) see it.

use std::sync::Arc;

use fraiseql_auth::{EncryptionAlgorithm, PkceError, PkceStateStore, StateEncryptionService};

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// A deterministic 32-byte key for test encryption instances.
fn enc_svc() -> Arc<StateEncryptionService> {
    Arc::new(StateEncryptionService::from_raw_key(
        &[0xABu8; 32],
        EncryptionAlgorithm::Chacha20Poly1305,
    ))
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

/// Full PKCE lifecycle with encryption enabled:
/// `create_state` → `consume_state` → verify fields → reject second consume.
///
/// This validates that:
/// - The outbound token is opaque (does not expose the verifier in plaintext).
/// - Encrypted tokens are longer than a raw 32-byte URL-safe base64 key.
/// - The verifier and redirect URI survive the full round-trip unchanged.
/// - One-shot guarantee: a second `consume_state` call returns `StateNotFound`.
#[tokio::test]
async fn test_pkce_state_roundtrip_with_encryption() {
    let store = PkceStateStore::new(300, Some(enc_svc()));
    let redirect_uri = "https://app.example.com/callback";

    let (token, verifier) =
        store.create_state(redirect_uri).await.expect("create_state must succeed");

    // The token must not contain the raw verifier — it is encrypted and base64-encoded.
    assert!(!token.contains(&verifier), "token must not contain plaintext verifier");

    // Encrypted tokens are longer than a raw 32-byte URL-safe base64 key (43 chars).
    // The 12-byte nonce and 16-byte auth tag add at least 28 bytes before base64 expansion.
    assert!(
        token.len() > 43,
        "encrypted token (len={}) should be longer than a raw key (43 chars)",
        token.len(),
    );

    // First consume: succeeds and returns the original fields.
    let consumed = store
        .consume_state(&token)
        .await
        .expect("consume_state must succeed on first call");

    assert_eq!(consumed.verifier, verifier, "verifier must round-trip unchanged");
    assert_eq!(consumed.redirect_uri, redirect_uri, "redirect_uri must round-trip unchanged");

    // One-shot guarantee: the state slot is atomically removed on consume.
    let second = store.consume_state(&token).await;
    assert!(
        matches!(second, Err(PkceError::StateNotFound)),
        "second consume must return StateNotFound — state is one-shot; got: {second:?}",
    );
}

/// Full PKCE lifecycle WITHOUT encryption: the plaintext internal key is the token.
///
/// Verifies the same lifecycle invariants hold when no encryption is configured,
/// matching the single-replica development scenario.
#[tokio::test]
async fn test_pkce_state_roundtrip_without_encryption() {
    let store = PkceStateStore::new(300, None);
    let redirect_uri = "https://app.example.com/callback";

    let (token, verifier) =
        store.create_state(redirect_uri).await.expect("create_state must succeed");

    let consumed = store
        .consume_state(&token)
        .await
        .expect("consume_state must succeed on first call");

    assert_eq!(consumed.verifier, verifier, "verifier must round-trip unchanged");
    assert_eq!(consumed.redirect_uri, redirect_uri, "redirect_uri must round-trip unchanged");

    // One-shot guarantee holds even without encryption.
    let second = store.consume_state(&token).await;
    assert!(
        matches!(second, Err(PkceError::StateNotFound)),
        "second consume must return StateNotFound — state is one-shot; got: {second:?}",
    );
}

/// A token whose ciphertext has been tampered with must be rejected.
///
/// We replace a character in the middle of the base64-encoded token.
/// Because the AEAD authentication tag covers the full ciphertext, even a
/// single-byte change causes decryption to fail, returning `StateNotFound`.
///
/// This ensures the tamper-evidence property of ChaCha20-Poly1305 is wired
/// through the `PkceStateStore` public API — a client cannot forge a valid
/// state token without possessing the server's encryption key.
#[tokio::test]
async fn test_pkce_state_tampered_token_rejected() {
    let store = PkceStateStore::new(300, Some(enc_svc()));

    let (token, _) = store
        .create_state("https://app.example.com/callback")
        .await
        .expect("create_state must succeed");

    // Replace a character in the ciphertext portion of the base64-encoded token.
    // The first ~16 characters encode the 12-byte nonce; we target the payload area.
    // Swapping 'A'↔'B' changes the decoded byte at that position while keeping the
    // string within the URL-safe base64 alphabet, exercising AEAD authentication.
    let mid = token.len() / 2;
    let tampered: String = token
        .chars()
        .enumerate()
        .map(|(i, c)| {
            if i == mid {
                if c == 'A' { 'B' } else { 'A' }
            } else {
                c
            }
        })
        .collect();

    assert_ne!(tampered, token, "tampered token must differ from original");

    let result = store.consume_state(&tampered).await;
    assert!(
        matches!(result, Err(PkceError::StateNotFound)),
        "tampered token must be rejected with StateNotFound; got: {result:?}",
    );
}
