//! Unit tests for § 5.3.1 field existence and § 5.3.3 leaf field selections.

#![allow(clippy::unwrap_used, clippy::panic)] // Reason: test code, panics acceptable

use super::*;
use crate::schema::{FieldDefinition, FieldType, TypeDefinition, UnionDefinition};

fn sel(name: &str) -> FieldSelection {
    FieldSelection {
        name:          name.to_string(),
        alias:         None,
        arguments:     vec![],
        nested_fields: vec![],
        directives:    vec![],
    }
}

fn nested(name: &str, children: Vec<FieldSelection>) -> FieldSelection {
    FieldSelection {
        nested_fields: children,
        ..sel(name)
    }
}

/// `User { id, name, status: Status(enum), profile: Profile, tags: [Tag],
/// blob: Json }` over `Profile { nickname }`, plus an empty-field-list type and
/// a union — one schema carrying every branch the two predicates can take.
fn schema() -> CompiledSchema {
    let mut s = CompiledSchema::new();

    let mut user = TypeDefinition::new("User", "v_user");
    user.fields = vec![
        FieldDefinition::new("id", FieldType::Id),
        FieldDefinition::nullable("name", FieldType::String),
        FieldDefinition::nullable("status", FieldType::Enum("Status".into())),
        FieldDefinition::nullable("profile", FieldType::Object("Profile".into())),
        FieldDefinition::nullable(
            "tags",
            FieldType::List(Box::new(FieldType::Object("Tag".into()))),
        ),
        FieldDefinition::nullable("blob", FieldType::Json),
        FieldDefinition::nullable("ghost", FieldType::Object("NotInSchema".into())),
        FieldDefinition::nullable("shell", FieldType::Object("Fieldless".into())),
    ];
    s.types.push(user);

    let mut profile = TypeDefinition::new("Profile", "v_profile");
    profile.fields = vec![FieldDefinition::nullable("nickname", FieldType::String)];
    s.types.push(profile);

    let mut tag = TypeDefinition::new("Tag", "v_tag");
    tag.fields = vec![FieldDefinition::new("id", FieldType::Id)];
    s.types.push(tag);

    // An object whose field list the compiler did not emit.
    s.types.push(TypeDefinition::new("Fieldless", "v_fieldless"));

    s.unions.push(UnionDefinition {
        name:         "Result".into(),
        member_types: vec!["User".into()],
        description:  None,
    });

    s.build_indexes();
    s
}

// ── § 5.3.3 at the root ──────────────────────────────────────────────────

#[test]
fn a_root_object_with_no_selection_set_is_refused() {
    let err = validate_leaf_field_selections(&schema(), "User", &[])
        .expect_err("a composite root named with no selection set is invalid");
    assert!(err.to_string().contains("User"), "must name the type: {err}");
}

#[test]
fn a_root_union_with_no_selection_set_is_refused() {
    let err = validate_leaf_field_selections(&schema(), "Result", &[])
        .expect_err("a union is composite, so § 5.3.3 applies to it too");
    assert!(err.to_string().contains("Result"), "must name the union: {err}");
}

/// The module's governing rule: a rejection it cannot justify would break a
/// working query. A type the compiled schema does not carry is not evidence of
/// a defect.
#[test]
fn a_root_type_the_schema_does_not_carry_still_passes() {
    validate_leaf_field_selections(&schema(), "NotInSchema", &[])
        .expect("an unknown type must stay a pass");
}

/// An object whose field list the compiler did not emit is the same unknown:
/// an object type must have at least one field, so an empty list means missing
/// information rather than a type with no fields.
#[test]
fn a_root_type_with_no_field_list_still_passes() {
    validate_leaf_field_selections(&schema(), "Fieldless", &[])
        .expect("a type with no emitted field list must stay a pass");
}

// ── § 5.3.3 one level down ───────────────────────────────────────────────

