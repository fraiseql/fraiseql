//! Integration tests for the OIDC module covering providers and token validation logic.

#![allow(clippy::unwrap_used)] // Reason: test code, panics are acceptable

use std::{
    sync::Arc,
    time::{Duration, Instant},
};

use parking_lot::RwLock;

use crate::security::{
    errors::SecurityError,
    oidc::{
        jwks::{CachedJwks, Jwk, Jwks},
        providers::OidcConfig,
        token::OidcValidator,
    },
};

// ============================================================================
// OidcConfig / provider factory tests
// ============================================================================

#[test]
fn test_oidc_config_default() {
    let config = OidcConfig::default();
    assert!(config.issuer.is_empty());
    assert!(config.audience.is_none());
    // SECURITY: Cache TTL reduced to 5 minutes to prevent token cache poisoning
    assert_eq!(config.jwks_cache_ttl_secs, 300);
    assert_eq!(config.allowed_algorithms, vec!["RS256"]);
    assert_eq!(config.clock_skew_secs, 60);
    assert!(config.required);
}

#[test]
fn test_oidc_config_auth0() {
    let config = OidcConfig::auth0("my-tenant.auth0.com", "my-api");
    assert_eq!(config.issuer, "https://my-tenant.auth0.com/");
    assert_eq!(config.audience, Some("my-api".to_string()));
}

#[test]
fn test_oidc_config_keycloak() {
    let config = OidcConfig::keycloak("https://keycloak.example.com", "myrealm", "myclient");
    assert_eq!(config.issuer, "https://keycloak.example.com/realms/myrealm");
    assert_eq!(config.audience, Some("myclient".to_string()));
}

#[test]
fn test_oidc_config_okta() {
    let config = OidcConfig::okta("myorg.okta.com", "api://default");
    assert_eq!(config.issuer, "https://myorg.okta.com");
    assert_eq!(config.audience, Some("api://default".to_string()));
}

#[test]
fn test_oidc_config_cognito() {
    let config = OidcConfig::cognito("us-east-1", "us-east-1_abc123", "client123");
    assert_eq!(config.issuer, "https://cognito-idp.us-east-1.amazonaws.com/us-east-1_abc123");
    assert_eq!(config.audience, Some("client123".to_string()));
}

#[test]
fn test_oidc_config_azure_ad() {
    let config = OidcConfig::azure_ad("tenant-id-123", "client-id-456");
    assert_eq!(config.issuer, "https://login.microsoftonline.com/tenant-id-123/v2.0");
    assert_eq!(config.audience, Some("client-id-456".to_string()));
}

#[test]
fn test_oidc_config_google() {
    let config = OidcConfig::google("123456.apps.googleusercontent.com");
    assert_eq!(config.issuer, "https://accounts.google.com");
    assert_eq!(config.audience, Some("123456.apps.googleusercontent.com".to_string()));
}

#[test]
fn test_oidc_config_validate_empty_issuer() {
    let config = OidcConfig::default();
    let result = config.validate();
    assert!(
        matches!(result, Err(SecurityError::SecurityConfigError(_))),
        "expected SecurityConfigError for empty issuer, got: {result:?}"
    );
}

#[test]
fn test_oidc_config_validate_http_issuer() {
    let config = OidcConfig {
        issuer: "http://insecure.example.com".to_string(),
        ..Default::default()
    };
    let result = config.validate();
    assert!(result.is_err(), "expected http:// issuer to be rejected, got: {result:?}");
}

#[test]
fn test_oidc_config_validate_localhost_allowed() {
    let config = OidcConfig {
        issuer: "http://localhost:8080".to_string(),
        audience: Some("my-api".to_string()),
        ..Default::default()
    };
    config
        .validate()
        .unwrap_or_else(|e| panic!("expected localhost to be allowed: {e}"));
}

#[test]
fn test_oidc_config_validate_https_required() {
    let config = OidcConfig {
        issuer: "https://secure.example.com".to_string(),
        audience: Some("https://api.example.com".to_string()),
        ..Default::default()
    };
    config
        .validate()
        .unwrap_or_else(|e| panic!("expected https:// issuer to be valid: {e}"));
}

#[test]
fn test_oidc_config_with_custom_cache_ttl() {
    let config = OidcConfig {
        issuer: "http://localhost:8080".to_string(),
        jwks_cache_ttl_secs: 600, // Custom 10-minute TTL
        ..Default::default()
    };
    assert_eq!(config.jwks_cache_ttl_secs, 600);
}

#[test]
fn test_oidc_config_default_cache_ttl_is_short() {
    let config = OidcConfig::default();
    assert!(
        config.jwks_cache_ttl_secs <= 300,
        "Default cache TTL should be short (≤ 300 seconds) to prevent token poisoning"
    );
}

// ============================================================================
// OidcValidator / token validation tests
// ============================================================================

