//! Variable-definition validation for the executed operation — GraphQL § 5.8.2,
//! § 5.8.3 and § 5.8.4.
//!
//! The load-bearing one is **§ 5.8.3 (All Variable Uses Defined)**: *"Every
//! variable used within an operation must be defined by that operation."* A
//! document that references `$x` without defining it is **invalid**, and an
//! invalid document must not execute.
//!
//! Its two siblings share this module because they share the reference walker,
//! but they do **not** share its risk profile, and the difference matters to
//! anyone deciding whether to upgrade:
//!
//! | Rule | What it catches | What breaks |
//! |---|---|---|
//! | § 5.8.3 | `$x` used, never defined | clients getting **wrong answers** today |
//! | § 5.8.2 | `$w: NoSuchTypeAtAll` | clients whose declared type name is not published |
//! | § 5.8.4 | `$unused` defined, never referenced | clients that work and answer **correctly** today |
//!
//! § 5.8.4 is the only rule here with no correctness payoff: nothing is silently
//! dropped and no answer is wrong. It outlaws a real client shape — one document
//! reused across call sites, sent with a superset of variable definitions. The
//! spec is unambiguous that this is invalid; the CHANGELOG says so plainly.
//!
//! Before this, an undefined reference was dropped on the floor along with the
//! argument that carried it. [`QueryMatcher::resolve_inline_arg`] resolves a
//! whole-argument variable by looking the name up in the request's variables map
//! and returning `None` when it is absent — which is correct for a *declared*
//! variable the caller chose not to supply, and silently destructive for one that
//! was never declared at all:
//!
//! ```graphql
//! query Q { orders(offset: $neverDeclared) { reference } }
//! ```
//!
//! returned **every row** under an HTTP 200 with no `errors` array. `where:`
//! loses the filter and returns the whole table; `limit:` loses the bound and
//! the now-unbounded query trips the complexity ceiling, so the client gets an
//! error *about cost* that never mentions the variable — worse for debugging
//! than no error at all.
//!
//! # The distinction this rule is built around
//!
//! **Definitions, never values.** A variable that *is* defined but is simply not
//! supplied still drops its argument, deliberately and spec-correctly:
//!
//! ```graphql
//! query Q($o: Int) { orders(offset: $o) { reference } }   # no variables sent
//! → all rows, no error                                    # CORRECT, unchanged
//! ```
//!
//! That behaviour is load-bearing — it is what lets `limit: $limit` fall back to
//! the query's compiled default instead of forcing `LIMIT NULL` — and
//! `test_resolve_inline_arg_variable_not_found_drops_the_argument` encodes it.
//! Nothing here reads the request's variables map. Validity of a *definition* is
//! a property of the document alone, which is also what makes the result safe to
//! memoise in the parse cache.
//!
//! # What this does not reject
//!
//! Following [`crate::graphql::validate_selection_set`] (#939): reject what the
//! document positively contradicts, pass everything it cannot adjudicate.
//!
//! * **Variables of a *later* operation in a multi-operation document.** [`parse_query`] executes
//!   the *first* operation and ignores `operationName`, while fragment definitions are
//!   document-global. Only the fragments transitively reachable from the executed operation are
//!   walked, so a second operation's fragments — which legitimately reference *that* operation's
//!   variables — are never scored against this one. Scanning `parsed.fragments` wholesale would
//!   reject a valid document.
//! * **Variable types, when the schema cannot adjudicate them.** § 5.8.2 resolves a declared type
//!   name against the *published* surface, and a schema carrying no enums and no input objects at
//!   all is treated as "the compiler emitted no input-type information" rather than "these names do
//!   not exist". See [`validate_variable_types`].
//! * **Argument values, coercion and input-object shape.** Checked further down; these rules are
//!   about names alone.
//! * **References nested deeper than [`value_json::MAX_DEPTH`].** The parser refuses to build
//!   anything deeper, so a deeper reference cannot exist; the cap keeps the walk off an unbounded
//!   stack rather than trading away coverage.

use std::collections::HashSet;

use crate::{
    error::{FraiseQLError, Result},
    graphql::{
        types::{Directive, FieldSelection, FragmentDefinition, GraphQLArgument, ParsedQuery},
        value_json,
    },
};

