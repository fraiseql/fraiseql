#![allow(clippy::unwrap_used, clippy::print_stdout, clippy::print_stderr)] // Reason: test/bench code, panics are acceptable
//! Integration tests for domain discovery feature
//!
//! Tests domain-driven schema organization where schemas are split across
//! multiple domains in subdirectories with automatic discovery.

use std::{
    fs,
    path::{Path, PathBuf},
};

use tempfile::TempDir;

/// Path to a shipped example, resolved from the repository root.
///
/// `cargo test` runs with the *package* directory as the working directory, so the
/// bare relative paths this file used (`examples/multitenant/fraiseql.toml`) never
/// existed — every example test took its `if !path.exists() { return; }` branch and
/// silently passed. All three shipped examples were in fact failing to compile.
/// Resolving from `CARGO_MANIFEST_DIR` fixes the lookup; the assertion below turns a
/// missing example into a failure rather than a skip.
fn example_path(name: &str) -> PathBuf {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(Path::parent)
        .expect("crates/<pkg> is two levels below the repository root")
        .to_path_buf();
    let path = root.join("examples").join(name).join("fraiseql.toml");
    assert!(path.exists(), "shipped example is missing: {}", path.display());
    path
}

/// Helper to compile a schema and verify success
fn compile_schema(toml_path: &Path) -> Result<String, String> {
    let cli_path = env!("CARGO_BIN_EXE_fraiseql-cli");

    // Run fraiseql compile
    let output = std::process::Command::new(cli_path)
        .arg("compile")
        .arg(toml_path)
        .current_dir(toml_path.parent().unwrap())
        .output()
        .map_err(|e| format!("Failed to run fraiseql-cli: {}", e))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(format!("Compilation failed: {}", stderr));
    }

    Ok(String::from_utf8_lossy(&output.stdout).to_string())
}

// `examples/ecommerce` was removed in 0ef37210c (Python-ecosystem removal). The test
// that compiled it survived the deletion and skipped silently for every release
// since, because it looked the example up on a path that could not resolve. It is
// deleted rather than repointed: `examples/ecommerce_api` is a SQL-only example with
// no `fraiseql.toml` to compile.

/// Test `SaaS` example with 4 domains
#[test]
fn test_saas_example_compiles() {
    let saas_path = example_path("saas");

    let output = compile_schema(&saas_path).expect("Failed to compile saas example");
    assert!(output.contains("Schema compiled successfully"), "Expected success message");
    assert!(output.contains("Types: 8"), "Expected 8 types");
    assert!(output.contains("Queries: 11"), "Expected 11 queries");
    // Queries only: the example's eight mutations declared no input type and no
    // backing SQL function, so none could ever execute. See its README.
    assert!(output.contains("Mutations: 0"), "Expected 0 mutations");
}

/// Test multi-tenant example with 3 domains
#[test]
fn test_multitenant_example_compiles() {
    let multitenant_path = example_path("multitenant");

    let output = compile_schema(&multitenant_path).expect("Failed to compile multitenant example");
    assert!(output.contains("Schema compiled successfully"), "Expected success message");
    assert!(output.contains("Types: 3"), "Expected 3 types");
    assert!(output.contains("Queries: 3"), "Expected 3 queries");
    assert!(output.contains("Mutations: 0"), "Expected 0 mutations");
}

/// Test domain discovery with simple schema
#[test]
fn test_domain_discovery_simple() {
    let temp_dir = TempDir::new().expect("Failed to create temp dir");
    let temp_path = temp_dir.path();

    // Create domain structure
    fs::create_dir(temp_path.join("schema")).expect("Failed to create schema dir");
    fs::create_dir(temp_path.join("schema/users")).expect("Failed to create users domain");

    // Create types.json
    let types_json = r#"{
        "types": [
            {
                "name": "User",
                "fields": [
                    {"name": "id", "type": "ID", "nullable": false},
                    {"name": "name", "type": "String", "nullable": false}
                ],
                "description": "User type"
            }
        ],
        "queries": [
            {
                "name": "getUser",
                "return_type": "User",
                "returns_list": false,
                "description": "Get user by ID"
            }
        ],
        "mutations": []
    }"#;

    fs::write(temp_path.join("schema/users/types.json"), types_json)
        .expect("Failed to write types.json");

    // Create fraiseql.toml
    let toml_content = r#"[schema]
