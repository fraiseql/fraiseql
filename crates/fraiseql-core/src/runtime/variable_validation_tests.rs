//! Tests for [`variable_validation`](super).

use super::*;
use crate::graphql::parse_query;

fn references(source: &str) -> Vec<String> {
    let parsed = parse_query(source).expect("query parses");
    collect_variable_references(&parsed).expect("walk succeeds")
}

/// [`references`] for a document with more than one operation, where the
/// caller must say which one is executing (GraphQL § 6.1).
fn references_of(source: &str, operation_name: &str) -> Vec<String> {
    let parsed = crate::graphql::parse_query_with_operation_name(source, Some(operation_name))
        .expect("query parses");
    collect_variable_references(&parsed).expect("walk succeeds")
}

fn defined(source: &str) -> Vec<String> {
    parse_query(source)
        .expect("query parses")
        .variables
        .into_iter()
        .map(|v| v.name)
        .collect()
}

// ---- Cycle 1: the reference walker, one test per reference site ----

#[test]
fn a_whole_argument_variable_is_a_reference() {
    assert_eq!(references("query Q { orders(offset: $off) { reference } }"), ["off"]);
}

#[test]
fn a_variable_nested_in_an_object_is_a_reference() {
    assert_eq!(references("query Q { orders(where: {status: {eq: $s}}) { reference } }"), ["s"]);
}

#[test]
fn a_variable_nested_in_a_list_is_a_reference() {
    assert_eq!(references("query Q { orders(where: {_or: [{a: $x}]}) { reference } }"), ["x"]);
}

#[test]
fn a_variable_in_a_mutation_input_object_is_a_reference() {
    assert_eq!(references("mutation M { createOrder(input: {name: $n}) { id } }"), ["n"]);
}

#[test]
fn a_directive_argument_variable_is_a_reference() {
    assert_eq!(references("query Q { orders { reference @skip(if: $hide) } }"), ["hide"]);
}

#[test]
fn an_include_directive_variable_is_a_reference() {
    assert_eq!(references("query Q { orders { reference @include(if: $show) } }"), ["show"]);
}

#[test]
fn a_stream_directive_takes_more_than_one_variable() {
    assert_eq!(
        references("query Q { orders @stream(if: $on, initialCount: $n) { reference } }"),
        ["on", "n"]
    );
}

#[test]
fn a_nested_field_argument_variable_is_a_reference() {
    assert_eq!(references("query Q { orders { items(first: $n) { id } } }"), ["n"]);
}

#[test]
fn a_reference_inside_a_reachable_fragment_counts() {
    assert_eq!(
        references("query Q { orders { ...F } } fragment F on Order { items(first: $n) { id } }"),
        ["n"]
    );
}

#[test]
fn a_reference_inside_a_transitively_reachable_fragment_counts() {
    assert_eq!(
        references(
            "query Q { orders { ...F } } \
             fragment F on Order { ...G } \
             fragment G on Order { items(first: $deep) { id } }"
        ),
        ["deep"]
    );
}

#[test]
fn a_directive_on_a_named_spread_is_a_reference() {
    assert_eq!(
        references("query Q { orders { ...F @include(if: $show) } } fragment F on Order { id }"),
        ["show"]
    );
}

#[test]
fn a_reference_inside_an_inline_fragment_counts() {
    assert_eq!(
        references("query Q { orders { ... on Order { items(first: $n) { id } } } }"),
        ["n"]
    );
}

#[test]
fn references_are_reported_in_document_order_without_duplicates() {
    assert_eq!(
        references("query Q { orders(limit: $a, offset: $b, where: {x: {eq: $a}}) { id } }"),
        ["a", "b"]
    );
}

#[test]
fn a_query_with_no_variables_references_nothing() {
    assert!(references("query Q { orders { reference } }").is_empty());
}

