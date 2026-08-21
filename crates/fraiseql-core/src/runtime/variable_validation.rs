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
//! dropped and no answer is wrong. What justifies it is not the spec text but the
//! ecosystem — `graphql-js` has enforced § 5.8.4 for years, as does every other
//! major implementation, so a client sending superset variable definitions is
//! already refused by every other server it talks to. The population this breaks
//! is not "everyone reusing documents"; it is code written specifically against
//! FraiseQL's leniency. This aligns with every other implementation rather than
//! inventing a restriction.
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
            || schema_declares_type_name(schema, type_name);
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

/// Whether the schema itself declares `name` as a type — in **field or argument
/// position** — even though introspection does not publish it as a scalar.
///
/// This is the #939 principle applied properly: reject what the schema
/// positively says is not there, not merely what introspection happens to
/// advertise. The published-scalar list is the set a client can *learn* names
/// from; it is not the set of names that are legitimate. A hand-authored or
/// externally-generated document does not need introspection to know a type the
/// schema declares.
///
/// The concrete case this exists for is the pgvector family.
/// `Vector`/`BitVector`/`HalfVector`/`SparseVector` are in the **authoring**
/// table but are never published as `SCALAR` types — a vector field's
/// introspection `type_ref` is `JSON` or `[Float!]!`. Resolving § 5.8.2 against
/// the published list alone would therefore refuse `query Q($v: Vector)` against
/// a schema that declares a `Vector`, turning a working query into an error.
///
/// The distinction that keeps this from collapsing into "union the two tables":
/// a name is accepted because **this schema declares something of that type**,
/// not because the name exists in some global list. A schema with no vector
/// anywhere still refuses `$v: Vector`.
fn schema_declares_type_name(schema: &crate::schema::CompiledSchema, name: &str) -> bool {
    let matches_type = |ft: &crate::schema::FieldType| declared_type_name(ft) == Some(name);

    schema
        .types
        .iter()
        .any(|t| t.fields.iter().any(|f| matches_type(&f.field_type)))
        || schema
            .queries
            .iter()
            .any(|q| q.arguments.iter().any(|a| matches_type(&a.arg_type)))
        || schema
            .mutations
            .iter()
            .any(|m| m.arguments.iter().any(|a| matches_type(&a.arg_type)))
}

/// The name an author writes for `ft` in `schema.json`.
///
/// [`FieldType::type_name`] covers only the composite variants, and
/// [`FieldType::to_graphql_string`] gives the *published* spelling — which for a
/// vector is `[Float!]!`, not a name at all. The authoring table is the only
/// place the scalar variants carry their written names, so it is the reverse
/// lookup used here.
fn declared_type_name(ft: &crate::schema::FieldType) -> Option<&str> {
    if let Some(name) = ft.type_name() {
        return Some(name);
    }
    if let crate::schema::FieldType::Scalar(name) = ft {
        return Some(name);
    }
    crate::schema::BUILTIN_SCALARS
        .iter()
        .find_map(|(name, builtin)| (builtin == ft).then_some(*name))
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
#[path = "variable_validation_tests.rs"]
mod tests;
