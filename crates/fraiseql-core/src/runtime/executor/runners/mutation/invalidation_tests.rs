//! Unit tests for [`plan_invalidation`](super::plan_invalidation).
//!
//! The whole point of extracting the planner is that these need no cache, no
//! adapter and no database: each case is one `(mutation, outcome, schema)`
//! triple and one expected plan.

use serde_json::json;

use super::*;
use crate::{
    runtime::cascade::MutationErrorClass,
    schema::{FieldDefinition, FieldType, MutationOperation, QueryDefinition, TypeDefinition},
};

fn schema_with(types: Vec<TypeDefinition>) -> CompiledSchema {
    CompiledSchema {
        types,
        queries: vec![QueryDefinition::new("users", "User").with_sql_source("v_user")],
        ..CompiledSchema::default()
    }
}

/// `User` on `v_user`, plus a payload type wrapping it on no view of its own.
fn user_schema() -> CompiledSchema {
    let mut payload = TypeDefinition::new("CreateUserSuccess", "");
    payload.fields = vec![FieldDefinition::new(
        "entity",
        FieldType::Object("User".to_string()),
    )];
    schema_with(vec![TypeDefinition::new("User", "v_user"), payload])
}

fn mutation_named(name: &str, return_type: &str) -> MutationDefinition {
    let mut m = MutationDefinition::new(name, return_type);
    m.sql_source = Some(format!("fn_{name}"));
    m
}

fn success(entity_type: Option<&str>, entity_id: Option<&str>) -> MutationOutcome {
    MutationOutcome::Success {
        entity:         json!({"id": "x"}),
        entity_type:    entity_type.map(ToString::to_string),
        entity_id:      entity_id.map(ToString::to_string),
        cascade:        None,
        updated_fields: vec![],
    }
}

fn view_names(plan: &InvalidationPlan) -> Vec<&str> {
    plan.views.iter().map(ViewName::as_str).collect()
}

// ---------------------------------------------------------------------------
// The kind of the mutation must not change which entries are swept
// ---------------------------------------------------------------------------

/// #741: a CREATE that stamps the new row's id used to take the entity-aware
/// branch and sweep nothing. The plan is identical whether `entity_id` is
/// present or not — the payload does not decide what is stale.
#[test]
fn entity_id_presence_does_not_change_the_view_sweep() {
    let schema = user_schema();
    let m = mutation_named("createUser", "User");

    let without = plan_invalidation(&m, &success(Some("User"), None), &schema);
    let with = plan_invalidation(&m, &success(Some("User"), Some("u-1")), &schema);

    assert_eq!(view_names(&without), ["v_user"]);
    assert_eq!(view_names(&with), ["v_user"], "#741: a CREATE carrying entity_id still sweeps");
    assert_eq!(without.entity, None);
    assert_eq!(with.entity, Some(("User".to_string(), "u-1".to_string())));
}

/// #763: nothing in the plan depends on the operation kind, so an UPDATE sweeps
/// the same views a CREATE does — including the lists the row newly matches.
#[test]
fn every_operation_kind_produces_the_same_view_sweep() {
    let schema = user_schema();
    let outcome = success(Some("User"), Some("u-1"));

    let kinds: [MutationOperation; 4] = [
        MutationOperation::Insert {
            table: "tb_user".to_string(),
        },
        MutationOperation::Update {
            table: "tb_user".to_string(),
        },
        MutationOperation::Delete {
            table: "tb_user".to_string(),
        },
        MutationOperation::Custom,
    ];
    for op in kinds {
        let mut m = mutation_named("mutateUser", "User");
        let kind = op.kind_str();
        m.operation = op;
        let plan = plan_invalidation(&m, &outcome, &schema);
        assert_eq!(view_names(&plan), ["v_user"], "kind '{kind}' must sweep v_user");
    }
}

// ---------------------------------------------------------------------------
// Where views come from
// ---------------------------------------------------------------------------

#[test]
fn declared_invalidates_views_come_first_and_are_all_kept() {
    let schema = user_schema();
    let mut m = mutation_named("createUser", "User");
    m.invalidates_views = vec!["v_audit".to_string(), "v_report".to_string()];

    let plan = plan_invalidation(&m, &success(Some("User"), None), &schema);
    assert_eq!(view_names(&plan), ["v_audit", "v_report", "v_user"]);
}

