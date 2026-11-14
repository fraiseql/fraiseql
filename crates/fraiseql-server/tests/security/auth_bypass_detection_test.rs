//! JWT attack detection tests.
//!
//! Each test:
//! 1. Mints a signed JWT from the shared ES256 test issuer.
//! 2. Applies an attack vector (payload tampering, an unpublished key, algorithm swap, expiry, a
//!    foreign issuer, malformed structure, …).
//! 3. Calls the real [`OidcValidator`], trusting that issuer, and asserts the exact
//!    [`SecurityError`] variant returned — **not** just that two strings differ.
//!
//! **Execution engine:** none
//! **Infrastructure:** an in-process issuer on loopback
//! **Parallelism:** safe

use fraiseql_core::security::{OidcConfig, OidcValidator, SecurityError};
use fraiseql_test_support::issuer::{Claims, TestIssuer};

/// A validator that trusts `issuer`'s keys, for its issuer and audience.
fn validator(issuer: &TestIssuer) -> OidcValidator {
    let config = OidcConfig {
        issuer: Some(issuer.issuer().to_string()),
        audience: Some(issuer.audience().to_string()),
        required: true,
        allowed_algorithms: vec!["ES256".to_string()],
        ..OidcConfig::default()
    };
    OidcValidator::with_jwks_uri(config, &issuer.jwks_uri()).expect("a loopback jwks_uri")
}

fn now() -> i64 {
    let secs = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .expect("system time before epoch")
        .as_secs();
    i64::try_from(secs).expect("epoch seconds fit i64")
}

fn valid_claims(issuer: &TestIssuer) -> Claims {
    issuer.claims().set("sub", "user123")
}

/// `issuer` and the validator that trusts it: the issuer must outlive every validation,
/// because the validator reads its JWKS on first use.
async fn rig() -> (TestIssuer, OidcValidator) {
    let issuer = TestIssuer::start().await;
    let validator = validator(&issuer);
    (issuer, validator)
}

async fn refusal(validator: &OidcValidator, token: &str) -> SecurityError {
    validator.validate_token(token).await.expect_err("the token must be refused")
}

// ============================================================================
// Happy path
// ============================================================================

#[tokio::test]
async fn test_valid_token_accepted() {
    let (issuer, validator) = rig().await;
    let token = issuer.mint(&valid_claims(&issuer));

    let user = validator.validate_token(&token).await.expect("a valid token is accepted");
    assert_eq!(user.user_id.as_str(), "user123");
}

// ============================================================================
// Payload tampering with original signature
// ============================================================================

/// A valid JWT signature covers header+payload. Swapping the payload for a different
/// one while keeping the original signature must be refused.
#[tokio::test]
async fn test_tampered_payload_rejected() {
    let (issuer, validator) = rig().await;
    let token = issuer.mint(&valid_claims(&issuer));

    let mut parts: Vec<&str> = token.splitn(3, '.').collect();
    // Replace the payload with elevated-privilege claims (still valid base64-url).
    let evil_payload = base64_url_no_pad(
        format!(
            r#"{{"sub":"admin","exp":9999999999,"iss":"{}","aud":"{}","iat":0}}"#,
            issuer.issuer(),
            issuer.audience()
        )
        .as_bytes(),
    );
    parts[1] = &evil_payload;
    let tampered = parts.join(".");

    let err = refusal(&validator, &tampered).await;
    assert!(
        matches!(err, SecurityError::InvalidToken),
        "expected InvalidToken; got: {err:?}"
    );
}

/// A token naming the issuer's key but signed by one it does not publish is refused.
#[tokio::test]
async fn test_wrong_key_rejected() {
    let (issuer, validator) = rig().await;
    let token = issuer.foreign_token(&valid_claims(&issuer));

    let err = refusal(&validator, &token).await;
    assert!(
        matches!(err, SecurityError::InvalidToken),
        "expected InvalidToken; got: {err:?}"
    );
}

// ============================================================================
// Algorithm attacks
// ============================================================================

