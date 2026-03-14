//! SR-7: R2b — `authorization_url()` generated OAuth state but never returned it,
//!       making CSRF verification impossible.
//!       Fix: `create_state()` returns the outbound state token, and `consume_state()`
//!       with a mismatched token is rejected — ensuring one-to-one callback binding.
//!
//! These tests exercise the `PkceStateStore` public API, which is the layer that
//! enforces PKCE CSRF protection. The store creates a one-shot state token that:
//! 1. Must be included in the authorization URL (so the IdP echoes it back).
//! 2. Is consumed exactly once by `consume_state()`.
//! 3. Cannot be guessed or forged (random 32-byte token).
//!
//! **Infrastructure:** none
//! **Parallelism:** safe

#![allow(clippy::unwrap_used)] // Reason: test code, panics are acceptable
#![allow(clippy::doc_markdown)] // Reason: doc comments use terms like "IdP" informally

use fraiseql_auth::{PkceError, PkceStateStore};

// ---------------------------------------------------------------------------
// SR-7 regression tests
// ---------------------------------------------------------------------------

/// `create_state` must return a non-empty state token.
///
/// R2b regression: before the fix, the state was generated but not returned to
/// the caller, making it impossible to include in the authorization URL or
/// verify in the callback.
#[tokio::test]
async fn create_state_returns_non_empty_state_token() {
    let store = PkceStateStore::new(300, None);
    let redirect_uri = "https://app.example.com/callback";

    let (token, verifier) =
        store.create_state(redirect_uri).await.expect("create_state must not fail");

    assert!(!token.is_empty(), "R2b regression: create_state returned empty state token");
    assert!(!verifier.is_empty(), "create_state returned empty code verifier");
}

/// Consuming the state with a completely wrong token must be rejected with
/// `StateNotFound`. This is the CSRF protection: an attacker who does not
/// possess the state token cannot complete the callback.
#[tokio::test]
async fn consume_state_with_wrong_token_is_rejected_as_csrf() {
    let store = PkceStateStore::new(300, None);

    // Generate a real state
    let (_real_token, _verifier) = store
        .create_state("https://app.example.com/callback")
        .await
        .expect("create_state must succeed");

    // Attempt to consume with an attacker-controlled fake token
    let result = store.consume_state("attacker_forged_state_token_00000000").await;

    assert!(result.is_err(), "R2b regression: CSRF check accepted mismatched state token");
    assert!(
        matches!(result, Err(PkceError::StateNotFound)),
        "CSRF rejection must return StateNotFound, got: {result:?}"
    );
}

/// The correct state token must be accepted exactly once.
///
/// This is the normal happy-path: the IdP echoes back the exact state token
/// that was stored, and `consume_state` returns the associated PKCE verifier.
#[tokio::test]
async fn consume_state_with_correct_token_succeeds() {
    let store = PkceStateStore::new(300, None);
    let redirect_uri = "https://app.example.com/callback";

    let (token, verifier) =
        store.create_state(redirect_uri).await.expect("create_state must succeed");

    let consumed = store.consume_state(&token).await.expect("consume_state must succeed");

    assert_eq!(
        consumed.verifier, verifier,
        "R2b regression: consumed verifier does not match created verifier"
    );
    assert_eq!(
        consumed.redirect_uri, redirect_uri,
        "consume_state must return the original redirect_uri unchanged"
    );
}

/// State tokens are one-shot — consuming the same token twice must fail.
///
/// This prevents replay attacks: if an attacker intercepts the callback URL
/// and replays it, the second call must be rejected.
#[tokio::test]
async fn state_token_is_consumed_exactly_once() {
    let store = PkceStateStore::new(300, None);

    let (token, _) = store
        .create_state("https://app.example.com/callback")
        .await
        .expect("create_state must succeed");

    // First consume: succeeds.
    store.consume_state(&token).await.expect("first consume must succeed");

    // Second consume: must fail — state is atomically removed on first consume.
    let second = store.consume_state(&token).await;
    assert!(
        matches!(second, Err(PkceError::StateNotFound)),
        "R2b regression: state token was accepted twice (replay not blocked); got: {second:?}"
    );
}

/// Two independently-created state tokens must be independent — consuming one
/// must not affect the other.
#[tokio::test]
async fn independent_state_tokens_do_not_interfere() {
    let store = PkceStateStore::new(300, None);

    let (token_a, _) = store.create_state("https://app.example.com/a").await.unwrap();
    let (token_b, _) = store.create_state("https://app.example.com/b").await.unwrap();

    // Tokens must be distinct
    assert_ne!(token_a, token_b, "state tokens must be unique");

    // Consuming token_a must not remove token_b
    store.consume_state(&token_a).await.expect("consume token_a must succeed");
    let result_b = store.consume_state(&token_b).await;
    assert!(
        result_b.is_ok(),
        "R2b regression: consuming token_a removed token_b; got: {result_b:?}"
    );
}