/// The return type is often an unbacked payload; the view then has to come from
/// the wrapped entity type or from the `entity_type` the function stamped.
#[test]
fn a_payload_return_type_resolves_through_the_entity_it_wraps() {
    let schema = user_schema();
    let m = mutation_named("createUser", "CreateUserSuccess");

    let plan = plan_invalidation(&m, &success(None, None), &schema);
    assert_eq!(view_names(&plan), ["v_user"], "payload's `entity` field names the backed type");
}

#[test]
fn the_stamped_entity_type_resolves_a_view_the_return_type_cannot() {
    // A payload with no `entity` field: only `mutation_response.entity_type` names User.
    let schema = schema_with(vec![
        TypeDefinition::new("User", "v_user"),
        TypeDefinition::new("OpaquePayload", ""),
    ]);
    let m = mutation_named("doThing", "OpaquePayload");

    let plan = plan_invalidation(&m, &success(Some("User"), Some("u-1")), &schema);
    assert_eq!(view_names(&plan), ["v_user"]);
}

#[test]
fn a_view_named_by_two_sources_is_swept_once() {
    let schema = user_schema();
    let mut m = mutation_named("createUser", "User");
    m.invalidates_views = vec!["v_user".to_string()];

    let plan = plan_invalidation(&m, &success(Some("User"), None), &schema);
    assert_eq!(view_names(&plan), ["v_user"]);
}

#[test]
fn cascade_side_effects_are_part_of_the_same_plan() {
    let schema = schema_with(vec![
        TypeDefinition::new("User", "v_user"),
        TypeDefinition::new("Post", "v_post"),
    ]);
    let mut m = mutation_named("updateUser", "User");
    m.cascade = true;

    let outcome = MutationOutcome::Success {
        entity:         json!({"id": "u-1"}),
        entity_type:    Some("User".to_string()),
        entity_id:      Some("u-1".to_string()),
        cascade:        Some(json!({"updated": [{"__typename": "Post", "id": "p-1"}]})),
        updated_fields: vec![],
    };

    let plan = plan_invalidation(&m, &outcome, &schema);
    assert_eq!(view_names(&plan), ["v_user", "v_post"]);
}

#[test]
fn cascade_entries_are_ignored_when_the_mutation_is_not_declared_cascade() {
    let schema = schema_with(vec![
        TypeDefinition::new("User", "v_user"),
        TypeDefinition::new("Post", "v_post"),
    ]);
    let m = mutation_named("updateUser", "User");

    let outcome = MutationOutcome::Success {
        entity:         json!({"id": "u-1"}),
        entity_type:    Some("User".to_string()),
        entity_id:      Some("u-1".to_string()),
        cascade:        Some(json!({"updated": [{"__typename": "Post", "id": "p-1"}]})),
        updated_fields: vec![],
    };

    assert_eq!(view_names(&plan_invalidation(&m, &outcome, &schema)), ["v_user"]);
}

// ---------------------------------------------------------------------------
// What must NOT be invalidated
// ---------------------------------------------------------------------------

#[test]
fn a_failed_mutation_invalidates_nothing() {
    let schema = user_schema();
    let mut m = mutation_named("createUser", "User");
    m.invalidates_views = vec!["v_audit".to_string()];

    let outcome = MutationOutcome::Error {
        error_class: MutationErrorClass::Conflict,
        message:     "already exists".to_string(),
        http_status: None,
        entity_type: None,
        metadata:    serde_json::Value::Null,
    };

    let plan = plan_invalidation(&m, &outcome, &schema);
    assert!(plan.views.is_empty(), "a mutation that wrote nothing invalidates nothing");
    assert_eq!(plan.entity, None);
}

#[test]
fn a_mutation_naming_no_resolvable_view_yields_only_the_entity_leg() {
    let schema = schema_with(vec![TypeDefinition::new("OpaquePayload", "")]);
    let m = mutation_named("doThing", "OpaquePayload");

    let plan = plan_invalidation(&m, &success(Some("Unknown"), Some("x-1")), &schema);
    assert!(plan.views.is_empty());
    assert_eq!(plan.entity, Some(("Unknown".to_string(), "x-1".to_string())));
}
