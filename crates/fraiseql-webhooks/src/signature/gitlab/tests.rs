#![allow(clippy::unwrap_used)] // Reason: test code, panics are acceptable

use super::*;

const VERIFIER: GitLabVerifier = GitLabVerifier;
const PAYLOAD: &[u8] = b"{\"object_kind\":\"push\"}";

/// A valid token in the header must be accepted.
#[test]
fn test_valid_token_accepted() {
    let secret = "super-secret-token";
    let result = VERIFIER.verify(PAYLOAD, secret, secret, None, None);
    assert!(result.unwrap(), "matching token must return true");
}

/// A wrong token must be rejected (returns false, not an error).
#[test]
fn test_wrong_token_rejected() {
    let result = VERIFIER.verify(PAYLOAD, "wrong-token", "correct-token", None, None);
    assert!(!result.unwrap(), "non-matching token must return false");
}

/// An empty secret must return an error (misconfiguration guard).
#[test]
fn test_empty_secret_returns_error() {
    let result = VERIFIER.verify(PAYLOAD, "some-token", "", None, None);
    assert!(result.is_err(), "empty secret must return an error");
}

/// Tokens that differ only in length must be rejected (no padding attack).
#[test]
fn test_prefix_match_rejected() {
    // "secret" is a prefix of "secret-extra" — must not accept
    let result = VERIFIER.verify(PAYLOAD, "secret", "secret-extra", None, None);
    assert!(!result.unwrap(), "prefix match must not be accepted");
}

/// Payload content is irrelevant — GitLab token auth ignores the body.
#[test]
fn test_payload_ignored() {
    let secret = "my-token";
    let r1 = VERIFIER.verify(b"payload-a", secret, secret, None, None).unwrap();
    let r2 = VERIFIER.verify(b"payload-b", secret, secret, None, None).unwrap();
    assert!(r1 && r2, "result must not depend on payload content");
}
