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
        roles:           vec!["ingest_writer".to_string()],
        scopes:          vec!["write:order".to_string()],
        tenant:          Some("acme".to_string()),
        user_id:         None,
        static_enriched: std::collections::BTreeMap::new(),
    });
    let round_tripped: SourceDefinition =
        serde_json::from_value(serde_json::to_value(&with).unwrap()).unwrap();
    assert_eq!(round_tripped, with);
}

#[test]
fn identity_maps_run_as_to_a_system_job_context() {
    let source = SourceDefinition::new("orders", "*/5 * * * *", "pollOrders").with_run_as(RunAs {
        roles:           vec!["ingest_writer".to_string()],
        scopes:          vec!["write:order".to_string()],
        tenant:          Some("acme".to_string()),
        user_id:         None,
        static_enriched: std::collections::BTreeMap::new(),
    });
    let ctx = source.identity("fire-1");
    assert_eq!(ctx.actor_type(), ActorType::SystemJob);
    assert!(ctx.has_role("ingest_writer"));
    assert!(ctx.has_scope("write:order"));
    assert_eq!(ctx.tenant_id.as_ref().map(|t| t.as_str()), Some("acme"));
}

/// #1700: `run_as.user_id` answers `jwt:sub` and `jwt:user_id`; the provenance stays
/// the source's system job.
#[test]
fn identity_answers_sub_with_the_run_as_user_id() {
    let principal = "6f1c2a9e-3b4d-4e5f-8a7b-9c0d1e2f3a4b";
    let source = SourceDefinition::new("orders", "*/5 * * * *", "pollOrders").with_run_as(RunAs {
        roles: vec!["ingest_writer".to_string()],
        user_id: Some(principal.to_string()),
        ..RunAs::default()
    });
    let ctx = source.identity("fire-1");
    for claim in ["sub", "user_id"] {
        assert_eq!(
            ctx.jwt_claim(claim, "tenant_id"),
            Some(serde_json::json!(principal)),
            "{claim}"
        );
    }
    assert_eq!(ctx.user_id.0, "system_job:orders", "provenance is unchanged");
    assert_eq!(ctx.actor_type(), ActorType::SystemJob);
}

/// #1700: without `user_id`, `jwt:sub` answers the provenance, as before.
#[test]
fn identity_answers_sub_with_its_provenance_without_a_user_id() {
    let source = SourceDefinition::new("orders", "*/5 * * * *", "pollOrders");
    let ctx = source.identity("fire-1");
    assert_eq!(ctx.jwt_claim("sub", "tenant_id"), Some(serde_json::json!("system_job:orders")));
}

/// #1700: a `run_as` key the ceiling does not carry is refused, never dropped.
#[test]
fn run_as_refuses_an_unknown_key() {
    let error = serde_json::from_value::<RunAs>(serde_json::json!({ "roles": ["r"], "sub": "x" }))
        .expect_err("unknown key");
    assert!(error.to_string().contains("sub"), "{error}");
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

/// #1989: a source's `run_as` loaded from the compiled schema, declaring two enriched
/// fields; one value is the empty string, which is a value, not an absence.
fn source_with_static_enriched() -> SourceDefinition {
    serde_json::from_value(serde_json::json!({
        "name": "orders",
        "schedule": "*/5 * * * *",
        "function": "pollOrders",
        "run_as": {
            "roles": ["ingest_writer"],
            "static_enriched": { "agent_role": "manager", "agent_id": "" }
        }
    }))
    .expect("a source whose run_as declares static_enriched loads")
}

/// #1989: `run_as.static_enriched` is what the identity presents to an
/// `enrichment`-sourced read, under the namespace the engine reads.
#[test]
fn identity_presents_the_run_as_static_enriched_fields() {
    let ctx = source_with_static_enriched().identity("fire-1");
    assert_eq!(
        ctx.attributes.get("fraiseql.enriched.agent_role"),
        Some(&serde_json::json!("manager"))
    );
    assert_eq!(ctx.attributes.get("fraiseql.enriched.agent_id"), Some(&serde_json::json!("")));
    assert_eq!(ctx.attributes.get("agent_role"), None, "only under the enriched namespace");
}

/// #1989: the enriched fields change what the identity presents, not who it is.
#[test]
fn static_enriched_leaves_the_provenance_and_the_mark_unchanged() {
    let ctx = source_with_static_enriched().identity("fire-1");
    assert_eq!(ctx.user_id.0, "system_job:orders");
    assert_eq!(ctx.actor_type(), ActorType::SystemJob);
    assert_eq!(ctx.enrichment_mark(), Some(crate::security::EnrichmentMark::Exempt));
}

/// #1989: without `static_enriched`, the identity presents no enriched field, so an
/// `enrichment`-sourced read stays refused.
#[test]
fn identity_presents_no_enriched_field_without_static_enriched() {
    let ctx = SourceDefinition::new("orders", "*/5 * * * *", "pollOrders").identity("fire-1");
    assert!(
        !ctx.attributes.keys().any(|k| k.starts_with("fraiseql.enriched.")),
        "{:?}",
        ctx.attributes
    );
}

/// #1989: the declared fields survive the compiled schema's round trip, and an empty
/// map is omitted, so a schema that declares none serializes as before.
#[test]
fn static_enriched_round_trips_and_is_omitted_when_empty() {
    let source = source_with_static_enriched();
    let json = serde_json::to_value(&source).unwrap();
    assert_eq!(json["run_as"]["static_enriched"]["agent_role"], "manager");
    let back: SourceDefinition = serde_json::from_value(json).unwrap();
    assert_eq!(back, source);

    let bare = serde_json::to_value(RunAs::default()).unwrap();
    assert!(bare.get("static_enriched").is_none(), "{bare}");
}
