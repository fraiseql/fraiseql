//! A tool's argument values, held to their declared types before the call runs.
//!
//! MCP passes tool arguments, and builds the GraphQL document that carries them as
//! variables itself, each declared at the type its argument publishes. Left to the
//! executor, a value that type refuses is refused at the document's variable (`$name`),
//! which names a variable the caller never wrote. The same checks run here first instead,
//! on the same values, so the refusal names the tool argument, as it does for every
//! transport that sends no document.

use fraiseql_core::{
    runtime::{
        published_argument_type, validate_mutation_argument_values, validate_resolved_arguments,
    },
    schema::CompiledSchema,
};
use fraiseql_error::{FraiseQLError, Result};

use super::tools::ExposedOperation;

/// Refuse a tool call whose argument value is not of its argument's declared type: a null
/// for an argument that is not nullable, or a value its type does not hold.
///
/// # Errors
///
/// [`FraiseQLError::Validation`] naming the argument and its declared type.
pub fn check_tool_arguments(
    operation: &ExposedOperation<'_>,
    arguments: Option<&serde_json::Map<String, serde_json::Value>>,
    schema: &CompiledSchema,
) -> Result<()> {
    let Some(arguments) = arguments else {
        return Ok(());
    };
    let (root, name) = match operation {
        ExposedOperation::Query(query) => ("Query", &query.name),
        ExposedOperation::Mutation(mutation) => ("Mutation", &mutation.name),
    };
    let field_label = format!("{root}.{}", schema.display_name(name));
    let declared = operation.arguments(schema);
    if let Some(argument) = declared
        .iter()
        .find(|a| !a.nullable && arguments.get(&a.name).is_some_and(serde_json::Value::is_null))
    {
        return Err(FraiseQLError::Validation {
            message: format!(
                "Argument `{}` on {field_label} has type `{}`, and may not be null",
                argument.name,
                published_argument_type(argument)
            ),
            path:    Some(argument.name.clone()),
        });
    }
    match operation {
        ExposedOperation::Mutation(_) => validate_mutation_argument_values(
            schema,
            &field_label,
            &declared,
            Some(&serde_json::Value::Object(arguments.clone())),
        ),
        ExposedOperation::Query(_) => validate_resolved_arguments(
            schema,
            || field_label.clone(),
            &declared,
            |name| arguments.get(name),
        ),
    }
}
