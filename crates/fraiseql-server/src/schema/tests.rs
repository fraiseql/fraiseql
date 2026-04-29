//! Tests for extended compiled schema loading.

#![allow(clippy::unwrap_used)] // Reason: test code, panics acceptable
#![allow(missing_docs)] // Reason: test code

use std::io::Write as _;
use std::path::PathBuf;

use tempfile::NamedTempFile;

use super::loader::{CompiledSchemaLoader, SchemaLoadError};

fn write_schema(json: &str) -> NamedTempFile {
    let mut file = NamedTempFile::new().unwrap();
    file.write_all(json.as_bytes()).unwrap();
    file.flush().unwrap();
    file
}

/// Minimal schema JSON that satisfies `CompiledSchema` deserialization.
fn minimal_schema() -> &'static str {
    r#"{"types": [], "mutations": []}"#
}


// ── Storage config ────────────────────────────────────────────────────────────

#[tokio::test]
async fn test_schema_loads_storage_config() {
    let json = r#"{
        "types": [],
        "storage": {
            "buckets": [
                {"name": "avatars", "access": "private"},
                {"name": "media", "access": "public_read", "max_object_bytes": 5242880}
            ]
        }
    }"#;
    let file = write_schema(json);
    let loader = CompiledSchemaLoader::new(file.path());

    let extended = loader.load_extended().await.unwrap();
    let storage = extended.storage.unwrap();

    assert_eq!(storage.buckets.len(), 2);
    assert_eq!(storage.buckets[0].name, "avatars");
    assert_eq!(storage.buckets[1].name, "media");
    assert_eq!(storage.buckets[1].max_object_bytes, Some(5_242_880));
}

#[tokio::test]
async fn test_schema_without_storage_returns_none() {
    let file = write_schema(minimal_schema());
    let loader = CompiledSchemaLoader::new(file.path());

    let extended = loader.load_extended().await.unwrap();
    assert!(extended.storage.is_none());
}

#[tokio::test]
async fn test_schema_validates_storage_bucket_names() {
    // bucket name with spaces is invalid
    let json = r#"{
        "types": [],
        "storage": {
            "buckets": [{"name": "bad name with spaces", "access": "private"}]
        }
    }"#;
    let file = write_schema(json);
    let loader = CompiledSchemaLoader::new(file.path());

    let result = loader.load_extended().await;
    assert!(
        matches!(result, Err(SchemaLoadError::ValidationError(_))),
        "expected ValidationError, got {result:?}"
    );
}

// ── Functions config ──────────────────────────────────────────────────────────

#[tokio::test]
async fn test_schema_loads_functions_config() {
    let json = r#"{
        "types": [],
        "mutations": [],
        "functions": {
            "module_dir": "/opt/fraiseql/functions",
            "definitions": [
                {
                    "name": "on_create_user",
                    "trigger": "after:mutation:createUser",
                    "runtime": "Wasm"
                },
                {
                    "name": "validate_user",
                    "trigger": "before:mutation:createUser",
                    "runtime": "Wasm",
                    "timeout_ms": 300
                }
            ]
        }
    }"#;
    let file = write_schema(json);
    let loader = CompiledSchemaLoader::new(file.path());

    let extended = loader.load_extended().await.unwrap();
    let functions = extended.functions.unwrap();

    assert_eq!(functions.definitions.len(), 2);
    assert_eq!(functions.definitions[0].name, "on_create_user");
    assert_eq!(functions.definitions[0].trigger, "after:mutation:createUser");
    assert_eq!(functions.definitions[1].timeout_ms, Some(300));
    assert_eq!(functions.module_dir, PathBuf::from("/opt/fraiseql/functions"));
}

#[tokio::test]
async fn test_schema_without_functions_returns_none() {
    let file = write_schema(minimal_schema());
    let loader = CompiledSchemaLoader::new(file.path());

    let extended = loader.load_extended().await.unwrap();
    assert!(extended.functions.is_none());
}