/// Prefix marking an inline fragment's synthetic field name (`...on User`), as
/// distinct from a named spread (`...UserFields`).
///
/// The trailing space is what disambiguates them: a fragment named `onCall`
/// yields `...onCall`, which must be read as a spread. GraphQL reserves `on` as
/// a fragment name, so `...on` alone cannot occur.
const INLINE_FRAGMENT_PREFIX: &str = "...on ";

/// Every variable referenced by the executed operation, in document order,
/// without duplicates.
///
/// Walks field arguments (whole and nested), directive arguments, nested field
/// arguments, and the selections of every fragment transitively reachable from
/// the operation. See the module header for what is deliberately outside the
/// walk.
///
/// # Errors
///
/// Returns [`FraiseQLError::Internal`] if a stored `value_json` is not valid
/// JSON — the same loud failure [`value_json::decode`] raises, for the same
/// reason: the alternative is dropping an argument, which widens a result set.
pub fn collect_variable_references(parsed: &ParsedQuery) -> Result<Vec<String>> {
    let mut acc = References::default();
    let mut reached: HashSet<&str> = HashSet::new();

    collect_from_selections(&parsed.selections, &parsed.fragments, &mut reached, &mut acc, 0)?;

    Ok(acc.ordered)
}

/// Check that every variable referenced by the operation is defined by it.
///
/// `operation_name` is the executed operation's name, used verbatim in the error
/// the way a client reads its own document; an anonymous operation drops the
/// clause rather than inventing a name.
///
/// # Errors
///
/// Returns [`FraiseQLError::Validation`] naming the first undefined variable in
/// document order, with a "did you mean" hint when a close *defined* name exists
/// — a variable typo is precisely the case this rule catches, and it is one edit
/// away from the name that works.
pub fn validate_variable_uses(
    operation_name: Option<&str>,
    defined: &[String],
    referenced: &[String],
) -> Result<()> {
    for name in referenced {
        if defined.iter().any(|d| d == name) {
            continue;
        }

        let candidates: Vec<&str> = defined.iter().map(String::as_str).collect();
        let subject = match operation_name {
            Some(op) => format!("Variable '${name}' is not defined by operation '{op}'."),
            None => format!("Variable '${name}' is not defined by the operation."),
        };
        let message = match super::suggest_similar(name, &candidates).as_slice() {
            [s] => format!("{subject} Did you mean '${s}'?"),
            [a, b] => format!("{subject} Did you mean '${a}' or '${b}'?"),
            [a, b, c, ..] => format!("{subject} Did you mean '${a}', '${b}', or '${c}'?"),
            _ => subject,
        };

        return Err(FraiseQLError::Validation {
            message,
            path: Some(match operation_name {
                Some(op) => format!("{op}.${name}"),
                None => format!("${name}"),
            }),
        });
    }

    Ok(())
}

