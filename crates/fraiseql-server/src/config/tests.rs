use std::time::Duration;

use fraiseql_error::ConfigError;

use super::*;
use crate::config::{
    env::{parse_duration, parse_size, resolve_env_value},
    validation::ConfigValidator,
};

#[test]
fn test_parse_minimal_config() {
    let toml = r#"
        [server]
        port = 4000

        [database]
        url_env = "DATABASE_URL"
    "#;

    std::env::set_var("DATABASE_URL", "postgres://localhost/test");

    let config: RuntimeConfig = toml::from_str(toml).unwrap();

    assert_eq!(config.server.port, 4000);
    assert_eq!(config.server.host, "127.0.0.1");
    assert_eq!(config.database.url_env, "DATABASE_URL");
    assert_eq!(config.database.pool_size, 10);
}

#[test]
fn test_parse_size() {
    assert_eq!(parse_size("10MB").unwrap(), 10 * 1024 * 1024);
    assert_eq!(parse_size("1GB").unwrap(), 1024 * 1024 * 1024);
    assert_eq!(parse_size("500KB").unwrap(), 500 * 1024);
    assert_eq!(parse_size("1000").unwrap(), 1000);
    assert_eq!(parse_size("100B").unwrap(), 100);
}

#[test]
fn test_parse_size_invalid() {
    assert!(parse_size("abc").is_err());
}

#[test]
fn test_parse_duration() {
    assert_eq!(parse_duration("30s").unwrap(), Duration::from_secs(30));
    assert_eq!(parse_duration("5m").unwrap(), Duration::from_secs(300));
    assert_eq!(parse_duration("1h").unwrap(), Duration::from_secs(3600));
    assert_eq!(parse_duration("100ms").unwrap(), Duration::from_millis(100));
    assert_eq!(parse_duration("1d").unwrap(), Duration::from_secs(86400));
}

#[test]
fn test_parse_duration_invalid() {
    assert!(parse_duration("30").is_err()); // Missing unit
    assert!(parse_duration("abc").is_err());
}

#[test]
fn test_env_resolution_with_default() {
    std::env::remove_var("NONEXISTENT_VAR");
    let result = resolve_env_value("${NONEXISTENT_VAR:-default_value}").unwrap();
    assert_eq!(result, "default_value");
}

#[test]
fn test_env_resolution_without_default() {
    std::env::set_var("EXISTING_VAR", "actual_value");
    let result = resolve_env_value("${EXISTING_VAR:-default}").unwrap();
    assert_eq!(result, "actual_value");
}

#[test]
fn test_validation_missing_env_var() {
    let toml = r#"
        [server]
        port = 4000

        [database]
        url_env = "NONEXISTENT_DB_URL"
    "#;

    std::env::remove_var("NONEXISTENT_DB_URL");

    let config: RuntimeConfig = toml::from_str(toml).unwrap();
    let result = ConfigValidator::new(&config).validate();

    assert!(!result.is_ok());
    assert!(result.errors.iter().any(|e| matches!(e, ConfigError::MissingEnvVar { .. })));
}

#[test]
fn test_validation_cross_field() {
    let toml = r#"
        [server]
        port = 4000

        [database]
        url_env = "DATABASE_URL"

        [observers.test]
        entity = "users"
        events = ["insert"]

        [[observers.test.actions]]
        type = "email"
        template = "welcome"
    "#;

    std::env::set_var("DATABASE_URL", "postgres://localhost/test");

    let config: RuntimeConfig = toml::from_str(toml).unwrap();
    let result = ConfigValidator::new(&config).validate();

    // Should fail because email action requires notifications config
    assert!(!result.is_ok());
}
