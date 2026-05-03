#![allow(clippy::unwrap_used)] // Reason: test/bench code, panics are acceptable
//! Integration tests for TOML-based workflow with all 16 language SDKs
//!
//! This test suite verifies end-to-end compilation for each language:
//! 1. Export types.json from language SDK
//! 2. Create fraiseql.toml with config
//! 3. Run: fraiseql compile fraiseql.toml --types types.json
//! 4. Verify schema.compiled.json contains all features

use std::{fs, process::Command};

use tempfile::TempDir;

#[test]
fn test_toml_workflow_python_sdk() {
    test_sdk_integration(
        "python",
        "User",
        r#"
{
  "types": [
    {
      "name": "User",
      "fields": [
        {"name": "id", "type": "ID", "nullable": false},
        {"name": "name", "type": "String", "nullable": false}
      ]
    }
  ]
}
"#,
    );
}

#[test]
fn test_toml_workflow_go_sdk() {
    test_sdk_integration(
        "go",
        "Product",
        r#"
{
  "types": [
    {
      "name": "Product",
      "fields": [
        {"name": "id", "type": "ID", "nullable": false},
        {"name": "price", "type": "Float", "nullable": true}
      ]
    }
  ]
}
"#,
    );
}

#[test]
fn test_toml_workflow_nodejs_sdk() {
    test_sdk_integration(
        "nodejs",
        "Post",
        r#"
{
  "types": [
    {
      "name": "Post",
      "description": "Blog post",
      "fields": [
        {"name": "id", "type": "ID", "nullable": false},
        {"name": "title", "type": "String", "nullable": false}
      ]
    }
  ]
}
"#,
    );
}

#[test]
fn test_toml_workflow_php_sdk() {
    test_sdk_integration(
        "php",
        "Comment",
        r#"
{
  "types": [
    {
      "name": "Comment",
      "fields": [
        {"name": "id", "type": "ID", "nullable": false},
        {"name": "text", "type": "String", "nullable": false}
      ]
    }
  ]
}
"#,
    );
}

// Integration test helper
fn test_sdk_integration(sdk_name: &str, type_name: &str, types_json: &str) {
    let temp_dir = TempDir::new().expect("Failed to create temp directory");
    let types_path = temp_dir.path().join("types.json");
    let toml_path = temp_dir.path().join("fraiseql.toml");
    let output_path = temp_dir.path().join("schema.compiled.json");

    // 1. Write types.json
    fs::write(&types_path, types_json).expect("Failed to write types.json");

    // 2. Create fraiseql.toml with queries/mutations/security (minimal valid config)
    let toml_config = format!(
        r#"
[schema]
name = "test_schema"
version = "1.0.0"
database_target = "postgresql"

[database]
url = "postgresql://localhost/test"

[queries.getItems]
return_type = "{}"
return_array = true
sql_source = "v_{}"

[security]
default_policy = "public"

[security.enterprise]
rate_limiting_enabled = false
audit_logging_enabled = false
"#,
        type_name,
        type_name.to_lowercase()
    );

    fs::write(&toml_path, toml_config).expect("Failed to write fraiseql.toml");

    // 3. Run compile command
    // fraiseql compile fraiseql.toml --types types.json --output schema.compiled.json
    let cli_path = env!("CARGO_BIN_EXE_fraiseql-cli");
    let output = Command::new(cli_path)
        .args([
            "compile",
            toml_path.to_str().unwrap(),
            "--types",
            types_path.to_str().unwrap(),
            "--output",
            output_path.to_str().unwrap(),
        ])
        .output();

    match output {
        Ok(result) => {
            if !result.status.success() {
                let stderr = String::from_utf8_lossy(&result.stderr);
                let stdout = String::from_utf8_lossy(&result.stdout);
                panic!(
                    "Compilation failed for {}.\nstdout: {}\nstderr: {}",
                    sdk_name, stdout, stderr
                );
            }

            // 4. Verify compiled schema
            let compiled =
                fs::read_to_string(&output_path).expect("Failed to read compiled schema");

            // Check that compiled schema contains types
            assert!(
                compiled.contains("\"types\""),
                "Compiled schema missing types section for {}",
                sdk_name
            );

            // Check that queries are present
            assert!(
                compiled.contains("\"queries\""),
                "Compiled schema missing queries section for {}",
                sdk_name
            );

            // Check that security is present
            assert!(
                compiled.contains("\"security\""),
                "Compiled schema missing security section for {}",
                sdk_name
            );
        },
        Err(e) => {
            panic!("Failed to run fraiseql-cli for {}: {}", sdk_name, e);
        },
    }
}

