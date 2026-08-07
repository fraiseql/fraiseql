//! Tests for the static half of mutation invalidation resolution (#910).

use super::*;
use crate::schema::{FieldDefinition, FieldType, MutationDefinition, TypeDefinition};

/// A type backed by `view`, with the given fields.
fn typed(name: &str, view: &str, fields: &[(&str, &str)]) -> TypeDefinition {
    let mut t = TypeDefinition::new(name, view);
    t.fields = fields
        .iter()
        .map(|(fname, ftype)| FieldDefinition::new(*fname, FieldType::parse(ftype)))
        .collect();
    t
}

fn mutation(name: &str, return_type: &str) -> MutationDefinition {
    MutationDefinition::new(name, return_type)
}

#[test]
fn a_return_type_backed_by_a_view_resolves() {
    let mut schema = CompiledSchema::new();
    schema.types.push(typed("User", "v_user", &[("id", "ID")]));

    let views = statically_resolved_views(&mutation("createUser", "User"), &schema);
    assert_eq!(views.iter().map(ViewName::as_str).collect::<Vec<_>>(), vec!["v_user"]);
}

#[test]
fn a_payload_resolves_through_its_entity_field() {
    let mut schema = CompiledSchema::new();
    schema.types.push(typed("User", "v_user", &[("id", "ID")]));
    schema.types.push(typed("CreateUserSuccess", "", &[("entity", "User")]));

    let views = statically_resolved_views(&mutation("createUser", "CreateUserSuccess"), &schema);
    assert_eq!(
        views.iter().map(ViewName::as_str).collect::<Vec<_>>(),
        vec!["v_user"],
        "an unbacked payload still resolves through the entity it wraps"
    );
}

#[test]
fn a_declaration_resolves_even_when_nothing_else_does() {
    let mut schema = CompiledSchema::new();
    schema.types.push(typed("RebuildResult", "", &[("rows", "Int")]));
    let mut m = mutation("rebuildPricing", "RebuildResult");
    m.invalidates_views = vec!["v_price".to_string()];

    let views = statically_resolved_views(&m, &schema);
    assert_eq!(views.iter().map(ViewName::as_str).collect::<Vec<_>>(), vec!["v_price"]);
}

/// The #910 shape: a payload with no `sql_source`, no `entity` field, and no
/// `invalidates_views`. Nothing in the schema says what the function wrote.
#[test]
fn an_unbacked_payload_with_no_declaration_resolves_to_nothing() {
    let mut schema = CompiledSchema::new();
    schema.types.push(typed("RebuildResult", "", &[("rows", "Int")]));
    schema.mutations.push(mutation("rebuildPricing", "RebuildResult"));

    assert!(
        statically_resolved_views(&schema.mutations[0], &schema).is_empty(),
        "the engine cannot know what a custom mutation wrote"
    );
    assert_eq!(unattributable_mutations(&schema), vec!["rebuildPricing"]);
}

#[test]
fn declares_cacheable_views_needs_both_a_source_and_a_ttl() {
    let mut schema = CompiledSchema::new();
    assert!(!declares_cacheable_views(&schema), "an empty schema caches nothing");

    let mut q = crate::schema::QueryDefinition::new("prices", "Price");
    q.sql_source = Some("v_price".to_string());
    schema.queries.push(q);
    assert!(
        !declares_cacheable_views(&schema),
        "without cache_ttl_seconds the adapter's opt-in mode bypasses the view"
    );

    schema.queries[0].cache_ttl_seconds = Some(0);
    assert!(
        declares_cacheable_views(&schema),
        "ttl = 0 is the mutation-invalidated-only annotation — the case #910 is about"
    );
}
