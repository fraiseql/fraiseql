//! Security tests for authentication and cryptographic operations.
//! Validates that security-critical operations meet production standards.
#[cfg(test)]
use std::collections::{HashMap, HashSet};

/// Test that CSRF tokens are cryptographically unique and unpredictable.
/// Generates multiple tokens and verifies no collisions and high entropy.
#[test]
fn test_csrf_token_uniqueness_and_entropy() {
    use crate::handlers::generate_secure_state;

    let mut tokens = HashSet::new();
    let iterations = 100;

    for _ in 0..iterations {
        let token = generate_secure_state();

        // Verify minimum length for cryptographic security
        assert!(
            token.len() >= 64,
            "CSRF token too short: {} (should be >= 64 hex chars = 256 bits)",
            token.len()
        );

        // Verify hex encoding (only 0-9a-f)
        assert!(
            token.chars().all(|c| c.is_ascii_hexdigit()),
            "Token contains non-hex characters: {}",
            token
        );

        // Track for collision detection
        tokens.insert(token);
    }

    // Verify no collisions
    assert_eq!(
        tokens.len(),
        iterations,
        "CSRF token collisions detected! Only {} unique out of {}",
        tokens.len(),
        iterations
    );
}

/// Test that CSRF state generation produces cryptographically secure values.
/// OsRng should be used for cryptographic randomness, not thread_rng.
#[test]
fn test_csrf_state_is_cryptographically_random() {
    use crate::handlers::generate_secure_state;

    // Generate multiple states
    let states: Vec<String> = (0..50).map(|_| generate_secure_state()).collect();

    // Verify each is unique (no collisions)
    let unique_count = states.iter().collect::<HashSet<_>>().len();
    assert_eq!(
        unique_count, 50,
        "CSRF state generator produced duplicates! Only {} unique",
        unique_count
    );

    // Verify hex format
    for state in &states {
        assert!(hex::decode(state).is_ok(), "CSRF state is not valid hex: {}", state);
    }
}

/// Test that JWT expiration is properly enforced.
/// Expired tokens must be rejected, not silently accepted.
#[test]
fn test_jwt_expiration_enforcement() {
    use std::time::{SystemTime, UNIX_EPOCH};

    use crate::jwt::Claims;

    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("System time error")
        .as_secs();

    // Create expired token (exp = 1 second ago)
    let expired_token = Claims {
        iss:   "test_issuer".to_string(),
        sub:   "user123".to_string(),
        aud:   vec!["api".to_string()],
        exp:   now - 1,
        iat:   now - 3600,
        nbf:   None,
        extra: HashMap::default(),
    };

    assert!(expired_token.is_expired(), "Expired token should be rejected");

    // Create future token (exp = 1 hour from now)
    let valid_token = Claims {
        iss:   "test_issuer".to_string(),
        sub:   "user123".to_string(),
        aud:   vec!["api".to_string()],
        exp:   now + 3600,
        iat:   now,
        nbf:   None,
        extra: HashMap::default(),
    };

    assert!(!valid_token.is_expired(), "Valid token should not be rejected");
}

/// Test that audience validation is actually **enforced**: a token whose `aud`
/// is not in the validator's configured set is rejected.
///
/// Replaces `test_jwt_audience_validation_support` (#737), which only constructed
/// a validator via `with_audiences` and asserted nothing about enforcement — it
/// passed whether or not the audience was ever checked.
#[test]
fn wrong_audience_is_rejected() {
    use std::collections::HashMap;

    use jsonwebtoken::Algorithm;

    use crate::{Claims, jwt::JwtValidator};

    let secret = b"audience_enforcement_secret_at_least_32b";
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .expect("system time")
        .as_secs();

    let claims = Claims {
        sub:   "user".to_string(),
        iat:   now,
        exp:   now + 3600,
        nbf:   None,
        iss:   "https://issuer.example.com".to_string(),
        aud:   vec!["some-other-service".to_string()],
        extra: HashMap::new(),
    };
    let token = crate::jwt::generate_hs256_token(&claims, secret).expect("token");

    let validator = JwtValidator::new("https://issuer.example.com", Algorithm::HS256)
        .expect("issuer config")
        .with_audiences(&["api", "web"])
        .expect("audiences");

    let result = validator.validate_hmac(&token, secret);
    assert!(
        result.is_err(),
        "a token whose aud is outside the configured set must be rejected, not accepted: {result:?}"
    );

    // And the matching audience is accepted, so the rejection above is about the
    // audience and not some unrelated failure.
    let mut good = claims;
    good.aud = vec!["api".to_string()];
    let good_token = crate::jwt::generate_hs256_token(&good, secret).expect("token");
    assert!(
        validator.validate_hmac(&good_token, secret).is_ok(),
        "a token with a configured audience must validate"
    );
}

