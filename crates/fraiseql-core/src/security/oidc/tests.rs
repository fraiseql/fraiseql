//! Integration tests for the OIDC module covering providers and token validation logic.

#![allow(clippy::unwrap_used)] // Reason: test code, panics are acceptable

use std::sync::Arc;
use std::time::{Duration, Instant};

use parking_lot::RwLock;

use crate::security::errors::SecurityError;
use crate::security::oidc::{
    jwks::{CachedJwks, Jwk, Jwks},
    providers::OidcConfig,
    token::OidcValidator,
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
    assert_eq!(
        config.issuer,
        "https://cognito-idp.us-east-1.amazonaws.com/us-east-1_abc123"
    );
    assert_eq!(config.audience, Some("client123".to_string()));
}

#[test]
fn test_oidc_config_azure_ad() {
    let config = OidcConfig::azure_ad("tenant-id-123", "client-id-456");
    assert_eq!(
        config.issuer,
        "https://login.microsoftonline.com/tenant-id-123/v2.0"
    );
    assert_eq!(config.audience, Some("client-id-456".to_string()));
}

#[test]
fn test_oidc_config_google() {
    let config = OidcConfig::google("123456.apps.googleusercontent.com");
    assert_eq!(config.issuer, "https://accounts.google.com");
    assert_eq!(
        config.audience,
        Some("123456.apps.googleusercontent.com".to_string())
    );
}

#[test]
fn test_oidc_config_validate_empty_issuer() {
    let config = OidcConfig::default();
    let result = config.validate();
    assert!(result.is_err());
    assert!(matches!(result, Err(SecurityError::SecurityConfigError(_))));
}

#[test]
fn test_oidc_config_validate_http_issuer() {
    let config = OidcConfig {
        issuer: "http://insecure.example.com".to_string(),
        ..Default::default()
    };
    let result = config.validate();
    assert!(result.is_err());
}

#[test]
fn test_oidc_config_validate_localhost_allowed() {
    let config = OidcConfig {
        issuer: "http://localhost:8080".to_string(),
        audience: Some("my-api".to_string()),
        ..Default::default()
    };
    let result = config.validate();
    assert!(result.is_ok());
}

#[test]
fn test_oidc_config_validate_https_required() {
    let config = OidcConfig {
        issuer: "https://secure.example.com".to_string(),
        audience: Some("https://api.example.com".to_string()),
        ..Default::default()
    };
    let result = config.validate();
    assert!(result.is_ok());
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
        config: OidcConfig {
            issuer: issuer.to_string(),
            ..Default::default()
        },
        http_client: reqwest::Client::new(),
        jwks_uri: format!("{issuer}/.well-known/jwks.json"),
        jwks_cache: Arc::new(RwLock::new(None)),
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
    let new_jwks = Jwks { keys: vec![make_jwk("key1")] };
    // Should not detect rotation when cache is empty
    assert!(!validator.detect_key_rotation(&new_jwks));
}

#[test]
fn test_detect_key_rotation_when_keys_removed() {
    let validator = make_validator("http://localhost:8080");

    let old_jwks = Jwks { keys: vec![make_jwk("old_key_1"), make_jwk("old_key_2")] };
    {
        let mut cache = validator.jwks_cache.write();
        *cache = Some(CachedJwks {
            jwks:       old_jwks,
            fetched_at: Instant::now(),
            ttl:        Duration::from_secs(300),
        });
    }

    // New JWKS with only 1 of the old keys (old_key_2 removed)
    let new_jwks = Jwks { keys: vec![make_jwk("old_key_1"), make_jwk("new_key_1")] };
    // Should detect rotation because old_key_2 is missing
    assert!(validator.detect_key_rotation(&new_jwks));
}

#[test]
fn test_detect_key_rotation_when_no_keys_removed() {
    let validator = make_validator("http://localhost:8080");

    let old_jwks = Jwks { keys: vec![make_jwk("key_1"), make_jwk("key_2")] };
    {
        let mut cache = validator.jwks_cache.write();
        *cache = Some(CachedJwks {
            jwks:       old_jwks,
            fetched_at: Instant::now(),
            ttl:        Duration::from_secs(300),
        });
    }

    // New JWKS with old keys + new key (no removal)
    let new_jwks =
        Jwks { keys: vec![make_jwk("key_1"), make_jwk("key_2"), make_jwk("new_key")] };
    // Should NOT detect rotation because all old keys still exist
    assert!(!validator.detect_key_rotation(&new_jwks));
}

#[test]
fn test_find_key_by_kid() {
    let validator = make_validator("http://localhost:8080");
    let jwks = Jwks { keys: vec![make_jwk("key1"), make_jwk("key2")] };

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
    use wiremock::{Mock, MockServer, ResponseTemplate, matchers::{method, path}};
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
        ..Default::default()
    };
    let result = OidcValidator::new(config).await;
    assert!(result.is_err(), "oversized discovery response must be rejected");
    let msg = result.err().unwrap().to_string();
    assert!(
        msg.contains("too large"),
        "error must mention size limit: {msg}"
    );
}

#[tokio::test]
async fn oidc_discovery_within_size_limit_proceeds_to_parse() {
    use wiremock::{Mock, MockServer, ResponseTemplate, matchers::{method, path}};

    let mock = MockServer::start().await;

    // A small (invalid JSON) body — size check passes, JSON parse fails gracefully
    Mock::given(method("GET"))
        .and(path("/.well-known/openid-configuration"))
        .respond_with(ResponseTemplate::new(200).set_body_string("not json"))
        .mount(&mock)
        .await;

    let config = OidcConfig {
        issuer: mock.uri(),
        ..Default::default()
    };
    let result = OidcValidator::new(config).await;
    // Must fail with a JSON parse error, NOT a size error
    assert!(result.is_err());
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
        issuer:   "https://example.com".to_string(),
        jwks_uri: Some("https://example.com/.well-known/jwks.json".to_string()),
        ..Default::default()
    };
    let validator = OidcValidator::with_jwks_uri(config, "https://example.com/jwks".to_string());
    assert_eq!(validator.issuer(), "https://example.com");
}
