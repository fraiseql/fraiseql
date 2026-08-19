//! Argument-name validation for a root field.
//!
//! GraphQL § 5.4.1 (Argument Names): *"Every argument provided to a field or
//! directive must be defined in the set of possible arguments of that field or
//! directive."* A document that names an argument the field does not declare is
//! **invalid**, and an invalid document must not execute.
//!
//! Before this, an undeclared argument was dropped on the floor (#1154). Only
//! *declared* arguments become WHERE conditions (`combine_explicit_arg_where`),
//! and only the auto-wired names are read by the pagination paths, so
//! `orders(contractId: "x")` against a query that does not declare `contractId`
//! returned **every row** under an HTTP 200 with no `errors` array. That reads as
//! a filtering bug in the server and is very hard to trace back to the argument
//! that was silently discarded.
//!
//! The selection-set rule next door ([`crate::graphql::validate_selection_set`])
//! already rejected an undeclared *field*, so validation ran — it just did not
//! cover argument names, which left the server more permissive than the schema it
//! publishes through introspection: a spec-compliant client-side validator
//! rejected queries this server happily answered.
//!
//! # What this does not cover
//!
//! * **Nested field arguments.** No object-type field declares arguments in the compiled schema, so
//!   every nested argument is undeclared and rejecting them would be a much wider behaviour change
//!   than the rule this fixes. They remain inert (only [`crate::graphql::complexity`] reads them,
//!   for cost estimation).
//! * **Request variables.** The matcher merges the whole `variables` object into the argument map
//!   and the runtime reads auto-params from there, so an unreferenced variable is not an argument
//!   written on a field and § 5.4.1 does not reach it.
//! * **Argument *values*.** Coercion and input-object shape are checked further down; this rule is
//!   about names alone.

use crate::{
    error::{FraiseQLError, Result},
    graphql::types::GraphQLArgument,
};

/// Check that every argument in `provided` is one of `accepted`.
///
/// `field_label` names the field in the error message the way a client reads a
/// schema — `Query.orders`, `Mutation.createUser` — and `accepted` is that
/// field's full accepted set (see
/// [`QueryDefinition::accepted_argument_names`](crate::schema::QueryDefinition::accepted_argument_names)
/// for the query side, which is deliberately wider than the declared list).
///
/// # Errors
///
/// Returns [`FraiseQLError::Validation`] naming the first undeclared argument and
/// the field it is not defined on, with a "did you mean" hint when a close
/// accepted name exists — a dropped filter is most often a typo or a renamed
/// argument, and both are one edit away from the name that works.
pub fn validate_argument_names(
    field_label: &str,
    accepted: &[String],
    provided: &[GraphQLArgument],
) -> Result<()> {
    for arg in provided {
        if accepted.iter().any(|name| name == &arg.name) {
            continue;
        }

        let candidates: Vec<&str> = accepted.iter().map(String::as_str).collect();
        let message = match super::suggest_similar(&arg.name, &candidates).as_slice() {
            [s] => format!(
                "Unknown argument '{}' on field '{field_label}'. Did you mean '{s}'?",
                arg.name
            ),
            [a, b] => format!(
                "Unknown argument '{}' on field '{field_label}'. Did you mean '{a}' or '{b}'?",
                arg.name
            ),
            [a, b, c, ..] => format!(
                "Unknown argument '{}' on field '{field_label}'. Did you mean '{a}', '{b}', or \
                 '{c}'?",
                arg.name
            ),
            _ => format!("Unknown argument '{}' on field '{field_label}'.", arg.name),
        };

        return Err(FraiseQLError::Validation {
            message,
            path: Some(format!("{field_label}.{}", arg.name)),
        });
    }

    Ok(())
}
