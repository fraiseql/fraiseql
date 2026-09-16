//! Integration tests for the OIDC module covering providers and token validation logic.

#![allow(clippy::unwrap_used, clippy::panic)] // Reason: test code, panics acceptable
use crate::security::{
    errors::SecurityError,
    oidc::{providers::OidcConfig, token::OidcValidator},
};

// ============================================================================
// OidcConfig / provider factory tests
// ============================================================================

#[test]
fn test_oidc_config_default() {
    let config = OidcConfig::default();
    assert!(config.issuer.is_none());
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
    assert_eq!(config.issuer.as_deref(), Some("https://my-tenant.auth0.com/"));
    assert_eq!(config.audience, Some("my-api".to_string()));
}

#[test]
fn test_oidc_config_keycloak() {
    let config = OidcConfig::keycloak("https://keycloak.example.com", "myrealm", "myclient");
    assert_eq!(config.issuer.as_deref(), Some("https://keycloak.example.com/realms/myrealm"));
    assert_eq!(config.audience, Some("myclient".to_string()));
}

#[test]
fn test_oidc_config_okta() {
    let config = OidcConfig::okta("myorg.okta.com", "api://default");
    assert_eq!(config.issuer.as_deref(), Some("https://myorg.okta.com"));
    assert_eq!(config.audience, Some("api://default".to_string()));
}

#[test]
fn test_oidc_config_cognito() {
    let config = OidcConfig::cognito("us-east-1", "us-east-1_abc123", "client123");
    assert_eq!(
        config.issuer.as_deref(),
        Some("https://cognito-idp.us-east-1.amazonaws.com/us-east-1_abc123")
    );
    assert_eq!(config.audience, Some("client123".to_string()));
}

#[test]
fn test_oidc_config_azure_ad() {
    let config = OidcConfig::azure_ad("tenant-id-123", "client-id-456");
    assert_eq!(
        config.issuer.as_deref(),
        Some("https://login.microsoftonline.com/tenant-id-123/v2.0")
    );
    assert_eq!(config.audience, Some("client-id-456".to_string()));
}

#[test]
fn test_oidc_config_google() {
    let config = OidcConfig::google("123456.apps.googleusercontent.com");
    assert_eq!(config.issuer.as_deref(), Some("https://accounts.google.com"));
    assert_eq!(config.audience, Some("123456.apps.googleusercontent.com".to_string()));
}

#[test]
fn test_oidc_config_validate_default_is_rejected() {
    // Default config has no issuer, no jwks_uri, and no audience — every guard
    // must reject it. The first guard reached is the issuer-less/no-jwks check.
    let config = OidcConfig::default();
    let result = config.validate();
    assert!(
        matches!(result, Err(SecurityError::SecurityConfigError(_))),
        "expected SecurityConfigError for an unconfigured OidcConfig, got: {result:?}"
    );
}

#[test]
fn test_oidc_config_validate_http_issuer() {
    let config = OidcConfig {
        issuer: Some("http://insecure.example.com".to_string()),
        ..Default::default()
    };
    let result = config.validate();
    assert!(result.is_err(), "expected http:// issuer to be rejected, got: {result:?}");
}

#[test]
fn test_oidc_config_validate_localhost_allowed() {
    let config = OidcConfig {
        issuer: Some("http://localhost:8080".to_string()),
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
        issuer: Some("https://secure.example.com".to_string()),
        audience: Some("https://api.example.com".to_string()),
        ..Default::default()
    };
    config
        .validate()
        .unwrap_or_else(|e| panic!("expected https:// issuer to be valid: {e}"));
}

// ============================================================================
// Issuer-less mode: IdPs whose access tokens omit the `iss` claim (e.g. Hanko).
// `issuer` is optional, symmetric with `audience`; when unset the JWKS URI must
// be pinned (discovery is impossible without an issuer) and `iss` is not checked.
// ============================================================================

#[test]
fn issuerless_config_with_pinned_jwks_and_audience_is_valid() {
    let config = OidcConfig {
        issuer: None,
        audience: Some("relying-party-id".to_string()),
        jwks_uri: Some("https://hanko.example.com/.well-known/jwks.json".to_string()),
        ..Default::default()
    };
    config
        .validate()
        .unwrap_or_else(|e| panic!("issuer-less config with pinned jwks_uri must be valid: {e}"));
}

