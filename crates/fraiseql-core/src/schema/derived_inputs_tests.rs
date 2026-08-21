//! Tests for the derived filter/sort input surface.

#![allow(clippy::panic)] // Reason: test code, panics are the failure mechanism

use super::*;
use crate::schema::{
    AutoParams, CompiledSchema, EnumDefinition, EnumValueDefinition, FieldDefinition, FieldType,
    InputFieldDefinition, InputObjectDefinition, QueryDefinition, TypeDefinition,
};

/// A schema exercising every shape the derivation has to decide about:
/// built-in scalars, an enum, an **unresolved** `Object` (the authoring layer's
/// Python type name), a list of scalars, a single relation, a list of relations,
/// and a cycle (`Order` → `Customer` → `Order`).
fn schema() -> CompiledSchema {
    let mut schema = CompiledSchema::default();

    let mut order = TypeDefinition::new("Order", "v_order");
    order.fields.push(FieldDefinition::new("id", FieldType::Id));
    order.fields.push(FieldDefinition::new("reference", FieldType::String));
    order.fields.push(FieldDefinition::new("total", FieldType::Int));
    order
        .fields
        .push(FieldDefinition::new("status", FieldType::Enum("OrderStatus".into())));
    // The authoring layer emits a Python type name where a scalar belongs; no
    // type declares `datetime`, so it is an opaque leaf, not a relation.
    order
        .fields
        .push(FieldDefinition::new("placedAt", FieldType::Object("datetime".into())));
    order
        .fields
        .push(FieldDefinition::new("tags", FieldType::List(Box::new(FieldType::String))));
    order
        .fields
        .push(FieldDefinition::new("customer", FieldType::Object("Customer".into())));
    order.fields.push(FieldDefinition::new(
        "lines",
        FieldType::List(Box::new(FieldType::Object("OrderLine".into()))),
    ));
    schema.types.push(order);

    let mut customer = TypeDefinition::new("Customer", "v_customer");
    customer.fields.push(FieldDefinition::new("id", FieldType::Id));
    customer.fields.push(FieldDefinition::new("name", FieldType::String));
    // Closes the cycle.
    customer.fields.push(FieldDefinition::new(
        "orders",
        FieldType::List(Box::new(FieldType::Object("Order".into()))),
    ));
    customer
        .fields
        .push(FieldDefinition::new("primaryOrder", FieldType::Object("Order".into())));
    schema.types.push(customer);

    let mut line = TypeDefinition::new("OrderLine", "v_order_line");
    line.fields.push(FieldDefinition::new("sku", FieldType::String));
    schema.types.push(line);

    schema
        .enums
        .push(EnumDefinition::new("OrderStatus").with_value(EnumValueDefinition::new("OPEN")));

    let mut orders = QueryDefinition::new("orders", "Order");
    orders.returns_list = true;
    orders.auto_params = AutoParams::all();
    schema.queries.push(orders);

    schema
}

fn derived(schema: &CompiledSchema) -> DerivedInputs {
    derive(schema)
}

fn input<'a>(d: &'a DerivedInputs, name: &str) -> &'a InputObjectDefinition {
    d.input_types
        .iter()
        .find(|i| i.name == name)
        .unwrap_or_else(|| panic!("`{name}` not derived; got {:?}", names(d)))
}

fn names(d: &DerivedInputs) -> Vec<&str> {
    d.input_types.iter().map(|i| i.name.as_str()).collect()
}

fn field<'a>(i: &'a InputObjectDefinition, name: &str) -> &'a InputFieldDefinition {
    i.fields.iter().find(|f| f.name == name).unwrap_or_else(|| {
        panic!(
            "`{name}` absent from `{}`; got {:?}",
            i.name,
            i.fields.iter().map(|f| f.name.as_str()).collect::<Vec<_>>()
        )
    })
}

// ---- Cycle 1: the shape of a derived `{Entity}WhereInput` ----

