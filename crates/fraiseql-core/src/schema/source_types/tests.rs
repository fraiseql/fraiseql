//! Unit tests for [`SourceDefinition`](super::SourceDefinition) and its place in
//! the compiled schema.
#![allow(clippy::unwrap_used)] // Reason: test module

use crate::{
    schema::{CompiledSchema, RunAs, SourceDefinition},
    security::ActorType,
};

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
fn run_as_round_trips_and_is_omitted_when_absent() {
    // A source with no run_as omits the field entirely (byte-stable pass-through).
    let source = SourceDefinition::new("orders", "*/5 * * * *", "pollOrders");
    let json = serde_json::to_value(&source).unwrap();
    assert!(json.get("run_as").is_none(), "an absent run_as is omitted");
    assert!(source.run_as.is_none());

    // A populated run_as survives a serde round-trip.
    let with = source.with_run_as(RunAs {
        roles:  vec!["ingest_writer".to_string()],
        scopes: vec!["write:order".to_string()],
        tenant: Some("acme".to_string()),
    });
    let round_tripped: SourceDefinition =
        serde_json::from_value(serde_json::to_value(&with).unwrap()).unwrap();
    assert_eq!(round_tripped, with);
}

#[test]
fn identity_maps_run_as_to_a_system_job_context() {
    let source = SourceDefinition::new("orders", "*/5 * * * *", "pollOrders").with_run_as(RunAs {
        roles:  vec!["ingest_writer".to_string()],
        scopes: vec!["write:order".to_string()],
        tenant: Some("acme".to_string()),
    });
    let ctx = source.identity("fire-1");
    assert_eq!(ctx.actor_type(), ActorType::SystemJob);
    assert!(ctx.has_role("ingest_writer"));
    assert!(ctx.has_scope("write:order"));
    assert_eq!(ctx.tenant_id.as_ref().map(|t| t.as_str()), Some("acme"));
}

#[test]
fn identity_is_fail_closed_without_run_as() {
    // No run_as ⇒ an authority-less identity: the source can write nothing until an
    // operator grants it a run_as ceiling.
    let source = SourceDefinition::new("orders", "*/5 * * * *", "pollOrders");
    let ctx = source.identity("fire-1");
    assert_eq!(ctx.actor_type(), ActorType::SystemJob);
    assert!(ctx.roles.is_empty(), "no roles → RBAC grants nothing");
    assert!(ctx.scopes.is_empty(), "no scopes → grants nothing");
    assert!(ctx.tenant_id.is_none(), "no tenant → global/deny");
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