name = "test"
version = "1.0.0"
database_target = "postgresql"

[database]
url = "postgresql://localhost/test"
pool_max = 20
ssl_mode = "prefer"
connect_timeout_ms = 5000

[domain_discovery]
enabled = true
root_dir = "schema"
"#;

    fs::write(temp_path.join("fraiseql.toml"), toml_content).expect("Failed to write TOML");

    // Compile
    let output = compile_schema(&temp_path.join("fraiseql.toml"))
        .expect("Failed to compile simple domain schema");

    assert!(output.contains("Schema compiled successfully"), "Expected success");
    assert!(output.contains("Types: 1"), "Expected 1 type");
    assert!(output.contains("Queries: 1"), "Expected 1 query");
}

/// Test domain discovery with multiple domains
#[test]
fn test_domain_discovery_multiple_domains() {
    let temp_dir = TempDir::new().expect("Failed to create temp dir");
    let temp_path = temp_dir.path();

    // Create domain structure
    fs::create_dir(temp_path.join("schema")).expect("Failed to create schema dir");
    fs::create_dir(temp_path.join("schema/users")).expect("Failed to create users domain");
    fs::create_dir(temp_path.join("schema/posts")).expect("Failed to create posts domain");

    // Create users domain
    let users_json = r#"{
        "types": [
            {
                "name": "User",
                "fields": [
                    {"name": "id", "type": "ID", "nullable": false},
                    {"name": "name", "type": "String", "nullable": false}
                ],
                "description": "User type"
            }
        ],
        "queries": [
            {
                "name": "getUser",
                "return_type": "User",
                "returns_list": false
            }
        ],
        "mutations": []
    }"#;

    fs::write(temp_path.join("schema/users/types.json"), users_json)
        .expect("Failed to write users types.json");

    // Create posts domain
    let posts_json = r#"{
        "types": [
            {
                "name": "Post",
                "fields": [
                    {"name": "id", "type": "ID", "nullable": false},
                    {"name": "title", "type": "String", "nullable": false},
                    {"name": "userId", "type": "ID", "nullable": false}
                ],
                "description": "Blog post"
            }
        ],
        "queries": [
            {
                "name": "getPost",
                "return_type": "Post",
                "returns_list": false
            },
            {
                "name": "listPosts",
                "return_type": "Post",
                "returns_list": true
            }
        ],
        "mutations": []
    }"#;

    fs::write(temp_path.join("schema/posts/types.json"), posts_json)
        .expect("Failed to write posts types.json");

    // Create fraiseql.toml
    let toml_content = r#"[schema]
name = "test"
version = "1.0.0"
database_target = "postgresql"

[database]
url = "postgresql://localhost/test"
pool_max = 20
ssl_mode = "prefer"
connect_timeout_ms = 5000

[domain_discovery]
enabled = true
root_dir = "schema"
"#;

    fs::write(temp_path.join("fraiseql.toml"), toml_content).expect("Failed to write TOML");

    // Compile
    let output = compile_schema(&temp_path.join("fraiseql.toml"))
        .expect("Failed to compile multi-domain schema");

    assert!(output.contains("Schema compiled successfully"), "Expected success");
    assert!(output.contains("Types: 2"), "Expected 2 types (User, Post)");
    assert!(output.contains("Queries: 3"), "Expected 3 queries");
}

