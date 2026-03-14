//! JWT attack detection tests.
//!
//! Each test:
//! 1. Mints a cryptographically signed JWT via `fraiseql_auth::jwt`.
//! 2. Applies an attack vector (payload tampering, algorithm swap, expiry manipulation, malformed
//!    structure, …).
//! 3. Calls the real [`JwtValidator`] and asserts the exact [`AuthError`] variant returned —
//!    **not** just that two strings differ.
//!
//! **Execution engine:** none
//! **Infrastructure:** none
//! **Parallelism:** safe

use std::collections::HashMap;

use fraiseql_auth::{
    error::AuthError,
    jwt::{Claims, JwtValidator, generate_hs256_token},
};
use jsonwebtoken::Algorithm;

/// 32-byte test secret — meets HS256 minimum key-length requirements.
const SECRET: &[u8] = b"fraiseql-test-secret-exactly-32b";
const ISSUER: &str = "https://test.fraiseql.dev";

/// Returns `(now, now + 3600)` as Unix timestamps.
fn timestamps() -> (u64, u64) {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .expect("system time before epoch")
        .as_secs();
    (now, now + 3600)
}

fn valid_claims() -> Claims {
    let (iat, exp) = timestamps();
    Claims {
        sub: "user123".to_string(),
        iat,
        exp,
        iss: ISSUER.to_string(),
        aud: vec!["test".to_string()],
        extra: HashMap::new(),
    }
}

fn validator() -> JwtValidator {
    JwtValidator::new(ISSUER, Algorithm::HS256)
        .expect("validator creation failed")
        .with_audiences(&["test"])
        .expect("audience configuration failed")
}

// ============================================================================
// Happy path
// ============================================================================

#[test]
fn test_valid_token_accepted() {
    let claims = valid_claims();
    let token = generate_hs256_token(&claims, SECRET).expect("token generation failed");
    let result = validator().validate_hmac(&token, SECRET);
    assert!(result.is_ok(), "Valid token should be accepted; got: {result:?}");
    let validated = result.expect("valid token should unwrap");
    assert_eq!(validated.sub, "user123");
    assert_eq!(validated.iss, ISSUER);
}

// ============================================================================
// Cycle 1: Payload tampering with original signature
// ============================================================================

/// A valid JWT signature covers header+payload.  If we swap the payload for a
/// different one while keeping the original signature, `validate_hmac` must
/// reject with `InvalidSignature`.
#[test]
fn test_tampered_payload_rejected() {
    let claims = valid_claims();
    let token = generate_hs256_token(&claims, SECRET).expect("token generation failed");

    let mut parts: Vec<&str> = token.splitn(3, '.').collect();
    // Replace payload with elevated-privilege claims (still valid base64-url)
    let evil_payload = base64_url_no_pad(
        br#"{"sub":"admin","exp":9999999999,"iss":"https://test.fraiseql.dev","iat":0,"aud":[]}"#,
    );
    parts[1] = &evil_payload;
    let tampered = parts.join(".");

    let err = validator()
        .validate_hmac(&tampered, SECRET)
        .expect_err("tampered payload must be rejected");
    assert!(
        matches!(err, AuthError::InvalidSignature | AuthError::InvalidToken { .. }),
        "expected signature or token error; got: {err:?}"
    );
}

/// Replacing just the signature with a different HMAC (wrong key) must be rejected.
#[test]
fn test_wrong_key_rejected() {
    let claims = valid_claims();
    let token = generate_hs256_token(&claims, SECRET).expect("token generation failed");

    let wrong_secret = b"wrong-secret-key-exactly-32-byte";
    let err = validator()
        .validate_hmac(&token, wrong_secret)
        .expect_err("token signed with different key must be rejected");
    assert!(
        matches!(err, AuthError::InvalidSignature | AuthError::InvalidToken { .. }),
        "expected signature error; got: {err:?}"
    );
}

// ============================================================================
// Cycle 2: Algorithm attacks
// ============================================================================

