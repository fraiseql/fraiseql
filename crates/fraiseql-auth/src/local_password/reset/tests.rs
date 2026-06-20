//! Unit tests for the reset-token primitives (no database).

use super::*;

#[test]
fn generate_then_parse_round_trips_selector_and_verifier_hash() {
    let token = ResetToken::generate();
    let parsed =
        ResetToken::parse(&token.to_token_string()).expect("freshly generated token parses");

    assert_eq!(parsed.selector, token.selector_b64(), "selector survives the round-trip");
    assert_eq!(
        parsed.verifier_hash,
        token.verifier_hash(),
        "sha256(verifier) matches the value the store would hold"
    );
}

#[test]
fn token_string_has_two_base64url_halves() {
    let token = ResetToken::generate();
    let s = token.to_token_string();
    assert_eq!(s.split('.').count(), 2, "token is exactly selector.verifier");
    // URL-safe base64 with no padding: no '+', '/', or '='.
    assert!(!s.contains('+') && !s.contains('/') && !s.contains('='), "url-safe, unpadded");
}

#[test]
fn distinct_generations_are_unique() {
    let a = ResetToken::generate();
    let b = ResetToken::generate();
    assert_ne!(a.to_token_string(), b.to_token_string(), "CSPRNG yields unique tokens");
    assert_ne!(a.verifier_hash(), b.verifier_hash(), "verifier hashes differ");
}

#[test]
fn verifier_hash_is_sha256_width() {
    let token = ResetToken::generate();
    assert_eq!(token.verifier_hash().len(), 32, "SHA-256 digest is 32 bytes");
}

#[test]
fn parse_rejects_missing_dot() {
    let err = ResetToken::parse("no-separator-here");
    assert!(matches!(err, Err(AuthError::InvalidToken { .. })), "missing dot is rejected");
}

#[test]
fn parse_rejects_too_many_dots() {
    // split_once keeps everything after the first '.', so a second dot makes the verifier
    // half undecodable as base64url.
    let token = ResetToken::generate();
    let bad = format!("{}.extra", token.to_token_string());
    assert!(matches!(ResetToken::parse(&bad), Err(AuthError::InvalidToken { .. })));
}

#[test]
fn parse_rejects_non_base64() {
    let bad = "!!!not-base64!!!.****also-not****";
    assert!(matches!(ResetToken::parse(bad), Err(AuthError::InvalidToken { .. })));
}

#[test]
fn parse_rejects_wrong_length_halves() {
    // Valid base64url but decoding to the wrong byte lengths (4 bytes each).
    let short = URL_SAFE_NO_PAD.encode([1u8, 2, 3, 4]);
    let bad = format!("{short}.{short}");
    assert!(matches!(ResetToken::parse(&bad), Err(AuthError::InvalidToken { .. })));
}

#[test]
fn parse_accepts_a_real_token() {
    let token = ResetToken::generate();
    assert!(ResetToken::parse(&token.to_token_string()).is_ok());
}