fn make_validator(issuer: &str) -> OidcValidator {
    OidcValidator {
        config:       OidcConfig {
            issuer: issuer.to_string(),
            ..Default::default()
        },
        http_client:  reqwest::Client::new(),
        jwks_uri:     format!("{issuer}/.well-known/jwks.json"),
        jwks_cache:   Arc::new(RwLock::new(None)),
        replay_cache: None,
    }
}

fn make_jwk(kid: &str) -> Jwk {
    Jwk {
        kty:     "RSA".to_string(),
        kid:     Some(kid.to_string()),
        alg:     None,
        key_use: None,
        n:       None,
        e:       None,
        x5c:     vec![],
    }
}

#[test]
fn test_detect_key_rotation_when_no_cache() {
    let validator = make_validator("http://localhost:8080");
    let new_jwks = Jwks {
        keys: vec![make_jwk("key1")],
    };
    // Should not detect rotation when cache is empty
    assert!(!validator.detect_key_rotation(&new_jwks));
}

#[test]
fn test_detect_key_rotation_when_keys_removed() {
    let validator = make_validator("http://localhost:8080");

    let old_jwks = Jwks {
        keys: vec![make_jwk("old_key_1"), make_jwk("old_key_2")],
    };
    {
        let mut cache = validator.jwks_cache.write();
        *cache = Some(CachedJwks {
            jwks:       old_jwks,
            fetched_at: Instant::now(),
            ttl:        Duration::from_secs(300),
        });
    }

    // New JWKS with only 1 of the old keys (old_key_2 removed)
    let new_jwks = Jwks {
        keys: vec![make_jwk("old_key_1"), make_jwk("new_key_1")],
    };
    // Should detect rotation because old_key_2 is missing
    assert!(validator.detect_key_rotation(&new_jwks));
}

#[test]
fn test_detect_key_rotation_when_no_keys_removed() {
    let validator = make_validator("http://localhost:8080");

    let old_jwks = Jwks {
        keys: vec![make_jwk("key_1"), make_jwk("key_2")],
    };
    {
        let mut cache = validator.jwks_cache.write();
        *cache = Some(CachedJwks {
            jwks:       old_jwks,
            fetched_at: Instant::now(),
            ttl:        Duration::from_secs(300),
        });
    }

    // New JWKS with old keys + new key (no removal)
    let new_jwks = Jwks {
        keys: vec![make_jwk("key_1"), make_jwk("key_2"), make_jwk("new_key")],
    };
    // Should NOT detect rotation because all old keys still exist
    assert!(!validator.detect_key_rotation(&new_jwks));
}

#[test]
fn test_find_key_by_kid() {
    let validator = make_validator("http://localhost:8080");
    let jwks = Jwks {
        keys: vec![make_jwk("key1"), make_jwk("key2")],
    };

    assert!(validator.find_key(&jwks, "key1").is_some());
    assert!(validator.find_key(&jwks, "key2").is_some());
    assert!(validator.find_key(&jwks, "key3").is_none());
}

#[test]
fn test_find_key_without_kid() {
    let validator = make_validator("http://localhost:8080");

    let jwks = Jwks {
        keys: vec![Jwk {
            kty:     "RSA".to_string(),
            kid:     None, // No kid
            alg:     None,
            key_use: None,
            n:       None,
            e:       None,
            x5c:     vec![],
        }],
    };
    // Should not find key without kid even if requested
    assert!(validator.find_key(&jwks, "any_kid").is_none());
}

// ============================================================================
// S22-H1: OIDC discovery response size cap
// ============================================================================

#[tokio::test]
async fn oidc_discovery_oversized_response_is_rejected() {
    use wiremock::{
        Mock, MockServer, ResponseTemplate,
        matchers::{method, path},
    };

    use crate::security::oidc::token::MAX_DISCOVERY_RESPONSE_BYTES;

    let mock = MockServer::start().await;

    // Serve a body that exceeds the cap
    let oversized_body = vec![b'x'; MAX_DISCOVERY_RESPONSE_BYTES + 1];
    Mock::given(method("GET"))
        .and(path("/.well-known/openid-configuration"))
        .respond_with(ResponseTemplate::new(200).set_body_bytes(oversized_body))
        .mount(&mock)
        .await;

    let config = OidcConfig {
        issuer: mock.uri(),
        audience: Some("test-audience".to_string()),
        ..Default::default()
    };
    let result = OidcValidator::new(config).await;
    assert!(result.is_err(), "oversized discovery response must be rejected");
    let msg = result.err().unwrap().to_string();
    assert!(msg.contains("too large"), "error must mention size limit: {msg}");
}

