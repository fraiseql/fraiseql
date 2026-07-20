#![allow(clippy::unwrap_used, clippy::panic)] // Reason: test code, panics are acceptable
//! End-to-end proof that the change-log GraphQL surface and per-mutation cascade compose
//! (#665). Before the fix these two features were mutually exclusive: the cascade pass
//! classified the framework's own `TransportCheckpoint` projection — keyed by
//! `transport_name` with no `id` by design — as a cascade entity and failed the
//! `CascadeNode` `id: ID!` contract on a type the user never wrote and could not fix.
//!
//! This drives the **whole SDK-shaped compile path** (`SchemaConverter::convert`, the same
//! entry the CLI's `compile` command uses) on a schema that turns on `[observers]`,
//! `[changelog] expose = true`, AND a `cascade = true` mutation together, and asserts:
//! the schema compiles (so `validate()` accepts the combined surface — i.e. it will load
//! and boot), the genuine user entity is a cascade node, and BOTH framework projections
//! are injected but excluded from cascade classification. The DB-backed round-trip halves
//! (cascade payload delivery, `entityChangeLogs` pagination) are covered by the existing
//! `changelog_e2e_full_stack` and the canned-adapter cascade tests, which now accept this
//! combined schema; this test is the deterministic, DB-free proof of the compile unblock.

use fraiseql_cli::schema::{IntermediateSchema, converter::SchemaConverter};

/// An SDK-shaped `schema.json` exercising all three features at once. Observers must be
/// enabled or the converter rejects an exposed change-log (`converter/mod.rs` guard).
const SDK_SCHEMA_CHANGELOG_PLUS_CASCADE: &str = r#"
{
  "version": "2.0.0",
  "types": [
    {
      "name": "Post",
      "fields": [
        {"name": "id", "type": "ID", "nullable": false},
        {"name": "title", "type": "String", "nullable": false}
      ],
      "sql_source": "v_post",
      "is_input": false
    }
  ],
  "queries": [],
  "mutations": [
    {
      "name": "createPost",
      "return_type": "Post",
      "cascade": true,
      "sql_source": "fn_create_post",
      "operation": "CREATE"
    }
  ],
  "subscriptions": [],
  "observers_config": {"enabled": true},
  "changelog_config": {"expose": true}
}
"#;

fn compile() -> fraiseql_core::schema::CompiledSchema {
    let intermediate: IntermediateSchema =
        serde_json::from_str(SDK_SCHEMA_CHANGELOG_PLUS_CASCADE).expect("parse SDK schema.json");
    // A successful convert() means the full pipeline ran — including `validate()` after
    // both `inject_changelog` and `synthesize_cascade_types` — so the compiled schema is
    // load-valid and the server will boot on it. Before #665 this returned `Err`.
    SchemaConverter::convert(intermediate)
        .expect("#665: changelog + observers + cascade must compile together")
}

fn implements_cascade_node(
    schema: &fraiseql_core::schema::CompiledSchema,
    type_name: &str,
) -> bool {
    schema
        .types
        .iter()
        .find(|t| t.name.as_str() == type_name)
        .unwrap_or_else(|| panic!("{type_name} must be present"))
        .implements
        .iter()
        .any(|i| i == "CascadeNode")
}

#[test]
fn changelog_observers_cascade_compile_together() {
    // The headline unblock: the combination compiles at all.
    let compiled = compile();

    // Both framework surfaces were injected.
    assert!(
        compiled.types.iter().any(|t| t.name.as_str() == "EntityChangeLog"),
        "change-log projection injected"
    );
    assert!(
        compiled.types.iter().any(|t| t.name.as_str() == "TransportCheckpoint"),
        "checkpoint projection injected"
    );
    assert!(
        compiled.queries.iter().any(|q| q.name == "entity_change_logs"),
        "change-log list query injected"
    );
    assert!(
        compiled.queries.iter().any(|q| q.name == "transport_checkpoint"),
        "checkpoint point-lookup query injected"
    );
    assert!(
        compiled.mutations.iter().any(|m| m.name == "upsert_transport_checkpoint"),
        "checkpoint upsert mutation injected"
    );

    // The cascade surface was synthesized for the user mutation.
    assert!(
        compiled.interfaces.iter().any(|i| i.name == "CascadeNode"),
        "CascadeNode interface synthesized"
    );
    assert!(
        compiled.types.iter().any(|t| t.name.as_str() == "CreatePostPayload"),
        "cascade payload wrapper synthesized"
    );
    let create_post = compiled
        .mutations
        .iter()
        .find(|m| m.name == "createPost")
        .expect("createPost present");
    assert_eq!(
        create_post.return_type, "CreatePostPayload",
        "cascade rewrites the mutation return type to its payload"
    );
}

#[test]
fn user_entity_is_a_cascade_node_but_framework_projections_are_not() {
    let compiled = compile();

    // The genuine user entity IS a cascade node — enforcement is unchanged for real entities.
    assert!(
        implements_cascade_node(&compiled, "Post"),
        "the user's Post entity implements CascadeNode"
    );

    // The framework projections are `internal` and excluded from cascade classification —
    // this is the whole #665 fix. `TransportCheckpoint` has no `id` and could never back the
    // contract; `EntityChangeLog` is bookkeeping, not a cascade-deliverable entity.
    assert!(
        !implements_cascade_node(&compiled, "EntityChangeLog"),
        "EntityChangeLog is a framework projection, not a cascade entity"
    );
    assert!(
        !implements_cascade_node(&compiled, "TransportCheckpoint"),
        "TransportCheckpoint is a framework projection, not a cascade entity"
    );
}