/// A fragment that is *defined* but never spread from the executed operation
/// contributes nothing. This is the multi-operation trap: `parse_query`
/// executes the first operation only, so a second operation's fragments must
/// not be scored against it.
///
/// The executed operation carries a reference of its own (`$own`) so this
/// discriminates *"excluded the unreachable fragment"* from *"collected
/// nothing at all"* — an empty-result assertion alone stays green against a
/// walker that has stopped working.
#[test]
fn an_unreachable_fragment_contributes_no_references() {
    // `One` is named explicitly: the document has two operations, so which
    // one executes is the request's choice, not the parser's (§ 6.1).
    assert_eq!(
        references_of(
            "query One($own: Int) { orders(limit: $own) { reference } } \
             query Two($t: Int) { orders(limit: $t) { ...F } } \
             fragment F on Order { items(first: $unreachable) { id } }",
            "One"
        ),
        ["own"],
        "only the executed operation's own reference should be collected"
    );
}

/// A cyclic spread is invalid GraphQL, but the walker must terminate on it
/// rather than depend on an earlier rule having rejected it.
#[test]
fn a_cyclic_fragment_spread_terminates() {
    assert_eq!(
        references(
            "query Q { orders { ...F } } \
             fragment F on Order { id ...G } \
             fragment G on Order { items(first: $n) { id } ...F }"
        ),
        ["n"]
    );
}

// ---- Cycle 3: the rule ----

#[test]
fn an_undefined_reference_is_a_validation_error_naming_the_variable() {
    let err = validate_variable_uses(Some("Q"), &[], &["neverDeclared".to_string()])
        .expect_err("undefined variable must be refused");
    let message = err.to_string();
    assert!(message.contains("$neverDeclared"), "message was: {message}");
    assert!(message.contains("not defined by operation 'Q'"), "message was: {message}");
}

#[test]
fn a_defined_reference_passes() {
    validate_variable_uses(Some("Q"), &["off".to_string()], &["off".to_string()])
        .expect("a defined variable is not an error");
}

#[test]
fn a_near_miss_gets_a_did_you_mean_hint() {
    let err = validate_variable_uses(Some("Q"), &["limit".to_string()], &["limt".to_string()])
        .expect_err("typo must be refused");
    let message = err.to_string();
    assert!(message.contains("Did you mean '$limit'?"), "message was: {message}");
}

#[test]
fn an_anonymous_operation_does_not_invent_a_name() {
    let err = validate_variable_uses(None, &[], &["x".to_string()])
        .expect_err("undefined variable must be refused");
    let message = err.to_string();
    assert!(message.contains("not defined by the operation"), "message was: {message}");
    assert!(!message.contains("operation ''"), "message was: {message}");
}

#[test]
fn the_error_path_locates_the_variable_in_the_operation() {
    let err = validate_variable_uses(Some("Q"), &[], &["x".to_string()])
        .expect_err("undefined variable must be refused");
    assert!(
        matches!(&err, FraiseQLError::Validation { path: Some(p), .. } if p == "Q.$x"),
        "expected a validation error pathed at the variable, got {err:?}"
    );
}

#[test]
fn the_first_undefined_reference_in_document_order_is_reported() {
    let err = validate_variable_uses(Some("Q"), &[], &["a".to_string(), "b".to_string()])
        .expect_err("undefined variable must be refused");
    assert!(err.to_string().contains("$a"), "message was: {err}");
}

/// The boundary this rule must not cross: a *declared* variable that the
/// request simply did not supply is spec-correct and stays legal.
#[test]
fn a_declared_but_unsupplied_variable_is_not_an_undefined_reference() {
    let source = "query Q($o: Int) { orders(offset: $o) { reference } }";
    validate_variable_uses(Some("Q"), &defined(source), &references(source))
        .expect("declared-but-unsupplied is not a definition error");
}

#[test]
fn a_multi_root_query_with_a_declared_variable_passes() {
    let source = "query Q($n: Int) { a: orders(limit: $n) { id } b: orders { id } }";
    validate_variable_uses(Some("Q"), &defined(source), &references(source))
        .expect("multi-root with a declared variable is valid");
}

#[test]
fn a_variable_used_only_inside_a_reachable_fragment_is_defined_by_the_operation() {
    let source = "query Q($n: Int) { orders { ...F } } \
                  fragment F on Order { items(first: $n) { id } }";
    validate_variable_uses(Some("Q"), &defined(source), &references(source))
        .expect("a fragment reference to a declared variable is valid");
}

