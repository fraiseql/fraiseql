//! Database Integration Tests for FraiseQL Server
//!
//! These tests verify:
//! - PostgreSQL adapter initialization with real database
//! - Connection pooling configuration and metrics
//! - Database connectivity and health checks
//! - Configuration loading and defaults
//! - Error handling for missing databases

use std::{path::PathBuf, sync::Arc};

use fraiseql_core::db::postgres::PostgresAdapter;
use fraiseql_server::{CompiledSchemaLoader, ServerConfig};

/// Test PostgreSQL adapter initialization with default configuration.
#[tokio::test]
async fn test_postgres_adapter_initialization() {
    // Get DATABASE_URL from environment or use test database
    let db_url =
        std::env::var("DATABASE_URL").unwrap_or_else(|_| "postgresql:///fraiseql_test".to_string());

    // Initialize adapter with default pool size
    let adapter = PostgresAdapter::new(&db_url).await;

    // Check that adapter was created successfully
    assert!(adapter.is_ok(), "Failed to initialize PostgresAdapter: {:?}", adapter.err());
}

/// Test PostgreSQL adapter with custom pool configuration.
#[tokio::test]
async fn test_postgres_adapter_with_pool_config() {
    let db_url =
        std::env::var("DATABASE_URL").unwrap_or_else(|_| "postgresql:///fraiseql_test".to_string());

    let min_size = 5;
    let max_size = 20;

    // Initialize adapter with explicit pool config
    let adapter = PostgresAdapter::with_pool_config(&db_url, min_size, max_size).await;

    assert!(
        adapter.is_ok(),
        "Failed to initialize PostgresAdapter with pool config: {:?}",
        adapter.err()
    );

    // Adapter should be cloneable for use in Server
    let adapter1 = adapter.unwrap();
    let _adapter2 = adapter1.clone();
}

/// Test server configuration defaults
#[test]
fn test_server_config_defaults() {
    let config = ServerConfig::default();

    assert_eq!(config.database_url, "postgresql://localhost/fraiseql");
    assert_eq!(config.pool_min_size, 5);
    assert_eq!(config.pool_max_size, 20);
    assert_eq!(config.pool_timeout_secs, 30);
    assert_eq!(config.graphql_path, "/graphql");
    assert_eq!(config.health_path, "/health");
    assert!(config.cors_enabled);
    assert!(config.compression_enabled);
}

/// Test server configuration with custom pool settings
#[test]
fn test_server_config_custom_pool_settings() {
    let config = ServerConfig {
        pool_min_size: 2,
        pool_max_size: 50,
        pool_timeout_secs: 60,
        ..ServerConfig::default()
    };

    assert_eq!(config.pool_min_size, 2);
    assert_eq!(config.pool_max_size, 50);
    assert_eq!(config.pool_timeout_secs, 60);
}

/// Test server configuration environment variable override
#[test]
fn test_server_config_database_url_override() {
    let original_url = "postgresql://localhost/fraiseql";
    let custom_url = "postgresql://user:pass@db.example.com/custom_db";

    let mut config = ServerConfig::default();
    assert_eq!(config.database_url, original_url);

    // Simulate environment variable override
    config.database_url = custom_url.to_string();

    assert_eq!(config.database_url, custom_url);
}

/// Test adapter cloning for Arc wrapper compatibility
#[tokio::test]
async fn test_postgres_adapter_arc_compatibility() {
    let db_url =
        std::env::var("DATABASE_URL").unwrap_or_else(|_| "postgresql:///fraiseql_test".to_string());

    let adapter = PostgresAdapter::new(&db_url).await.expect("Failed to create adapter");

    // Should be able to wrap in Arc for Server usage
    let adapter_arc: Arc<PostgresAdapter> = Arc::new(adapter);

    // Should be able to clone the Arc
    let adapter_arc2 = adapter_arc.clone();

    // Both should reference the same adapter
    assert_eq!(Arc::strong_count(&adapter_arc), 2);
    assert_eq!(Arc::strong_count(&adapter_arc2), 2);
}

