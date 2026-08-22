//! Tests for compiled schema loading (basic + extended).

#![allow(clippy::unwrap_used)] // Reason: test code, panics acceptable
#![allow(clippy::cast_precision_loss)] // Reason: test metrics reporting
#![allow(clippy::cast_sign_loss)] // Reason: test data uses small positive integers
#![allow(clippy::cast_possible_truncation)] // Reason: test data values are bounded
#![allow(clippy::cast_possible_wrap)] // Reason: test data values are bounded
#![allow(clippy::missing_panics_doc)] // Reason: test helpers
#![allow(clippy::missing_errors_doc)] // Reason: test helpers
#![allow(missing_docs)] // Reason: test code
#![allow(clippy::items_after_statements)] // Reason: test helpers defined near use site

use std::{io::Write as _, path::PathBuf};

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
    r#"{"types": []}"#
}

// ── Basic loader tests ────────────────────────────────────────────────────────

#[tokio::test]
async fn test_loader_not_found() {
    let loader = CompiledSchemaLoader::new("/nonexistent/path/schema.json");
    let result = loader.load().await;
    assert!(matches!(result, Err(SchemaLoadError::NotFound(_))));
}

#[tokio::test]
async fn test_loader_invalid_json() {
    let mut file = NamedTempFile::new().unwrap();
    writeln!(file, "{{invalid json").unwrap();
    file.flush().unwrap();

    let loader = CompiledSchemaLoader::new(file.path());
    let result = loader.load().await;
    assert!(matches!(result, Err(SchemaLoadError::ParseError(_))));
}

// ── Storage config ────────────────────────────────────────────────────────────

/// #1008: a `storage` section in the compiled schema is refused, not silently
/// dropped.
///
/// It was parsed, validated by `validate_storage_config`, stored on
/// `ExtendedCompiledSchema.storage` — and read by nothing. `main.rs` takes
/// `.schema` and `.functions` and drops `.storage` on the floor; the server's
/// storage backend is built from `[storage]` in the **server config file**. So an
/// author who read "configuration is embedded in the compiled schema" and put
/// bucket policy there got a successful compile, a successful boot, and either no
/// storage backend at all or whatever unrelated `[storage]` the server config
/// named.
#[tokio::test]
async fn a_storage_section_in_the_compiled_schema_is_refused() {
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

    let err = loader.load_extended().await.expect_err(
        "a compiled-schema `storage` section must be refused, not accepted and dropped",
    );

    assert!(
        matches!(err, SchemaLoadError::ValidationError(_)),
        "expected ValidationError, got {err:?}"
    );
    assert!(
        err.to_string().contains("[storage]"),
        "the refusal must name the working surface so the author can move the config there: {err}"
    );
}

/// A `null` storage key is the shape a producer emits for "absent", and must
/// stay bootable — refusing it would fail a schema that declares nothing.
#[tokio::test]
async fn a_null_storage_key_is_not_refused() {
    let file = write_schema(r#"{"types": [], "storage": null}"#);
    let loader = CompiledSchemaLoader::new(file.path());

    assert!(
        loader.load_extended().await.is_ok(),
        "a null `storage` key declares nothing and must not fail the boot"
    );
}

#[tokio::test]
async fn test_schema_without_storage_returns_none() {
    let file = write_schema(minimal_schema());
    let loader = CompiledSchemaLoader::new(file.path());

    assert!(loader.load_extended().await.is_ok());
}

// ── Functions config ──────────────────────────────────────────────────────────

#[tokio::test]
async fn test_schema_loads_functions_config() {
    let json = r#"{
        "types": [],
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

// ── Realtime config (removed in #605 — warn-and-ignore posture) ─────────────────

#[tokio::test]
async fn test_schema_with_realtime_key_is_ignored() {
    // The `/realtime/v1` subsystem was removed (#605). A compiled schema that still
    // carries a `"realtime"` section (hand-authored or stale) must load clean — the
    // section is ignored (with a warning), not parsed, and never fails the load.
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
    // Core schema still loads; the realtime section is dropped (no field to inspect).
    assert_eq!(extended.schema.types.len(), 2);
}

#[tokio::test]
async fn test_schema_without_realtime_key_loads_clean() {
    // Survival pin (#605 Phase 00 pin #4): a realtime-free schema — the overwhelmingly
    // common case — loads without error before, during, and after the removal.
    let file = write_schema(minimal_schema());
    let loader = CompiledSchemaLoader::new(file.path());

    loader.load_extended().await.unwrap();
}

#[tokio::test]
async fn test_schema_realtime_key_with_unknown_entity_is_ignored() {
    // Before #605 this errored (`validate_realtime_config` rejected an entity absent
    // from the schema types). Under warn-and-ignore, the whole section is dropped, so a
    // "ghost" entity can no longer fail the load — the section is never validated.
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
        result.is_ok(),
        "realtime section must be ignored, not validated, got {result:?}"
    );
}

// ── All-sections fixture ──────────────────────────────────────────────────────

#[tokio::test]
async fn test_schema_full_loads_all_sections() {
    let json = r#"{
        "types": [{"name": "User", "sql_source": "t_users"}],
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

    // functions load; the legacy `"realtime"` section (still in the fixture) is ignored
    // with a warning (#605), so the load still succeeds. `storage` is no longer in this
    // fixture because it is now refused outright (#1008) — the two postures are
    // deliberately different and each has its own test.
    assert!(extended.functions.is_some());
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