#[test]
fn issuerless_config_without_jwks_uri_is_rejected() {
    // Without an issuer, discovery cannot find the JWKS endpoint, so `jwks_uri`
    // must be pinned. Audience is set so this isolates the issuer/jwks_uri guard.
    let config = OidcConfig {
        issuer: None,
        audience: Some("relying-party-id".to_string()),
        jwks_uri: None,
        ..Default::default()
    };
    let result = config.validate();
    assert!(
        matches!(result, Err(SecurityError::SecurityConfigError(_))),
        "issuer-less config without a pinned jwks_uri must be rejected, got: {result:?}"
    );
}

#[test]
fn test_oidc_config_with_custom_cache_ttl() {
    let config = OidcConfig {
        issuer: Some("http://localhost:8080".to_string()),
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

/// A validator pinned to `jwks_uri`, in the shape the `[auth]` path builds.
///
/// Built through the real constructor: `OidcValidator` no longer has a cache of
/// its own to reach into, so a test cannot hand-assemble one — which is the point
/// (#1335). Seeding a private `jwks_cache` was how three tests asserted the
/// *shape* of a cache this crate no longer owns.
fn make_validator(jwks_uri: &str) -> OidcValidator {
    let config = OidcConfig {
        issuer: Some("https://idp.example.com".to_string()),
        audience: Some("fraiseql-test".to_string()),
        ..Default::default()
    };
    OidcValidator::with_jwks_uri(config, jwks_uri)
        .expect("an https or loopback jwks_uri is accepted")
}

/// A JWKS endpoint serving `kids`, counting every GET.
async fn jwks_serving(kids: &[&str]) -> wiremock::MockServer {
    use serde_json::json;
    use wiremock::{
        Mock, MockServer, ResponseTemplate,
        matchers::{method, path},
    };

    let keys: Vec<serde_json::Value> = kids
        .iter()
        .map(|kid| json!({ "kty": "RSA", "kid": kid, "n": TEST_RSA_N, "e": TEST_RSA_E }))
        .collect();
    let mock = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path(JWKS_FIXTURE_PATH))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({ "keys": keys })))
        .mount(&mock)
        .await;
    mock
}

/// Where every fixture publisher in this file serves its keys.
const JWKS_FIXTURE_PATH: &str = "/.well-known/jwks.json";

/// How many times a fixture publisher was asked for its keys.
async fn jwks_fetches(mock: &wiremock::MockServer) -> usize {
    mock.received_requests()
        .await
        .expect("the fixture publisher records its requests")
        .len()
}

// ============================================================================
// #1335: the JWKS cache moved, and these are the seams that stayed
// ============================================================================
//
// What used to be here: three `detect_key_rotation` cases, two `find_key` cases,
// a `CachedJwks::is_expired` case that slept 1.1 s, a JWK/JWKS deserialisation
// pair, and three "sentinels" that compared `MAX + 1 > MAX` **in the test body**
// and so pinned nothing in the code at all.
//
// Every one of those described the cache's internals, and the cache is now
// `fraiseql_jwks` — one implementation instead of the two that had drifted, with
// each of those properties exercised there against the code that runs:
// `a_withdrawn_key_is_reported_as_a_rotation_and_an_added_one_is_not`,
// `a_key_is_selected_by_kid_and_only_by_kid`,
// `an_expired_set_is_not_served_even_while_the_cooldown_holds`,
// `a_refetch_that_drops_a_key_stops_that_key_verifying`,
// `an_oversized_key_set_is_refused_before_it_is_parsed` and
// `a_key_set_exactly_at_the_size_cap_is_accepted`.
//
// What stays here is what is genuinely this crate's: that the operator's two
// controls reach the shared client. The refetch bound is measured from this side
// too, in `mod jwks_refetch_bound` at the end of this file — from the caller that
// #1335 measured, because the `alg`-ordering half of the defect is invisible from
// anywhere else.