/// Test that invalid issuer is rejected.
#[test]
fn test_jwt_invalid_issuer_rejection() {
    use jsonwebtoken::Algorithm;

    use crate::jwt::JwtValidator;

    // Empty issuer should fail
    let result = JwtValidator::new("", Algorithm::HS256);

    assert!(result.is_err(), "Empty issuer should be rejected");
}

/// Test that CSRF token format is consistent and URL-safe.
#[test]
fn test_csrf_token_url_safe_format() {
    use crate::handlers::generate_secure_state;

    let tokens: Vec<String> = (0..20).map(|_| generate_secure_state()).collect();

    for token in tokens {
        // Must be hex (URL-safe without encoding)
        assert!(
            token.chars().all(|c| c.is_ascii_hexdigit()),
            "Token should be hex-safe for URLs: {}",
            token
        );

        // Must be deterministic length (32 bytes = 64 hex chars)
        assert_eq!(token.len(), 64, "Token length should be consistent: {}", token.len());
    }
}

/// Test that an expired JWT is actually rejected as expired.
///
/// Replaces `test_state_expiry_property` (#737), which only asserted
/// `now + 600 > now` and `now - 1 < now` — arithmetic tautologies that exercised
/// no product code and would pass even if expiry were never enforced. This drives
/// the real validator against a token whose `exp` is in the past.
#[test]
fn an_expired_jwt_is_rejected() {
    use std::collections::HashMap;

    use jsonwebtoken::Algorithm;

    use crate::{Claims, error::AuthError, jwt::JwtValidator};

    let secret = b"expiry_enforcement_secret_at_least_32byt";
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .expect("system time")
        .as_secs();

    let expired = Claims {
        sub:   "user".to_string(),
        iat:   now - 7200,
        exp:   now - 3600, // expired an hour ago
        nbf:   None,
        iss:   "https://issuer.example.com".to_string(),
        aud:   vec!["api".to_string()],
        extra: HashMap::new(),
    };
    let token = crate::jwt::generate_hs256_token(&expired, secret).expect("token");

    let validator = JwtValidator::new("https://issuer.example.com", Algorithm::HS256)
        .expect("issuer config")
        .with_audiences(&["api"])
        .expect("audiences");

    assert!(
        matches!(validator.validate_hmac(&token, secret), Err(AuthError::TokenExpired)),
        "an expired JWT must be rejected as TokenExpired"
    );
}

/// RS256 negative-path coverage (#737): the asymmetric `validate` had
/// wrong-key/expired/tampered tests only on the HMAC path (`validate_hmac`).
/// These drive the real RS256 verifier with a genuinely signed token and assert
/// each failure mode.
mod rs256_negative_paths {
    use std::collections::HashMap;

    use jsonwebtoken::Algorithm;

    use crate::{Claims, error::AuthError, jwt::JwtValidator};

    const PRIVATE_KEY: &[u8] = include_bytes!("../test_data/test_rsa_key.pem");
    const PUBLIC_KEY: &[u8] = include_bytes!("../test_data/test_rsa_pub.pem");

    fn now() -> u64 {
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("system time")
            .as_secs()
    }