#[test]
fn a_where_enabled_query_derives_a_where_input_for_its_return_type() {
    let d = derived(&schema());
    assert!(names(&d).contains(&"OrderWhereInput"), "got {:?}", names(&d));
}

#[test]
fn a_schema_with_no_auto_params_derives_nothing() {
    let mut schema = schema();
    schema.queries[0].auto_params = AutoParams::none();
    let d = derived(&schema);
    assert!(
        d.input_types.is_empty(),
        "nothing to filter, so nothing to derive: {:?}",
        names(&d)
    );
    assert!(d.enums.is_empty(), "no sort surface, so no SortDirection");
}

#[test]
fn the_where_input_carries_one_field_per_declared_field() {
    let d = derived(&schema());
    let order = input(&d, "OrderWhereInput");
    for declared in [
        "id",
        "reference",
        "total",
        "status",
        "placedAt",
        "tags",
        "customer",
        "lines",
    ] {
        field(order, declared);
    }
}

/// The combinator spelling is the engine's, not v1's. `parse_where_object`
/// matches `_and`/`_or`/`_not` and nothing else, so publishing v1's
/// `AND`/`OR`/`NOT` would advertise three fields that cannot execute.
#[test]
fn the_where_input_carries_exactly_the_combinators_the_engine_implements() {
    let d = derived(&schema());
    let order = input(&d, "OrderWhereInput");
    for spelling in ["_and", "_or", "_not"] {
        field(order, spelling);
    }
    for v1_spelling in ["AND", "OR", "NOT"] {
        assert!(
            !order.fields.iter().any(|f| f.name == v1_spelling),
            "`{v1_spelling}` is v1's spelling; the v2 engine cannot execute it"
        );
    }
}

#[test]
fn the_conjunction_combinators_take_a_list_and_negation_takes_one_predicate() {
    let d = derived(&schema());
    let order = input(&d, "OrderWhereInput");
    assert_eq!(field(order, "_and").field_type, "[OrderWhereInput!]");
    assert_eq!(field(order, "_or").field_type, "[OrderWhereInput!]");
    // v1 left `NOT` as `JSON` while typing `AND`/`OR`; an untyped hole in a
    // typed surface is where the next silent wrong answer lives.
    assert_eq!(field(order, "_not").field_type, "OrderWhereInput");
}

#[test]
fn a_relation_field_resolves_to_the_related_where_input() {
    let d = derived(&schema());
    assert_eq!(field(input(&d, "OrderWhereInput"), "customer").field_type, "CustomerWhereInput");
}

/// The closure is what keeps the surface from dangling: naming
/// `CustomerWhereInput` obliges the derivation to define it, even though no
/// query returns `Customer`.
#[test]
fn a_related_entity_gets_its_own_where_input_even_with_no_query_of_its_own() {
    let d = derived(&schema());
    let customer = input(&d, "CustomerWhereInput");
    field(customer, "name");
    field(customer, "primaryOrder");
}

#[test]
fn a_cycle_between_two_entities_terminates() {
    let d = derived(&schema());
    assert_eq!(
        field(input(&d, "CustomerWhereInput"), "primaryOrder").field_type,
        "OrderWhereInput",
        "the cycle must close on the type already derived, not recurse"
    );
}

/// `Object("datetime")` names no declared type. The runtime already treats it
/// as text (`scalar_cast_hint` falls through to `Text`), so an opaque-leaf
/// filter is what it *is*; resolving it as a relation would name a
/// `datetimeWhereInput` that nothing backs.
#[test]
fn an_unresolved_object_type_is_an_opaque_leaf_not_a_relation() {
    let d = derived(&schema());
    assert_eq!(field(input(&d, "OrderWhereInput"), "placedAt").field_type, "datetimeFilter");
    assert!(
        !names(&d).contains(&"datetimeWhereInput"),
        "an undeclared type is not an entity: {:?}",
        names(&d)
    );
    // And its operand is the leaf name the schema itself uses, not a guess.
    assert_eq!(field(input(&d, "datetimeFilter"), "eq").field_type, "datetime");
}

