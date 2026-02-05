//! JWT Audience Validation Tests
//!
//! Tests for OIDC configuration audience validation:
//! - Audience is now mandatory for security
//! - Prevents token confusion attacks
//! - Configuration validation enforces audience requirement

use fraiseql_core::security::oidc::OidcConfig;

#[test]
fn test_oidc_config_without_audience_fails_validation() {
    let config = OidcConfig {
        issuer: "https://example.auth0.com/".to_string(),
        audience: None,
        additional_audiences: vec![],
        ..Default::default()
    };

    let result = config.validate();
    assert!(
        result.is_err(),
        "OIDC configuration without audience should fail validation"
    );

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
        issuer: "https://example.auth0.com/".to_string(),
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
        issuer: "https://example.auth0.com/".to_string(),
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
        issuer: "https://example.auth0.com/".to_string(),
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
        issuer: "https://my-tenant.auth0.com/".to_string(),
        audience: Some("https://api.myapp.com".to_string()),
        ..Default::default()
    };

    assert!(config.validate().is_ok());
}

#[test]
fn test_oidc_config_keycloak_pattern() {
    // Typical Keycloak configuration pattern
    let config = OidcConfig {
        issuer: "https://keycloak.example.com/auth/realms/my-realm".to_string(),
        audience: Some("my-client-id".to_string()),
        ..Default::default()
    };

    assert!(config.validate().is_ok());
}

#[test]
fn test_oidc_config_okta_pattern() {
    // Typical Okta configuration pattern
    let config = OidcConfig {
        issuer: "https://dev-12345.okta.com".to_string(),
        audience: Some("api://myapp".to_string()),
        ..Default::default()
    };

    assert!(config.validate().is_ok());
}

#[test]
fn test_oidc_config_multiple_audiences() {
    // Configuration allowing tokens for multiple services
    let config = OidcConfig {
        issuer: "https://example.auth0.com/".to_string(),
        audience: Some("https://api.example.com".to_string()),
        additional_audiences: vec![
            "https://api-v2.example.com".to_string(),
            "https://internal-api.example.com".to_string(),
        ],
        ..Default::default()
    };

    assert!(config.validate().is_ok());
}

#[test]
fn test_oidc_config_issuer_validation_still_required() {
    // Audience being required doesn't override issuer requirement
    let config = OidcConfig {
        issuer: "".to_string(), // Missing issuer
        audience: Some("https://api.example.com".to_string()),
        ..Default::default()
    };

    let result = config.validate();
    assert!(result.is_err());
    assert!(format!("{:?}", result.unwrap_err()).contains("issuer"));
}

#[test]
fn test_oidc_config_https_requirement_still_enforced() {
    // Audience being required doesn't override HTTPS requirement
    let config = OidcConfig {
        issuer: "http://example.com/".to_string(), // Not HTTPS (and not localhost)
        audience: Some("https://api.example.com".to_string()),
        ..Default::default()
    };

    let result = config.validate();
    assert!(result.is_err());
    assert!(format!("{:?}", result.unwrap_err()).contains("HTTPS"));
}

#[test]
fn test_oidc_config_localhost_exception_still_works() {
    // Audience requirement doesn't affect localhost exception
    let config = OidcConfig {
        issuer: "http://localhost:8080/".to_string(),
        audience: Some("localhost".to_string()),
        ..Default::default()
    };

    assert!(config.validate().is_ok());
}