/// Check that every variable definition names a type the schema publishes as an
/// input type (GraphQL § 5.8.2).
///
/// Resolves the **innermost** type name — the list and non-null wrappers are
/// structural — against the three sources a client can legitimately learn a name
/// from: the scalars introspection publishes
/// ([`published_scalar_names`](crate::schema::published_scalar_names)), declared
/// enums, and declared input objects. Custom scalars named by a declared field
/// are also accepted: introspection does not publish them as `SCALAR` types, but
/// the schema does not positively say they are absent either.
///
/// # What this does not reject
///
/// **A schema carrying no enums *and* no input objects is not adjudicated.**
/// That shape means the compiler emitted no input-type information, not that the
/// schema genuinely has none, and a rejection cannot be justified from an absence
/// of evidence — the #939 principle in
/// [`crate::graphql::validate_selection_set`].
///
/// # Errors
///
/// Returns [`FraiseQLError::Validation`] naming the first unknown type name.
pub fn validate_variable_types(
    schema: &crate::schema::CompiledSchema,
    operation_name: Option<&str>,
    definitions: &[crate::graphql::types::VariableDefinition],
) -> Result<()> {
    // Cannot adjudicate: no input-type information was emitted at all.
    if schema.enums.is_empty() && schema.input_types.is_empty() {
        return Ok(());
    }

    let scalars = crate::schema::published_scalar_names();

    for def in definitions {
        let type_name = innermost_type_name(&def.var_type.name);
        let known = scalars.iter().any(|s| s == type_name)
            || schema.find_enum(type_name).is_some()
            || schema.find_input_type(type_name).is_some()
            || custom_scalar_is_declared(schema, type_name);
        if known {
            continue;
        }

        let subject = match operation_name {
            Some(op) => format!(
                "Variable '${}' of operation '{op}' declares unknown type '{type_name}'.",
                def.name
            ),
            None => {
                format!("Variable '${}' declares unknown type '{type_name}'.", def.name)
            },
        };
        let candidates: Vec<&str> = scalars
            .iter()
            .map(String::as_str)
            .chain(schema.enums.iter().map(|e| e.name.as_str()))
            .chain(schema.input_types.iter().map(|i| i.name.as_str()))
            .collect();
        let message = match super::suggest_similar(type_name, &candidates).as_slice() {
            [s] => format!("{subject} Did you mean '{s}'?"),
            [a, b] => format!("{subject} Did you mean '{a}' or '{b}'?"),
            [a, b, c, ..] => format!("{subject} Did you mean '{a}', '{b}', or '{c}'?"),
            _ => subject,
        };

        return Err(FraiseQLError::Validation {
            message,
            path: Some(match operation_name {
                Some(op) => format!("{op}.${}", def.name),
                None => format!("${}", def.name),
            }),
        });
    }

    Ok(())
}

/// Strip the list wrappers a parsed variable type carries in its *name*.
///
/// `GraphQLType` records list-ness twice and inconsistently: `parse_graphql_type`
/// sets `list: true` **and** rewrites the name to `"[Inner]"`, while a non-null
/// wrapper only flips `nullable` and leaves the name alone. So `[ID!]!` arrives
/// as `name: "[ID]"`, not `name: "ID"` with flags.
///
/// § 5.8.2 is a rule about the *named* type, so the wrappers are peeled off
/// before resolution — and the innermost name is what the error reports, since
/// `'[ID]'` is not a name a client could look up.
fn innermost_type_name(name: &str) -> &str {
    let mut inner = name;
    while let Some(stripped) = inner.strip_prefix('[').and_then(|s| s.strip_suffix(']')) {
        inner = stripped;
    }
    inner
}

/// Whether any declared field uses `name` as a custom scalar.
fn custom_scalar_is_declared(schema: &crate::schema::CompiledSchema, name: &str) -> bool {
    schema.types.iter().any(|t| {
        t.fields
            .iter()
            .any(|f| matches!(&f.field_type, crate::schema::FieldType::Scalar(s) if s == name))
    })
}

/// Check that every variable the operation defines is also used by it
/// (GraphQL § 5.8.4).
///
/// The inverse of [`validate_variable_uses`], over the same reference set — which
/// means it inherits the fragment-reachability walk. That is load-bearing rather
/// than incidental: a variable referenced **only** inside a reachable fragment
/// *is* used, and a rule that missed that traversal would reject any document
/// whose variables are consumed through fragments.
///
/// Unlike § 5.8.3, this rejects documents that execute and answer **correctly**
/// today — a document reused across call sites and sent with a superset of
/// variable definitions is a real client shape. The spec is unambiguous that it
/// is invalid, but it is the one rule in this module with no correctness payoff.
///
/// # Errors
///
/// Returns [`FraiseQLError::Validation`] naming the first unused definition, in
/// declaration order.
pub fn validate_variables_used(
    operation_name: Option<&str>,
    defined: &[String],
    referenced: &[String],
) -> Result<()> {
    for name in defined {
        if referenced.iter().any(|r| r == name) {
            continue;
        }

        let message = match operation_name {
            Some(op) => format!("Variable '${name}' is never used in operation '{op}'."),
            None => format!("Variable '${name}' is never used in the operation."),
        };

        return Err(FraiseQLError::Validation {
            message,
            path: Some(match operation_name {
                Some(op) => format!("{op}.${name}"),
                None => format!("${name}"),
            }),
        });
    }

    Ok(())
}

