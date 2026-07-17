#![allow(clippy::unwrap_used)] // Reason: test/bench code, panics are acceptable
//! Unit tests for the four [security.*] TOML subsection structs,
//! and the `[auth]` `OidcClientConfig`.

use fraiseql_cli::config::toml_schema::{
    CodeChallengeMethod, ErrorSanitizationTomlConfig, RateLimitingSecurityConfig, TomlSchema,
};

// ---------------------------------------------------------------------------
// ErrorSanitizationTomlConfig
// ---------------------------------------------------------------------------

#[test]
fn test_error_sanitization_config_parses() {
    let toml = r#"
        [schema]
        name = "test"
        version = "1.0.0"
        database_target = "postgresql"

        [database]
        url = "postgresql://localhost/test"

        [security.error_sanitization]
        enabled                     = true
        hide_implementation_details = true
        sanitize_database_errors    = true
        custom_error_message        = "An error occurred"
    "#;
    let schema: TomlSchema = toml::from_str(toml).unwrap();
    let cfg = schema.security.error_sanitization.unwrap();
    assert!(cfg.enabled);
    assert!(cfg.hide_implementation_details);
    assert_eq!(cfg.custom_error_message.as_deref(), Some("An error occurred"));
}

#[test]
fn test_error_sanitization_defaults_are_safe() {
    // Default: enabled=false (opt-in), but sub-flags default to protective values.
    let cfg = ErrorSanitizationTomlConfig::default();
    assert!(!cfg.enabled);
    assert!(cfg.hide_implementation_details);
    assert!(cfg.sanitize_database_errors);
}

// ---------------------------------------------------------------------------
// RateLimitingSecurityConfig
// ---------------------------------------------------------------------------

#[test]
fn test_rate_limiting_config_parses() {
    let toml = r#"
        [schema]
        name = "test"
        version = "1.0.0"
        database_target = "postgresql"

        [database]
        url = "postgresql://localhost/test"

        [security.rate_limiting]
        enabled                   = true
        requests_per_second       = 100
        burst_size                = 200
        auth_start_max_requests   = 5
        auth_start_window_secs    = 60
        failed_login_max_attempts = 10
        failed_login_lockout_secs = 900
        redis_url                 = "redis://localhost:6379"
    "#;
    let schema: TomlSchema = toml::from_str(toml).unwrap();
    let cfg = schema.security.rate_limiting.unwrap();
    assert!(cfg.enabled);
    assert_eq!(cfg.requests_per_second, 100);
    assert_eq!(cfg.auth_start_max_requests, 5);
    assert_eq!(cfg.redis_url.as_deref(), Some("redis://localhost:6379"));
}

#[test]
fn test_rate_limiting_defaults() {
    let cfg = RateLimitingSecurityConfig::default();
    assert!(!cfg.enabled);
    assert_eq!(cfg.requests_per_second, 100);
    assert_eq!(cfg.burst_size, 200);
}

// ---------------------------------------------------------------------------
// StateEncryptionConfig
// ---------------------------------------------------------------------------

#[test]
fn test_state_encryption_config_parses() {
    let toml = r#"
        [schema]
        name = "test"
        version = "1.0.0"
        database_target = "postgresql"

        [database]
        url = "postgresql://localhost/test"

        [security.state_encryption]
        enabled    = true
        algorithm  = "chacha20-poly1305"
        key_source = "env"
        key_env    = "STATE_ENCRYPTION_KEY"
    "#;
    let schema: TomlSchema = toml::from_str(toml).unwrap();
    let cfg = schema.security.state_encryption.unwrap();
    assert!(cfg.enabled);
    assert_eq!(cfg.algorithm.to_string(), "chacha20-poly1305");
    assert_eq!(cfg.key_env.as_deref(), Some("STATE_ENCRYPTION_KEY"));
}

#[test]
fn test_state_encryption_valid_algorithms() {
    for algo in ["chacha20-poly1305", "aes-256-gcm"] {
        let toml = format!(
            "[schema]\nname=\"t\"\nversion=\"1.0.0\"\ndatabase_target=\"postgresql\"\n\
             [database]\nurl=\"postgresql://localhost/t\"\n\
             [security.state_encryption]\nenabled=true\nalgorithm=\"{algo}\""
        );
        let schema: TomlSchema = toml::from_str(&toml).unwrap();
        assert_eq!(schema.security.state_encryption.unwrap().algorithm.to_string(), algo);
    }
}

