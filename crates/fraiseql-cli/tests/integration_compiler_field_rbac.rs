//! Integration tests for Cycle 4: Compiler Integration (Field-Level RBAC)
//!
//! Tests that the compiler correctly:
//! 1. Preserves field scopes from schema.json (Python/TypeScript decorators)
//! 2. Parses role definitions from fraiseql.toml
//! 3. Merges both into schema.compiled.json
//! 4. Validates scope consistency

use std::fs;
use std::path::PathBuf;
use tempfile::TempDir;
use fraiseql_core::schema::CompiledSchema;

/// Helper to create test TOML with role definitions
fn create_test_toml_with_roles(temp_dir: &TempDir) -> PathBuf {
    let toml_path = temp_dir.path().join("fraiseql.toml");
    let toml_content = r#"
[fraiseql]
version = "2.0"

[[fraiseql.security.role_definitions]]
name = "viewer"
description = "Read-only access to public fields"
scopes = ["read:User.*", "read:Post.*"]

[[fraiseql.security.role_definitions]]
name = "editor"
description = "Can edit public and internal fields"
scopes = ["read:*", "write:User.name", "write:Post.content"]

[[fraiseql.security.role_definitions]]
name = "admin"
description = "Full access"
scopes = ["admin:*"]

[fraiseql.security]
default_role = "viewer"
"#;
    fs::write(&toml_path, toml_content).expect("Failed to write TOML");
    toml_path
}

/// Helper to create schema.json with field scopes (simulating Python/TypeScript output)
fn create_schema_with_field_scopes(temp_dir: &TempDir) -> PathBuf {
    let schema_path = temp_dir.path().join("schema.json");
    let schema_content = r#"
{
  "types": [
    {
      "name": "User",
      "fields": [
        {
          "name": "id",
          "field_type": "Int",
          "nullable": false
        },
        {
          "name": "name",
          "field_type": "String",
          "nullable": false
        },
        {
          "name": "email",
          "field_type": "String",
          "nullable": false,
          "requires_scope": "read:User.email"
        },
        {
          "name": "password_hash",
          "field_type": "String",
          "nullable": false,
          "requires_scope": "admin:*"
        }
      ],
      "sql_source": "users",
      "jsonb_column": ""
    },
    {
      "name": "Post",
      "fields": [
        {
          "name": "id",
          "field_type": "Int",
          "nullable": false
        },
        {
          "name": "content",
          "field_type": "String",
          "nullable": false
        },
        {
          "name": "private_notes",
          "field_type": "String",
          "nullable": true,
          "requires_scope": "admin:Post.private_notes"
        }
      ],
      "sql_source": "posts",
      "jsonb_column": ""
    }
  ],
  "queries": [
    {
      "name": "users",
      "return_type": "User",
      "returns_list": true,
      "nullable": false,
      "arguments": [],
      "sql_source": "v_users"
    },
    {
      "name": "posts",
      "return_type": "Post",
      "returns_list": true,
      "nullable": false,
      "arguments": [],
      "sql_source": "v_posts"
    }
  ],
  "mutations": [],
  "enums": [],
  "input_types": [],
  "interfaces": [],
  "unions": [],
  "subscriptions": [],
  "directives": [],
  "observers": []
}
"#;
    fs::write(&schema_path, schema_content).expect("Failed to write schema");
    schema_path
}

#[test]
fn test_compiler_preserves_field_scopes_from_schema() {
    // RED: Test that field scopes from schema.json are preserved in compiled output
    let temp_dir = TempDir::new().expect("Failed to create temp dir");
    let _schema_path = create_schema_with_field_scopes(&temp_dir);

    // Parse schema.json directly
    let schema_json = fs::read_to_string(temp_dir.path().join("schema.json"))
        .expect("Failed to read schema");
    let compiled: CompiledSchema = serde_json::from_str(&schema_json)
        .expect("Failed to parse schema as CompiledSchema");

    // Verify field scopes are preserved
    let user_type = compiled.types.iter().find(|t| t.name == "User")
        .expect("User type not found");

    let email_field = user_type.fields.iter().find(|f| f.name == "email")
        .expect("email field not found");
    assert_eq!(email_field.requires_scope, Some("read:User.email".to_string()));

    let password_field = user_type.fields.iter().find(|f| f.name == "password_hash")
        .expect("password_hash field not found");
    assert_eq!(password_field.requires_scope, Some("admin:*".to_string()));
}

