#![allow(clippy::unwrap_used, clippy::panic)] // Reason: test code, panics are acceptable
//! End-to-end coverage for the `federation` block surviving `fraiseql compile`.
//!
//! Regression for the bug where `fraiseql compile` silently dropped the input
//! schema's federation configuration: the SDK emits the block under the top-level
//! `"federation"` key, but `IntermediateSchema` only bound `"federation_config"`,
//! so the compiled subgraph lost all federation metadata (`_service` / SDL gone).
//!
//! These tests exercise both authoring workflows:
//! - **Legacy JSON** (`compile schema.json`): the SDK's `"federation"` key must bind into
//!   `IntermediateSchema.federation_config` and carry through to `CompiledSchema.federation`.
//! - **TOML** (`compile fraiseql.toml`): the `[federation]` section must carry through the merger
//!   into the compiled schema.
//!
//! The headline assertion is a server-readable round-trip: the compiled schema's
//! `federation_metadata()` must produce an Apollo Federation v2 `_service` SDL.

use std::io::Write;

use fraiseql_cli::{
    commands::compile::{CompileOptions, compile_to_schema},
    schema::{IntermediateSchema, converter::SchemaConverter, merger::SchemaMerger},
};
use tempfile::{NamedTempFile, TempDir};

/// A realistic SDK-shaped `schema.json` carrying a federation block exactly as the
/// Python SDK's `export_schema(..., federation=Federation(...))` emits it:
/// top-level `"federation"` key, `apollo_version`, and entities shaped as
/// `{ "name": ..., "key_fields": [...] }` (NOT `{type_name, key_fields: "id"}`).
const SDK_SCHEMA_WITH_FEDERATION: &str = r#"
{
  "version": "2.0.0",
  "types": [
    {
      "name": "Order",
      "fields": [
        {"name": "id", "type": "ID", "nullable": false},
        {"name": "total", "type": "Float", "nullable": false}
      ],
      "sql_source": "v_orders",
      "is_input": false
    }
  ],
  "queries": [
    {
      "name": "list_orders",
      "return_type": "Order",
      "returns_list": true,
      "sql_source": "v_orders",
      "nullable": false,
      "arguments": []
    }
  ],
  "mutations": [],
  "subscriptions": [],
  "federation": {
    "enabled": true,
    "service_name": "orders",
    "apollo_version": 2,
    "entities": [
      {"name": "Order", "key_fields": ["id"]}
    ]
  }
}
"#;

/// The same schema with no federation block at all.
const SDK_SCHEMA_WITHOUT_FEDERATION: &str = r#"
{
  "version": "2.0.0",
  "types": [
    {
      "name": "Order",
      "fields": [
        {"name": "id", "type": "ID", "nullable": false}
      ],
      "sql_source": "v_orders",
      "is_input": false
    }
  ],
  "queries": [
    {
      "name": "list_orders",
      "return_type": "Order",
      "returns_list": true,
      "sql_source": "v_orders",
      "nullable": false,
      "arguments": []
    }
  ],
  "mutations": [],
  "subscriptions": []
}
"#;

// ---- Legacy JSON workflow: deserialize + convert (CWD-free, parallel-safe) ----

#[test]
fn legacy_json_federation_key_binds_into_intermediate() {
    // The crux of the bug: the SDK's top-level `"federation"` key must bind into
    // `IntermediateSchema.federation_config` (it deserialized only from
    // `"federation_config"` before, so the block silently vanished here).
    let intermediate: IntermediateSchema =
        serde_json::from_str(SDK_SCHEMA_WITH_FEDERATION).expect("parse SDK schema.json");
    assert!(
        intermediate.federation_config.is_some(),
        "the SDK's `federation` key must bind into IntermediateSchema.federation_config"
    );
}

#[test]
fn legacy_json_federation_carries_into_compiled_schema() {
    let intermediate: IntermediateSchema =
        serde_json::from_str(SDK_SCHEMA_WITH_FEDERATION).expect("parse SDK schema.json");
    let compiled = SchemaConverter::convert(intermediate).expect("convert to compiled schema");

    let fed = compiled.federation.as_ref().expect("compiled schema must carry federation");
    assert!(fed.enabled, "federation must be enabled");
    assert_eq!(fed.service_name.as_deref(), Some("orders"));
    assert_eq!(fed.entities.len(), 1);
    assert_eq!(fed.entities[0].name, "Order");
    assert_eq!(fed.entities[0].key_fields, vec!["id".to_string()]);

    // The compiled JSON must expose the top-level `federation` key (the consumer's
    // `jq 'has("federation")'` check that returned `false` before the fix).
    let json = serde_json::to_value(&compiled).expect("serialize compiled schema");
    assert!(
        json.get("federation").is_some(),
        "serialized compiled schema must have a top-level `federation` key"
    );
}

#[test]
fn legacy_json_federation_produces_apollo_v2_service_sdl() {
    // Server-readable round-trip: compiled federation → `_service { sdl }` emits a
    // proper Apollo Federation v2 SDL with `@key` directives on the entity.
    let intermediate: IntermediateSchema =
        serde_json::from_str(SDK_SCHEMA_WITH_FEDERATION).expect("parse SDK schema.json");
    let compiled = SchemaConverter::convert(intermediate).expect("convert to compiled schema");

    let metadata = compiled
        .federation_metadata()
        .expect("federation_metadata must be Some when federation is enabled");
    let sdl = fraiseql_core::federation::generate_service_sdl(&compiled.raw_schema(), &metadata);

    assert!(
        sdl.contains("https://specs.apollo.dev/federation/v2.0"),
        "SDL must @link the Apollo Federation v2 spec, got:\n{sdl}"
    );
    assert!(
        sdl.contains("@key(fields: \"id\")"),
        "SDL must carry the Order entity's @key directive, got:\n{sdl}"
    );
}

