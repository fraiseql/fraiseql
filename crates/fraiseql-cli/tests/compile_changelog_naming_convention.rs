#![allow(clippy::unwrap_used, clippy::panic)] // Reason: test code, panics are acceptable
//! End-to-end coverage for the SDK's `naming_convention` key surviving `fraiseql compile`
//! and reaching the built-in change-log injection.
//!
//! The Python SDK unconditionally recases every emitted identifier to `camelCase` and now
//! declares that by exporting a top-level `"naming_convention": "camelCase"` key (#500).
//! The compiler's change-log injection (#149) renders `EntityChangeLog` /
//! `TransportCheckpoint` identifiers via the schema's `naming_convention` (#498). Without
//! the SDK declaring it, the convention defaulted to `Preserve` → `snake_case` change-log
//! fields in an otherwise `camelCase` schema, so the #498 `camelCase` support never fired.
//!
//! These tests prove the whole chain is intact: the SDK's `"naming_convention"` key must
//! bind into `IntermediateSchema.naming_convention`, thread through `SchemaConverter` onto
//! `CompiledSchema.naming_convention`, and drive `inject_changelog` to emit `camelCase`
//! change-log fields — no manual intervention.

use fraiseql_cli::schema::{IntermediateSchema, converter::SchemaConverter};
use fraiseql_core::schema::NamingConvention;

const ENTITY_CHANGE_LOG: &str = "EntityChangeLog";

/// An SDK-shaped `schema.json` declaring the camelCase convention the SDK applies, with
/// the built-in change-log exposed. Observers must be enabled, or the converter rejects
/// an exposed change-log over tables the observer system installs.
const SDK_SCHEMA_CAMELCASE_CHANGELOG: &str = r#"
{
  "version": "2.0.0",
  "naming_convention": "camelCase",
  "types": [
    {
      "name": "Order",
      "fields": [
        {"name": "id", "type": "ID", "nullable": false},
        {"name": "customerId", "type": "String", "nullable": false}
      ],
      "sql_source": "v_orders",
      "is_input": false
    }
  ],
  "queries": [],
  "mutations": [],
  "subscriptions": [],
  "observers_config": {"enabled": true},
  "changelog_config": {"expose": true}
}
"#;

/// The same schema with no `naming_convention` key — the compiler defaults to `Preserve`.
const SDK_SCHEMA_NO_NAMING_CONVENTION: &str = r#"
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
  "queries": [],
  "mutations": [],
  "subscriptions": [],
  "observers_config": {"enabled": true},
  "changelog_config": {"expose": true}
}
"#;

#[test]
fn naming_convention_camelcase_key_binds_into_intermediate() {
    // The crux: the SDK's top-level `"naming_convention"` key must bind into the
    // intermediate schema (it deserializes the engine's serde wire value `"camelCase"`).
    let intermediate: IntermediateSchema =
        serde_json::from_str(SDK_SCHEMA_CAMELCASE_CHANGELOG).expect("parse SDK schema.json");
    assert_eq!(
        intermediate.naming_convention,
        NamingConvention::CamelCase,
        "the SDK's `naming_convention` key must bind into IntermediateSchema.naming_convention"
    );
}

#[test]
fn absent_naming_convention_defaults_to_preserve() {
    let intermediate: IntermediateSchema =
        serde_json::from_str(SDK_SCHEMA_NO_NAMING_CONVENTION).expect("parse SDK schema.json");
    assert_eq!(
        intermediate.naming_convention,
        NamingConvention::Preserve,
        "an absent `naming_convention` key must default to Preserve"
    );
}

#[test]
fn sdk_camelcase_schema_injects_camelcase_changelog() {
    // The headline regression: a camelCase SDK schema with the change-log exposed must
    // compile to a change-log surface that is *also* camelCase, with no snake_case leak.
    let intermediate: IntermediateSchema =
        serde_json::from_str(SDK_SCHEMA_CAMELCASE_CHANGELOG).expect("parse SDK schema.json");
    let compiled = SchemaConverter::convert(intermediate).expect("convert to compiled schema");

    assert_eq!(compiled.naming_convention, NamingConvention::CamelCase);

    let ecl = compiled
        .types
        .iter()
        .find(|t| t.name == ENTITY_CHANGE_LOG)
        .expect("change-log must be injected when exposed");

    // Fields follow the SDK's camelCase convention; the snake_case forms are gone.
    for camel in ["pkEntityChangeLog", "objectType", "objectData", "createdAt"] {
        assert!(ecl.find_field(camel).is_some(), "missing camelCase change-log field {camel}");
    }
    assert!(
        ecl.find_field("pk_entity_change_log").is_none(),
        "snake_case change-log field leaked despite a camelCase schema"
    );
    assert!(ecl.find_field("object_type").is_none(), "snake_case change-log field leaked");

    // Operation names recased too — the change-log list query is camelCase.
    assert!(
        compiled.queries.iter().any(|q| q.name == "entityChangeLogs"),
        "change-log list query must be camelCase"
    );
    assert!(
        !compiled.queries.iter().any(|q| q.name == "entity_change_logs"),
        "snake_case change-log query leaked"
    );
}

#[test]
fn absent_naming_convention_keeps_snake_case_changelog() {
    // Control: with no convention declared the injection stays snake_case — proving the
    // SDK's `naming_convention` key is exactly what flips the change-log to camelCase.
    let intermediate: IntermediateSchema =
        serde_json::from_str(SDK_SCHEMA_NO_NAMING_CONVENTION).expect("parse SDK schema.json");
    let compiled = SchemaConverter::convert(intermediate).expect("convert to compiled schema");

    assert_eq!(compiled.naming_convention, NamingConvention::Preserve);

    let ecl = compiled
        .types
        .iter()
        .find(|t| t.name == ENTITY_CHANGE_LOG)
        .expect("change-log must be injected when exposed");
    assert!(
        ecl.find_field("pk_entity_change_log").is_some(),
        "default convention is snake_case"
    );
    assert!(
        ecl.find_field("pkEntityChangeLog").is_none(),
        "camelCase must not appear by default"
    );
}