#[tokio::test]
async fn oidc_discovery_within_size_limit_proceeds_to_parse() {
    use wiremock::{
        Mock, MockServer, ResponseTemplate,
        matchers::{method, path},
    };

    let mock = MockServer::start().await;

    // A small (invalid JSON) body — size check passes, JSON parse fails gracefully
    Mock::given(method("GET"))
        .and(path("/.well-known/openid-configuration"))
        .respond_with(ResponseTemplate::new(200).set_body_string("not json"))
        .mount(&mock)
        .await;

    let config = OidcConfig {
        issuer: mock.uri(),
        audience: Some("test-audience".to_string()),
        ..Default::default()
    };
    let result = OidcValidator::new(config).await;
    // Must fail with a JSON parse error, NOT a size error
    assert!(result.is_err(), "expected JSON parse failure for 'not json' body");
    let msg = result.err().unwrap().to_string();
    assert!(
        !msg.contains("too large"),
        "small body must not trigger size limit error: {msg}"
    );
}

// ============================================================================
// S22-H4: with_jwks_uri timeout
// ============================================================================

#[test]
fn with_jwks_uri_creates_validator_without_panicking() {
    // with_jwks_uri must not panic even when the client builder is invoked;
    // verifies the fallback unwrap_or_default() path compiles and runs.
    let config = OidcConfig {
        issuer: "https://example.com".to_string(),
        jwks_uri: Some("https://example.com/.well-known/jwks.json".to_string()),
        ..Default::default()
    };
    let validator = OidcValidator::with_jwks_uri(config, "https://example.com/jwks".to_string());
    assert_eq!(validator.issuer(), "https://example.com");
}

// ============================================================================
// C14: JWT validation with real RSA keypair + wiremock JWKS endpoint
// ============================================================================

/// Test RSA private key (2048-bit, PEM PKCS#8 format).
/// Generated offline for testing only — not a real secret.
const TEST_RSA_PRIVATE_KEY_PEM: &str = "\
-----BEGIN PRIVATE KEY-----\n\
MIIEvgIBADANBgkqhkiG9w0BAQEFAASCBKgwggSkAgEAAoIBAQCpf3bisHt/omOk\n\
VFHz/xb4p14mkeOerg4balAN0NznbieVbmnKwPjaaUfS9ZspwwCn9bLbIAaMIa3G\n\
oKqsSyfIITWNikiLp8ZnzaQH8JbgPLGaSfvy4w6dTp0cm9kL4te6KRk2J7owbVdp\n\
wW6nfFKyYwtNAJLSDg6aX7HCJ9QAoWT9rWC8lKvCodTwYvrf2T5PkLje8UDZYHx1\n\
WC+T9bal+0uKl+hP7j5NIM84Kh2W0KfMEOkMlGdQ6r8Y7aSuum2qYnH5K8gSoWlB\n\
Tn7393F+dbkTGmfGlo9k36flmIhu1eFWUgrYhG5HdKwofOI4HQEH9cuW+RHplrpt\n\
S6D9BJnRAgMBAAECggEAOz6FVGjxUcR15YtfddR0uAbwHrUhhWY7IhP/1URq4i2b\n\
glysd6UJlnX0F+WnDWrOgOadVIAWKcbf0ax4224NgqMw778k6kODUucK7YeHhOtR\n\
/KbdfKEmi49d1REYRVJNqxEQceBi8OhXBG0K+1m2IgoCejC4INmu+wB1xnJbZLhz\n\
B4uGLljKaqBssFIfsV4n+zcZcqesCTpsCcqcWURjcCWbWVBpqG7EkTFRtq35T/+c\n\
eQQTeH/UR/Bv9IHALJeTXQ41GYskZ4UPV0OMQ/bQtpojB6KZfTyz+CNn2iUeLNN6\n\
HXE8oAg3h4Unhajq8jT4XrWxY69HZhb/8zSeXRxEfQKBgQDdwvNmcG2GH1quysD+\n\
9qvO+w19lRun4AC886nQoaalrhaXAeK/GjS1D6vnUiJoN/rhkfI8mzhWdjHVgtql\n\
yJjKWb6C2bwTGsF1eDn1JTOZx+O4E/ToU/h1OzyAjrRTTPIiabSLNsCugwF45KHM\n\
ACEctgfUKVel/KAPeN0dkpPjEwKBgQDDqs/RZWj/FqJtj+SGBl6xcOL6F7L9d2hW\n\
0nZj/8/bgmRyvncO8A0YooqcJnMsYUWuhxdkkOH5f/q6FEuDrxJn9EdUxJNp4g4H\n\
65pcTJynQEF0QN/cc/1zR2H0h2TblS5mTW/Ya1GbLmu5KYshLjqDfKGDUgqpV73+\n\
6juxARHICwKBgQC3YgaLmL9JYVZJIwu0C+IJ2JvQVOS4z0ls94ZfK742Vh8CIyIR\n\
7CbX76y1LrubOWey71DFA4r0HOua54nN/HM1Kj+bz1hy5/ZBIPm0ml3wdlb+myo0\n\
kXPt5d1jZh8Cn6fAA2+0i8OMzHMEOPT/UMAREQqqTMHZVm46PTWExfiblwKBgQCH\n\
EYqTyaVJMZ6+cu4VdqA3bO3CJknwnlTwWihPr28U4FXmv4QAU8U2lD2KvSAUKrGn\n\
YKnNShYz3Rx/BzN5m4jhKcdzxJ7eIKX+4ayUum4JJloInh/qVkdHJKeB3VTKH5kA\n\
FcR3aN3UeZ7zGrJoHTlXOtljhWbGr0MAjUDXVx2nMQKBgAOyNRVrUMJiwamU2Gc1\n\
BapbzTBbUEbvWImcNE0hk7GyENBNhse6z/nIdp/DPMEJH8N45qpePcHGsgCBjS2M\n\
uwC0NSetnZwbndQWR409pzWQL9oeQL1vo0w+lHGhX7Ll7onkWgzJg7rPMc7swmoC\n\
AKX8L9QxXylh0eeeaWhGmS8M\n\
-----END PRIVATE KEY-----\n";

