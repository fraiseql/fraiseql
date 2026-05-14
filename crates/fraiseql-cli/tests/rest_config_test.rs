#![allow(clippy::unwrap_used)] // Reason: test code, panics are acceptable
//! Integration tests: [rest] TOML section through the full compiler pipeline.
//!
//! Tests the three pipeline stages:
//! 1. TOML parsing to `RestTomlConfig`
//! 2. Merger to `IntermediateSchema.rest_config`
//! 3. Converter to `CompiledSchema.rest_config`

use std::io::Write;

use fraiseql_cli::schema::converter::SchemaConverter;
use fraiseql_cli::schema::merger::SchemaMerger;
use tempfile::NamedTempFile;

/// Helper: write TOML content to a temp file and return the path.
fn toml_file(content: &str) -> NamedTempFile {
    let mut f = NamedTempFile::new().unwrap();
    f.write_all(content.as_bytes()).unwrap();
    f.flush().unwrap();
    f
}

// ---- Merger stage ----

#[test]
fn test_rest_config_flows_through_merger() {
    let f = toml_file(
        r#"
        [schema]
        name = "test"

        [rest]
        enabled = true
        path = "/api/v1"
        max_page_size = 500
        require_auth = true
    "#,
    );
    let intermediate =
        SchemaMerger::merge_toml_only(f.path().to_str().unwrap()).unwrap();

    let rest = intermediate
        .rest_config
        .expect("rest_config should be Some");
    assert!(rest.enabled);
    assert_eq!(rest.path, "/api/v1");
    assert_eq!(rest.max_page_size, 500);
    assert!(rest.require_auth);
    // Verify defaults flow through
    assert_eq!(rest.sse_heartbeat_seconds, 30);
    assert!(rest.etag);
}

#[test]
fn test_rest_config_absent_when_disabled() {
    let f = toml_file(
        r#"
        [schema]
        name = "test"
    "#,
    );
    let intermediate =
        SchemaMerger::merge_toml_only(f.path().to_str().unwrap()).unwrap();

    assert!(intermediate.rest_config.is_none());
}

#[test]
fn test_rest_config_absent_when_explicitly_disabled() {
    let f = toml_file(
        r#"
        [schema]
        name = "test"

        [rest]
        enabled = false
        path = "/api/v1"
        max_page_size = 500
    "#,
    );
    let intermediate =
        SchemaMerger::merge_toml_only(f.path().to_str().unwrap()).unwrap();

    // Explicit [rest] section with enabled=false should NOT embed rest_config
    assert!(intermediate.rest_config.is_none());
}

#[test]
fn test_rest_config_rejects_invalid_path() {
    let f = toml_file(
        r#"
        [schema]
        name = "test"

        [rest]
        enabled = true
        path = "no-leading-slash"
    "#,
    );
    let result = SchemaMerger::merge_toml_only(f.path().to_str().unwrap());
    assert!(result.is_err());
    assert!(
        result
            .unwrap_err()
            .to_string()
            .contains("must start with '/'")
    );
}

#[test]
fn test_rest_config_accepts_root_path() {
    let f = toml_file(
        r#"
        [schema]
        name = "test"

        [rest]
        enabled = true
        path = "/"
    "#,
    );
    let intermediate =
        SchemaMerger::merge_toml_only(f.path().to_str().unwrap()).unwrap();

    let rest = intermediate
        .rest_config
        .expect("rest_config should be Some");
    assert_eq!(rest.path, "/");
}

#[test]
fn test_rest_config_accepts_trailing_slash() {
    let f = toml_file(
        r#"
        [schema]
        name = "test"

        [rest]
        enabled = true
        path = "/api/v1/"
    "#,
    );
    let intermediate =
        SchemaMerger::merge_toml_only(f.path().to_str().unwrap()).unwrap();

    let rest = intermediate
        .rest_config
        .expect("rest_config should be Some");
    assert_eq!(rest.path, "/api/v1/");
}

#[test]
fn test_rest_config_rejects_empty_path() {
    let f = toml_file(
        r#"
        [schema]
        name = "test"

        [rest]
        enabled = true
        path = ""
    "#,
    );
    let result = SchemaMerger::merge_toml_only(f.path().to_str().unwrap());
    assert!(result.is_err());
    assert!(
        result
            .unwrap_err()
            .to_string()
            .contains("must start with '/'")
    );
}

// ---- Full round-trip: TOML → merger → converter → CompiledSchema ----

#[test]
fn test_rest_config_round_trip_toml_to_compiled() {
    let f = toml_file(
        r#"
        [schema]
        name = "rest_test"

        [types.Post]
        sql_source = "posts"

        [types.Post.fields.pk_id]
        type = "ID"

        [types.Post.fields.title]
        type = "String"

        [queries.listPosts]
        return_type = "Post"
        return_array = true
        sql_source = "SELECT pk_id, title FROM posts"

        [rest]
        enabled = true
        path = "/api/v1"
        max_page_size = 250
        delete_response = "entity"
    "#,
    );
    let intermediate =
        SchemaMerger::merge_toml_only(f.path().to_str().unwrap()).unwrap();
    let compiled = SchemaConverter::convert(intermediate).unwrap();

    let rest = compiled
        .rest_config
        .expect("rest_config should be Some");
    assert!(rest.enabled);
    assert_eq!(rest.path, "/api/v1");
    assert_eq!(rest.max_page_size, 250);
    assert_eq!(
        rest.delete_response,
        fraiseql_core::schema::DeleteResponse::Entity
    );
    // Defaults preserved
    assert_eq!(rest.default_page_size, 100);
    assert_eq!(rest.sse_heartbeat_seconds, 30);
    assert!(rest.etag);
}

#[test]
fn test_rest_config_absent_in_compiled_when_disabled() {
    let f = toml_file(
        r#"
        [schema]
        name = "no_rest"

        [types.Post]
        sql_source = "posts"

        [types.Post.fields.pk_id]
        type = "ID"
    "#,
    );
    let intermediate =
        SchemaMerger::merge_toml_only(f.path().to_str().unwrap()).unwrap();
    let compiled = SchemaConverter::convert(intermediate).unwrap();

    assert!(compiled.rest_config.is_none());
}
