//! The arguments a subscription operation passes its root field (#1158).
//!
//! A subscription's filter (`filter_fields`, `argument_paths`) compares each declared
//! argument with the event. The value has to be the one the document gave the argument,
//! however it was spelled: inline (`orderChanged(status: "shipped")`), or through a variable
//! of any name (`orderChanged(status: $wanted)`). Reading the request's variables by the
//! *argument's* name instead dropped both spellings, and the subscription silently
//! delivered every event.

use std::collections::HashMap;

use crate::{error::Result, graphql::ParsedQuery, schema::CompiledSchema};

/// The arguments `document`'s root field is given, by argument name.
///
/// Inline values and variables alike, resolved against `variables` and the variables'
/// declared defaults. An argument naming a variable the request omitted is absent, as an
/// omitted nullable argument is.
///
/// `max_depth` bounds fragment resolution, as for any document.
///
/// # Errors
///
/// `Validation` for a document that selects no root field or more than one, a
/// subscription the schema does not declare, or an argument the subscription does not
/// declare (with the message a query field gives).
pub fn subscription_arguments(
    schema: &CompiledSchema,
    document: &ParsedQuery,
    variables: Option<&serde_json::Value>,
    max_depth: u32,
) -> Result<serde_json::Map<String, serde_json::Value>> {
    let defaults = crate::graphql::value_json::variable_defaults(&document.variables)?;
    let defaulted = crate::graphql::value_json::with_variable_defaults(&defaults, variables);
    let variables_map: HashMap<String, serde_json::Value> =
        crate::graphql::selection_set::variables_map(defaulted.as_ref().or(variables));
    let resolved = crate::graphql::selection_set::resolve_and_filter(
        &document.selections,
        &document.fragments,
        &variables_map,
        max_depth,
    )?;
    let mut roots = resolved.iter().filter(|s| s.name != "__typename");
    let (Some(root), None) = (roots.next(), roots.next()) else {
        return Err(crate::error::FraiseQLError::validation(
            "a subscription operation selects exactly one root field",
        ));
    };
    let definition = schema.find_subscription(&root.name).ok_or_else(|| {
        crate::error::FraiseQLError::validation(format!(
            "Subscription '{}' not found in schema",
            root.name
        ))
    })?;
    let declared: Vec<String> = definition.arguments.iter().map(|a| a.name.clone()).collect();
    crate::runtime::validate_argument_names(&root.name, &declared, &root.arguments)?;

    let mut arguments = serde_json::Map::new();
    for argument in &root.arguments {
        if let Some(value) =
            crate::runtime::matcher::QueryMatcher::resolve_inline_arg(argument, &variables_map)?
        {
            arguments.insert(argument.name.clone(), value);
        }
    }
    Ok(arguments)
}