/// The engine lowers `{lines: {sku: {eq: …}}}` to `data->'lines'->>'sku'`,
/// which cannot index into an array and so matches nothing, silently.
/// Publishing the nested form would advertise exactly that silent miss.
#[test]
fn a_list_of_relations_gets_a_list_filter_not_a_nested_where_input() {
    let d = derived(&schema());
    assert_eq!(field(input(&d, "OrderWhereInput"), "lines").field_type, "OrderLineListFilter");
    assert!(
        !names(&d).contains(&"OrderLineWhereInput"),
        "a list element is not reachable as a relation: {:?}",
        names(&d)
    );
}

#[test]
fn a_list_of_scalars_gets_a_list_filter() {
    let d = derived(&schema());
    assert_eq!(field(input(&d, "OrderWhereInput"), "tags").field_type, "StringListFilter");
    let tags = input(&d, "StringListFilter");
    assert_eq!(field(tags, "eq").field_type, "[String!]");
    field(tags, "array_contains");
    field(tags, "len_eq");
}

#[test]
fn an_enum_field_filters_by_the_enum_type() {
    let d = derived(&schema());
    assert_eq!(field(input(&d, "OrderWhereInput"), "status").field_type, "OrderStatusFilter");
    let status = input(&d, "OrderStatusFilter");
    assert_eq!(field(status, "eq").field_type, "OrderStatus");
    assert_eq!(field(status, "in").field_type, "[OrderStatus!]");
    assert!(
        !status.fields.iter().any(|f| f.name == "contains"),
        "LIKE on an enum is not a filter anyone wants advertised"
    );
}

#[test]
fn the_scalar_filters_carry_operand_types_that_follow_the_operator() {
    let d = derived(&schema());
    let s = input(&d, "StringFilter");
    assert_eq!(field(s, "eq").field_type, "String");
    assert_eq!(field(s, "in").field_type, "[String!]");
    assert_eq!(field(s, "isnull").field_type, "Boolean");
    assert_eq!(field(s, "contains").field_type, "String");

    let i = input(&d, "IntFilter");
    assert_eq!(field(i, "eq").field_type, "Int");
    assert_eq!(field(i, "gte").field_type, "Int");
    assert!(
        !i.fields.iter().any(|f| f.name == "icontains"),
        "ILIKE on an Int is not a filter anyone wants advertised"
    );
}

/// Canonical names only. `ne`, `notin`, `is_null` and the rest keep working at
/// runtime — advertising both spellings would double the surface to say the
/// same thing twice.
#[test]
fn only_canonical_operator_names_are_advertised() {
    let d = derived(&schema());
    let s = input(&d, "StringFilter");
    field(s, "neq");
    assert!(!s.fields.iter().any(|f| f.name == "ne"), "`ne` is an alias of `neq`");
    assert!(!s.fields.iter().any(|f| f.name == "notin"), "`notin` is an alias of `nin`");
}

// ---- Cycle 1b: the sort surface ----

#[test]
fn an_order_by_enabled_query_derives_an_order_by_input_and_the_direction_enum() {
    let d = derived(&schema());
    let order_by = input(&d, "OrderOrderByInput");
    // `field` is `String!` on purpose: `enrich_order_by_clauses` accepts a
    // declared field *or* a native column, and a native column need not be a
    // declared field. An enum would advertise a narrower surface than the
    // engine's on exactly the queries compiled with `--database`.
    assert_eq!(field(order_by, "field").field_type, "String");
    assert!(!field(order_by, "field").nullable, "a sort key is required");
    assert_eq!(field(order_by, "direction").field_type, "SortDirection");

    let direction = d
        .enums
        .iter()
        .find(|e| e.name == "SortDirection")
        .expect("the direction enum is part of the sort surface");
    let values: Vec<&str> = direction.values.iter().map(|v| v.name.as_str()).collect();
    assert_eq!(values, ["ASC", "DESC"]);
}