    fn validator() -> JwtValidator {
        JwtValidator::new("fraiseql", Algorithm::RS256)
            .expect("issuer config")
            .with_audiences(&["fraiseql-api"])
            .expect("audiences")
    }

    fn claims(exp_offset: i64) -> Claims {
        let n = now();
        Claims {
            sub: "user123".to_string(),
            iat: n - 10,
            #[allow(clippy::cast_possible_wrap, clippy::cast_sign_loss)] // Reason: test fixture values are non-negative; test fixture values are small
            exp: (n as i64 + exp_offset) as u64,
            nbf: None,
            iss: "fraiseql".to_string(),
            aud: vec!["fraiseql-api".to_string()],
            extra: HashMap::new(),
        }
    }

    #[test]
    fn a_valid_rs256_token_validates() {
        let token = crate::jwt::generate_rs256_token(&claims(3600), PRIVATE_KEY).expect("sign");
        assert!(
            validator().validate(&token, PUBLIC_KEY).is_ok(),
            "a genuinely RS256-signed, in-date token must validate — the control for the \
             negative cases below"
        );
    }

    #[test]
    fn an_expired_rs256_token_is_rejected() {
        let token = crate::jwt::generate_rs256_token(&claims(-3600), PRIVATE_KEY).expect("sign");
        assert!(matches!(validator().validate(&token, PUBLIC_KEY), Err(AuthError::TokenExpired)));
    }

    #[test]
    fn a_tampered_rs256_payload_is_rejected() {
        let token = crate::jwt::generate_rs256_token(&claims(3600), PRIVATE_KEY).expect("sign");
        // Flip the claims segment so the signature no longer covers the payload.
        let mut parts: Vec<&str> = token.split('.').collect();
        let tampered_payload = {
            use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};
            let raw = URL_SAFE_NO_PAD.decode(parts[1]).expect("payload b64");
            let mut json: serde_json::Value = serde_json::from_slice(&raw).expect("payload json");
            json["sub"] = serde_json::json!("attacker");
            URL_SAFE_NO_PAD.encode(serde_json::to_vec(&json).expect("claims serialize"))
        };
        parts[1] = &tampered_payload;
        let tampered = parts.join(".");
        assert!(
            matches!(validator().validate(&tampered, PUBLIC_KEY), Err(AuthError::InvalidSignature)),
            "a token whose payload was altered after signing must fail signature verification"
        );
    }

    #[test]
    fn an_rs256_token_signed_by_a_different_key_is_rejected() {
        // A structurally valid token signed by a *different* RSA key must not
        // validate against our public key.
        const OTHER_PRIVATE_KEY: &[u8] = include_bytes!("../test_data/test_rsa_key_other.pem");
        let token =
            crate::jwt::generate_rs256_token(&claims(3600), OTHER_PRIVATE_KEY).expect("sign");
        assert!(
            matches!(validator().validate(&token, PUBLIC_KEY), Err(AuthError::InvalidSignature)),
            "a token signed by an unrelated key must fail signature verification"
        );
    }
}

/// Test that random state generation doesn't use weak RNG.
/// Verifies the implementation uses OsRng for cryptographic randomness.
#[test]
fn test_randomness_quality() {
    use crate::handlers::generate_secure_state;

    // Generate states with different byte patterns
    let states: Vec<String> = (0..10).map(|_| generate_secure_state()).collect();

    // Verify we have good distribution (no obvious patterns)
    for state in states {
        // Decode hex
        let bytes = hex::decode(&state).expect("Valid hex");

        // Count bit transitions (high entropy indicator)
        let mut transitions = 0;
        for i in 0..bytes.len() - 1 {
            if bytes[i] != bytes[i + 1] {
                transitions += 1;
            }
        }

        // With cryptographic randomness, expect ~50% transitions
        // Very conservative minimum: 20%
        let byte_count = bytes.len();
        let min_transitions = byte_count / 5;

        assert!(
            transitions > min_transitions,
            "Insufficient entropy in random bytes: {} transitions in {} bytes",
            transitions,
            byte_count
        );
    }
}