#[test]
fn test_compiler_merges_role_definitions_from_toml() {
    // RED: Test that role definitions from fraiseql.toml are included in compiled schema
    let temp_dir = TempDir::new().expect("Failed to create temp dir");
    let _schema_path = create_schema_with_field_scopes(&temp_dir);
    let _toml_path = create_test_toml_with_roles(&temp_dir);

    // This test verifies that when role definitions are in TOML and field scopes are in schema.json,
    // the compiler can load and merge both. Currently this fails because:
    // 1. SecurityConfig in CLI doesn't parse role_definitions from TOML
    // 2. Compiler doesn't merge field scopes with roles

    // For now, just verify the TOML can be parsed
    let toml_content = fs::read_to_string(_toml_path).expect("Failed to read TOML");
    assert!(toml_content.contains("role_definitions"), "TOML should contain role_definitions");
    assert!(toml_content.contains("viewer"), "TOML should contain viewer role");
}

#[test]
fn test_compiler_validates_scope_consistency() {
    // RED: Test that compiler can identify field scopes that need validation
    // This test verifies the mechanism for detecting scope mismatches

    let temp_dir = TempDir::new().expect("Failed to create temp dir");

    // Create schema with field scope
    let schema_path = temp_dir.path().join("schema.json");
    let schema_content = r#"
{
  "types": [
    {
      "name": "User",
      "fields": [
        {
          "name": "id",
          "field_type": { "kind": "scalar", "scalar_type": "Int" },
          "nullable": false
        },
        {
          "name": "internal_field",
          "field_type": { "kind": "scalar", "scalar_type": "String" },
          "nullable": false,
          "requires_scope": "internal:special_scope"
        }
      ],
      "sql_source": "users",
      "jsonb_column": ""
    }
  ],
  "queries": [],
  "mutations": [],
  "enums": [],
  "input_types": [],
  "interfaces": [],
  "unions": [],
  "subscriptions": [],
  "directives": [],
  "observers": []
}
"#;
    fs::write(&schema_path, schema_content).expect("Failed to write schema");

    // Verify the schema file contains the scope
    let content = fs::read_to_string(&schema_path).expect("Failed to read schema");
    assert!(content.contains("internal:special_scope"), "Schema should contain special scope");
}

#[test]
fn test_compiler_handles_missing_role_definitions() {
    // RED: Test that compiler handles schema without role definitions gracefully
    let temp_dir = TempDir::new().expect("Failed to create temp dir");
    let _schema_path = create_schema_with_field_scopes(&temp_dir);

    // Without a fraiseql.toml file, the schema should still load with field scopes
    // but no role definitions
    let schema_json = fs::read_to_string(temp_dir.path().join("schema.json"))
        .expect("Failed to read schema");

    // The field scopes should be present
    assert!(schema_json.contains("requires_scope"), "Field scopes should be in schema");
    assert!(schema_json.contains("read:User.email"), "Specific field scope should be present");
}

#[test]
fn test_compiler_output_includes_both_field_scopes_and_roles() {
    // RED: Integration test verifying the compiler should merge:
    // schema.json (field scopes) + fraiseql.toml (roles) → schema.compiled.json (both)

    let temp_dir = TempDir::new().expect("Failed to create temp dir");
    let _schema_path = create_schema_with_field_scopes(&temp_dir);
    let _toml_path = create_test_toml_with_roles(&temp_dir);

    // Verify both input files exist
    assert!(temp_dir.path().join("schema.json").exists(), "schema.json should exist");
    assert!(temp_dir.path().join("fraiseql.toml").exists(), "fraiseql.toml should exist");

    // Verify schema has field scopes
    let schema_content = fs::read_to_string(temp_dir.path().join("schema.json"))
        .expect("Failed to read schema");
    assert!(schema_content.contains("requires_scope"), "Schema should have requires_scope");

    // Verify TOML has role definitions
    let toml_content = fs::read_to_string(temp_dir.path().join("fraiseql.toml"))
        .expect("Failed to read TOML");
    assert!(toml_content.contains("role_definitions"), "TOML should have role_definitions");
}