/// Test domain discovery with cross-domain type references
#[test]
fn test_cross_domain_references() {
    let temp_dir = TempDir::new().expect("Failed to create temp dir");
    let temp_path = temp_dir.path();

    // Create domain structure
    fs::create_dir(temp_path.join("schema")).expect("Failed to create schema dir");
    fs::create_dir(temp_path.join("schema/users")).expect("Failed to create users domain");
    fs::create_dir(temp_path.join("schema/posts")).expect("Failed to create posts domain");

    // Create users domain
    let users_json = r#"{
        "types": [
            {
                "name": "User",
                "fields": [
                    {"name": "id", "type": "ID", "nullable": false},
                    {"name": "email", "type": "String", "nullable": false}
                ]
            }
        ],
        "queries": [],
        "mutations": []
    }"#;

    fs::write(temp_path.join("schema/users/types.json"), users_json)
        .expect("Failed to write users types.json");

    // Create posts domain with reference to User
    let posts_json = r#"{
        "types": [
            {
                "name": "Post",
                "fields": [
                    {"name": "id", "type": "ID", "nullable": false},
                    {"name": "title", "type": "String", "nullable": false},
                    {"name": "authorId", "type": "ID", "nullable": false}
                ]
            }
        ],
        "queries": [
            {
                "name": "getUserPosts",
                "return_type": "Post",
                "returns_list": true,
                "description": "Get all posts by user (cross-domain reference to User)"
            }
        ],
        "mutations": []
    }"#;

    fs::write(temp_path.join("schema/posts/types.json"), posts_json)
        .expect("Failed to write posts types.json");

    // Create fraiseql.toml
    let toml_content = r#"[schema]
name = "test"
version = "1.0.0"
database_target = "postgresql"

[database]
url = "postgresql://localhost/test"
pool_max = 20
ssl_mode = "prefer"
connect_timeout_ms = 5000

[domain_discovery]
enabled = true
root_dir = "schema"
"#;

    fs::write(temp_path.join("fraiseql.toml"), toml_content).expect("Failed to write TOML");

    // Compile - should succeed because Post query refers to Post which exists
    let output = compile_schema(&temp_path.join("fraiseql.toml"))
        .expect("Failed to compile cross-domain schema");

    assert!(output.contains("Schema compiled successfully"), "Expected success");
    assert!(output.contains("Types: 2"), "Expected 2 types");
    assert!(output.contains("Queries: 1"), "Expected 1 query");
}

/// Test that disabled domain discovery falls back correctly
#[test]
fn test_domain_discovery_disabled() {
    let temp_dir = TempDir::new().expect("Failed to create temp dir");
    let temp_path = temp_dir.path();

    // Create domain structure (but won't be used)
    fs::create_dir(temp_path.join("schema")).expect("Failed to create schema dir");
    fs::create_dir(temp_path.join("schema/users")).expect("Failed to create users domain");

    let types_json = r#"{
        "types": [
            {
                "name": "User",
                "fields": [
                    {"name": "id", "type": "ID", "nullable": false}
                ]
            }
        ],
        "queries": [],
        "mutations": []
    }"#;

    fs::write(temp_path.join("schema/users/types.json"), types_json)
        .expect("Failed to write types.json");

    // Create fraiseql.toml with disabled domain discovery
    let toml_content = r#"[schema]
name = "test"
version = "1.0.0"
database_target = "postgresql"

[database]
url = "postgresql://localhost/test"
pool_max = 20
ssl_mode = "prefer"
connect_timeout_ms = 5000

[domain_discovery]
enabled = false
root_dir = "schema"
"#;

    fs::write(temp_path.join("fraiseql.toml"), toml_content).expect("Failed to write TOML");

    // Compile - should succeed but with empty schema (no domains discovered)
    let output = compile_schema(&temp_path.join("fraiseql.toml"))
        .expect("Failed to compile with disabled discovery");

    assert!(output.contains("Schema compiled successfully"), "Expected success");
    assert!(output.contains("Types: 0"), "Expected 0 types (discovery disabled)");
}
