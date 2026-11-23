//! A mutation document's variables, held to their declarations as a query's are.
//!
//! § 5.8.5 (All Variable Usages Are Allowed): a variable is declared at a type its argument
//! can take. A property of the document, checked when it is classified.
//!
//! § 6.1.2 (Coercing Variable Values): each value a request supplies has its variable's
//! declared type, and a non-null variable has one. A property of the request, checked
//! before the operation's first root runs, so a document whose later root is at fault
//! writes nothing.
//!
//! The mutation runner holds every resolved value to its argument's type besides, for
//! every transport: REST, gRPC and MCP send no document, so this is the GraphQL half.

use super::{super::MutationRoot, classify::DocumentVariables};
use crate::{
    error::Result,
    graphql::{FieldSelection, GraphQLArgument},
    runtime::{
        Executor, validate_argument_values, validate_enum_variable_values, validate_variable_values,
    },
};

impl Executor {
    /// Refuse a variable of the mutation document declared at a type its argument cannot
    /// take (§ 5.8.5), in `roots`, the operation's root fields.
    ///
    /// Only the variable usages are judged here: a literal argument is judged by the runner,
    /// which reads a localized argument's locale map. A root naming no mutation is refused by
    /// the runner, which says so.
    ///
    /// # Errors
    ///
    /// [`FraiseQLError::Validation`](crate::error::FraiseQLError::Validation) naming the
    /// variable, its declared type, the argument and the argument's type.
    pub(in crate::runtime::executor) fn check_mutation_variable_usages(
        &self,
        roots: &[FieldSelection],
        definitions: &[crate::graphql::types::VariableDefinition],
    ) -> Result<()> {
        let schema = &self.ctx.schema;
        for root in roots {
            let Some(mutation) = schema.find_mutation(&root.name) else {
                continue;
            };
            let variables: Vec<GraphQLArgument> = root
                .arguments
                .iter()
                .filter(|arg| arg.value_type == "variable")
                .cloned()
                .collect();
            validate_argument_values(
                &format!("Mutation.{}", schema.display_name(&mutation.name)),
                &mutation.arguments,
                &variables,
                definitions,
            )?;
        }
        Ok(())
    }

    /// Refuse a value of this request's `variables` that is not of its variable's declared
    /// type, or a non-null variable with none (§ 6.1.2), before any of `roots` runs.
    ///
    /// # Errors
    ///
    /// [`FraiseQLError::Validation`](crate::error::FraiseQLError::Validation) naming the
    /// variable and its declared type; the value is described, never quoted.
    pub(in crate::runtime::executor) fn check_mutation_variable_values(
        &self,
        roots: &[MutationRoot],
        document: &DocumentVariables,
        variables: Option<&serde_json::Value>,
    ) -> Result<()> {
        if roots.is_empty() {
            return Ok(());
        }
        let operation_name = document.operation_name();
        validate_variable_values(operation_name, document.definitions(), variables)?;
        validate_enum_variable_values(
            &self.ctx.schema,
            operation_name,
            document.definitions(),
            variables,
        )
    }
}