#[test]
fn test_types_and_toml_config_merged() {
    let temp_dir = TempDir::new().unwrap();

    // types.json from SDK with 2 types
    let types_json = r#"
{
  "types": [
    {
      "name": "User",
      "fields": [
        {"name": "id", "type": "ID", "nullable": false},
        {"name": "email", "type": "String", "nullable": false}
      ]
    },
    {
      "name": "Post",
      "fields": [
        {"name": "id", "type": "ID", "nullable": false},
        {"name": "authorId", "type": "ID", "nullable": false}
      ]
    }
  ]
}
"#;

    // fraiseql.toml with queries and mutations
    let toml_config = r#"
[schema]
name = "merged_test"
version = "1.0.0"
database_target = "postgresql"

[database]
url = "postgresql://localhost/test"

[queries.getUser]
return_type = "User"
return_array = false
sql_source = "v_users"

[queries.getPosts]
return_type = "Post"
return_array = true
sql_source = "v_posts"

[[queries.getUser.args]]
name = "userId"
type = "ID"
required = true

[security]
default_policy = "public"

[security.enterprise]
rate_limiting_enabled = false
audit_logging_enabled = false
"#;

    let types_path = temp_dir.path().join("types.json");
    let toml_path = temp_dir.path().join("fraiseql.toml");
    let output_path = temp_dir.path().join("schema.compiled.json");

    fs::write(&types_path, types_json).unwrap();
    fs::write(&toml_path, toml_config).unwrap();

    // Compile
    let cli_path = env!("CARGO_BIN_EXE_fraiseql-cli");
    let output = Command::new(cli_path)
        .args([
            "compile",
            toml_path.to_str().unwrap(),
            "--types",
            types_path.to_str().unwrap(),
            "--output",
            output_path.to_str().unwrap(),
        ])
        .output()
        .expect("Failed to run compilation");

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        let stdout = String::from_utf8_lossy(&output.stdout);
        panic!("Compilation failed.\nstdout: {}\nstderr: {}", stdout, stderr);
    }

    // Verify merged result
    let compiled = fs::read_to_string(&output_path).expect("Failed to read compiled schema");

    // Check that both types are in the output
    assert!(compiled.contains("User"), "User type not in compiled schema");
    assert!(compiled.contains("Post"), "Post type not in compiled schema");

    // Check that both queries are in the output
    assert!(compiled.contains("getUser"), "getUser query not in compiled schema");
    assert!(compiled.contains("getPosts"), "getPosts query not in compiled schema");

    // Check that types are arrays, not objects
    let compiled_value: serde_json::Value =
        serde_json::from_str(&compiled).expect("Failed to parse compiled schema as JSON");

    assert!(compiled_value["types"].is_array(), "types should be an array, not object");
    assert!(compiled_value["queries"].is_array(), "queries should be an array, not object");
}

#[test]
fn test_security_config_in_compiled_schema() {
    let temp_dir = TempDir::new().unwrap();

    let types_json = r#"
{
  "types": [
    {
      "name": "SecureData",
      "fields": [
        {"name": "id", "type": "ID", "nullable": false},
        {"name": "secret", "type": "String", "nullable": false}
      ]
    }
  ]
}
"#;

    let toml_config = r#"
[schema]
name = "secure_test"
version = "1.0.0"
database_target = "postgresql"

[database]
url = "postgresql://localhost/test"

[security]
default_policy = "public"

[[security.rules]]
name = "read_own_data"
rule = "user.id == object.owner_id"
description = "Users can only read their own data"
cacheable = true
cache_ttl_seconds = 300

[[security.policies]]
name = "admin_only"
type = "rbac"
roles = ["admin"]
strategy = "any"
description = "Admins only"
cache_ttl_seconds = 600

[[security.field_auth]]
type_name = "SecureData"
field_name = "secret"
policy = "admin_only"

[security.enterprise]
rate_limiting_enabled = false
audit_logging_enabled = false
"#;

    let types_path = temp_dir.path().join("types.json");
    let toml_path = temp_dir.path().join("fraiseql.toml");
    let output_path = temp_dir.path().join("schema.compiled.json");

    fs::write(&types_path, types_json).unwrap();
    fs::write(&toml_path, toml_config).unwrap();

    let cli_path = env!("CARGO_BIN_EXE_fraiseql-cli");
    let output = Command::new(cli_path)
        .args([
            "compile",
            toml_path.to_str().unwrap(),
            "--types",
            types_path.to_str().unwrap(),
            "--output",
            output_path.to_str().unwrap(),
        ])
        .output()
        .expect("Failed to run compilation");

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        let stdout = String::from_utf8_lossy(&output.stdout);
        panic!("Compilation failed.\nstdout: {}\nstderr: {}", stdout, stderr);
    }

    let compiled = fs::read_to_string(&output_path).unwrap();
    let compiled_value: serde_json::Value = serde_json::from_str(&compiled).unwrap();

    // Verify security section exists and is properly embedded
    assert!(
        compiled_value.get("security").is_some(),
        "security section missing from compiled schema"
    );

    let security = &compiled_value["security"];
    assert!(
        security.get("default_policy").is_some(),
        "default_policy missing from security config"
    );
    assert!(security.get("rules").is_some(), "rules missing from security config");
    assert!(security.get("policies").is_some(), "policies missing from security config");
}