/// `orderBy` never nests, so the sort surface follows queries, not the relation
/// closure. `Customer` is reachable as a relation but no query sorts it.
#[test]
fn order_by_inputs_follow_queries_not_the_relation_closure() {
    let d = derived(&schema());
    assert!(
        !names(&d).contains(&"CustomerOrderByInput"),
        "no query sorts Customer: {:?}",
        names(&d)
    );
}

#[test]
fn a_query_that_filters_but_does_not_sort_derives_no_order_by_input() {
    let mut schema = schema();
    schema.queries[0].auto_params = AutoParams {
        has_where:    true,
        has_order_by: false,
        has_limit:    false,
        has_offset:   false,
    };
    let d = derived(&schema);
    assert!(names(&d).contains(&"OrderWhereInput"));
    assert!(!names(&d).contains(&"OrderOrderByInput"), "got {:?}", names(&d));
    assert!(d.enums.is_empty(), "no sort surface, so no SortDirection");
}

// ---- Cycle 1c: an author's declaration always wins ----

#[test]
fn an_author_declared_name_is_not_derived_over() {
    let mut schema = schema();
    schema.input_types.push(
        InputObjectDefinition::new("OrderWhereInput")
            .with_field(InputFieldDefinition::new("mine", "String")),
    );
    let d = derived(&schema);
    assert!(
        !names(&d).contains(&"OrderWhereInput"),
        "the author declared it; deriving over it would silently replace their surface"
    );
    // The rest of the closure is still derived.
    assert!(names(&d).contains(&"CustomerWhereInput"), "got {:?}", names(&d));
}

// ---- Cycle 2: the contract with the engine ----

/// #869's guard, at the seam where the generation now lives: the compiler must
/// not advertise a WHERE operator the runtime cannot serve. Every field of
/// every derived `*Filter` is an operator name, so every one of them must parse.
#[test]
fn every_advertised_operator_parses_as_a_where_operator() {
    let d = derived(&schema());
    let mut checked = 0_usize;
    for input_type in &d.input_types {
        if !input_type.name.ends_with("Filter") {
            continue;
        }
        for f in &input_type.fields {
            assert!(
                fraiseql_db::where_clause::WhereOperator::from_str(&f.name).is_ok(),
                "`{}` advertises operator `{}`, which WhereOperator::from_str cannot parse — \
                 the request would fail at runtime with 'Unknown WHERE operator'",
                input_type.name,
                f.name
            );
            checked += 1;
        }
    }
    assert!(checked > 0, "the walk found no operator fields, so it proved nothing");
}

/// The entity filter types are the other family: every field is either a
/// declared field of that entity or one of the three combinators. A stray
/// operator name here would mean an entity filter had been generated with a
/// scalar filter's body.
#[test]
fn every_entity_filter_field_is_a_declared_field_or_a_combinator() {
    let schema = schema();
    let d = derived(&schema);
    for input_type in &d.input_types {
        let Some(entity) = input_type.name.strip_suffix("WhereInput") else {
            continue;
        };
        let type_def = schema.find_type(entity).expect("an entity filter names a declared type");
        for f in &input_type.fields {
            assert!(
                matches!(f.name.as_str(), "_and" | "_or" | "_not")
                    || type_def.find_field(&f.name).is_some(),
                "`{}` advertises `{}`, which is neither a combinator nor a field of `{entity}`",
                input_type.name,
                f.name
            );
        }
    }
}

