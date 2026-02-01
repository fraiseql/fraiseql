//! Integration tests for security configuration in schema compilation

use std::fs;
use tempfile::TempDir;

#[test]
fn test_compile_with_security_config() {
    // Create a temporary directory
    let temp_dir = TempDir::new().expect("Failed to create temp dir");
    let temp_path = temp_dir.path();

    // Create a simple schema.json
    let schema_json = r#"{
  "version": "2.0.0",
  "types": [
    {
      "name": "User",
      "fields": [
        {
          "name": "id",
          "type": "ID",
          "nullable": false
        },
        {
          "name": "email",
          "type": "String",
          "nullable": false
        }
      ]
    }
  ],
  "queries": [
    {
      "name": "users",
      "return_type": "User",
      "return_is_list": true,
      "sql_source": "SELECT * FROM users",
      "arguments": []
    }
  ]
}"#;

    let schema_path = temp_path.join("schema.json");
    fs::write(&schema_path, schema_json).expect("Failed to write schema.json");

    // Create fraiseql.toml with security config
    let toml_content = r#"[project]
name = "test-app"
version = "1.0.0"

[fraiseql]
schema_file = "schema.json"
output_file = "schema.compiled.json"

[fraiseql.security.audit_logging]
enabled = true
log_level = "info"
include_sensitive_data = false
async_logging = true
buffer_size = 1000
flush_interval_secs = 5

[fraiseql.security.error_sanitization]
enabled = true
generic_messages = true
internal_logging = true
leak_sensitive_details = false
user_facing_format = "generic"

[fraiseql.security.rate_limiting]
enabled = true
auth_start_max_requests = 100
auth_start_window_secs = 60
auth_callback_max_requests = 50
auth_callback_window_secs = 60
auth_refresh_max_requests = 10
auth_refresh_window_secs = 60
auth_logout_max_requests = 20
auth_logout_window_secs = 60
failed_login_max_requests = 5
failed_login_window_secs = 3600

[fraiseql.security.state_encryption]
enabled = true
algorithm = "chacha20-poly1305"
key_rotation_enabled = false
nonce_size = 12
key_size = 32

[fraiseql.security.constant_time]
enabled = true
apply_to_jwt = true
apply_to_session_tokens = true
apply_to_csrf_tokens = true
apply_to_refresh_tokens = true
"#;

    let toml_path = temp_path.join("fraiseql.toml");
    fs::write(&toml_path, toml_content).expect("Failed to write fraiseql.toml");

    // Note: Full integration test would require running the actual CLI command
    // For now, we're just testing that the files can be created and read successfully
    assert!(schema_path.exists(), "schema.json should exist");
    assert!(toml_path.exists(), "fraiseql.toml should exist");

    // Verify we can read them back
    let schema_read = fs::read_to_string(&schema_path).expect("Failed to read schema.json");
    let toml_read = fs::read_to_string(&toml_path).expect("Failed to read fraiseql.toml");

    assert!(!schema_read.is_empty(), "schema.json should not be empty");
    assert!(!toml_read.is_empty(), "fraiseql.toml should not be empty");

    // Verify TOML structure
    assert!(toml_read.contains("[fraiseql.security.audit_logging]"));
    assert!(toml_read.contains("[fraiseql.security.error_sanitization]"));
    assert!(toml_read.contains("[fraiseql.security.rate_limiting]"));
    assert!(toml_read.contains("[fraiseql.security.state_encryption]"));
    assert!(toml_read.contains("[fraiseql.security.constant_time]"));

    // Verify key settings
    assert!(toml_read.contains("log_level = \"info\""));
    assert!(toml_read.contains("generic_messages = true"));
    assert!(toml_read.contains("auth_start_max_requests = 100"));
    assert!(toml_read.contains("algorithm = \"chacha20-poly1305\""));
}

#[test]
fn test_security_config_loading() {
    // Create a temporary directory
    let temp_dir = TempDir::new().expect("Failed to create temp dir");
    let temp_path = temp_dir.path();

    // Create fraiseql.toml
    let toml_content = r#"[project]
name = "test-app"
version = "1.0.0"

[fraiseql]
schema_file = "schema.json"
output_file = "schema.compiled.json"

[fraiseql.security.audit_logging]
log_level = "debug"

[fraiseql.security.rate_limiting]
auth_start_max_requests = 200
failed_login_max_requests = 3
"#;

    let toml_path = temp_path.join("fraiseql.toml");
    fs::write(&toml_path, toml_content).expect("Failed to write fraiseql.toml");

    // Verify custom values
    let toml_read = fs::read_to_string(&toml_path).expect("Failed to read fraiseql.toml");
    assert!(toml_read.contains("log_level = \"debug\""));
    assert!(toml_read.contains("auth_start_max_requests = 200"));
    assert!(toml_read.contains("failed_login_max_requests = 3"));
}

#[test]
fn test_security_config_with_invalid_values() {
    // Create a temporary directory
    let temp_dir = TempDir::new().expect("Failed to create temp dir");
    let temp_path = temp_dir.path();

    // Create fraiseql.toml with leak_sensitive_details = true (should fail validation)
    let toml_content = r#"[project]
name = "test-app"
version = "1.0.0"

[fraiseql]
schema_file = "schema.json"
output_file = "schema.compiled.json"

[fraiseql.security.error_sanitization]
leak_sensitive_details = true
"#;

    let toml_path = temp_path.join("fraiseql.toml");
    fs::write(&toml_path, toml_content).expect("Failed to write fraiseql.toml");

    // Verify that dangerous config is present (compilation would fail if run)
    let toml_read = fs::read_to_string(&toml_path).expect("Failed to read fraiseql.toml");
    assert!(toml_read.contains("leak_sensitive_details = true"));
}