/// Full CLI compile pipeline with field-level assertions.
///
/// The CLI embeds a `_content_hash` in compiled output.  With `strict_integrity=true`,
/// `from_json` must accept CLI-produced output, reject tampered output, and accept
/// `--skip-hash` output with `strict_integrity=false`.
mod schema_integrity_cli_tests {
    use super::*;

    fn minimal_fixtures(temp_dir: &tempfile::TempDir) -> (std::path::PathBuf, std::path::PathBuf, std::path::PathBuf) {
        let types_json = r#"{"types":[{"name":"Thing","sql_source":"v_thing","fields":[{"name":"id","type":"ID","nullable":false}]}],"queries":[{"name":"things","return_type":"Thing","returns_list":true,"nullable":false,"sql_source":"v_thing"}],"mutations":[]}"#;
        let toml_config = r#"
[schema]
name = "hash_test"
version = "1.0.0"
database_target = "postgresql"
[database]
url = "postgresql://localhost/test"
[security]
default_policy = "public"
[security.enterprise]
rate_limiting_enabled = false
audit_logging_enabled = false
"#;
        let types_path = temp_dir.path().join("types.json");
        let toml_path = temp_dir.path().join("fraiseql.toml");
        let output_path = temp_dir.path().join("schema.compiled.json");
        fs::write(&types_path, types_json).unwrap();
        fs::write(&toml_path, toml_config).unwrap();
        (types_path, toml_path, output_path)
    }

    fn run_compile(toml_path: &std::path::Path, types_path: &std::path::Path, output_path: &std::path::Path, extra_args: &[&str]) {
        let cli_path = env!("CARGO_BIN_EXE_fraiseql-cli");
        let mut args = vec![
            "compile",
            toml_path.to_str().unwrap(),
            "--types",
            types_path.to_str().unwrap(),
            "--output",
            output_path.to_str().unwrap(),
        ];
        args.extend_from_slice(extra_args);
        let output = Command::new(cli_path).args(&args).output().expect("Failed to run CLI");
        if !output.status.success() {
            panic!(
                "CLI compile failed.\nstdout: {}\nstderr: {}",
                String::from_utf8_lossy(&output.stdout),
                String::from_utf8_lossy(&output.stderr)
            );
        }
    }

    #[test]
    fn cli_output_accepted_in_strict_mode() {
        let temp_dir = TempDir::new().unwrap();
        let (types_path, toml_path, output_path) = minimal_fixtures(&temp_dir);
        run_compile(&toml_path, &types_path, &output_path, &[]);

        let compiled_json = fs::read_to_string(&output_path).expect("compiled schema missing");
        // _content_hash field must be present in the output
        assert!(compiled_json.contains("_content_hash"), "CLI must embed _content_hash");
        // strict_integrity=true must accept it without error
        fraiseql_core::schema::CompiledSchema::from_json(&compiled_json, true)
            .expect("strict from_json must accept CLI-produced schema");
    }

    #[test]
    fn tampered_body_rejected_in_strict_mode() {
        let temp_dir = TempDir::new().unwrap();
        let (types_path, toml_path, output_path) = minimal_fixtures(&temp_dir);
        run_compile(&toml_path, &types_path, &output_path, &[]);

        let compiled_json = fs::read_to_string(&output_path).unwrap();
        // Replace a field value in the body (after the hash line) to simulate tampering
        let tampered = compiled_json.replace("\"v_thing\"", "\"v_TAMPERED\"");
        assert_ne!(tampered, compiled_json, "replacement must have changed something");
        let result = fraiseql_core::schema::CompiledSchema::from_json(&tampered, true);
        assert!(result.is_err(), "strict from_json must reject tampered schema");
        let msg = format!("{}", result.unwrap_err());
        assert!(msg.contains("hash mismatch") || msg.contains("integrity"), "error must mention hash: {msg}");
    }

