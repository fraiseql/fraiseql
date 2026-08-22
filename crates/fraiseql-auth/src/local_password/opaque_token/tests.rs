//! Unit tests for the selector + verifier opaque-token codec (no database).

use base64::engine::general_purpose::URL_SAFE_NO_PAD;

use super::*;
use crate::error::AuthError;

/// Flow label; the codec is flow-agnostic, so any label exercises the same paths.
const KIND: &str = "test";

#[test]
fn generate_then_parse_round_trips_selector_and_verifier_hash() {
    let token = OpaqueToken::generate();
    let parsed =
        OpaqueToken::parse(KIND, &token.to_token_string()).expect("freshly generated token parses");

    assert_eq!(parsed.selector, token.selector_b64(), "selector survives the round-trip");
    assert_eq!(
        parsed.verifier_hash,
        token.verifier_hash(),
        "sha256(verifier) matches the value the store would hold"
    );
}

#[test]
fn token_string_has_two_base64url_halves() {
    let token = OpaqueToken::generate();
    let s = token.to_token_string();
    assert_eq!(s.split('.').count(), 2, "token is exactly selector.verifier");
    // URL-safe base64 with no padding: no '+', '/', or '='.
    assert!(!s.contains('+') && !s.contains('/') && !s.contains('='), "url-safe, unpadded");
}

#[test]
fn distinct_generations_are_unique() {
    let a = OpaqueToken::generate();
    let b = OpaqueToken::generate();
    assert_ne!(a.to_token_string(), b.to_token_string(), "CSPRNG yields unique tokens");
    assert_ne!(a.verifier_hash(), b.verifier_hash(), "verifier hashes differ");
}

#[test]
fn verifier_hash_is_sha256_width() {
    let token = OpaqueToken::generate();
    assert_eq!(token.verifier_hash().len(), 32, "SHA-256 digest is 32 bytes");
}

#[test]
fn parse_rejects_missing_dot() {
    let err = OpaqueToken::parse(KIND, "no-separator-here");
    assert!(matches!(err, Err(AuthError::InvalidToken { .. })), "missing dot is rejected");
}

#[test]
fn parse_rejects_too_many_dots() {
    // split_once keeps everything after the first '.', so a second dot makes the verifier
    // half undecodable as base64url.
    let token = OpaqueToken::generate();
    let bad = format!("{}.extra", token.to_token_string());
    assert!(matches!(OpaqueToken::parse(KIND, &bad), Err(AuthError::InvalidToken { .. })));
}

#[test]
fn parse_rejects_non_base64() {
    let bad = "!!!not-base64!!!.****also-not****";
    assert!(matches!(OpaqueToken::parse(KIND, bad), Err(AuthError::InvalidToken { .. })));
}

#[test]
fn parse_rejects_wrong_length_halves() {
    // Valid base64url but decoding to the wrong byte lengths (4 bytes each).
    let short = URL_SAFE_NO_PAD.encode([1u8, 2, 3, 4]);
    let bad = format!("{short}.{short}");
    assert!(matches!(OpaqueToken::parse(KIND, &bad), Err(AuthError::InvalidToken { .. })));
}

#[test]
fn parse_accepts_a_real_token() {
    let token = OpaqueToken::generate();
    assert!(OpaqueToken::parse(KIND, &token.to_token_string()).is_ok());
}