/// Replace the `alg` header with `"none"` to bypass signature verification.
#[tokio::test]
async fn test_algorithm_none_rejected() {
    let (issuer, validator) = rig().await;
    let header = base64_url_no_pad(br#"{"alg":"none","typ":"JWT"}"#);
    let now = now();
    let payload = base64_url_no_pad(
        format!(
            r#"{{"sub":"admin","exp":{},"iat":{now},"iss":"{}","aud":"{}"}}"#,
            now + 3600,
            issuer.issuer(),
            issuer.audience()
        )
        .as_bytes(),
    );
    let unsigned_token = format!("{header}.{payload}.");

    let err = refusal(&validator, &unsigned_token).await;
    assert!(
        matches!(err, SecurityError::InvalidToken | SecurityError::InvalidTokenAlgorithm { .. }),
        "expected an invalid token or a refused algorithm; got: {err:?}"
    );
}

/// Swap `alg` from ES256 to RS256 in the header (the signature stays ES256). The
/// validator allows ES256 alone, so the header is refused.
#[tokio::test]
async fn test_algorithm_mismatch_rejected() {
    let (issuer, validator) = rig().await;
    let token = issuer.mint(&valid_claims(&issuer));

    let rs256_header = base64_url_no_pad(br#"{"alg":"RS256","typ":"JWT"}"#);
    let rest = token.split_once('.').map(|x| x.1).expect("token has two dots");
    let modified = format!("{rs256_header}.{rest}");

    let err = refusal(&validator, &modified).await;
    assert!(
        matches!(err, SecurityError::InvalidToken | SecurityError::InvalidTokenAlgorithm { .. }),
        "expected an invalid token or a refused algorithm; got: {err:?}"
    );
}

// ============================================================================
// Expiration enforcement
// ============================================================================

/// A token with `exp` in the past must be refused as expired.
#[tokio::test]
async fn test_expired_token_rejected() {
    let (issuer, validator) = rig().await;
    let now = now();
    let claims = valid_claims(&issuer)
        .set("iat", now - 7200)
        .set("nbf", now - 7200)
        .set("exp", now - 3600); // expired 1 h ago
    let token = issuer.mint(&claims);

    let err = refusal(&validator, &token).await;
    assert!(
        matches!(err, SecurityError::TokenExpired { .. }),
        "expected TokenExpired; got: {err:?}"
    );
}

/// A token expiring far in the future must still be accepted today.
#[tokio::test]
async fn test_long_lived_valid_token_accepted() {
    let (issuer, validator) = rig().await;
    let claims = valid_claims(&issuer).set("exp", now() + 365 * 24 * 3600); // 1 year
    let token = issuer.mint(&claims);

    assert!(
        validator.validate_token(&token).await.is_ok(),
        "long-lived token should be accepted"
    );
}

// ============================================================================
// Issuer mismatch
// ============================================================================

/// A token naming a different issuer must be refused.
#[tokio::test]
async fn test_wrong_issuer_rejected() {
    let (issuer, validator) = rig().await;
    let token = issuer.mint(&valid_claims(&issuer).set("iss", "https://evil.example.com"));

    let err = refusal(&validator, &token).await;
    assert!(
        matches!(err, SecurityError::InvalidToken),
        "expected InvalidToken; got: {err:?}"
    );
}

// ============================================================================
// Malformed token structure
// ============================================================================

/// A token with only one segment (no dots) must be refused.
#[tokio::test]
async fn test_missing_segments_rejected() {
    let (_issuer, validator) = rig().await;
    let err = refusal(&validator, "thisisnotajwt").await;
    assert!(
        matches!(err, SecurityError::InvalidToken),
        "expected InvalidToken; got: {err:?}"
    );
}

/// An empty token must be refused.
#[tokio::test]
async fn test_empty_token_rejected() {
    let (_issuer, validator) = rig().await;
    let err = refusal(&validator, "").await;
    assert!(
        matches!(err, SecurityError::InvalidToken),
        "expected InvalidToken; got: {err:?}"
    );
}

/// A token with only two segments (missing signature) must be refused.
#[tokio::test]
async fn test_missing_signature_rejected() {
    let (issuer, validator) = rig().await;
    let token = issuer.mint(&valid_claims(&issuer));
    // Keep header.payload, drop the signature.
    let no_sig: String = token.rsplitn(2, '.').last().unwrap_or("").to_string();

    let err = refusal(&validator, &no_sig).await;
    assert!(
        matches!(err, SecurityError::InvalidToken),
        "expected InvalidToken; got: {err:?}"
    );
}

/// A token with extra segments (four dots) must be refused.
#[tokio::test]
async fn test_extra_segments_rejected() {
    let (issuer, validator) = rig().await;
    let token = issuer.mint(&valid_claims(&issuer));
    let bloated = format!("{token}.extrapart");

    let err = refusal(&validator, &bloated).await;
    assert!(
        matches!(err, SecurityError::InvalidToken),
        "expected InvalidToken; got: {err:?}"
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