#[tokio::test]
async fn test_schema_validates_function_triggers() {
    // trigger with unknown format (not after:, before:, cron:, http:, after:storage:)
    let json = r#"{
        "types": [],
        "functions": {
            "module_dir": "/opt/fraiseql/functions",
            "definitions": [
                {
                    "name": "bad_fn",
                    "trigger": "unknown_trigger_format",
                    "runtime": "Wasm"
                }
            ]
        }
    }"#;
    let file = write_schema(json);
    let loader = CompiledSchemaLoader::new(file.path());

    let result = loader.load_extended().await;
    assert!(
        matches!(result, Err(SchemaLoadError::ValidationError(_))),
        "expected ValidationError for unknown trigger format, got {result:?}"
    );
}

// ── Realtime config ───────────────────────────────────────────────────────────

#[tokio::test]
async fn test_schema_loads_realtime_config() {
    // Types must have sql_source to satisfy CompiledSchema deserialization.
    let json = r#"{
        "types": [
            {"name": "Post", "sql_source": "t_posts"},
            {"name": "Comment", "sql_source": "t_comments"}
        ],
        "realtime": {
            "enabled": true,
            "entities": ["Post", "Comment"],
            "max_connections_per_context": 50
        }
    }"#;
    let file = write_schema(json);
    let loader = CompiledSchemaLoader::new(file.path());

    let extended = loader.load_extended().await.unwrap();
    let realtime = extended.realtime.unwrap();

    assert!(realtime.enabled);
    assert_eq!(realtime.entities.len(), 2);
    assert!(realtime.entities.contains(&"Post".to_string()));
    assert_eq!(realtime.max_connections_per_context, Some(50));
}

#[tokio::test]
async fn test_schema_without_realtime_returns_none() {
    let file = write_schema(minimal_schema());
    let loader = CompiledSchemaLoader::new(file.path());

    let extended = loader.load_extended().await.unwrap();
    assert!(extended.realtime.is_none());
}

#[tokio::test]
async fn test_schema_validates_realtime_entities() {
    // "Ghost" is not in schema types → validation error
    let json = r#"{
        "types": [{"name": "Post", "sql_source": "t_posts"}],
        "realtime": {
            "enabled": true,
            "entities": ["Post", "Ghost"]
        }
    }"#;
    let file = write_schema(json);
    let loader = CompiledSchemaLoader::new(file.path());

    let result = loader.load_extended().await;
    assert!(
        matches!(result, Err(SchemaLoadError::ValidationError(_))),
        "expected ValidationError for unknown realtime entity, got {result:?}"
    );
}

// ── All-sections fixture ──────────────────────────────────────────────────────

#[tokio::test]
async fn test_schema_full_loads_all_sections() {
    let json = r#"{
        "types": [{"name": "User", "sql_source": "t_users"}],
        "mutations": [],
        "storage": {
            "buckets": [{"name": "avatars", "access": "private"}]
        },
        "functions": {
            "module_dir": "/functions",
            "definitions": [
                {"name": "on_create", "trigger": "after:mutation:createUser", "runtime": "Wasm"}
            ]
        },
        "realtime": {
            "enabled": true,
            "entities": ["User"]
        }
    }"#;
    let file = write_schema(json);
    let loader = CompiledSchemaLoader::new(file.path());

    let extended = loader.load_extended().await.unwrap();

    assert!(extended.storage.is_some());
    assert!(extended.functions.is_some());
    assert!(extended.realtime.is_some());
}

#[tokio::test]
async fn test_schema_unknown_sections_ignored() {
    // Forward compatibility: unknown top-level keys should not cause errors.
    let json = r#"{
        "types": [],
        "future_feature": {"some_key": "some_value"},
        "another_new_thing": 42
    }"#;
    let file = write_schema(json);
    let loader = CompiledSchemaLoader::new(file.path());

    let result = loader.load_extended().await;
    assert!(result.is_ok(), "unknown sections should be ignored: {result:?}");
}

