#![allow(missing_docs)]

use std::path::PathBuf;

use fraiseql_error::ConfigError;

#[test]
fn not_found_error_code_and_display() {
    assert_eq!(ConfigError::NotFound.error_code(), "config_not_found");
    assert_eq!(
        ConfigError::NotFound.to_string(),
        "Configuration file not found"
    );
}

#[test]
fn read_error_code_and_display() {
    let err = ConfigError::ReadError {
        path:   PathBuf::from("/etc/fraiseql.toml"),
        source: std::io::Error::new(std::io::ErrorKind::PermissionDenied, "denied"),
    };
    assert_eq!(err.error_code(), "config_read_error");
    assert!(err
        .to_string()
        .starts_with("Failed to read configuration file /etc/fraiseql.toml:"));
}

#[test]
fn parse_error_code() {
    let toml_err = toml::from_str::<toml::Value>("bad [[").unwrap_err();
    let err = ConfigError::ParseError { source: toml_err };
    assert_eq!(err.error_code(), "config_parse_error");
    assert!(err
        .to_string()
        .starts_with("Failed to parse configuration:"));
}

#[test]
fn parse_error_from_toml_error() {
    let toml_err = toml::from_str::<toml::Value>("bad [[").unwrap_err();
    let err: ConfigError = toml_err.into();
    assert_eq!(err.error_code(), "config_parse_error");
}

#[test]
fn validation_error_code_and_display() {
    let err = ConfigError::ValidationError {
        field:   "port".into(),
        message: "must be between 1 and 65535".into(),
    };
    assert_eq!(err.error_code(), "config_validation_error");
    assert_eq!(
        err.to_string(),
        "Validation error in port: must be between 1 and 65535"
    );
}

#[test]
fn missing_env_var_error_code_and_display() {
    let err = ConfigError::MissingEnvVar {
        name: "DATABASE_URL".into(),
    };
    assert_eq!(err.error_code(), "config_missing_env");
    assert_eq!(
        err.to_string(),
        "Missing required environment variable: DATABASE_URL"
    );
}

#[test]
fn multiple_errors_code_and_display() {
    let err = ConfigError::MultipleErrors {
        errors: vec![ConfigError::NotFound],
    };
    assert_eq!(err.error_code(), "config_multiple_errors");
    assert_eq!(err.to_string(), "Multiple configuration errors");
}