// ---- § 5.8.2: a variable's type must be one the schema publishes ----

/// A schema carrying an enum and an input object, so § 5.8.2 has enough
/// information to adjudicate. `OrderStatus` and `OrderFilter` are the
/// declared names; everything else must resolve against the published
/// scalars.
fn schema_with_input_types() -> crate::schema::CompiledSchema {
    use crate::schema::{
        CompiledSchema, EnumDefinition, EnumValueDefinition, InputFieldDefinition,
        InputObjectDefinition,
    };

    let mut schema = CompiledSchema::default();
    schema
        .enums
        .push(EnumDefinition::new("OrderStatus").with_value(EnumValueDefinition::new("OPEN")));
    schema.input_types.push(
        InputObjectDefinition::new("OrderFilter")
            .with_field(InputFieldDefinition::new("reference", "String")),
    );
    schema
}

fn var_defs(source: &str) -> Vec<crate::graphql::types::VariableDefinition> {
    parse_query(source).expect("query parses").variables
}

#[test]
fn a_variable_typed_with_an_unpublished_name_is_a_validation_error() {
    let err = validate_variable_types(
        &schema_with_input_types(),
        Some("Q"),
        &var_defs("query Q($w: NoSuchTypeAtAll) { orders(where: $w) { id } }"),
    )
    .expect_err("a type name the schema does not publish must be refused");
    let message = err.to_string();
    assert!(message.contains("NoSuchTypeAtAll"), "message was: {message}");
    assert!(message.contains("$w"), "message was: {message}");
}

/// **The landmine.** `BUILTIN_SCALARS` — the *authoring* table — spells this
/// `"Json"`. Introspection publishes `"JSON"`, and a client writes what
/// introspection told it. Resolving § 5.8.2 against the authoring table
/// would reject the spelling the server itself advertises.
#[test]
fn json_is_accepted_with_the_spelling_introspection_publishes() {
    validate_variable_types(
        &schema_with_input_types(),
        Some("Q"),
        &var_defs("query Q($w: JSON) { orders(where: $w) { id } }"),
    )
    .expect("`JSON` is what introspection publishes and what a client writes");
}

/// Iterated from the source list rather than hand-copied, so a scalar added
/// to the published surface cannot silently fall out of the accepted set.
#[test]
fn every_published_scalar_is_accepted_as_a_variable_type() {
    let schema = schema_with_input_types();
    for name in crate::schema::published_scalar_names() {
        let source = format!("query Q($v: {name}) {{ orders(where: $v) {{ id }} }}");
        let result = validate_variable_types(&schema, Some("Q"), &var_defs(&source));
        assert!(result.is_ok(), "published scalar '{name}' must be accepted: {result:?}");
    }
}

#[test]
fn list_and_non_null_wrappers_are_structural_and_unwrapped() {
    let schema = schema_with_input_types();
    for source in [
        "query Q($ids: [ID!]!) { orders(where: $ids) { id } }",
        "query Q($ids: [ID]) { orders(where: $ids) { id } }",
        "query Q($id: ID!) { orders(where: $id) { id } }",
    ] {
        let result = validate_variable_types(&schema, Some("Q"), &var_defs(source));
        assert!(result.is_ok(), "wrappers are structural, not names: {source}: {result:?}");
    }
}

#[test]
fn a_declared_enum_and_input_object_are_accepted() {
    let schema = schema_with_input_types();
    validate_variable_types(
        &schema,
        Some("Q"),
        &var_defs("query Q($s: OrderStatus) { orders(where: $s) { id } }"),
    )
    .expect("a declared enum is an input type");
    validate_variable_types(
        &schema,
        Some("Q"),
        &var_defs("query Q($f: OrderFilter) { orders(where: $f) { id } }"),
    )
    .expect("a declared input object is an input type");
}

