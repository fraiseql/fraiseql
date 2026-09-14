//! Tests for compiled schema loading (basic + extended).

#![allow(clippy::unwrap_used, clippy::panic)] // Reason: test code, panics acceptable
#![allow(clippy::cast_precision_loss)] // Reason: test metrics reporting
#![allow(clippy::cast_sign_loss)] // Reason: test data uses small positive integers
#![allow(clippy::cast_possible_truncation)] // Reason: test data values are bounded
#![allow(clippy::cast_possible_wrap)] // Reason: test data values are bounded
#![allow(clippy::missing_panics_doc)] // Reason: test helpers
#![allow(clippy::missing_errors_doc)] // Reason: test helpers
#![allow(missing_docs)] // Reason: test code
#![allow(clippy::items_after_statements)] // Reason: test helpers defined near use site

use std::io::Write as _;

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

/// A `functions` section loads — in a build that can run one. Gated deliberately:
/// since #1326 a build without `functions-runtime` **refuses** this same fixture, so
/// asserting success unconditionally would pin the silent-drop behaviour the refusal
/// removes. The other arm is `a_functions_section_is_refused_without_a_runtime`.
#[cfg(feature = "functions-runtime")]
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
    assert_eq!(functions.module_dir, std::path::PathBuf::from("/opt/fraiseql/functions"));
}

/// #1325 RED: the loader's own grammar copy and the trigger registry's disagree.
///
/// `validate_functions_config` kept a private `VALID_TRIGGER_PREFIXES` list that
/// `ParsedTrigger::parse` — the grammar the dispatcher actually uses — has since
/// outgrown. `after:capture:` (#366) and `after:ingest:` are valid triggers the
/// registry loads and dispatches, and the loader rejected both before the schema
/// reached `build_functions_subsystem`. Two copies of one rule, and this is the
/// fourth case one of them lost.
#[cfg(feature = "functions-runtime")]
#[tokio::test]
async fn a_capture_or_ingest_trigger_is_not_rejected_by_the_loader() {
    for trigger in ["after:capture:User:update", "after:ingest:email"] {
        let json = format!(
            r#"{{
            "types": [],
            "functions": {{
                "module_dir": "/opt/fraiseql/functions",
                "definitions": [
                    {{"name": "fn_under_test", "trigger": "{trigger}", "runtime": "Wasm"}}
                ]
            }}
        }}"#
        );
        let file = write_schema(&json);
        let loader = CompiledSchemaLoader::new(file.path());

        let extended = loader.load_extended().await.unwrap_or_else(|e| {
            panic!(
                "`{trigger}` is a trigger the registry parses and dispatches, but the loader \
                 refused the schema: {e}"
            )
        });
        assert_eq!(
            extended.functions.expect("the section must load").definitions[0].trigger,
            trigger
        );
    }
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
    // Assert the trigger diagnosis by NAME, not merely `ValidationError`. Since #1326 a
    // lean build refuses any `functions` section at all, and that refusal is a
    // `ValidationError` too — so the loose assertion would pass in a lean build while
    // the trigger grammar itself had stopped being checked.
    let message = match result {
        Err(SchemaLoadError::ValidationError(m)) => m,
        other => panic!("expected ValidationError for unknown trigger format, got {other:?}"),
    };
    assert!(
        message.contains("unknown_trigger_format"),
        "the error must name the rejected trigger, got: {message}"
    );
}

// ── #1326: a build that cannot serve a section refuses it ─────────────────────