#[tokio::test]
async fn the_operators_forced_refresh_reaches_the_provider() {
    let mock = jwks_serving(&["rotated_in_kid"]).await;
    let validator = make_validator(&format!("{}{JWKS_FIXTURE_PATH}", mock.uri()));

    let count = validator.refresh_jwks().await.expect("force refresh should succeed");
    assert_eq!(count, 1, "refresh_jwks returns the number of keys the provider now serves");

    // This is the operator's response to a known key compromise, so the refetch
    // cooldown — which the call above has just armed — must not refuse it.
    let again = validator.refresh_jwks().await.expect("and again, immediately");
    assert_eq!(again, 1, "a second forced refresh is not rate-limited");
    assert_eq!(jwks_fetches(&mock).await, 2, "both refreshes reached the provider");
}

#[tokio::test]
async fn invalidating_makes_the_very_next_validation_refetch() {
    let mock = jwks_serving(&["compromised_kid"]).await;
    let validator = make_validator(&format!("{}{JWKS_FIXTURE_PATH}", mock.uri()));

    validator.get_decoding_key("compromised_kid").await.expect("published");
    assert_eq!(jwks_fetches(&mock).await, 1);

    // The flush has to clear the cooldown as well as the keys. Dropping only the
    // keys would answer the operator's "stop trusting these" by refusing every
    // token until the cooldown lapsed, rather than by fetching.
    validator.invalidate_jwks_cache();
    validator.get_decoding_key("compromised_kid").await.expect("re-fetched");
    assert_eq!(
        jwks_fetches(&mock).await,
        2,
        "the flush must make the next validation fetch, not merely stop answering"
    );
}

#[test]
fn a_jwks_uri_that_could_never_be_fetched_refuses_the_validator() {
    // It used to be stored unexamined, so this built a validator that refused
    // every token at its first delivery instead of refusing to exist. The sibling
    // cache in `fraiseql_auth` had always checked at construction (#1335).
    let config = OidcConfig {
        audience: Some("fraiseql-test".to_string()),
        ..Default::default()
    };
    for uri in ["not a url", "http://idp.example.com/jwks", "ftp://idp/jwks"] {
        let error = OidcValidator::with_jwks_uri(config.clone(), uri)
            .err()
            .unwrap_or_else(|| panic!("{uri} must be refused at construction"));
        assert!(
            matches!(error, SecurityError::SecurityConfigError(_)),
            "and refused as the operator's configuration error: {error:?}"
        );
    }
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
        issuer: Some(mock.uri()),
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
        issuer: Some(mock.uri()),
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
// with_jwks_uri: what it keeps from the config it is handed
// ============================================================================

/// The positive counterweight to
/// [`a_jwks_uri_that_could_never_be_fetched_refuses_the_validator`]: a usable URI
/// builds a validator that still knows its issuer.
///
/// It replaces `with_jwks_uri_creates_validator_without_panicking`, whose subject
/// no longer exists — the constructor built an HTTP client it never used and
/// `expect`ed on it, and that client is gone (#1335). A test named for a panic
/// that cannot happen reads as coverage and is none.
#[test]
fn with_jwks_uri_keeps_the_configured_issuer() {
    let config = OidcConfig {
        issuer: Some("https://example.com".to_string()),
        jwks_uri: Some("https://example.com/.well-known/jwks.json".to_string()),
        ..Default::default()
    };
    let validator = OidcValidator::with_jwks_uri(config, "https://example.com/jwks")
        .expect("an https jwks_uri is accepted");
    assert_eq!(validator.issuer(), Some("https://example.com"));
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
        issuer: Some(issuer.clone()),
        audience: Some("fraiseql-test-api".to_string()),
        allowed_algorithms: vec!["RS256".to_string()],
        ..Default::default()
    };
    let validator = OidcValidator::with_jwks_uri(config, &format!("{issuer}{jwks_path}"))
        .expect("a loopback http jwks_uri is accepted");

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
        issuer: Some(issuer.clone()),
        audience: Some("fraiseql-test-api".to_string()),
        allowed_algorithms: vec!["RS256".to_string()],
        ..Default::default()
    };
    let validator = OidcValidator::with_jwks_uri(config, &format!("{issuer}{jwks_path}"))
        .expect("a loopback http jwks_uri is accepted");

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
        issuer: Some(issuer.clone()),
        audience: Some("fraiseql-test-api".to_string()),
        allowed_algorithms: vec!["RS256".to_string()],
        ..Default::default()
    };
    let validator = OidcValidator::with_jwks_uri(config, &format!("{issuer}{jwks_path}"))
        .expect("a loopback http jwks_uri is accepted");

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
        issuer: Some(issuer.clone()),
        audience: Some("fraiseql-test-api".to_string()),
        allowed_algorithms: vec!["RS256".to_string()],
        ..Default::default()
    };
    let validator = OidcValidator::with_jwks_uri(config, &format!("{issuer}{jwks_path}"))
        .expect("a loopback http jwks_uri is accepted");

    let result = validator.validate_token(&token).await;
    assert!(result.is_err(), "wrong audience must be rejected");
}