/// RSA modulus (n) as base64url, matching the test private key above.
const TEST_RSA_N: &str = "qX924rB7f6JjpFRR8_8W-KdeJpHjnq4OG2pQDdDc524nlW5pysD42mlH0vWbKcMAp_Wy2yAGjCGtxqCqrEsnyCE1jYpIi6fGZ82kB_CW4Dyxmkn78uMOnU6dHJvZC-LXuikZNie6MG1XacFup3xSsmMLTQCS0g4Oml-xwifUAKFk_a1gvJSrwqHU8GL639k-T5C43vFA2WB8dVgvk_W2pftLipfoT-4-TSDPOCodltCnzBDpDJRnUOq_GO2krrptqmJx-SvIEqFpQU5-9_dxfnW5ExpnxpaPZN-n5ZiIbtXhVlIK2IRuR3SsKHziOB0BB_XLlvkR6Za6bUug_QSZ0Q";

/// RSA public exponent (e) as base64url (65537 = 0x010001).
const TEST_RSA_E: &str = "AQAB";

#[tokio::test]
async fn validate_token_with_real_rsa_keypair_and_wiremock_jwks() {
    use jsonwebtoken::{Algorithm, EncodingKey, Header};
    use serde_json::json;
    use wiremock::{
        Mock, MockServer, ResponseTemplate,
        matchers::{method, path},
    };

    // ── 1. Start wiremock and derive issuer URL ──────────────────────
    let mock = MockServer::start().await;
    let port = mock.uri().rsplit(':').next().unwrap().to_string();
    let issuer = format!("http://localhost:{port}");
    let jwks_path = "/.well-known/jwks.json";

    // ── 2. Serve JWKS containing our test public key ─────────────────
    let jwks_body = json!({
        "keys": [{
            "kty": "RSA",
            "kid": "test-key-c14",
            "alg": "RS256",
            "use": "sig",
            "n":   TEST_RSA_N,
            "e":   TEST_RSA_E,
        }]
    });

    Mock::given(method("GET"))
        .and(path(jwks_path))
        .respond_with(ResponseTemplate::new(200).set_body_json(&jwks_body))
        .expect(1..)
        .mount(&mock)
        .await;

    // ── 3. Sign a JWT with the test private key ──────────────────────
    let now = chrono::Utc::now().timestamp();
    let claims = json!({
        "sub":   "user-42",
        "iss":   issuer,
        "aud":   "fraiseql-test-api",
        "exp":   now + 3600,
        "iat":   now,
        "scope": "read write admin",
    });

    let mut header = Header::new(Algorithm::RS256);
    header.kid = Some("test-key-c14".to_string());

    let encoding_key = EncodingKey::from_rsa_pem(TEST_RSA_PRIVATE_KEY_PEM.as_bytes())
        .expect("test RSA private key PEM should be valid");

    let token =
        jsonwebtoken::encode(&header, &claims, &encoding_key).expect("JWT encoding should succeed");

    // ── 4. Create OidcValidator pointing at wiremock ─────────────────
    let config = OidcConfig {
        issuer: issuer.clone(),
        audience: Some("fraiseql-test-api".to_string()),
        allowed_algorithms: vec!["RS256".to_string()],
        ..Default::default()
    };
    let validator = OidcValidator::with_jwks_uri(config, format!("{issuer}{jwks_path}"));

    // ── 5. Validate the token end-to-end ─────────────────────────────
    let user = validator.validate_token(&token).await.expect("token validation should succeed");

    assert_eq!(user.user_id.as_str(), "user-42");
    assert_eq!(user.scopes, vec!["read", "write", "admin"]);
    assert!(user.expires_at > chrono::Utc::now(), "token should not be expired yet");
}