// ---------------------------------------------------------------------------
// PkceConfig
// ---------------------------------------------------------------------------

#[test]
fn test_pkce_config_parses() {
    let toml = r#"
        [schema]
        name = "test"
        version = "1.0.0"
        database_target = "postgresql"

        [database]
        url = "postgresql://localhost/test"

        [security.pkce]
        enabled               = true
        code_challenge_method = "S256"
        state_ttl_secs        = 600
    "#;
    let schema: TomlSchema = toml::from_str(toml).unwrap();
    let cfg = schema.security.pkce.unwrap();
    assert!(cfg.enabled);
    assert_eq!(cfg.code_challenge_method, CodeChallengeMethod::S256);
    assert_eq!(cfg.state_ttl_secs, 600);
}

#[test]
fn test_pkce_plain_method_allowed_in_config() {
    // "plain" is allowed by the spec; we warn at runtime but must not error at parse time.
    let toml = r#"
        [schema]
        name = "test"
        version = "1.0.0"
        database_target = "postgresql"

        [database]
        url = "postgresql://localhost/test"

        [security.pkce]
        enabled               = true
        code_challenge_method = "plain"
        state_ttl_secs        = 300
    "#;
    let schema: TomlSchema = toml::from_str(toml).unwrap();
    assert_eq!(schema.security.pkce.unwrap().code_challenge_method, CodeChallengeMethod::Plain);
}

// ---------------------------------------------------------------------------
// All four fields present simultaneously
// ---------------------------------------------------------------------------

#[test]
fn test_all_security_subsections_parse_together() {
    let toml = r#"
        [schema]
        name = "test"
        version = "1.0.0"
        database_target = "postgresql"

        [database]
        url = "postgresql://localhost/test"

        [security.error_sanitization]
        enabled = true

        [security.rate_limiting]
        enabled             = true
        requests_per_second = 50

        [security.state_encryption]
        enabled   = true
        algorithm = "aes-256-gcm"

        [security.pkce]
        enabled        = true
        state_ttl_secs = 300
    "#;
    let schema: TomlSchema = toml::from_str(toml).unwrap();
    assert!(schema.security.error_sanitization.unwrap().enabled);
    assert_eq!(schema.security.rate_limiting.unwrap().requests_per_second, 50);
    assert_eq!(schema.security.state_encryption.unwrap().algorithm.to_string(), "aes-256-gcm");
    assert_eq!(schema.security.pkce.unwrap().state_ttl_secs, 300);
}

#[test]
fn test_existing_enterprise_field_not_broken() {
    // Regression: the existing [security.enterprise] section still works.
    let toml = r#"
        [schema]
        name = "test"
        version = "1.0.0"
        database_target = "postgresql"

        [database]
        url = "postgresql://localhost/test"

        [security.enterprise]
        rate_limiting_enabled = false
        pkce_enabled          = false
    "#;
    let schema: TomlSchema = toml::from_str(toml).unwrap();
    assert!(!schema.security.enterprise.rate_limiting_enabled);
    assert!(!schema.security.enterprise.pkce_enabled);
    // New fields should be None when omitted
    assert!(schema.security.error_sanitization.is_none());
    assert!(schema.security.rate_limiting.is_none());
}

// ---------------------------------------------------------------------------
// OidcClientConfig / [auth]
// ---------------------------------------------------------------------------

// #612 item 9: the CLI [auth] schema and the server's OidcConfig read the same
// fraiseql.toml [auth], but the CLI's old schema (required PKCE client fields +
// deny_unknown_fields) rejected a JWT-only block, so a single valid [auth] could not
// exist. The union schema accepts the JWT group (issuer/audience) — functional — while
// a complete PKCE client group is rejected loud (not yet functional on the compiled
// path — #621), never silently accepted.

// M-612: a JWT-only [auth] block now compiles (was rejected — this is the item-9 fix).
#[test]
fn test_auth_jwt_only_compiles() {
    let toml = r#"
        [schema]
        name = "test"
        version = "1.0.0"
        database_target = "postgresql"

        [auth]
        issuer   = "https://accounts.google.com"
        audience = "my-api"
    "#;
    let schema: TomlSchema = toml::from_str(toml).unwrap();
    schema.validate().expect("a JWT-validation [auth] block must compile");
    let auth = schema.auth.expect("auth section should be present");
    assert_eq!(auth.issuer.as_deref(), Some("https://accounts.google.com"));
    assert_eq!(auth.audience.as_deref(), Some("my-api"));
}

