//! Unit tests for [`SourceDefinition`](super::SourceDefinition) and its place in
//! the compiled schema.
#![allow(clippy::unwrap_used)] // Reason: test module

use crate::schema::{CompiledSchema, SourceDefinition};

#[test]
fn cursor_name_defaults_to_the_source_name() {
    let source = SourceDefinition::new("orders", "*/5 * * * *", "pollOrders");
    assert_eq!(source.cursor_name(), "orders");
    assert!(source.enabled);

    let with_cursor = source.with_cursor("orders-cursor");
    assert_eq!(with_cursor.cursor_name(), "orders-cursor");
}

#[test]
fn cron_trigger_desugars_the_schedule() {
    let source = SourceDefinition::new("orders", "*/5 * * * *", "pollOrders");
    assert_eq!(source.cron_trigger(), "cron:*/5 * * * *");
}

#[test]
fn serde_round_trips_and_omits_defaults() {
    // A minimal source omits the cursor and options (defaults) and keeps enabled.
    let source = SourceDefinition::new("orders", "0 * * * *", "pollOrders");
    let json = serde_json::to_value(&source).unwrap();
    assert!(json.get("cursor").is_none(), "an unset cursor is omitted");
    assert!(json.get("options").is_none(), "null options are omitted");
    assert_eq!(json["enabled"], serde_json::json!(true));

    let round_tripped: SourceDefinition = serde_json::from_value(json).unwrap();
    assert_eq!(round_tripped, source);
}

#[test]
fn enabled_defaults_to_true_when_absent() {
    // A payload without `enabled` deserializes as enabled (serde default).
    let source: SourceDefinition = serde_json::from_value(serde_json::json!({
        "name": "orders",
        "schedule": "0 * * * *",
        "function": "pollOrders",
    }))
    .unwrap();
    assert!(source.enabled);
    assert_eq!(source.cursor_name(), "orders");
}

#[test]
fn compiled_schema_sources_round_trip_and_empty_is_omitted() {
    // Empty sources are omitted from the compiled JSON (byte-stable pass-through).
    let empty = CompiledSchema::default();
    let json = serde_json::to_value(&empty).unwrap();
    assert!(json.get("sources").is_none(), "an empty sources array is omitted");

    // A populated sources array survives a serde round-trip.
    let schema = CompiledSchema {
        sources: vec![SourceDefinition::new("orders", "*/5 * * * *", "pollOrders")],
        ..CompiledSchema::default()
    };
    let round_tripped: CompiledSchema =
        serde_json::from_value(serde_json::to_value(&schema).unwrap()).unwrap();
    assert_eq!(round_tripped.sources, schema.sources);
    assert_eq!(round_tripped, schema);
}