/// Ordered, de-duplicated accumulator for referenced variable names.
///
/// Document order is preserved so the error names the *first* offending
/// reference a reader will find in their own query, rather than the
/// alphabetically first.
#[derive(Default)]
struct References {
    ordered: Vec<String>,
    seen:    HashSet<String>,
}

impl References {
    fn push(&mut self, name: &str) {
        if self.seen.insert(name.to_string()) {
            self.ordered.push(name.to_string());
        }
    }
}

fn collect_from_selections<'a>(
    selections: &'a [FieldSelection],
    fragments: &'a [FragmentDefinition],
    reached: &mut HashSet<&'a str>,
    acc: &mut References,
    depth: usize,
) -> Result<()> {
    if depth > value_json::MAX_DEPTH {
        return Ok(());
    }

    for sel in selections {
        // Directives ride on every selection kind, including a named spread's
        // synthetic pseudo-field: `...F @include(if: $x)` references `$x`
        // without the fragment itself doing so.
        collect_from_directives(&sel.directives, acc)?;
        collect_from_arguments(&sel.arguments, acc)?;

        // A named spread carries no nested fields of its own; its references
        // live in the fragment definition, which is walked at most once.
        if let Some(fragment_name) = named_spread(&sel.name) {
            if reached.insert(fragment_name) {
                if let Some(def) = fragments.iter().find(|f| f.name == fragment_name) {
                    collect_from_selections(&def.selections, fragments, reached, acc, depth + 1)?;
                }
            }
            continue;
        }

        collect_from_selections(&sel.nested_fields, fragments, reached, acc, depth + 1)?;
    }

    Ok(())
}

/// The fragment name a selection spreads, if it is a named spread.
///
/// Returns `None` for an ordinary field and for an inline fragment — the latter
/// keeps its references in `nested_fields`, which the caller recurses into.
fn named_spread(selection_name: &str) -> Option<&str> {
    if selection_name.starts_with(INLINE_FRAGMENT_PREFIX) {
        return None;
    }
    selection_name.strip_prefix("...")
}

fn collect_from_directives(directives: &[Directive], acc: &mut References) -> Result<()> {
    for directive in directives {
        collect_from_arguments(&directive.arguments, acc)?;
    }
    Ok(())
}

fn collect_from_arguments(arguments: &[GraphQLArgument], acc: &mut References) -> Result<()> {
    for arg in arguments {
        let value = value_json::decode(&arg.value_json)?;
        collect_from_value(&value, acc, 0);
    }
    Ok(())
}

/// Walk a decoded argument value for `{"$var": "name"}` markers.
///
/// A marker is terminal: it is a reference, not a container, so the walk does
/// not descend into it.
fn collect_from_value(value: &serde_json::Value, acc: &mut References, depth: usize) {
    if depth > value_json::MAX_DEPTH {
        return;
    }

    if let Some(name) = value_json::variable_name(value) {
        acc.push(name);
        return;
    }

    match value {
        serde_json::Value::Object(map) => {
            for nested in map.values() {
                collect_from_value(nested, acc, depth + 1);
            }
        },
        serde_json::Value::Array(items) => {
            for item in items {
                collect_from_value(item, acc, depth + 1);
            }
        },
        _ => {},
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::graphql::parse_query;

    fn references(source: &str) -> Vec<String> {
        let parsed = parse_query(source).expect("query parses");
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
        assert_eq!(
            references("query Q { orders(where: {status: {eq: $s}}) { reference } }"),
            ["s"]
        );
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
            references(
                "query Q { orders { ...F } } fragment F on Order { items(first: $n) { id } }"
            ),
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
            references(
                "query Q { orders { ...F @include(if: $show) } } fragment F on Order { id }"
            ),
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
        assert_eq!(
            references(
                "query One($own: Int) { orders(limit: $own) { reference } } \
                 query Two($t: Int) { orders(limit: $t) { ...F } } \
                 fragment F on Order { items(first: $unreachable) { id } }"
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

    /// `Vector`/`BitVector`/`HalfVector`/`SparseVector` live in the *authoring*
    /// table but are **not** published as scalars — a vector field's
    /// introspection `type_ref` is `JSON` or `[Float!]!`. They are therefore
    /// correctly not acceptable variable type names, and this pins it so a
    /// future author does not "fix" § 5.8.2 by unioning the two tables.
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
}