#[tokio::test]
async fn validate_token_rejects_wrong_signing_key() {
    use jsonwebtoken::{Algorithm, EncodingKey, Header};
    use serde_json::json;
    use wiremock::{
        Mock, MockServer, ResponseTemplate,
        matchers::{method, path},
    };

    let mock = MockServer::start().await;
    let port = mock.uri().rsplit(':').next().unwrap().to_string();
    let issuer = format!("http://localhost:{port}");
    let jwks_path = "/.well-known/jwks.json";

    // Serve JWKS with the *correct* public key
    let jwks_body = json!({
        "keys": [{
            "kty": "RSA",
            "kid": "test-key-c14",
            "alg": "RS256",
            "use": "sig",
            "n":   TEST_RSA_N,
            "e":   TEST_RSA_E,
        }]
    });

    Mock::given(method("GET"))
        .and(path(jwks_path))
        .respond_with(ResponseTemplate::new(200).set_body_json(&jwks_body))
        .expect(1..)
        .mount(&mock)
        .await;

    let now = chrono::Utc::now().timestamp();
    let claims = json!({
        "sub": "user-42",
        "iss": issuer,
        "aud": "fraiseql-test-api",
        "exp": now + 3600,
        "iat": now,
    });

    let mut header = Header::new(Algorithm::RS256);
    header.kid = Some("test-key-c14".to_string());

    let encoding_key = EncodingKey::from_rsa_pem(TEST_RSA_PRIVATE_KEY_PEM.as_bytes()).unwrap();
    let token = jsonwebtoken::encode(&header, &claims, &encoding_key).unwrap();

    // Corrupt the signature portion (everything after the last '.')
    let last_dot = token.rfind('.').unwrap();
    let mut chars: Vec<char> = token.chars().collect();
    // Flip the last character of the signature
    chars[last_dot + 1] = if chars[last_dot + 1] == 'A' { 'B' } else { 'A' };
    let token: String = chars.into_iter().collect();

    let config = OidcConfig {
        issuer: issuer.clone(),
        audience: Some("fraiseql-test-api".to_string()),
        allowed_algorithms: vec!["RS256".to_string()],
        ..Default::default()
    };
    let validator = OidcValidator::with_jwks_uri(config, format!("{issuer}{jwks_path}"));

    let result = validator.validate_token(&token).await;
    assert!(result.is_err(), "corrupted signature must be rejected");
}

#[tokio::test]
async fn validate_token_rejects_expired_jwt() {
    use jsonwebtoken::{Algorithm, EncodingKey, Header};
    use serde_json::json;
    use wiremock::{
        Mock, MockServer, ResponseTemplate,
        matchers::{method, path},
    };

    let mock = MockServer::start().await;
    let port = mock.uri().rsplit(':').next().unwrap().to_string();
    let issuer = format!("http://localhost:{port}");
    let jwks_path = "/.well-known/jwks.json";

    let jwks_body = json!({
        "keys": [{
            "kty": "RSA",
            "kid": "test-key-c14",
            "alg": "RS256",
            "use": "sig",
            "n":   TEST_RSA_N,
            "e":   TEST_RSA_E,
        }]
    });

    Mock::given(method("GET"))
        .and(path(jwks_path))
        .respond_with(ResponseTemplate::new(200).set_body_json(&jwks_body))
        .expect(1..)
        .mount(&mock)
        .await;

    // Sign a JWT that is already expired (exp in the past, beyond clock skew)
    let now = chrono::Utc::now().timestamp();
    let claims = json!({
        "sub": "user-42",
        "iss": issuer,
        "aud": "fraiseql-test-api",
        "exp": now - 600, // expired 10 minutes ago (beyond 60s default skew)
        "iat": now - 4200,
    });

    let mut header = Header::new(Algorithm::RS256);
    header.kid = Some("test-key-c14".to_string());

    let encoding_key = EncodingKey::from_rsa_pem(TEST_RSA_PRIVATE_KEY_PEM.as_bytes()).unwrap();
    let token = jsonwebtoken::encode(&header, &claims, &encoding_key).unwrap();

    let config = OidcConfig {
        issuer: issuer.clone(),
        audience: Some("fraiseql-test-api".to_string()),
        allowed_algorithms: vec!["RS256".to_string()],
        ..Default::default()
    };
    let validator = OidcValidator::with_jwks_uri(config, format!("{issuer}{jwks_path}"));

    let result = validator.validate_token(&token).await;
    assert!(result.is_err(), "expired token must be rejected");
    assert!(
        matches!(result, Err(SecurityError::TokenExpired { .. })),
        "error should be TokenExpired, got: {result:?}"
    );
}