/// **The other half of the vector pair.** A schema that *declares* something
/// of a vector type accepts that name as a variable type, even though
/// introspection never publishes it as a `SCALAR` (a vector field's
/// `type_ref` is `JSON` or `[Float!]!`).
///
/// Resolving § 5.8.2 against the published list alone would refuse
/// `query Q($v: Vector)` against a schema that has a `Vector` — turning a
/// working hand-authored or externally-generated query into an error. The
/// published list is the set a client can *learn* names from; it is not the
/// set of names that are legitimate.
#[test]
fn a_vector_type_the_schema_declares_is_an_acceptable_variable_type() {
    use crate::schema::{FieldDefinition, FieldType, TypeDefinition};

    for (name, field_type) in [
        ("Vector", FieldType::Vector),
        ("BitVector", FieldType::BitVector),
        ("HalfVector", FieldType::HalfVector),
        ("SparseVector", FieldType::SparseVector),
    ] {
        let mut schema = schema_with_input_types();
        let mut doc = TypeDefinition::new("Doc", "v_doc");
        doc.fields.push(FieldDefinition::new("embedding", field_type));
        schema.types.push(doc);

        let source = format!("query Q($v: {name}) {{ orders(where: $v) {{ id }} }}");
        let result = validate_variable_types(&schema, Some("Q"), &var_defs(&source));
        assert!(
            result.is_ok(),
            "'{name}' is declared by this schema, so refusing it would break a working \
             query: {result:?}"
        );
    }
}

/// `Vector`/`BitVector`/`HalfVector`/`SparseVector` live in the *authoring*
/// table but are **not** published as scalars. A schema that declares no
/// vector anywhere therefore still refuses them — which is what keeps the
/// rule above from collapsing into "union the two tables": a name is
/// accepted because *this* schema declares something of that type, never
/// because the name exists in some global list.
#[test]
fn authoring_only_scalars_are_not_acceptable_variable_types() {
    let schema = schema_with_input_types();
    for name in ["Vector", "BitVector", "HalfVector", "SparseVector"] {
        let source = format!("query Q($v: {name}) {{ orders(where: $v) {{ id }} }}");
        let result = validate_variable_types(&schema, Some("Q"), &var_defs(&source));
        assert!(
            result.is_err(),
            "'{name}' is in the authoring table but is not published as a scalar, so it is \
             not a name a client can write: {result:?}"
        );
    }
}

/// Fail open: a schema with no enums *and* no input objects carries no
/// input-type information, which is not the same as declaring that these
/// names do not exist.
#[test]
fn a_schema_that_cannot_adjudicate_accepts_any_type_name() {
    validate_variable_types(
        &crate::schema::CompiledSchema::default(),
        Some("Q"),
        &var_defs("query Q($w: NoSuchTypeAtAll) { orders(where: $w) { id } }"),
    )
    .expect("no input-type information emitted — a rejection cannot be justified");
}

// ---- § 5.8.4: a defined variable must be used ----

#[test]
fn an_unused_definition_is_a_validation_error() {
    let source = "query Q($unused: Int) { orders(limit: 1) { reference } }";
    let err = validate_variables_used(Some("Q"), &defined(source), &references(source))
        .expect_err("a definition that is never referenced must be refused");
    let message = err.to_string();
    assert!(message.contains("$unused"), "message was: {message}");
    assert!(message.contains("never used"), "message was: {message}");
}

#[test]
fn a_used_definition_passes() {
    let source = "query Q($o: Int) { orders(offset: $o) { reference } }";
    validate_variables_used(Some("Q"), &defined(source), &references(source))
        .expect("a referenced definition is used");
}

/// The fragment-reachability walk in reverse. Missing this traversal turns
/// § 5.8.4 into a false-rejection machine on any document using fragments.
#[test]
fn a_variable_used_only_inside_a_reachable_fragment_counts_as_used() {
    let source = "query Q($n: Int) { orders { ...F } } \
                  fragment F on Order { items(first: $n) { id } }";
    validate_variables_used(Some("Q"), &defined(source), &references(source))
        .expect("a fragment reference is a use");
}

/// A declared-and-referenced-but-unsupplied variable is still *used* — this
/// rule never reads the request's variable values either.
#[test]
fn a_declared_but_unsupplied_variable_is_still_used() {
    let source = "query Q($o: Int) { orders(offset: $o) { reference } }";
    validate_variables_used(Some("Q"), &defined(source), &references(source))
        .expect("use is about references, not supplied values");
}