/// Issuer-less end-to-end: a Hanko-shaped access token that carries **no `iss`
/// claim** must validate when `issuer` is unset and the JWKS URI is pinned.
/// Signature (against the pinned JWKS) and `audience` still gate the token.
#[tokio::test]
async fn validate_token_without_iss_claim_accepted_when_issuer_unset() {
    use jsonwebtoken::{Algorithm, EncodingKey, Header};
    use serde_json::json;
    use wiremock::{
        Mock, MockServer, ResponseTemplate,
        matchers::{method, path},
    };

    let mock = MockServer::start().await;
    let jwks_path = "/.well-known/jwks.json";
    let jwks_body = json!({
        "keys": [{
            "kty": "RSA",
            "kid": "hanko-key",
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

    // Hanko 2.x access-token shape: sub/aud/exp/iat (+ session_id, email); no `iss`.
    let now = chrono::Utc::now().timestamp();
    let claims = json!({
        "sub":        "hanko-user-1",
        "aud":        ["relying-party-id"],
        "exp":        now + 3600,
        "iat":        now,
        "email":      "user@example.com",
        "session_id": "sess-123",
    });

    let mut header = Header::new(Algorithm::RS256);
    header.kid = Some("hanko-key".to_string());
    let encoding_key = EncodingKey::from_rsa_pem(TEST_RSA_PRIVATE_KEY_PEM.as_bytes()).unwrap();
    let token = jsonwebtoken::encode(&header, &claims, &encoding_key).unwrap();

    // Issuer-less config: no `issuer`, pinned `jwks_uri`, mandatory `audience`.
    let config = OidcConfig {
        issuer: None,
        audience: Some("relying-party-id".to_string()),
        jwks_uri: Some(format!("{}{jwks_path}", mock.uri())),
        allowed_algorithms: vec!["RS256".to_string()],
        ..Default::default()
    };
    let validator = OidcValidator::with_jwks_uri(config, &format!("{}{jwks_path}", mock.uri()))
        .expect("a loopback http jwks_uri is accepted");

    let user = validator
        .validate_token(&token)
        .await
        .expect("issuer-less token (no `iss`) must validate against a pinned JWKS");
    assert_eq!(user.user_id.as_str(), "hanko-user-1");
    assert_eq!(user.email.as_deref(), Some("user@example.com"));
}

/// Security boundary: when an `issuer` **is** configured, a token that omits
/// `iss` must still be rejected — setting an issuer keeps `iss` mandatory
/// (jsonwebtoken's `set_issuer` requires the claim). This pins that the
/// issuer-less relaxation only applies when the operator opts out of `iss`.
#[tokio::test]
async fn validate_token_missing_iss_rejected_when_issuer_set() {
    use jsonwebtoken::{Algorithm, EncodingKey, Header};
    use serde_json::json;
    use wiremock::{
        Mock, MockServer, ResponseTemplate,
        matchers::{method, path},
    };

    let mock = MockServer::start().await;
    let jwks_path = "/.well-known/jwks.json";
    let jwks_body = json!({
        "keys": [{
            "kty": "RSA",
            "kid": "with-issuer-key",
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

    // Token omits `iss`.
    let now = chrono::Utc::now().timestamp();
    let claims = json!({
        "sub": "user-42",
        "aud": "fraiseql-test-api",
        "exp": now + 3600,
        "iat": now,
    });
    let mut header = Header::new(Algorithm::RS256);
    header.kid = Some("with-issuer-key".to_string());
    let encoding_key = EncodingKey::from_rsa_pem(TEST_RSA_PRIVATE_KEY_PEM.as_bytes()).unwrap();
    let token = jsonwebtoken::encode(&header, &claims, &encoding_key).unwrap();

    // Issuer IS configured → `iss` is mandatory, so the token is rejected.
    let config = OidcConfig {
        issuer: Some("https://issuer.example.com".to_string()),
        audience: Some("fraiseql-test-api".to_string()),
        jwks_uri: Some(format!("{}{jwks_path}", mock.uri())),
        allowed_algorithms: vec!["RS256".to_string()],
        ..Default::default()
    };
    let validator = OidcValidator::with_jwks_uri(config, &format!("{}{jwks_path}", mock.uri()))
        .expect("a loopback http jwks_uri is accepted");

    let result = validator.validate_token(&token).await;
    assert!(
        result.is_err(),
        "a token missing `iss` must be rejected when an issuer is configured"
    );
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

mod discovery_document_tests {
    #![allow(clippy::unwrap_used)] // Reason: test code, panics are acceptable

    use crate::security::oidc::*;

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

    /// The size cap is the shared client's own constant, re-exported rather than
    /// repeated, so this crate cannot come to hold a second copy of the number.
    ///
    /// The three tests that used to sit here compared `MAX + 1 > MAX` *in the test
    /// body*: they pinned the arithmetic of `>`, never the guard. Both sides of
    /// the real boundary are exercised against the fetch itself in
    /// `fraiseql_jwks`.
    #[test]
    fn the_size_cap_is_the_shared_clients_own_constant() {
        assert_eq!(MAX_JWKS_RESPONSE_BYTES, fraiseql_jwks::MAX_RESPONSE_BYTES);
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
        let result = cache.check_and_record("jti-abc", Duration::from_mins(15)).await;
        assert!(result.is_ok(), "first use should be accepted");
    }

    #[tokio::test]
    async fn test_replay_rejected() {
        let cache = ReplayCache::new(MemoryReplayCache::new());
        cache.check_and_record("jti-abc", Duration::from_mins(15)).await.unwrap();
        let result = cache.check_and_record("jti-abc", Duration::from_mins(15)).await;
        assert!(
            matches!(result, Err(ReplayCacheError::Replayed)),
            "second use of same jti should be rejected"
        );
    }

    #[tokio::test]
    async fn test_different_jtis_accepted_independently() {
        let cache = ReplayCache::new(MemoryReplayCache::new());
        cache.check_and_record("jti-1", Duration::from_mins(15)).await.unwrap();
        let result = cache.check_and_record("jti-2", Duration::from_mins(15)).await;
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
        let result = cache.check_and_record("jti-xyz", Duration::from_mins(15)).await;
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
        let result = cache.check_and_record("jti-xyz", Duration::from_mins(15)).await;
        assert!(result.is_err(), "fail-closed should reject on backend error");
    }

    #[tokio::test]
    async fn test_replay_counter_increments() {
        let before = jwt_replay_rejected_total();
        let cache = ReplayCache::new(MemoryReplayCache::new());
        cache.check_and_record("jti-counter", Duration::from_mins(15)).await.unwrap();
        let _ = cache.check_and_record("jti-counter", Duration::from_mins(15)).await;
        let after = jwt_replay_rejected_total();
        assert!(after > before, "replay counter should have incremented");
    }
}

// ============================================================================
// #1335: the JWKS refetch is bounded, and the algorithm allow-list runs first
// ============================================================================

/// Every request whose `kid` the cache does not hold used to make the server
/// fetch the `IdP`'s JWKS again: no negative cache, no cooldown between refetches,
/// no single-flight for concurrent misses, and the algorithm allow-list checked
/// *after* the key lookup (`token.rs` took `kid` and called `get_decoding_key`
/// before `get_algorithm`).
///
/// A `/graphql` request needs no credential to reach this code, so any anonymous
/// client could make the server issue one outbound HTTPS request per inbound
/// one. The consequence is not the amplification itself but what it turns into:
/// once the `IdP` throttles this server, a genuine key rotation cannot be fetched
/// and every user holding a new-`kid` token is refused — an authentication
/// outage produced by unauthenticated traffic.
///
/// # Why these count fetches through `validate_token`
///
/// `get_decoding_key` cannot see half the defect. The `alg` ordering is only
/// observable from the caller that does both things, and that caller is
/// `validate_token`. Counting at the mock rather than asserting on an internal
/// counter also keeps the suite honest across cycle A3, where this logic moves
/// into a shared primitive: what is pinned here is the number of requests the
/// *provider* sees, which is the property the issue is about.
mod jwks_refetch_bound {
    use std::time::Duration;

    use jsonwebtoken::{Algorithm, EncodingKey, Header};
    use serde_json::json;
    use wiremock::{
        Mock, MockServer, ResponseTemplate,
        matchers::{method, path},
    };

    use super::{OidcConfig, OidcValidator, TEST_RSA_E, TEST_RSA_N, TEST_RSA_PRIVATE_KEY_PEM};

    /// Where the fixture `IdP` publishes its keys.
    const JWKS_PATH: &str = "/.well-known/jwks.json";

    /// The one `kid` the fixture `IdP` publishes. Every token below names a
    /// different one, so each is a genuine cache miss.
    const PUBLISHED_KID: &str = "published-kid";

    /// How many deliveries one burst of unauthenticated traffic stands for.
    const BURST: usize = 20;

    /// A JWKS endpoint that publishes one RSA key and counts every GET.
    ///
    /// Mounted without `.expect(...)`: the count is read back with
    /// [`MockServer::received_requests`], because one of these tests asserts the
    /// endpoint is reached **zero** times and an unmet `expect` would panic on
    /// drop instead of failing the assertion that matters.
    async fn published_jwks(delay: Duration) -> MockServer {
        let mock = MockServer::start().await;
        let body = json!({
            "keys": [{
                "kty": "RSA",
                "kid": PUBLISHED_KID,
                "alg": "RS256",
                "use": "sig",
                "n":   TEST_RSA_N,
                "e":   TEST_RSA_E,
            }]
        });
        Mock::given(method("GET"))
            .and(path(JWKS_PATH))
            .respond_with(ResponseTemplate::new(200).set_body_json(&body).set_delay(delay))
            .mount(&mock)
            .await;
        mock
    }

    /// How many times the fixture `IdP` was asked for its keys.
    async fn fetches(mock: &MockServer) -> usize {
        mock.received_requests()
            .await
            .expect("the fixture JWKS server records its requests")
            .len()
    }

    /// A validator pinned to the fixture's JWKS, in the issuer-less mode #708
    /// added and #1335 measured against. `RS256` only, so the `HS256` token
    /// below is outside the allow-list.
    fn validator_for(mock: &MockServer) -> OidcValidator {
        let config = OidcConfig {
            issuer: None,
            audience: Some("fraiseql-1335".to_string()),
            allowed_algorithms: vec!["RS256".to_string()],
            ..Default::default()
        };
        let uri = format!("{}{JWKS_PATH}", mock.uri());
        OidcValidator::with_jwks_uri(config, &uri).expect("a loopback http jwks_uri is accepted")
    }

    /// Claims good enough to reach the key lookup: the point of every token here
    /// is its *header*, so the body only has to decode.
    fn claims() -> serde_json::Value {
        let now = chrono::Utc::now().timestamp();
        json!({ "sub": "anonymous-caller", "aud": "fraiseql-1335", "exp": now + 3600 })
    }

    /// A genuinely RS256-signed token naming `kid`. Signed with the fixture key,
    /// so the only thing wrong with it is that `kid` is not published — which is
    /// what an attacker sends, and what a legitimate client sends during a
    /// rotation the server has not yet seen.
    fn rs256_token(kid: &str) -> String {
        let mut header = Header::new(Algorithm::RS256);
        header.kid = Some(kid.to_string());
        let key = EncodingKey::from_rsa_pem(TEST_RSA_PRIVATE_KEY_PEM.as_bytes())
            .expect("the fixture RSA private key parses");
        jsonwebtoken::encode(&header, &claims(), &key).expect("the fixture token encodes")
    }

    /// An `HS256` token naming an unknown `kid`. `HS256` is not in
    /// `allowed_algorithms`, so this token is refused whatever key the `IdP`
    /// publishes — and must therefore cost no outbound request.
    fn hs256_token(kid: &str) -> String {
        let mut header = Header::new(Algorithm::HS256);
        header.kid = Some(kid.to_string());
        let key = EncodingKey::from_secret(b"not a key this server would ever accept");
        jsonwebtoken::encode(&header, &claims(), &key).expect("the fixture token encodes")
    }

    #[tokio::test]
    async fn many_distinct_unknown_kids_share_one_jwks_fetch() {
        let mock = published_jwks(Duration::ZERO).await;
        let validator = validator_for(&mock);

        for index in 0..BURST {
            let token = rs256_token(&format!("unknown-kid-{index}"));
            assert!(
                validator.validate_token(&token).await.is_err(),
                "a token signed by a key the `IdP` does not publish must be refused (kid \
                 unknown-kid-{index})"
            );
        }

        // Exactly one, not "at most one": the server has to look once to learn the
        // kid is unpublished, and an implementation that never looks could not pick
        // up a rotation at all.
        assert_eq!(
            fetches(&mock).await,
            1,
            "#1335: {BURST} requests with {BURST} distinct unknown kids must cost one JWKS \
             fetch, not one each — otherwise any anonymous caller sets the server's outbound \
             request rate, and the `IdP`'s throttling turns that into an authentication outage"
        );
    }

    #[tokio::test]
    async fn a_repeated_unknown_kid_is_remembered_as_unknown() {
        let mock = published_jwks(Duration::ZERO).await;
        let validator = validator_for(&mock);
        let token = rs256_token("unknown-kid-repeated");

        for _ in 0..BURST {
            assert!(
                validator.validate_token(&token).await.is_err(),
                "a token signed by a key the `IdP` does not publish must be refused"
            );
        }

        assert_eq!(
            fetches(&mock).await,
            1,
            "#1335: the same unknown kid {BURST} times must cost one JWKS fetch. A miss that \
             is never remembered is the cheapest possible amplification — one captured token \
             replayed is enough"
        );
    }

    #[tokio::test]
    async fn a_disallowed_algorithm_never_reaches_the_jwks_endpoint() {
        let mock = published_jwks(Duration::ZERO).await;
        let validator = validator_for(&mock);

        let token = hs256_token("unknown-kid-hs256");
        assert!(
            validator.validate_token(&token).await.is_err(),
            "HS256 is outside allowed_algorithms, so the token must be refused"
        );

        assert_eq!(
            fetches(&mock).await,
            0,
            "#1335: the algorithm allow-list must be checked BEFORE the key lookup. A token \
             this server would refuse on its header alone must not cost an outbound request"
        );
    }

    /// How many deliveries arrive together in the burst below.
    const CONCURRENT: usize = 8;

    #[tokio::test]
    async fn concurrent_unknown_kid_misses_collapse_onto_one_fetch() {
        // The response is delayed so every miss below is in flight before the
        // first fetch completes. Without that, a cooldown alone passes this test
        // while leaving concurrent misses unbounded — the fetches would merely be
        // serialised, and the test would agree for the wrong reason.
        let mock = published_jwks(Duration::from_millis(300)).await;
        let validator = validator_for(&mock);

        let tokens: Vec<String> =
            (0..CONCURRENT).map(|i| rs256_token(&format!("concurrent-kid-{i}"))).collect();
        let outcomes =
            futures::future::join_all(tokens.iter().map(|token| validator.validate_token(token)))
                .await;

        assert!(
            outcomes.iter().all(std::result::Result::is_err),
            "every token names an unpublished kid, so every one must be refused"
        );
        assert_eq!(
            fetches(&mock).await,
            1,
            "#1335: {CONCURRENT} concurrent misses must single-flight onto one JWKS fetch. A \
             cooldown alone does not give this — it is only consulted once a fetch has \
             finished, and these all start first"
        );
    }
}