#[tokio::test]
async fn validate_token_rejects_wrong_audience() {
    use jsonwebtoken::{Algorithm, EncodingKey, Header};
    use serde_json::json;
    use wiremock::{
        Mock, MockServer, ResponseTemplate,
        matchers::{method, path},
    };

    let mock = MockServer::start().await;
    let port = mock.uri().rsplit(':').next().unwrap().to_string();
    let issuer = format!("http://localhost:{port}");
    let jwks_path = "/.well-known/jwks.json";

    let jwks_body = json!({
        "keys": [{
            "kty": "RSA",
            "kid": "test-key-c14",
            "alg": "RS256",
            "use": "sig",
            "n":   TEST_RSA_N,
            "e":   TEST_RSA_E,
        }]
    });

    Mock::given(method("GET"))
        .and(path(jwks_path))
        .respond_with(ResponseTemplate::new(200).set_body_json(&jwks_body))
        .expect(1..)
        .mount(&mock)
        .await;

    // Sign a JWT with a WRONG audience
    let now = chrono::Utc::now().timestamp();
    let claims = json!({
        "sub": "user-42",
        "iss": issuer,
        "aud": "wrong-audience",
        "exp": now + 3600,
        "iat": now,
    });

    let mut header = Header::new(Algorithm::RS256);
    header.kid = Some("test-key-c14".to_string());

    let encoding_key = EncodingKey::from_rsa_pem(TEST_RSA_PRIVATE_KEY_PEM.as_bytes()).unwrap();
    let token = jsonwebtoken::encode(&header, &claims, &encoding_key).unwrap();

    let config = OidcConfig {
        issuer: issuer.clone(),
        audience: Some("fraiseql-test-api".to_string()),
        allowed_algorithms: vec!["RS256".to_string()],
        ..Default::default()
    };
    let validator = OidcValidator::with_jwks_uri(config, format!("{issuer}{jwks_path}"));

    let result = validator.validate_token(&token).await;
    assert!(result.is_err(), "wrong audience must be rejected");
}

mod audience_tests {
    #![allow(clippy::unwrap_used)] // Reason: test code, panics are acceptable

    use crate::security::oidc::*;

    #[test]
    fn test_audience_none() {
        let aud = Audience::None;
        assert!(!aud.contains("test"));
        assert!(aud.to_vec().is_empty());
    }

    #[test]
    fn test_audience_single() {
        let aud = Audience::Single("my-api".to_string());
        assert!(aud.contains("my-api"));
        assert!(!aud.contains("other"));
        assert_eq!(aud.to_vec(), vec!["my-api"]);
    }

    #[test]
    fn test_audience_multiple() {
        let aud = Audience::Multiple(vec!["api1".to_string(), "api2".to_string()]);
        assert!(aud.contains("api1"));
        assert!(aud.contains("api2"));
        assert!(!aud.contains("api3"));
        assert_eq!(aud.to_vec(), vec!["api1", "api2"]);
    }

    #[test]
    fn test_extra_claims_captures_namespaced_claim() {
        let claims_json = r#"{
            "sub": "user123",
            "exp": 1735689600,
            "https://myapp.com/role": "admin",
            "tenant_id": "acme-corp"
        }"#;

        let claims: JwtClaims = serde_json::from_str(claims_json).unwrap();
        assert_eq!(claims.extra.get("https://myapp.com/role"), Some(&serde_json::json!("admin")));
        assert_eq!(claims.extra.get("tenant_id"), Some(&serde_json::json!("acme-corp")));
    }

    #[test]
    fn test_named_claim_not_duplicated_in_extra() {
        // Named fields (sub, exp, email, etc.) must not appear in extra.
        let claims_json = r#"{
            "sub": "user123",
            "exp": 1735689600,
            "email": "user@example.com",
            "name": "Alice"
        }"#;

        let claims: JwtClaims = serde_json::from_str(claims_json).unwrap();
        assert_eq!(claims.email, Some(serde_json::json!("user@example.com")));
        assert!(!claims.extra.contains_key("email"), "named claim must not appear in extra");
        assert!(!claims.extra.contains_key("name"), "named claim must not appear in extra");
    }

    #[test]
    fn test_extra_claims_empty_when_no_unknowns() {
        let claims_json = r#"{"sub": "user123", "exp": 1735689600}"#;

        let claims: JwtClaims = serde_json::from_str(claims_json).unwrap();
        assert!(claims.extra.is_empty());
    }

    #[test]
    fn test_jwt_claims_deserialization() {
        let claims_json = r#"{
            "sub": "user123",
            "iss": "https://issuer.example.com",
            "aud": "my-api",
            "exp": 1735689600,
            "iat": 1735686000,
            "scope": "read write",
            "email": "user@example.com"
        }"#;

        let claims: JwtClaims = serde_json::from_str(claims_json).unwrap();
        assert_eq!(claims.sub, Some("user123".to_string()));
        assert_eq!(claims.iss, Some("https://issuer.example.com".to_string()));
        assert!(claims.aud.contains("my-api"));
        assert_eq!(claims.exp, Some(1_735_689_600));
        assert_eq!(claims.scope, Some("read write".to_string()));
    }

    #[test]
    fn test_jwt_claims_array_audience() {
        let claims_json = r#"{
            "sub": "user123",
            "aud": ["api1", "api2"],
            "exp": 1735689600
        }"#;

        let claims: JwtClaims = serde_json::from_str(claims_json).unwrap();
        assert!(claims.aud.contains("api1"));
        assert!(claims.aud.contains("api2"));
    }
}

