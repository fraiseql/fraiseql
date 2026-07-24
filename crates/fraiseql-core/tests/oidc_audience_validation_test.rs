//! JWT Audience Validation Tests
//!
//! Tests for OIDC configuration audience validation:
//! - Audience is now mandatory for security
//! - Prevents token confusion attacks
//! - Configuration validation enforces audience requirement

#![allow(clippy::unwrap_used, clippy::panic)] // Reason: test code, panics acceptable
#![allow(clippy::no_effect_underscore_binding)] // Reason: _ bindings used to satisfy destructuring patterns in test assertions
use fraiseql_core::security::oidc::OidcConfig;

#[test]
fn test_oidc_config_without_audience_fails_validation() {
    let config = OidcConfig {
        issuer: Some("https://example.auth0.com/".to_string()),
        audience: None,
        additional_audiences: vec![],
        ..Default::default()
    };

    let result = config.validate();
    assert!(result.is_err(), "OIDC configuration without audience should fail validation");

    let error_message = format!("{:?}", result.unwrap_err());
    assert!(
        error_message.contains("audience") && error_message.contains("REQUIRED"),
        "Error message should clearly indicate audience is required: {}",
        error_message
    );
}

#[test]
fn test_oidc_config_with_audience_passes_validation() {
    let config = OidcConfig {
        issuer: Some("https://example.auth0.com/".to_string()),
        audience: Some("https://api.example.com".to_string()),
        additional_audiences: vec![],
        ..Default::default()
    };

    assert!(
        config.validate().is_ok(),
        "OIDC configuration with primary audience should pass validation"
    );
}

#[test]
fn test_oidc_config_with_additional_audiences_passes_validation() {
    let config = OidcConfig {
        issuer: Some("https://example.auth0.com/".to_string()),
        audience: None,
        additional_audiences: vec!["https://api.example.com".to_string()],
        ..Default::default()
    };

    assert!(
        config.validate().is_ok(),
        "OIDC configuration with additional_audiences should pass validation"
    );
}

#[test]
fn test_oidc_config_with_both_audience_and_additional_passes_validation() {
    let config = OidcConfig {
        issuer: Some("https://example.auth0.com/".to_string()),
        audience: Some("https://api.example.com".to_string()),
        additional_audiences: vec!["https://api2.example.com".to_string()],
        ..Default::default()
    };

    assert!(
        config.validate().is_ok(),
        "OIDC configuration with both audience and additional_audiences should pass validation"
    );
}

#[test]
fn test_oidc_config_auth0_pattern() {
    // Typical Auth0 configuration pattern
    let config = OidcConfig {
        issuer: Some("https://my-tenant.auth0.com/".to_string()),
        audience: Some("https://api.myapp.com".to_string()),
        ..Default::default()
    };

    config
        .validate()
        .unwrap_or_else(|e| panic!("Auth0 config should pass validation: {e}"));
}

#[test]
fn test_oidc_config_keycloak_pattern() {
    // Typical Keycloak configuration pattern
    let config = OidcConfig {
        issuer: Some("https://keycloak.example.com/auth/realms/my-realm".to_string()),
        audience: Some("my-client-id".to_string()),
        ..Default::default()
    };

    config
        .validate()
        .unwrap_or_else(|e| panic!("Keycloak config should pass validation: {e}"));
}

#[test]
fn test_oidc_config_okta_pattern() {
    // Typical Okta configuration pattern
    let config = OidcConfig {
        issuer: Some("https://dev-12345.okta.com".to_string()),
        audience: Some("api://myapp".to_string()),
        ..Default::default()
    };

    config
        .validate()
        .unwrap_or_else(|e| panic!("Okta config should pass validation: {e}"));
}

#[test]
fn test_oidc_config_multiple_audiences() {
    // Configuration allowing tokens for multiple services
    let config = OidcConfig {
        issuer: Some("https://example.auth0.com/".to_string()),
        audience: Some("https://api.example.com".to_string()),
        additional_audiences: vec![
            "https://api-v2.example.com".to_string(),
            "https://internal-api.example.com".to_string(),
        ],
        ..Default::default()
    };

    config
        .validate()
        .unwrap_or_else(|e| panic!("multiple audiences config should pass validation: {e}"));
}

#[test]
fn test_oidc_config_issuerless_requires_pinned_jwks_uri() {
    // `issuer` is optional (symmetric with `audience`), but without it OIDC
    // discovery can't locate the JWKS endpoint — so an unset issuer with no
    // pinned `jwks_uri` must be rejected. Audience is set to isolate this guard.
    let config = OidcConfig {
        issuer: None,   // issuer-less mode …
        jwks_uri: None, // … but no pinned JWKS endpoint to fall back on
        audience: Some("https://api.example.com".to_string()),
        ..Default::default()
    };

    let result = config.validate();
    assert!(
        result.is_err(),
        "expected Err for issuer-less config without jwks_uri, got: {result:?}"
    );
    assert!(format!("{:?}", result.unwrap_err()).contains("jwks_uri"));
}

#[test]
fn test_oidc_config_issuerless_with_pinned_jwks_uri_is_valid() {
    // The complementary positive case: an IdP whose tokens omit `iss` (e.g.
    // Hanko) is usable when the JWKS endpoint is pinned and audience is set.
    let config = OidcConfig {
        issuer: None,
        jwks_uri: Some("https://hanko.example.com/.well-known/jwks.json".to_string()),
        audience: Some("relying-party-id".to_string()),
        ..Default::default()
    };

    config
        .validate()
        .unwrap_or_else(|e| panic!("issuer-less config with pinned jwks_uri should validate: {e}"));
}

#[test]
fn test_oidc_config_https_requirement_still_enforced() {
    // Audience being required doesn't override HTTPS requirement
    let config = OidcConfig {
        issuer: Some("http://example.com/".to_string()), // Not HTTPS (and not localhost)
        audience: Some("https://api.example.com".to_string()),
        ..Default::default()
    };

    let result = config.validate();
    assert!(result.is_err(), "expected Err for non-HTTPS issuer, got: {result:?}");
    assert!(format!("{:?}", result.unwrap_err()).contains("HTTPS"));
}

#[test]
fn test_oidc_config_localhost_exception_still_works() {
    // Audience requirement doesn't affect localhost exception
    let config = OidcConfig {
        issuer: Some("http://localhost:8080/".to_string()),
        audience: Some("localhost".to_string()),
        ..Default::default()
    };

    config
        .validate()
        .unwrap_or_else(|e| panic!("localhost OIDC config should pass validation: {e}"));
}
