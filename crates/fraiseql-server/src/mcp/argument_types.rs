//! A mutation tool's argument values, held to their declared types before the call runs.
//!
//! MCP passes tool arguments, and builds the GraphQL document that carries them as
//! variables itself. Left to the executor, a value of the wrong type is refused at the
//! document's variable (`$name`), which names a variable the caller never wrote. The
//! mutation runner's own check runs here first instead, on the same values, so the refusal
//! names the tool argument, as it does for every transport that sends no document.

use fraiseql_core::schema::CompiledSchema;
use fraiseql_error::Result;

use super::tools::ExposedOperation;

/// Refuse a mutation tool call whose argument value is not of its argument's declared type.
///
/// A query tool is not checked here: its arguments are adjudicated by the executor.
///
/// # Errors
///
/// [`FraiseQLError::Validation`](fraiseql_error::FraiseQLError::Validation) from
/// [`validate_mutation_argument_values`](fraiseql_core::runtime::validate_mutation_argument_values),
/// naming the argument and its declared type.
pub fn check_mutation_arguments(
    operation: &ExposedOperation<'_>,
    arguments: Option<&serde_json::Map<String, serde_json::Value>>,
    schema: &CompiledSchema,
) -> Result<()> {
    let (ExposedOperation::Mutation(mutation), Some(arguments)) = (operation, arguments) else {
        return Ok(());
    };
    fraiseql_core::runtime::validate_mutation_argument_values(
        schema,
        &format!("Mutation.{}", schema.display_name(&mutation.name)),
        &mutation.arguments,
        Some(&serde_json::Value::Object(arguments.clone())),
    )
}