#[test]
fn a_nested_object_field_with_no_selection_set_is_refused() {
    let err = validate_leaf_field_selections(&schema(), "User", &[sel("id"), sel("profile")])
        .expect_err("`{ id profile }` names a composite field with no selection set");
    assert!(err.to_string().contains("profile"), "must name the field: {err}");
    assert!(err.to_string().contains("Profile"), "must name its type: {err}");
}

/// The list wrapper is unwrapped, so `[Tag]` is adjudicated as `Tag`.
#[test]
fn a_nested_list_of_objects_with_no_selection_set_is_refused() {
    let err = validate_leaf_field_selections(&schema(), "User", &[sel("tags")])
        .expect_err("a list of composites needs a selection set on its element type");
    assert!(err.to_string().contains("tags"), "must name the field: {err}");
}

#[test]
fn a_nested_object_field_with_a_selection_set_is_accepted() {
    validate_leaf_field_selections(
        &schema(),
        "User",
        &[sel("id"), nested("profile", vec![sel("nickname")])],
    )
    .expect("a composite field with a selection set is exactly right");
}

// ── Leaf types keep their bare form ──────────────────────────────────────

/// An enum is a leaf type: `status` with no sub-selection is correct, and
/// refusing it would reject a working document.
///
/// ⚠ This pins the **behaviour**, not `composite_type_of`'s enum arm. Admitting
/// `FieldType::Enum` there leaves this green, because enums live in
/// `schema.enums` and `requires_selection_set` consults `find_type`/`find_union`
/// — so it answers `false` for an enum by either route. That is an equivalent
/// mutant rather than a coverage gap, and it is recorded here so a later reader
/// measuring this file does not read the surviving mutant as an untested guard.
#[test]
fn a_nested_enum_field_with_no_selection_set_is_accepted() {
    validate_leaf_field_selections(&schema(), "User", &[sel("status")])
        .expect("an enum is a leaf type");
}

#[test]
fn nested_scalar_fields_with_no_selection_set_are_accepted() {
    validate_leaf_field_selections(&schema(), "User", &[sel("id"), sel("name"), sel("blob")])
        .expect("scalars are leaf types");
}

/// A composite field whose type the schema does not carry: unknown, so a pass.
#[test]
fn a_nested_field_of_an_unknown_composite_type_still_passes() {
    validate_leaf_field_selections(&schema(), "User", &[sel("ghost")])
        .expect("an unknown child type must stay a pass");
}

#[test]
fn a_nested_field_of_a_type_with_no_field_list_still_passes() {
    validate_leaf_field_selections(&schema(), "User", &[sel("shell")])
        .expect("a child type with no emitted field list must stay a pass");
}

/// The unknown-composite pass must skip **that field**, not the rest of the set.
///
/// `ghost` is a composite the schema does not carry, so § 5.3.3 lets it through;
/// `profile` two slots later is a real violation. Written as an early `return
/// Ok(())` rather than a `continue`, the pass on `ghost` would end the whole walk
/// and `profile` would never be scored.
#[test]
fn an_unknown_composite_does_not_excuse_the_fields_after_it() {
    let err = validate_leaf_field_selections(
        &schema(),
        "User",
        &[sel("ghost"), sel("id"), sel("profile")],
    )
    .expect_err("the violation after the unknown must still be found");
    assert!(err.to_string().contains("profile"), "must name the later field: {err}");
}

// ── § 5.3.1 is unchanged ─────────────────────────────────────────────────

#[test]
fn an_undeclared_field_is_still_refused() {
    let err = validate_selection_set(&schema(), "User", &[sel("emial")])
        .expect_err("#939's rule must survive the § 5.3.3 change");
    assert!(err.to_string().contains("emial"), "must name the field: {err}");
}

#[test]
fn meta_fields_are_still_accepted() {
    validate_selection_set(&schema(), "User", &[sel("__typename")])
        .expect("`__typename` is valid on every selection set");
}