/// A lean build (no `functions-runtime`) refuses a compiled schema declaring a
/// `functions` section, rather than loading it and dropping it.
///
/// This is the arm that matters and the arm `--all-features` can never compile: with the
/// feature ON the section is servable and loads. The test leg runs
/// `cargo test -p fraiseql-server --lib` with default features, which is where this runs.
#[cfg(not(feature = "functions-runtime"))]
#[tokio::test]
async fn a_functions_section_is_refused_without_a_runtime() {
    let json = r#"{
        "types": [],
        "functions": {
            "module_dir": "/opt/fraiseql/functions",
            "definitions": [
                {"name": "on_create", "trigger": "after:mutation:createUser", "runtime": "Wasm"}
            ]
        }
    }"#;
    let file = write_schema(json);
    let loader = CompiledSchemaLoader::new(file.path());

    let message = match loader.load_extended().await {
        Err(SchemaLoadError::ValidationError(m)) => m,
        other => panic!(
            "a build with no function runtime must refuse a declared `functions` section, \
             not load and drop it; got {other:?}"
        ),
    };
    assert!(message.contains("functions"), "the error must name the section: {message}");
    assert!(
        message.contains("functions-runtime"),
        "the error must name the missing feature, or an operator cannot act on it: {message}"
    );
    assert!(
        message.contains("-platform"),
        "the error must name the remedy — the image tag that can run it: {message}"
    );
}

/// The wiring pin for this build's arm: `gated_sections()` must report what `cfg!`
/// actually compiled, or the refusal above is decided by a constant.
#[cfg(not(feature = "functions-runtime"))]
#[test]
fn gated_sections_reports_functions_as_absent_in_a_lean_build() {
    let sections = super::loader::gated_sections();
    let functions = sections.iter().find(|s| s.name == "functions").expect("listed");
    assert!(!functions.compiled_in, "this build has no `functions-runtime`");
}

/// The same pin for the other arm, so neither is decided by a constant.
#[cfg(feature = "functions-runtime")]
#[test]
fn gated_sections_reports_functions_as_present_with_the_runtime() {
    let sections = super::loader::gated_sections();
    let functions = sections.iter().find(|s| s.name == "functions").expect("listed");
    assert!(functions.compiled_in, "this build has `functions-runtime`");
}

// ── The refusal policy itself, exercised in both directions in every build ────
//
// `refuse_unservable_sections` is pure, so a build that HAS a feature can still assert
// what a build without it does. The `cfg`-gated tests above pin that this build passes
// the real `cfg!` value in; these pin what the policy then does with it.

fn probe(name: &'static str, compiled_in: bool) -> super::loader::GatedSection {
    super::loader::GatedSection {
        name,
        feature: "some-feature",
        compiled_in,
        remedy: "pull the `-platform` image tag",
        activity: super::loader::Activity::NonEmptyDefinitions,
    }
}

fn probe_with(
    name: &'static str,
    compiled_in: bool,
    activity: super::loader::Activity,
) -> super::loader::GatedSection {
    super::loader::GatedSection {
        activity,
        ..probe(name, compiled_in)
    }
}

#[test]
fn a_declared_section_this_build_cannot_serve_is_refused() {
    let raw = serde_json::json!({ "types": [], "functions": { "definitions": [{ "name": "f" }] } });
    let err = super::loader::refuse_unservable_sections(&raw, &[probe("functions", false)])
        .expect_err("a section nothing can serve must be refused");
    assert!(matches!(err, SchemaLoadError::ValidationError(_)), "{err:?}");
}

#[test]
fn a_declared_section_this_build_can_serve_is_accepted() {
    let raw = serde_json::json!({ "types": [], "functions": { "definitions": [{ "name": "f" }] } });
    assert!(
        super::loader::refuse_unservable_sections(&raw, &[probe("functions", true)]).is_ok(),
        "a servable section must load"
    );
}

#[test]
fn an_undeclared_section_is_not_refused() {
    let raw = serde_json::json!({ "types": [] });
    assert!(
        super::loader::refuse_unservable_sections(&raw, &[probe("functions", false)]).is_ok(),
        "a lean build must still boot a schema that declares nothing it cannot serve"
    );
}

/// A `null` is not a declaration — the same rule the #1008 `storage` refusal uses, so a
/// serializer that emits every key with an empty value does not fail the boot.
#[test]
fn a_null_section_is_not_a_declaration() {
    let raw = serde_json::json!({ "types": [], "functions": serde_json::Value::Null });
    assert!(
        super::loader::refuse_unservable_sections(&raw, &[probe("functions", false)]).is_ok(),
        "an explicit null must not be read as a declaration"
    );
}