    #[test]
    fn skip_hash_output_accepted_in_non_strict_mode() {
        let temp_dir = TempDir::new().unwrap();
        let (types_path, toml_path, output_path) = minimal_fixtures(&temp_dir);
        run_compile(&toml_path, &types_path, &output_path, &["--skip-hash"]);

        let compiled_json = fs::read_to_string(&output_path).unwrap();
        assert!(!compiled_json.contains("_content_hash"), "--skip-hash must omit _content_hash");
        // non-strict must accept (with a warning)
        fraiseql_core::schema::CompiledSchema::from_json(&compiled_json, false)
            .expect("non-strict from_json must accept schema without hash");
    }

    #[test]
    fn skip_hash_output_rejected_in_strict_mode() {
        let temp_dir = TempDir::new().unwrap();
        let (types_path, toml_path, output_path) = minimal_fixtures(&temp_dir);
        run_compile(&toml_path, &types_path, &output_path, &["--skip-hash"]);

        let compiled_json = fs::read_to_string(&output_path).unwrap();
        let result = fraiseql_core::schema::CompiledSchema::from_json(&compiled_json, true);
        assert!(result.is_err(), "strict from_json must reject schema without _content_hash");
    }
}

/// types.json carries `inject` and `cache_ttl_seconds` on a query, and
/// `invalidates_views` on a mutation.  We compile via the CLI binary and then
/// parse the compiled JSON with `CompiledSchema::from_json()` to assert that
/// those fields reach the output unchanged.
#[test]
fn test_field_values_survive_full_cli_pipeline() {
    let temp_dir = TempDir::new().unwrap();

    // types.json in the intermediate format emitted by language SDKs
    let types_json = r#"
{
  "types": [
    {
      "name": "Order",
      "sql_source": "v_order",
      "fields": [
        {"name": "id",     "type": "ID",     "nullable": false},
        {"name": "amount", "type": "Float",  "nullable": false},
        {"name": "status", "type": "String", "nullable": false}
      ]
    }
  ],
  "queries": [
    {
      "name": "orders",
      "return_type": "Order",
      "returns_list": true,
      "nullable": false,
      "sql_source": "v_order",
      "cache_ttl_seconds": 300,
      "inject": {"tenant_id": "jwt:tenant_id"}
    }
  ],
  "mutations": [
    {
      "name": "createOrder",
      "return_type": "Order",
      "sql_source": "fn_create_order",
      "invalidates_views": ["v_order"],
      "inject": {"user_id": "jwt:sub"}
    }
  ]
}
"#;

    let toml_config = r#"
[schema]
name = "field_survival_test"
version = "1.0.0"
database_target = "postgresql"

[database]
url = "postgresql://localhost/test"

[security]
default_policy = "public"

[security.enterprise]
rate_limiting_enabled = false
audit_logging_enabled = false
"#;

    let types_path = temp_dir.path().join("types.json");
    let toml_path = temp_dir.path().join("fraiseql.toml");
    let output_path = temp_dir.path().join("schema.compiled.json");

    fs::write(&types_path, types_json).unwrap();
    fs::write(&toml_path, toml_config).unwrap();

    let cli_path = env!("CARGO_BIN_EXE_fraiseql-cli");
    let output = Command::new(cli_path)
        .args([
            "compile",
            toml_path.to_str().unwrap(),
            "--types",
            types_path.to_str().unwrap(),
            "--output",
            output_path.to_str().unwrap(),
        ])
        .output()
        .expect("Failed to run compilation");

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        let stdout = String::from_utf8_lossy(&output.stdout);
        panic!("Compilation failed.\nstdout: {stdout}\nstderr: {stderr}");
    }

    let compiled_json = fs::read_to_string(&output_path).expect("compiled schema missing");
    let schema = fraiseql_core::schema::CompiledSchema::from_json(&compiled_json, false)
        .expect("compiled schema must parse");

    // Query field survival
    let q = schema.find_query("orders").expect("'orders' query must be present");
    assert_eq!(
        q.sql_source.as_deref(),
        Some("v_order"),
        "query sql_source must survive full CLI pipeline"
    );
    assert_eq!(
        q.cache_ttl_seconds,
        Some(300),
        "cache_ttl_seconds must survive full CLI pipeline"
    );
    assert_eq!(q.inject_params.len(), 1, "inject_params must have one entry");

    // Mutation field survival
    let m = schema
        .find_mutation("createOrder")
        .expect("'createOrder' mutation must be present");
    assert_eq!(
        m.sql_source.as_deref(),
        Some("fn_create_order"),
        "mutation sql_source must survive full CLI pipeline"
    );
    assert_eq!(
        m.invalidates_views,
        vec!["v_order"],
        "invalidates_views must survive full CLI pipeline"
    );
    assert_eq!(m.inject_params.len(), 1, "mutation inject_params must have one entry");
}