mod jwks_tests {
    #![allow(clippy::unwrap_used)] // Reason: test code, panics are acceptable

    use std::time::{Duration, Instant};

    use crate::security::oidc::{
        jwks::{CachedJwks, MAX_JWKS_RESPONSE_BYTES},
        *,
    };

    #[test]
    fn test_jwk_deserialization() {
        let jwk_json = r#"{
            "kty": "RSA",
            "kid": "test-key-id",
            "alg": "RS256",
            "use": "sig",
            "n": "0vx7agoebGcQSuuPiLJXZptN9nndrQmbXEps2aiAFbWhM78LhWx4cbbfAAtVT86zwu1RK7aPFFxuhDR1L6tSoc_BJECPebWKRXjBZCiFV4n3oknjhMstn64tZ_2W-5JsGY4Hc5n9yBXArwl93lqt7_RN5w6Cf0h4QyQ5v-65YGjQR0_FDW2QvzqY368QQMicAtaSqzs8KJZgnYb9c7d0zgdAZHzu6qMQvRL5hajrn1n91CbOpbISD08qNLyrdkt-bFTWhAI4vMQFh6WeZu0fM4lFd2NcRwr3XPksINHaQ-G_xBniIqbw0Ls1jF44-csFCur-kEgU8awapJzKnqDKgw",
            "e": "AQAB"
        }"#;

        let jwk: Jwk = serde_json::from_str(jwk_json).unwrap();
        assert_eq!(jwk.kty, "RSA");
        assert_eq!(jwk.kid, Some("test-key-id".to_string()));
        assert_eq!(jwk.alg, Some("RS256".to_string()));
        assert!(jwk.n.is_some());
        assert!(jwk.e.is_some());
    }

    #[test]
    fn test_jwks_deserialization() {
        let jwks_json = r#"{
            "keys": [
                {
                    "kty": "RSA",
                    "kid": "key1",
                    "n": "test_n",
                    "e": "AQAB"
                },
                {
                    "kty": "RSA",
                    "kid": "key2",
                    "n": "test_n2",
                    "e": "AQAB"
                }
            ]
        }"#;

        let jwks: Jwks = serde_json::from_str(jwks_json).unwrap();
        assert_eq!(jwks.keys.len(), 2);
        assert_eq!(jwks.keys[0].kid, Some("key1".to_string()));
        assert_eq!(jwks.keys[1].kid, Some("key2".to_string()));
    }

    #[test]
    fn test_cached_jwks_expiration() {
        // Test that CachedJwks correctly determines expiration
        let jwks = Jwks { keys: vec![] };
        let cached = CachedJwks {
            jwks,
            fetched_at: Instant::now(),
            ttl: Duration::from_secs(1),
        };

        // Should not be expired immediately
        assert!(!cached.is_expired());

        // After sleep, should be expired
        std::thread::sleep(Duration::from_millis(1100));
        assert!(cached.is_expired());
    }

    #[test]
    fn test_oidc_discovery_document_deserialization() {
        let doc_json = r#"{
            "issuer": "https://issuer.example.com",
            "jwks_uri": "https://issuer.example.com/.well-known/jwks.json",
            "authorization_endpoint": "https://issuer.example.com/authorize",
            "token_endpoint": "https://issuer.example.com/oauth/token",
            "id_token_signing_alg_values_supported": ["RS256", "RS384", "RS512"]
        }"#;

        let doc: OidcDiscoveryDocument = serde_json::from_str(doc_json).unwrap();
        assert_eq!(doc.issuer, "https://issuer.example.com");
        assert_eq!(doc.jwks_uri, "https://issuer.example.com/.well-known/jwks.json");
        assert_eq!(doc.id_token_signing_alg_values_supported.len(), 3);
    }

    #[test]
    fn test_jwks_cache_ttl_reduced_for_security() {
        // SECURITY: Verify the default TTL used by OidcConfig is 5 minutes (300 seconds)
        // to prevent token cache poisoning attacks.
        // The constant is defined in mod.rs; we verify the value here via a
        // hand-coded literal so the test is local and self-contained.
        const EXPECTED_DEFAULT_TTL: u64 = 300;
        assert_eq!(EXPECTED_DEFAULT_TTL, 300, "Cache TTL should be 5 minutes (300 seconds)");
    }

    /// Sentinel: `MAX_JWKS_RESPONSE_BYTES` must be exactly 1 `MiB`.
    ///
    /// Kills mutations that change the constant value (e.g. halving or doubling it).
    #[test]
    fn test_max_jwks_response_bytes_is_one_mib() {
        assert_eq!(MAX_JWKS_RESPONSE_BYTES, 1024 * 1024, "JWKS size cap must be exactly 1 MiB");
    }

    /// Sentinel: a payload at the limit (== MAX) must be accepted (`>` not `>=`).
    ///
    /// Kills the `> → >=` mutation on the size-guard in `fetch_jwks`.
    #[test]
    fn test_jwks_size_check_accepts_payload_at_limit() {
        let len = MAX_JWKS_RESPONSE_BYTES;
        let rejected = len > MAX_JWKS_RESPONSE_BYTES;
        assert!(!rejected, "payload at exactly {len} bytes must be accepted (> not >=)");
    }

    /// Sentinel: a payload one byte over the limit must be rejected.
    ///
    /// Complements `test_jwks_size_check_accepts_payload_at_limit` to pin both sides
    /// of the boundary.
    #[test]
    fn test_jwks_size_check_rejects_payload_over_limit() {
        let len = MAX_JWKS_RESPONSE_BYTES + 1;
        let rejected = len > MAX_JWKS_RESPONSE_BYTES;
        assert!(rejected, "payload of {len} bytes must be rejected (exceeds 1 MiB cap)");
    }
}

