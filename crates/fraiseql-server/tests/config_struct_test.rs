//! # Config Struct Tests: `ServerConfig` Defaults and Validation
//!
//! Tests `ServerConfig` default values, field validation, and serialization
//! round-trips. Verifies that configuration defaults match documented behavior.
//!
//! **Execution engine:** none (config struct only, no server started)
//! **Infrastructure:** none
//! **Parallelism:** safe (no shared mutable state)
#![allow(clippy::unwrap_used)] // Reason: test code, panics acceptable
#![allow(clippy::cast_precision_loss)] // Reason: test metrics reporting
#![allow(clippy::cast_sign_loss)] // Reason: test data uses small positive integers
#![allow(clippy::cast_possible_truncation)] // Reason: test data values are bounded
#![allow(clippy::cast_possible_wrap)] // Reason: test data values are bounded
#![allow(clippy::cast_lossless)] // Reason: test code readability
#![allow(clippy::missing_panics_doc)] // Reason: test helper functions
#![allow(clippy::missing_errors_doc)] // Reason: test helper functions
#![allow(missing_docs)] // Reason: test code
#![allow(clippy::items_after_statements)] // Reason: test helpers near use site
#![allow(clippy::used_underscore_binding)] // Reason: test variables use _ prefix
#![allow(clippy::needless_pass_by_value)] // Reason: test helper signatures
#![allow(clippy::match_same_arms)] // Reason: test data clarity
#![allow(clippy::branches_sharing_code)] // Reason: test assertion clarity
#![allow(clippy::undocumented_unsafe_blocks)] // Reason: test exercises unsafe paths

use std::{io::Write, path::PathBuf};

use fraiseql_server::{CompiledSchemaLoader, ServerConfig};
use tempfile::NamedTempFile;

/// Test default configuration
#[test]
fn test_default_config() {
    let config = ServerConfig::default();

    assert_eq!(config.graphql_path, "/graphql");
    assert_eq!(config.health_path, "/health");
    assert_eq!(config.introspection_path, "/introspection");
    assert_eq!(config.schema_path, PathBuf::from("schema.compiled.json"));
    assert!(config.cors_enabled);
    assert!(!config.compression_enabled);
    assert!(config.tracing_enabled);
}

/// Test configuration serialization with serde
#[test]
fn test_config_serialization() {
    let config = ServerConfig::default();
    let toml_str = toml::to_string(&config).expect("Failed to serialize config");

    // Should be valid TOML
    assert!(toml_str.contains("schema_path"));
    assert!(toml_str.contains("bind_addr"));
}

/// Test configuration deserialization from TOML
#[test]
fn test_config_deserialization() {
    let toml_str = r#"
        schema_path = "custom_schema.json"
        graphql_path = "/api/graphql"
        health_path = "/api/health"
        cors_enabled = false
        compression_enabled = true
    "#;

    let config: ServerConfig = toml::from_str(toml_str).expect("Failed to deserialize config");

    assert_eq!(config.schema_path, PathBuf::from("custom_schema.json"));
    assert_eq!(config.graphql_path, "/api/graphql");
    assert_eq!(config.health_path, "/api/health");
    assert!(!config.cors_enabled);
    assert!(config.compression_enabled);
}

/// Test schema loader with non-existent file
#[tokio::test]
async fn test_schema_loader_missing_file() {
    let loader = CompiledSchemaLoader::new("/nonexistent/schema.json");
    let result = loader.load().await;

    assert!(result.is_err(), "expected Err loading nonexistent schema, got Ok");
    let err_msg = result.unwrap_err().to_string();
    assert!(err_msg.contains("not found"), "expected 'not found' in error, got: {err_msg}");
}

/// Test schema loader with invalid JSON
#[tokio::test]
async fn test_schema_loader_invalid_json() {
    let mut temp_file = NamedTempFile::new().expect("Failed to create temp file");
    writeln!(temp_file, "{{invalid json").expect("Failed to write to temp file");

    let loader = CompiledSchemaLoader::new(temp_file.path());
    let result = loader.load().await;

    assert!(result.is_err(), "expected Err loading invalid JSON schema, got Ok");
    let err_msg = result.unwrap_err().to_string();
    assert!(
        err_msg.to_lowercase().contains("parse") || err_msg.to_lowercase().contains("json"),
        "expected 'parse' or 'json' in error, got: {err_msg}"
    );
}

/// Test schema loader path getter
#[test]
fn test_schema_loader_path() {
    let path = "/tmp/test_schema.json";
    let loader = CompiledSchemaLoader::new(path);

    assert_eq!(loader.path(), PathBuf::from(path).as_path());
}

/// Test schema loader path display
#[test]
fn test_schema_loader_path_display() {
    let path = "/home/user/schema.compiled.json";
    let loader = CompiledSchemaLoader::new(path);

    let path_display = loader.path().display().to_string();
    assert_eq!(path_display, path);
}

/// Test multiple configurations can coexist
#[test]
fn test_multiple_configs() {
    let config1 = ServerConfig::default();
    let config2 = ServerConfig {
        schema_path: PathBuf::from("other.json"),
        ..ServerConfig::default()
    };

    assert_eq!(config1.schema_path, PathBuf::from("schema.compiled.json"));
    assert_eq!(config2.schema_path, PathBuf::from("other.json"));
}

/// Test server config with custom bind address
#[test]
fn test_config_custom_bind_addr() {
    let config = ServerConfig {
        bind_addr: "0.0.0.0:8080".parse().unwrap(),
        ..ServerConfig::default()
    };

    assert_eq!(config.bind_addr.ip().to_string(), "0.0.0.0");
    assert_eq!(config.bind_addr.port(), 8080);
}

/// Test pool min size is positive and consistent in default config.
#[test]
fn pool_min_size_is_positive_in_default_config() {
    let cfg = ServerConfig::default();
    assert!(cfg.pool_min_size > 0, "default pool_min_size should be > 0, got {}", cfg.pool_min_size);
    assert!(
        cfg.pool_min_size <= cfg.pool_max_size,
        "pool_min_size ({}) must not exceed pool_max_size ({})",
        cfg.pool_min_size,
        cfg.pool_max_size,
    );
}

/// Test pool timeout default is sensible.
#[test]
fn pool_timeout_default_is_positive() {
    let cfg = ServerConfig::default();
    assert!(cfg.pool_timeout_secs > 0, "pool_timeout_secs should be > 0");
    assert_eq!(cfg.pool_timeout_secs, 30, "expected default pool_timeout_secs = 30");
}

/// Test server config flags
#[test]
fn test_config_feature_flags() {
    let config = ServerConfig {
        cors_enabled: false,
        compression_enabled: false,
        tracing_enabled: false,
        apq_enabled: false,
        cache_enabled: false,
        ..ServerConfig::default()
    };

    assert!(!config.cors_enabled);
    assert!(!config.compression_enabled);
    assert!(!config.tracing_enabled);
    assert!(!config.apq_enabled);
    assert!(!config.cache_enabled);
}