/// Both sides enumerated, neither hand-copied: the operator set a filter
/// advertises is exactly what the bucket table says, and every bucketed name is
/// a name in `WHERE_OPERATORS`.
#[test]
fn the_advertised_operator_set_is_exactly_the_bucketed_set() {
    let d = derived(&schema());
    let s = input(&d, "StringFilter");
    let advertised: Vec<&str> = s.fields.iter().map(|f| f.name.as_str()).collect();
    let expected: Vec<&str> = COMPARISON_OPERATORS
        .iter()
        .chain(NULL_OPERATORS)
        .chain(STRING_OPERATORS)
        .copied()
        .collect();
    assert_eq!(advertised, expected);

    for name in COMPARISON_OPERATORS
        .iter()
        .chain(NULL_OPERATORS)
        .chain(STRING_OPERATORS)
        .chain(ARRAY_OPERATORS)
        .chain(CONTAINMENT_OPERATORS)
        .chain(VECTOR_OPERATORS)
    {
        assert!(
            fraiseql_db::where_clause::WHERE_OPERATORS.iter().any(|spec| spec.name == *name),
            "`{name}` is bucketed but is not a canonical name in WHERE_OPERATORS"
        );
    }
}

// ---- Cycle 3: the published surface and the runtime answer the same question ----

/// A field gets a nested filter in the published type **iff** the runtime treats
/// a nested predicate on it as legitimate. Answering that twice is how a schema
/// ends up advertising a filter the engine refuses, or accepting one the schema
/// says is not there — so both read [`nested_filter_entity`].
#[test]
fn the_published_nesting_and_the_runtime_relation_flag_agree_field_by_field() {
    let schema = schema();
    let d = derived(&schema);
    let order = input(&d, "OrderWhereInput");
    let keys = where_keys_of(&schema, schema.find_type("Order").expect("Order is declared"));

    let mut checked = 0_usize;
    for f in &order.fields {
        if matches!(f.name.as_str(), "_and" | "_or" | "_not") {
            continue;
        }
        let info = keys
            .get(&crate::utils::to_snake_case(&f.name))
            .unwrap_or_else(|| panic!("`{}` published but absent from the runtime keys", f.name));
        let published_as_relation = f.field_type.ends_with("WhereInput");
        assert_eq!(
            published_as_relation,
            info.is_relation,
            "`{}` publishes `{}` but the runtime calls it {}a relation",
            f.name,
            f.field_type,
            if info.is_relation { "" } else { "not " }
        );
        if published_as_relation {
            assert_eq!(
                info.relation_type.as_deref().map(where_input_type_name),
                Some(f.field_type.clone()),
                "`{}` must descend into the type it publishes",
                f.name
            );
        }
        checked += 1;
    }
    assert!(checked > 0, "the walk compared nothing, so it proved nothing");
}

/// The two shapes that are *not* relations, stated directly: a list of entities
/// (the engine's JSON path cannot index into an array) and an `Object` no type
/// declares (an opaque leaf).
#[test]
fn a_list_of_entities_and_an_unresolved_object_are_not_relations() {
    let schema = schema();
    let keys = where_keys_of(&schema, schema.find_type("Order").expect("Order is declared"));

    assert!(
        !keys["lines"].is_relation,
        "a list of entities takes a list filter, not a predicate"
    );
    assert!(!keys["placed_at"].is_relation, "`datetime` is declared by no type");
    assert!(keys["customer"].is_relation, "a single declared relation still is one");
    assert_eq!(keys["customer"].relation_type.as_deref(), Some("Customer"));
}

#[test]
fn the_relation_field_maps_cover_every_type_that_declares_fields() {
    let schema = schema();
    let maps = relation_field_maps(&schema);
    for name in ["Order", "Customer", "OrderLine"] {
        assert!(maps.contains_key(name), "`{name}` declares fields, so its keys must be carried");
    }
    assert_eq!(
        maps["Customer"].get("primary_order").and_then(|i| i.relation_type.as_deref()),
        Some("Order"),
        "the map must carry the target type, or a nested level cannot be resolved"
    );
}