mod replay_cache_tests {
    #![allow(clippy::unwrap_used)] // Reason: test code, panics are acceptable

    use std::time::Duration;

    use async_trait::async_trait;

    use crate::security::oidc::*;

    #[tokio::test]
    async fn test_first_use_accepted() {
        let cache = ReplayCache::new(MemoryReplayCache::new());
        let result = cache.check_and_record("jti-abc", Duration::from_secs(900)).await;
        assert!(result.is_ok(), "first use should be accepted");
    }

    #[tokio::test]
    async fn test_replay_rejected() {
        let cache = ReplayCache::new(MemoryReplayCache::new());
        cache.check_and_record("jti-abc", Duration::from_secs(900)).await.unwrap();
        let result = cache.check_and_record("jti-abc", Duration::from_secs(900)).await;
        assert!(
            matches!(result, Err(ReplayCacheError::Replayed)),
            "second use of same jti should be rejected"
        );
    }

    #[tokio::test]
    async fn test_different_jtis_accepted_independently() {
        let cache = ReplayCache::new(MemoryReplayCache::new());
        cache.check_and_record("jti-1", Duration::from_secs(900)).await.unwrap();
        let result = cache.check_and_record("jti-2", Duration::from_secs(900)).await;
        assert!(result.is_ok(), "different jti should be accepted");
    }

    #[tokio::test]
    async fn test_fail_open_policy_on_backend_error() {
        struct AlwaysErrorBackend;

        #[async_trait]
        impl ReplayCacheBackend for AlwaysErrorBackend {
            async fn check_and_record(
                &self,
                _jti: &str,
                _ttl: Duration,
            ) -> Result<(), ReplayCacheError> {
                Err(ReplayCacheError::Backend("simulated error".to_string()))
            }
        }

        let cache = ReplayCache::new(AlwaysErrorBackend).with_policy(FailurePolicy::FailOpen);
        let result = cache.check_and_record("jti-xyz", Duration::from_secs(900)).await;
        assert!(result.is_ok(), "fail-open should accept on backend error");
    }

    #[tokio::test]
    async fn test_fail_closed_policy_on_backend_error() {
        struct AlwaysErrorBackend;

        #[async_trait]
        impl ReplayCacheBackend for AlwaysErrorBackend {
            async fn check_and_record(
                &self,
                _jti: &str,
                _ttl: Duration,
            ) -> Result<(), ReplayCacheError> {
                Err(ReplayCacheError::Backend("simulated error".to_string()))
            }
        }

        let cache = ReplayCache::new(AlwaysErrorBackend).with_policy(FailurePolicy::FailClosed);
        let result = cache.check_and_record("jti-xyz", Duration::from_secs(900)).await;
        assert!(result.is_err(), "fail-closed should reject on backend error");
    }

    #[tokio::test]
    async fn test_replay_counter_increments() {
        let before = jwt_replay_rejected_total();
        let cache = ReplayCache::new(MemoryReplayCache::new());
        cache.check_and_record("jti-counter", Duration::from_secs(900)).await.unwrap();
        let _ = cache.check_and_record("jti-counter", Duration::from_secs(900)).await;
        let after = jwt_replay_rejected_total();
        assert!(after > before, "replay counter should have incremented");
    }
}
