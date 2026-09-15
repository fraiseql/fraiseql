#![allow(clippy::unwrap_used)] // Reason: test code, panics are acceptable

use std::collections::BTreeMap;

use super::*;

/// Drive the scheme the way the route does: put the credential under the header
/// the scheme reads, and hand it the whole request.
///
/// The argument order is the one `verify` had before #1321, so the rewrite of
/// these call sites carried no judgement about which value is which.
fn check(
    verifier: &impl SignatureVerifier,
    payload: &[u8],
    signature: &str,
    secret: &str,
) -> Result<Verified, SignatureError> {
    let mut headers = BTreeMap::new();
    headers.insert("X-Gitlab-Token".to_ascii_lowercase(), signature.to_string());
    verifier.verify(&InboundRequest::new(&headers, payload, None), secret)
}

const VERIFIER: GitLabVerifier = GitLabVerifier;
const PAYLOAD: &[u8] = b"{\"object_kind\":\"push\"}";

/// A valid token in the header must be accepted.
#[test]
fn test_valid_token_accepted() {
    let secret = "super-secret-token";
    let result = check(&VERIFIER, PAYLOAD, secret, secret);
    assert_eq!(result.unwrap(), Verified::Body, "matching token must return true");
}

/// A wrong token must be rejected (returns false, not an error).
#[test]
fn test_wrong_token_rejected() {
    let result = check(&VERIFIER, PAYLOAD, "wrong-token", "correct-token");
    assert!(
        matches!(result, Err(SignatureError::Mismatch)),
        "non-matching token must return false"
    );
}

/// An empty secret must return an error (misconfiguration guard).
#[test]
fn test_empty_secret_returns_error() {
    let result = check(&VERIFIER, PAYLOAD, "some-token", "");
    assert!(result.is_err(), "empty secret must return an error");
}

/// Tokens that differ only in length must be rejected (no padding attack).
#[test]
fn test_prefix_match_rejected() {
    // "secret" is a prefix of "secret-extra" — must not accept
    let result = check(&VERIFIER, PAYLOAD, "secret", "secret-extra");
    assert!(
        matches!(result, Err(SignatureError::Mismatch)),
        "prefix match must not be accepted"
    );
}

/// Payload content is irrelevant — GitLab token auth ignores the body.
#[test]
fn test_payload_ignored() {
    let secret = "my-token";
    let r1 = check(&VERIFIER, b"payload-a", secret, secret).unwrap();
    let r2 = check(&VERIFIER, b"payload-b", secret, secret).unwrap();
    assert_eq!(r1, Verified::Body, "result must not depend on payload content");
    assert_eq!(r2, Verified::Body, "result must not depend on payload content");
}