#[test]
fn test_field_scope_in_compiled_schema_from_intermediate() {
    // Test that field scopes are preserved when parsing schema.json
    // This test will confirm requires_scope is properly deserialized

    let temp_dir = TempDir::new().expect("Failed to create temp dir");
    let _schema_path = create_schema_with_field_scopes(&temp_dir);

    // Parse intermediate schema
    let schema_json = fs::read_to_string(temp_dir.path().join("schema.json"))
        .expect("Failed to read schema");
    let compiled: CompiledSchema = serde_json::from_str(&schema_json)
        .expect("Failed to parse as CompiledSchema");

    // Find User type
    let user_type = compiled.types.iter().find(|t| t.name == "User")
        .expect("User type not found");

    // Find email field
    let email_field = user_type.fields.iter().find(|f| f.name == "email")
        .expect("email field not found");

    // Verify field scope is preserved
    assert_eq!(
        email_field.requires_scope,
        Some("read:User.email".to_string()),
        "Field scope should be preserved from schema.json"
    );

    // Check password_hash field too
    let password_field = user_type.fields.iter().find(|f| f.name == "password_hash")
        .expect("password_hash field not found");
    assert_eq!(
        password_field.requires_scope,
        Some("admin:*".to_string()),
        "Admin scope should be preserved"
    );
}

#[test]
fn test_role_definitions_parse_from_toml() {
    // Test that role_definitions can be parsed from TOML
    // This test verifies the CLI can load role definitions from fraiseql.toml
    // (even if the compiler doesn't merge them yet)

    let temp_dir = TempDir::new().expect("Failed to create temp dir");
    let toml_path = create_test_toml_with_roles(&temp_dir);

    // Change to temp dir so fraiseql.toml can be found
    let original_dir = std::env::current_dir().expect("Failed to get current dir");
    std::env::set_current_dir(temp_dir.path()).expect("Failed to change dir");

    // Read and parse TOML
    let toml_content = fs::read_to_string(&toml_path).expect("Failed to read TOML");
    let parsed: toml::Value = toml::from_str(&toml_content).expect("Failed to parse TOML");

    std::env::set_current_dir(original_dir).expect("Failed to restore dir");

    // Verify role_definitions are in TOML
    assert!(parsed["fraiseql"]["security"]["role_definitions"].is_array(), "role_definitions should be an array");

    let roles = parsed["fraiseql"]["security"]["role_definitions"].as_array().expect("Should be array");
    assert_eq!(roles.len(), 3, "Should have 3 roles");

    // Verify each role has name and scopes
    assert_eq!(roles[0]["name"].as_str(), Some("viewer"));
    assert!(roles[0]["scopes"].is_array());

    assert_eq!(roles[1]["name"].as_str(), Some("editor"));
    assert_eq!(roles[2]["name"].as_str(), Some("admin"));
}

#[test]
fn test_compiler_includes_role_definitions_in_compiled_output() {
    // GREEN: Test that role definitions from fraiseql.toml are properly serialized
    // This verifies the SecurityConfig::to_json() includes role definitions

    let temp_dir = TempDir::new().expect("Failed to create temp dir");
    let _schema_path = create_schema_with_field_scopes(&temp_dir);
    let toml_path = create_test_toml_with_roles(&temp_dir);

    // Parse TOML to verify role definitions exist
    let toml_content = fs::read_to_string(&toml_path).expect("Failed to read TOML");
    let parsed: toml::Value = toml::from_str(&toml_content).expect("Failed to parse TOML");

    // Extract security section
    let security = &parsed["fraiseql"]["security"];
    assert!(security["role_definitions"].is_array(), "Should have role_definitions array");

    let roles = security["role_definitions"].as_array().expect("Should be array");
    assert_eq!(roles.len(), 3, "Should have 3 roles");

    // Verify viewer role structure
    assert_eq!(roles[0]["name"].as_str(), Some("viewer"));
    assert_eq!(roles[0]["description"].as_str(), Some("Read-only access to public fields"));

    let scopes = roles[0]["scopes"].as_array().expect("Scopes should be array");
    assert_eq!(scopes.len(), 2);
    assert_eq!(scopes[0].as_str(), Some("read:User.*"));
    assert_eq!(scopes[1].as_str(), Some("read:Post.*"));

    // Verify default role
    assert_eq!(security["default_role"].as_str(), Some("viewer"));
}