#[test]
fn test_auth_issuer_without_audience_compiles() {
    let toml = r#"
        [schema]
        name = "test"
        version = "1.0.0"
        database_target = "postgresql"

        [auth]
        issuer = "https://accounts.example.com"
    "#;
    let schema: TomlSchema = toml::from_str(toml).unwrap();
    schema.validate().expect("issuer alone is a valid JWT [auth] block");
}

// M-612: a complete PKCE client group is rejected loud (recognized, not yet functional).
#[test]
fn test_auth_complete_client_group_rejected_loud() {
    let toml = r#"
        [schema]
        name = "test"
        version = "1.0.0"
        database_target = "postgresql"

        [auth]
        discovery_url       = "https://accounts.google.com"
        client_id           = "my-client-id"
        client_secret_env   = "OIDC_CLIENT_SECRET"
        server_redirect_uri = "https://api.example.com/auth/callback"
    "#;
    let schema: TomlSchema = toml::from_str(toml).unwrap();
    let err = schema.validate().expect_err("a complete PKCE client group must be rejected");
    let msg = err.to_string();
    assert!(msg.contains("not yet functional"), "explains why it is rejected: {msg}");
    assert!(msg.contains("#621"), "points at the tracking follow-up: {msg}");
}

#[test]
fn test_auth_partial_client_group_rejected_names_missing() {
    let toml = r#"
        [schema]
        name = "test"
        version = "1.0.0"
        database_target = "postgresql"

        [auth]
        issuer        = "https://accounts.example.com"
        discovery_url = "https://accounts.example.com"
        client_id     = "my-client-id"
    "#;
    let schema: TomlSchema = toml::from_str(toml).unwrap();
    let err = schema.validate().expect_err("an incomplete PKCE client group must be rejected");
    let msg = err.to_string();
    assert!(msg.contains("incomplete"), "flags the incomplete group: {msg}");
    assert!(msg.contains("client_secret_env"), "names a missing field: {msg}");
    assert!(msg.contains("server_redirect_uri"), "names a missing field: {msg}");
}

#[test]
fn test_auth_empty_block_rejected() {
    // A present-but-empty [auth] table configures nothing and must not pass silently.
    let toml = r#"
        [schema]
        name = "test"
        version = "1.0.0"
        database_target = "postgresql"

        [auth]
    "#;
    let schema: TomlSchema = toml::from_str(toml).unwrap();
    let err = schema.validate().expect_err("an empty [auth] block must be rejected");
    assert!(err.to_string().contains("empty"), "explains the empty block: {err}");
}

#[test]
fn test_auth_audience_without_issuer_rejected() {
    let toml = r#"
        [schema]
        name = "test"
        version = "1.0.0"
        database_target = "postgresql"

        [auth]
        audience = "my-api"
    "#;
    let schema: TomlSchema = toml::from_str(toml).unwrap();
    let err = schema.validate().expect_err("audience without issuer must be rejected");
    assert!(err.to_string().contains("issuer"), "explains issuer is required: {err}");
}

#[test]
fn test_auth_absent_by_default() {
    let toml = r#"
        [schema]
        name = "test"
        version = "1.0.0"
        database_target = "postgresql"

        [database]
        url = "postgresql://localhost/test"
    "#;
    let schema: TomlSchema = toml::from_str(toml).unwrap();
    assert!(schema.auth.is_none(), "[auth] should be absent when not specified");
}

#[test]
fn test_auth_client_secret_field_rejected() {
    // SECURITY: `client_secret` must NEVER appear in the TOML config.
    // `deny_unknown_fields` on OidcClientConfig must reject it.
    let toml = r#"
        [schema]
        name = "test"
        version = "1.0.0"
        database_target = "postgresql"

        [database]
        url = "postgresql://localhost/test"

        [auth]
        discovery_url       = "https://accounts.example.com"
        client_id           = "x"
        client_secret_env   = "MY_SECRET"
        server_redirect_uri = "https://example.com/auth/callback"
        client_secret       = "this-must-fail"
    "#;
    let result = toml::from_str::<TomlSchema>(toml);
    assert!(
        result.is_err(),
        "client_secret in TOML must be rejected — secrets belong in env vars, not config files"
    );
}
