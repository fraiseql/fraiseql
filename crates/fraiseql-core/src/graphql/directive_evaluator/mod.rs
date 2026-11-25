//! Directive evaluation for GraphQL queries.
//!
//! Handles:
//! - `@skip` directive (conditionally skip a field)
//! - `@include` directive (conditionally include a field)

mod evaluator;
mod types;

pub use evaluator::DirectiveEvaluator;
pub use types::{
    DirectiveError, DirectiveHandler, DirectiveResult, EvaluationContext, OperationType,
};

#[cfg(test)]
mod tests;