/// Test configuration with disabled features
#[test]
fn test_server_config_disabled_features() {
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

/// Test schema loader path validation
#[test]
fn test_schema_loader_path() {
    let path = "/tmp/test_schema.json";
    let loader = CompiledSchemaLoader::new(path);

    assert_eq!(loader.path(), PathBuf::from(path).as_path());
}

/// Test that multiple configurations can coexist
#[test]
fn test_multiple_server_configs() {
    let config1 = ServerConfig {
        pool_min_size: 5,
        pool_max_size: 20,
        ..ServerConfig::default()
    };

    let config2 = ServerConfig {
        pool_min_size: 10,
        pool_max_size: 50,
        ..ServerConfig::default()
    };

    assert_eq!(config1.pool_min_size, 5);
    assert_eq!(config1.pool_max_size, 20);

    assert_eq!(config2.pool_min_size, 10);
    assert_eq!(config2.pool_max_size, 50);
}

/// Test custom bind address configuration
#[test]
fn test_custom_bind_address() {
    let config = ServerConfig {
        bind_addr: "0.0.0.0:8080".parse().unwrap(),
        ..ServerConfig::default()
    };

    assert_eq!(config.bind_addr.ip().to_string(), "0.0.0.0");
    assert_eq!(config.bind_addr.port(), 8080);
}

/// Test adapter initialization error handling with invalid URL
#[tokio::test]
async fn test_postgres_adapter_invalid_url() {
    let invalid_url = "invalid://malformed";

    let result = PostgresAdapter::new(invalid_url).await;

    // Should fail gracefully with connection error
    assert!(result.is_err(), "Should fail to connect with invalid URL");
}

/// Test adapter initialization with missing database
#[tokio::test]
async fn test_postgres_adapter_missing_database() {
    let missing_db_url = "postgresql://localhost/nonexistent_test_db_12345";

    let result = PostgresAdapter::new(missing_db_url).await;

    // Should fail gracefully when database doesn't exist
    assert!(result.is_err(), "Should fail when database doesn't exist");
}

/// Test CORS configuration variants
#[test]
fn test_cors_configuration_variants() {
    // CORS enabled with empty origins (allow all)
    let config_allow_all = ServerConfig {
        cors_enabled: true,
        cors_origins: vec![],
        ..ServerConfig::default()
    };

    assert!(config_allow_all.cors_enabled);
    assert!(config_allow_all.cors_origins.is_empty());

    // CORS enabled with specific origins
    let config_specific = ServerConfig {
        cors_enabled: true,
        cors_origins: vec![
            "https://example.com".to_string(),
            "https://app.example.com".to_string(),
        ],
        ..ServerConfig::default()
    };

    assert!(config_specific.cors_enabled);
    assert_eq!(config_specific.cors_origins.len(), 2);

    // CORS disabled
    let config_disabled = ServerConfig {
        cors_enabled: false,
        cors_origins: vec![],
        ..ServerConfig::default()
    };

    assert!(!config_disabled.cors_enabled);
}

/// Test that schema path can be customized
#[test]
fn test_custom_schema_path() {
    let custom_path = "/app/schemas/custom_schema.compiled.json";

    let config = ServerConfig {
        schema_path: PathBuf::from(custom_path),
        ..ServerConfig::default()
    };

    assert_eq!(config.schema_path, PathBuf::from(custom_path));
}

/// Test GraphQL endpoint path configuration
#[test]
fn test_custom_graphql_path() {
    let paths = vec!["/graphql", "/api/graphql", "/v1/graphql", "/gql"];

    for path in paths {
        let config = ServerConfig {
            graphql_path: path.to_string(),
            ..ServerConfig::default()
        };

        assert_eq!(config.graphql_path, path);
    }
}

/// Test health check endpoint path configuration
#[test]
fn test_custom_health_path() {
    let paths = vec!["/health", "/healthz", "/api/health", "/status"];

    for path in paths {
        let config = ServerConfig {
            health_path: path.to_string(),
            ..ServerConfig::default()
        };

        assert_eq!(config.health_path, path);
    }
}
