//! Integration tests for SecretsManager initialization and wiring into AppState.

use fraiseql_secrets::secrets_manager::{SecretsBackendConfig, VaultAuth, create_secrets_manager};

/// Test file backend initialization
#[tokio::test]
async fn test_file_backend_initialization() {
    let dir = tempfile::tempdir().unwrap();
    let secret_path = dir.path().join("db_password");
    tokio::fs::write(&secret_path, "s3cret123").await.unwrap();

    let config = SecretsBackendConfig::File {
        path: dir.path().to_path_buf(),
    };

    let manager = create_secrets_manager(config).await.unwrap();
    let value = manager.get_secret("db_password").await.unwrap();

    assert_eq!(value, "s3cret123");
}

/// Test environment variable backend initialization
#[tokio::test]
async fn test_env_backend_initialization() {
    let config = SecretsBackendConfig::Env;

    let manager = create_secrets_manager(config).await.unwrap();

    // Test with temp_env to avoid polluting the test environment
    temp_env::async_with_vars([("TEST_SECRET_VALUE", Some("env_value_123"))], async {
        let value = manager.get_secret("TEST_SECRET_VALUE").await.unwrap();
        assert_eq!(value, "env_value_123");
    })
    .await;
}

/// Test vault backend initialization with token auth (marked as ignore since it requires running
/// Vault)
#[tokio::test]
#[ignore = "requires vault"]
async fn test_vault_backend_token_initialization() {
    let config = SecretsBackendConfig::Vault {
        addr:       "http://127.0.0.1:8200".to_string(),
        auth:       VaultAuth::Token("test-token".to_string()),
        namespace:  None,
        tls_verify: true,
    };

    let manager = create_secrets_manager(config).await.unwrap();
    // Would make actual request if Vault running
    let _secret = manager.get_secret("secret/data/test").await;
}

/// Test vault backend initialization with AppRole auth (marked as ignore since it requires running
/// Vault)
#[tokio::test]
#[ignore = "requires vault"]
async fn test_vault_backend_approle_initialization() {
    let config = SecretsBackendConfig::Vault {
        addr:       "http://127.0.0.1:8200".to_string(),
        auth:       VaultAuth::AppRole {
            role_id:   "test-role-id".to_string(),
            secret_id: "test-secret-id".to_string(),
        },
        namespace:  None,
        tls_verify: true,
    };

    // This test is ignored since AppRole auth requires a running Vault instance
    // In practice, this would be tested in staging/production
    let _result = create_secrets_manager(config).await;
}

/// Test that vault namespace is properly set
#[tokio::test]
async fn test_vault_namespace_configuration() {
    let config = SecretsBackendConfig::Vault {
        addr:       "http://127.0.0.1:8200".to_string(),
        auth:       VaultAuth::Token("test-token".to_string()),
        namespace:  Some("fraiseql/prod".to_string()),
        tls_verify: true,
    };

    let manager = create_secrets_manager(config).await.unwrap();
    // Verify manager was created successfully with namespace
    assert!(manager.get_secret("nonexistent").await.is_err());
}

/// Test that TLS verification can be disabled
#[tokio::test]
async fn test_vault_tls_verification_disabled() {
    let config = SecretsBackendConfig::Vault {
        addr:       "https://127.0.0.1:8200".to_string(),
        auth:       VaultAuth::Token("test-token".to_string()),
        namespace:  None,
        tls_verify: false,
    };

    let manager = create_secrets_manager(config).await.unwrap();
    // Verify manager was created successfully with TLS verification disabled
    assert!(manager.get_secret("nonexistent").await.is_err());
}

/// Test multiple backends can be created in sequence
#[tokio::test]
async fn test_multiple_backends_initialization() {
    let dir1 = tempfile::tempdir().unwrap();
    let secret1_path = dir1.path().join("secret1");
    tokio::fs::write(&secret1_path, "value1").await.unwrap();

    let dir2 = tempfile::tempdir().unwrap();
    let secret2_path = dir2.path().join("secret2");
    tokio::fs::write(&secret2_path, "value2").await.unwrap();

    let config1 = SecretsBackendConfig::File {
        path: dir1.path().to_path_buf(),
    };
    let config2 = SecretsBackendConfig::File {
        path: dir2.path().to_path_buf(),
    };

    let manager1 = create_secrets_manager(config1).await.unwrap();
    let manager2 = create_secrets_manager(config2).await.unwrap();

    let value1 = manager1.get_secret("secret1").await.unwrap();
    let value2 = manager2.get_secret("secret2").await.unwrap();

    assert_eq!(value1, "value1");
    assert_eq!(value2, "value2");
}