/// Replace the `alg` header with `"none"` to bypass signature verification.
/// The validator only accepts HS256 — the `none` algorithm must be rejected.
#[test]
fn test_algorithm_none_rejected() {
    // Build a "none"-algorithm token manually (header.payload.)
    let header = base64_url_no_pad(br#"{"alg":"none","typ":"JWT"}"#);
    let (iat, exp) = timestamps();
    let payload_json =
        format!(r#"{{"sub":"admin","exp":{exp},"iat":{iat},"iss":"{ISSUER}","aud":[]}}"#);
    let payload = base64_url_no_pad(payload_json.as_bytes());
    let unsigned_token = format!("{header}.{payload}.");

    let err = validator()
        .validate_hmac(&unsigned_token, SECRET)
        .expect_err("alg:none token must be rejected");
    assert!(
        matches!(err, AuthError::InvalidToken { .. } | AuthError::InvalidSignature),
        "expected invalid token error; got: {err:?}"
    );
}

/// Swap `alg` from HS256 to RS256 in the header (the signature stays HS256).
/// The decoder must reject it because the algorithm in the header does not
/// match the validator's expected algorithm.
#[test]
fn test_algorithm_mismatch_rejected() {
    let claims = valid_claims();
    let token = generate_hs256_token(&claims, SECRET).expect("token generation failed");

    // Replace first segment (header) with an RS256 header
    let rs256_header = base64_url_no_pad(br#"{"alg":"RS256","typ":"JWT"}"#);
    let rest = token.split_once('.').map(|x| x.1).expect("token has two dots");
    let modified = format!("{rs256_header}.{rest}");

    let err = validator()
        .validate_hmac(&modified, SECRET)
        .expect_err("algorithm-mismatch token must be rejected");
    assert!(
        matches!(err, AuthError::InvalidToken { .. } | AuthError::InvalidSignature),
        "expected invalid token; got: {err:?}"
    );
}

// ============================================================================
// Cycle 3: Expiration enforcement
// ============================================================================

/// A token with `exp` in the past must be rejected with `TokenExpired`.
#[test]
fn test_expired_token_rejected() {
    let (iat, _) = timestamps();
    let mut claims = valid_claims();
    claims.iat = iat - 7200;
    claims.exp = iat - 3600; // expired 1 h ago

    let token = generate_hs256_token(&claims, SECRET).expect("token generation failed");

    let err = validator()
        .validate_hmac(&token, SECRET)
        .expect_err("expired token must be rejected");
    assert!(matches!(err, AuthError::TokenExpired), "expected TokenExpired; got: {err:?}");
}

/// A token expiring far in the future must still be accepted today.
#[test]
fn test_long_lived_valid_token_accepted() {
    let (iat, _) = timestamps();
    let mut claims = valid_claims();
    claims.exp = iat + 365 * 24 * 3600; // 1 year

    let token = generate_hs256_token(&claims, SECRET).expect("token generation failed");
    assert!(
        validator().validate_hmac(&token, SECRET).is_ok(),
        "long-lived token should be accepted"
    );
}

// ============================================================================
// Cycle 4: Issuer mismatch
// ============================================================================

/// A valid token issued by a different issuer must be rejected.
#[test]
fn test_wrong_issuer_rejected() {
    let mut claims = valid_claims();
    claims.iss = "https://evil.attacker.com".to_string();

    let token = generate_hs256_token(&claims, SECRET).expect("token generation failed");

    let err = validator()
        .validate_hmac(&token, SECRET)
        .expect_err("wrong-issuer token must be rejected");
    assert!(
        matches!(err, AuthError::InvalidToken { .. }),
        "expected InvalidToken for wrong issuer; got: {err:?}"
    );
}

// ============================================================================
// Cycle 5: Malformed token structure
// ============================================================================

/// A token with only one segment (no dots) must be rejected.
#[test]
fn test_missing_segments_rejected() {
    let err = validator()
        .validate_hmac("thisisnotajwt", SECRET)
        .expect_err("one-segment token must be rejected");
    assert!(
        matches!(err, AuthError::InvalidToken { .. }),
        "expected InvalidToken; got: {err:?}"
    );
}

/// An empty token must be rejected.
#[test]
fn test_empty_token_rejected() {
    let err = validator().validate_hmac("", SECRET).expect_err("empty token must be rejected");
    assert!(
        matches!(err, AuthError::InvalidToken { .. }),
        "expected InvalidToken; got: {err:?}"
    );
}

/// A token with only two segments (missing signature) must be rejected.
#[test]
fn test_missing_signature_rejected() {
    let claims = valid_claims();
    let token = generate_hs256_token(&claims, SECRET).expect("token generation failed");
    // Keep header.payload, drop signature
    let no_sig: String = token.rsplitn(2, '.').last().unwrap_or("").to_string();

    let err = validator()
        .validate_hmac(&no_sig, SECRET)
        .expect_err("two-segment token must be rejected");
    assert!(
        matches!(err, AuthError::InvalidToken { .. } | AuthError::InvalidSignature),
        "expected token error; got: {err:?}"
    );
}

/// A token with extra segments (four dots) must be rejected.
#[test]
fn test_extra_segments_rejected() {
    let claims = valid_claims();
    let token = generate_hs256_token(&claims, SECRET).expect("token generation failed");
    let bloated = format!("{token}.extrapart");

    let err = validator()
        .validate_hmac(&bloated, SECRET)
        .expect_err("four-segment token must be rejected");
    assert!(
        matches!(err, AuthError::InvalidToken { .. } | AuthError::InvalidSignature),
        "expected token error; got: {err:?}"
    );
}

// ============================================================================
// Cycle 6: Cross-user / cross-tenant token swap
// ============================================================================

/// Token minted for `tenant-A` must not validate when used with a different
/// secret (simulating a different tenant's signing key).
#[test]
fn test_cross_tenant_token_rejected() {
    let tenant_a_secret = b"tenant-a-signing-secret-32bytes!";
    let tenant_b_secret = b"tenant-b-signing-secret-32bytes!";

    let mut claims = valid_claims();
    claims.sub = "user-in-tenant-a".to_string();

    let token_a = generate_hs256_token(&claims, tenant_a_secret).expect("token A generation");

    // Tenant B's validator uses a different secret
    let err = JwtValidator::new(ISSUER, Algorithm::HS256)
        .expect("validator")
        .validate_hmac(&token_a, tenant_b_secret)
        .expect_err("cross-tenant token must be rejected");
    assert!(
        matches!(err, AuthError::InvalidSignature | AuthError::InvalidToken { .. }),
        "expected signature error; got: {err:?}"
    );
}

// ============================================================================
// Helpers
// ============================================================================

/// Base64-URL encode `input` without padding (`=` chars).
fn base64_url_no_pad(input: &[u8]) -> String {
    use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};
    URL_SAFE_NO_PAD.encode(input)
}