/// A section that is present but **switched off** is not a misconfiguration: the
/// operator turned it off, and refusing on it would break a working deployment that
/// carries a fuller schema than its binary serves. Each predicate mirrors the test the
/// serving subsystem itself makes.
#[test]
fn a_section_switched_off_is_not_refused() {
    let raw = serde_json::json!({ "types": [], "rest_config": { "enabled": false } });
    assert!(
        super::loader::refuse_unservable_sections(
            &raw,
            &[probe_with(
                "rest_config",
                false,
                super::loader::Activity::EnabledFlag
            )]
        )
        .is_ok(),
        "a disabled section asks for nothing to run"
    );
}

#[test]
fn a_section_switched_on_is_refused() {
    let raw = serde_json::json!({ "types": [], "rest_config": { "enabled": true } });
    assert!(
        super::loader::refuse_unservable_sections(
            &raw,
            &[probe_with(
                "rest_config",
                false,
                super::loader::Activity::EnabledFlag
            )]
        )
        .is_err(),
        "an enabled section this build cannot serve must be refused"
    );
}

/// An empty `definitions` list asks for nothing — the same early return
/// `prepare_functions_runtime` makes before doing any work.
#[test]
fn an_empty_definitions_list_is_not_refused() {
    let raw = serde_json::json!({ "types": [], "functions": { "definitions": [] } });
    assert!(
        super::loader::refuse_unservable_sections(&raw, &[probe("functions", false)]).is_ok(),
        "a functions section with nothing in it starts nothing"
    );
}

/// A `sources` array in which every entry is disabled starts no poller, so it is not a
/// misconfiguration — but one enabled entry among them is.
#[test]
fn sources_are_refused_only_when_one_is_enabled() {
    let off = serde_json::json!({
        "types": [], "sources": [{ "name": "a", "enabled": false }]
    });
    let on = serde_json::json!({
        "types": [],
        "sources": [{ "name": "a", "enabled": false }, { "name": "b", "enabled": true }]
    });
    let section = probe_with("sources", false, super::loader::Activity::AnyEntryEnabled);
    assert!(
        super::loader::refuse_unservable_sections(&off, &[section]).is_ok(),
        "all-disabled sources start no poller"
    );
    assert!(
        super::loader::refuse_unservable_sections(&on, &[section]).is_err(),
        "one enabled source among disabled ones must still be refused"
    );
}

/// Every section in the real table must name a feature that exists in this crate's
/// `Cargo.toml`, or its remedy tells an operator to pass a flag that does nothing.
#[test]
fn every_gated_section_names_a_real_feature() {
    let manifest = include_str!("../../Cargo.toml");
    for section in super::loader::gated_sections() {
        assert!(
            manifest.contains(&format!("\n{} = [", section.feature))
                || manifest.contains(&format!("\n{} = {{", section.feature)),
            "`{}` names feature `{}`, which is not declared in fraiseql-server/Cargo.toml",
            section.name,
            section.feature
        );
    }
}

/// The refusal names the first unservable section it finds, and every section is
/// checked — not just the first in the list.
#[test]
fn every_listed_section_is_checked_not_only_the_first() {
    let raw = serde_json::json!({ "types": [], "sources": [{ "name": "s", "enabled": true }] });
    let err = super::loader::refuse_unservable_sections(
        &raw,
        &[
            probe("functions", false),
            probe_with("sources", false, super::loader::Activity::AnyEntryEnabled),
        ],
    )
    .expect_err("the second entry must be checked too");
    let SchemaLoadError::ValidationError(message) = err else {
        panic!("expected ValidationError")
    };
    assert!(
        message.contains("sources"),
        "the refusal must name the section found: {message}"
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

/// Same reason as `test_schema_loads_functions_config`: this fixture declares a
/// `functions` section, so it can only load in a build that can serve one (#1326).
#[cfg(feature = "functions-runtime")]
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
