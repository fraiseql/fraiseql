#![allow(clippy::unwrap_used, clippy::panic)] // Reason: test code, panics are acceptable

//! #1156: `__schema` must not advertise a type name that resolves to nothing.
//!
//! A field typed `Object(T)` where no type declares `T` introspected as an
//! `OBJECT` reference to an undefined type, so a client that walked
//! `__schema.types` looking for `T` found no node at all. That is not a rare
//! authoring accident: an authoring layer that puts a host-language type name
//! where a scalar belongs (`datetime`, `date`, `dict`) produces one per
//! occurrence, and every one of them is a dangling reference.
//!
//! The SDL surface already handles this — `referenced_scalars()` collects every
//! leaf name that is neither a built-in nor a defined composite and declares
//! `scalar Name`, so the federation SDL is type-complete. The property under
//! test is that the **two surfaces agree**, which is why the central assertion
//! compares them rather than checking either alone.

use std::collections::BTreeSet;

use fraiseql_core::schema::{CompiledSchema, IntrospectionBuilder, TypeKind};
use serde_json::json;

/// A compiled schema whose `Order` type references three leaf names that no type
/// defines — the shape an authoring layer emits when it writes a host-language
/// type name into a field position.
fn schema_with_dangling_leaves() -> CompiledSchema {
    let raw = json!({
        "types": [{
            "name": "Order",
            "sql_source": "v_order",
            "fields": [
                {"name": "id", "field_type": "ID", "nullable": false},
                {"name": "placedAt", "field_type": {"Object": "datetime"}, "nullable": true},
                {"name": "dueOn", "field_type": {"Object": "date"}, "nullable": true},
                {"name": "metadata", "field_type": {"Object": "dict"}, "nullable": true},
                {"name": "customer", "field_type": {"Object": "Customer"}, "nullable": true}
            ]
        }, {
            "name": "Customer",
            "sql_source": "v_customer",
            "fields": [{"name": "id", "field_type": "ID", "nullable": false}]
        }],
        "queries": [{
            "name": "orders",
            "return_type": "Order",
            "returns_list": true,
            "nullable": false,
            "sql_source": "v_order",
            "arguments": []
        }]
    });

    CompiledSchema::from_json(&raw.to_string(), false).expect("fixture compiles")
}

/// The names `__schema.types` publishes as `SCALAR`.
fn introspected_scalars(schema: &CompiledSchema) -> BTreeSet<String> {
    IntrospectionBuilder::build(schema)
        .types
        .iter()
        .filter(|t| t.kind == TypeKind::Scalar)
        .filter_map(|t| t.name.clone())
        .collect()
}

/// The names the SDL declares with `scalar Name`.
fn sdl_scalars(schema: &CompiledSchema) -> BTreeSet<String> {
    schema
        .raw_schema()
        .lines()
        .filter_map(|line| line.trim().strip_prefix("scalar ").map(str::trim))
        .map(str::to_string)
        .collect()
}

/// Every name `__schema` mentions, whatever its kind.
fn introspected_names(schema: &CompiledSchema) -> Vec<String> {
    IntrospectionBuilder::build(schema)
        .types
        .iter()
        .filter_map(|t| t.name.clone())
        .collect()
}

/// The issue's own repro: a leaf name nothing defines must still be lookupable.
#[test]
fn a_referenced_but_undefined_leaf_publishes_a_scalar_node() {
    let schema = schema_with_dangling_leaves();
    let scalars = introspected_scalars(&schema);

    for name in ["datetime", "date", "dict"] {
        assert!(
            scalars.contains(name),
            "#1156: __schema references '{name}' from Order but publishes no node for it, so \
             an introspecting client cannot resolve the name it was just handed. Published \
             scalars: {scalars:?}"
        );
    }
}

/// The property the fix exists to establish: the two surfaces agree by
/// construction, both being driven by `referenced_scalars()`.
///
/// Asserting only that `datetime` appears would let the surfaces drift apart
/// again the next time one of them gains a name.
#[test]
fn introspection_publishes_every_scalar_the_sdl_declares() {
    let schema = schema_with_dangling_leaves();

    let missing: Vec<String> = sdl_scalars(&schema)
        .difference(&introspected_scalars(&schema))
        .cloned()
        .collect();

    assert!(
        missing.is_empty(),
        "#1156: the SDL declares {missing:?} as scalars and introspection publishes no node \
         for them, so the two surfaces disagree about what the schema contains"
    );
}

/// Counterweight: a name the schema *does* define must not also be published as
/// a scalar. Without this, "declare everything referenced" would pass the test
/// above while turning every object type into a duplicate scalar.
#[test]
fn a_defined_composite_is_not_also_published_as_a_scalar() {
    let schema = schema_with_dangling_leaves();
    let scalars = introspected_scalars(&schema);

    for name in ["Order", "Customer", "Query"] {
        assert!(
            !scalars.contains(name),
            "'{name}' is a defined composite type and must not also appear as a SCALAR"
        );
    }
}

/// A duplicate name in `__schema.types` is invalid GraphQL and breaks
/// `build_type_map`, which is a last-write-wins `HashMap`. This is the failure
/// mode a naive "append every referenced name" fix produces for the built-in
/// scalars, which `referenced_scalars()` does not exclude — it excludes only the
/// five GraphQL built-ins, while introspection publishes eleven.
#[test]
fn no_type_name_is_published_twice() {
    let schema = schema_with_dangling_leaves();
    let names = introspected_names(&schema);

    let mut seen = BTreeSet::new();
    let duplicates: Vec<&String> = names.iter().filter(|n| !seen.insert((*n).clone())).collect();

    assert!(
        duplicates.is_empty(),
        "__schema.types publishes duplicate names: {duplicates:?}"
    );
}

/// The same check against a schema that references the rich built-in scalars by
/// their canonical names. `DateTime`/`UUID`/`JSON` are published by
/// `builtin_scalars()` but are *not* in `referenced_scalars()`'s five-name
/// built-in exclusion list, so this is where a duplicate would appear.
#[test]
fn referencing_a_rich_builtin_scalar_does_not_duplicate_its_node() {
    let raw = json!({
        "types": [{
            "name": "Event",
            "sql_source": "v_event",
            "fields": [
                {"name": "id", "field_type": "ID", "nullable": false},
                {"name": "at", "field_type": "DateTime", "nullable": false},
                {"name": "ref", "field_type": "UUID", "nullable": false},
                {"name": "payload", "field_type": "Json", "nullable": true}
            ]
        }],
        "queries": [{
            "name": "events",
            "return_type": "Event",
            "returns_list": true,
            "nullable": false,
            "sql_source": "v_event",
            "arguments": []
        }]
    });
    let schema = CompiledSchema::from_json(&raw.to_string(), false).expect("fixture compiles");

    let names = introspected_names(&schema);
    for builtin in ["DateTime", "UUID", "JSON"] {
        assert_eq!(
            names.iter().filter(|n| n.as_str() == builtin).count(),
            1,
            "'{builtin}' must be published exactly once; published: {names:?}"
        );
    }
}
