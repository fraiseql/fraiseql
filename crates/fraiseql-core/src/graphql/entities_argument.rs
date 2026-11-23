//! The list an `_entities` field's `representations` argument is bound to.
//!
//! GraphQL lets an operation bind any variable to an argument, or write the value inline, so
//! the list is read from the argument of the selected operation's `_entities` field, never
//! from a variable that happens to be named `representations`. Every reader of the list (the
//! resolver, its authorization, a transport's per-entity circuit breaker) goes through
//! [`entities_representations`](crate::graphql::entities_argument::entities_representations),
//! so none of them can read a different list.

use std::collections::HashMap;

use serde_json::Value;

use crate::{
    error::{FraiseQLError, Result},
    graphql::{operation_selection_error, parse_query_with_operation_name, value_json},
};

/// The list an `_entities` field's `representations` argument is bound to.
///
/// The field is the one in the operation `operation_name` selects (GraphQL § 6.1); the value
/// has variables substituted and the operation's variable defaults applied (§ 6.4.1).
///
/// # Errors
///
/// - [`FraiseQLError::Parse`] or [`FraiseQLError::Validation`] when the document does not parse or
///   `operation_name` selects no operation.
/// - [`FraiseQLError::Validation`] when the operation has no root `_entities` field, the field has
///   no `representations` argument, or the argument resolves to `null`.
pub fn entities_representations(
    query: &str,
    operation_name: Option<&str>,
    variables: Option<&Value>,
) -> Result<Value> {
    let parsed = parse_query_with_operation_name(query, operation_name)
        .map_err(|e| operation_selection_error(&e))?;
    let field = parsed
        .selections
        .iter()
        .find(|selection| selection.name == "_entities")
        .ok_or_else(|| refusal("the operation selects no `_entities` field"))?;
    let argument = field
        .arguments
        .iter()
        .find(|argument| argument.name == "representations")
        .ok_or_else(|| refusal("`_entities` requires a `representations` argument"))?;

    let defaults = value_json::variable_defaults(&parsed.variables)?;
    let defaulted = value_json::with_variable_defaults(&defaults, variables);
    let variables: HashMap<String, Value> = match defaulted.as_ref().or(variables) {
        Some(Value::Object(map)) => map.iter().map(|(k, v)| (k.clone(), v.clone())).collect(),
        _ => HashMap::new(),
    };
    match value_json::decode_resolved(&argument.value_json, &variables)? {
        Value::Null => Err(refusal("the `representations` argument of `_entities` is null")),
        representations => Ok(representations),
    }
}

fn refusal(message: &str) -> FraiseQLError {
    FraiseQLError::Validation {
        message: message.to_string(),
        path:    Some("_entities.representations".to_string()),
    }
}

#[cfg(test)]
#[path = "entities_argument_tests.rs"]
mod tests;