#[test]
fn legacy_json_absent_federation_stays_absent() {
    let intermediate: IntermediateSchema =
        serde_json::from_str(SDK_SCHEMA_WITHOUT_FEDERATION).expect("parse SDK schema.json");
    assert!(intermediate.federation_config.is_none());

    let compiled = SchemaConverter::convert(intermediate).expect("convert to compiled schema");
    assert!(compiled.federation.is_none(), "no federation block ⇒ no compiled federation");
    assert!(compiled.federation_metadata().is_none());

    let json = serde_json::to_value(&compiled).expect("serialize compiled schema");
    assert!(
        json.get("federation").is_none(),
        "absent federation must not synthesize a `federation` key in the compiled JSON"
    );
}

#[test]
fn legacy_json_malformed_federation_fails_loudly() {
    // A non-empty federation block that cannot be carried must fail the compile,
    // not silently vanish: `entities` typed as a string is structurally invalid.
    let malformed = r#"
    {
      "version": "2.0.0",
      "types": [
        {"name": "Order", "fields": [{"name": "id", "type": "ID", "nullable": false}],
         "sql_source": "v_orders", "is_input": false}
      ],
      "queries": [],
      "mutations": [],
      "subscriptions": [],
      "federation": {"enabled": true, "entities": "not-an-array"}
    }
    "#;
    let intermediate: IntermediateSchema =
        serde_json::from_str(malformed).expect("federation_config is untyped JSON, so this parses");
    let err = SchemaConverter::convert(intermediate)
        .expect_err("a structurally invalid federation block must fail conversion, not vanish");
    let msg = format!("{err:#}");
    assert!(
        msg.contains("federation"),
        "the error must point at the federation block, got: {msg}"
    );
}

// ---- TOML workflow: the merger must carry `[federation]` through ----

#[test]
fn toml_federation_carries_through_merger_and_converter() {
    let mut f = NamedTempFile::new().unwrap();
    f.write_all(
        br#"
        [schema]
        name = "orders"

        [types.Order]
        sql_source = "v_orders"

        [federation]
        enabled = true
        apollo_version = 2

        [[federation.entities]]
        name = "Order"
        key_fields = ["id"]
    "#,
    )
    .unwrap();
    f.flush().unwrap();

    let intermediate = SchemaMerger::merge_toml_only(f.path().to_str().unwrap()).unwrap();
    assert!(
        intermediate.federation_config.is_some(),
        "the merger must carry `[federation]` into IntermediateSchema.federation_config"
    );

    let compiled = SchemaConverter::convert(intermediate).expect("convert to compiled schema");
    let fed = compiled.federation.as_ref().expect("compiled schema must carry federation");
    assert!(fed.enabled);
    assert_eq!(fed.entities.len(), 1);
    assert_eq!(fed.entities[0].name, "Order");
    assert_eq!(fed.entities[0].key_fields, vec!["id".to_string()]);
}

#[test]
fn toml_federation_service_name_and_version_reach_compiled_schema() {
    // The `[federation]` section must be able to name the subgraph and pin the spec
    // version — these used to be rejected by `deny_unknown_fields` and, even once
    // accepted, were dropped on the way to the compiled schema.
    let mut f = NamedTempFile::new().unwrap();
    f.write_all(
        br#"
        [schema]
        name = "orders"

        [types.Order]
        sql_source = "v_orders"

        [federation]
        enabled = true
        service_name = "orders"
        version = "v2"

        [[federation.entities]]
        name = "Order"
        key_fields = ["id"]
    "#,
    )
    .unwrap();
    f.flush().unwrap();

    let intermediate = SchemaMerger::merge_toml_only(f.path().to_str().unwrap()).unwrap();
    let compiled = SchemaConverter::convert(intermediate).expect("convert to compiled schema");
    let fed = compiled.federation.as_ref().expect("compiled schema must carry federation");
    assert_eq!(fed.service_name.as_deref(), Some("orders"));
    assert_eq!(fed.version.as_deref(), Some("v2"));

    // Server-readable round-trip: the named subgraph serves an Apollo Fed v2 SDL.
    let metadata = compiled.federation_metadata().expect("federation metadata");
    let sdl = fraiseql_core::federation::generate_service_sdl(&compiled.raw_schema(), &metadata);
    assert!(sdl.contains("https://specs.apollo.dev/federation/v2.0"), "got:\n{sdl}");
    assert!(sdl.contains("@key(fields: \"id\")"), "got:\n{sdl}");
}

// ---- Real CLI pipeline: `compile_to_schema` (mutates CWD; keep it alone) ----

#[tokio::test]
async fn compile_to_schema_legacy_json_carries_federation_end_to_end() {
    let dir = TempDir::new().expect("temp dir");
    std::fs::write(dir.path().join("schema.json"), SDK_SCHEMA_WITH_FEDERATION)
        .expect("write schema.json");

    let original = std::env::current_dir().expect("cwd");
    std::env::set_current_dir(dir.path()).expect("chdir into temp dir");
    let result = compile_to_schema(CompileOptions::new("schema.json")).await;
    std::env::set_current_dir(original).expect("restore cwd");

    let (schema, _report) = result.expect("compile must succeed");
    let fed = schema.federation.as_ref().expect("compiled schema must carry federation");
    assert!(fed.enabled);
    assert_eq!(fed.service_name.as_deref(), Some("orders"));

    let json = serde_json::to_value(&schema).expect("serialize compiled schema");
    assert!(
        json.get("federation").is_some(),
        "the real compile pipeline must write a top-level `federation` key"
    );
}
