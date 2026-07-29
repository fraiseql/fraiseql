//! The one routine that answers "which fields did the client ask for?".
//!
//! Every GraphQL entry point — `/graphql`, the multi-root fan-out, `node(id:)`,
//! mutations — has to expand fragment spreads and evaluate `@skip`/`@include`,
//! and each one used to do it (or skip doing it) on its own. `node(id:)` never
//! expanded spreads at all (#827); the multi-root fan-out re-serialized each root
//! and lost both the fragments and the directives; the two paths that did resolve
//! dropped a spread's own directives on the floor (#826).
//!
//! Resolution is split in two because the runtime is:
//!
//! * `resolve` expands spreads. It depends only on the document, so it is safe to memoise — the
//!   executor's parse cache holds classified mutation selections keyed by the query string alone.
//! * `resolve_and_filter` additionally evaluates `@skip`/`@include`, which needs the request's
//!   variables and therefore must run per request.
//!
//! Entry points that classify before they see variables (mutations, `node(id:)`)
//! call `resolve` at classification time and `filter` in their runner; every
//! other caller uses `resolve_and_filter`.

use std::collections::HashMap;

use serde_json::Value as JsonValue;
use thiserror::Error;

use crate::graphql::{
    DirectiveError, DirectiveEvaluator, FragmentError, FragmentResolver,
    types::{FieldSelection, FragmentDefinition},
};

/// Failure to reduce a document's selection set to the fields it requests.
#[derive(Debug, Error)]
#[non_exhaustive]
pub enum SelectionError {
    /// A fragment spread could not be expanded.
    #[error(transparent)]
    Fragment(#[from] FragmentError),

    /// A `@skip`/`@include` condition could not be evaluated.
    #[error(transparent)]
    Directive(#[from] DirectiveError),
}

impl From<SelectionError> for crate::error::FraiseQLError {
    fn from(err: SelectionError) -> Self {
        let path = match &err {
            SelectionError::Fragment(_) => "fragments",
            SelectionError::Directive(_) => "directives",
        };
        Self::Validation {
            message: err.to_string(),
            path:    Some(path.to_string()),
        }
    }
}

/// Expand every fragment spread, carrying each spread's directives onto the
/// fields it contributes.
///
/// Variable-independent, so the result may be cached against the query string.
///
/// # Errors
///
/// Returns [`SelectionError::Fragment`] if a spread names an undefined fragment,
/// the document is circular, or nesting exceeds the resolver's depth limit.
pub fn resolve(
    selections: &[FieldSelection],
    fragments: &[FragmentDefinition],
) -> Result<Vec<FieldSelection>, SelectionError> {
    Ok(FragmentResolver::new(fragments).resolve_spreads(selections)?)
}

/// Evaluate `@skip`/`@include` over an already-expanded selection set.
///
/// # Errors
///
/// Returns [`SelectionError::Directive`] if a condition references an undefined
/// variable, or is not a boolean.
#[allow(clippy::implicit_hasher)]
// Reason: the variables map is built by the runtime with the default hasher; a
// generic parameter here would leak into every caller for no benefit (same
// treatment as `value_json::resolve_variables`).
pub fn filter(
    selections: &[FieldSelection],
    variables: &HashMap<String, JsonValue>,
) -> Result<Vec<FieldSelection>, SelectionError> {
    Ok(DirectiveEvaluator::filter_selections(selections, variables)?)
}

/// Expand spreads and evaluate directives in one step — the field set the client
/// actually asked for.
///
/// # Errors
///
/// Returns [`SelectionError`] if expansion or directive evaluation fails.
#[allow(clippy::implicit_hasher)]
// Reason: the variables map is built by the runtime with the default hasher; a
// generic parameter here would leak into every caller for no benefit (same
// treatment as `value_json::resolve_variables`).
pub fn resolve_and_filter(
    selections: &[FieldSelection],
    fragments: &[FragmentDefinition],
    variables: &HashMap<String, JsonValue>,
) -> Result<Vec<FieldSelection>, SelectionError> {
    filter(&resolve(selections, fragments)?, variables)
}

/// Convert a request's `variables` payload into the map the directive evaluator
/// takes.
///
/// A non-object payload (including `null`) yields an empty map, matching the
/// GraphQL treatment of an omitted `variables` key.
#[must_use]
pub fn variables_map(variables: Option<&JsonValue>) -> HashMap<String, JsonValue> {
    match variables {
        Some(JsonValue::Object(map)) => map.iter().map(|(k, v)| (k.clone(), v.clone())).collect(),
        _ => HashMap::new(),
    }
}

#[cfg(test)]
#[path = "selection_set_tests.rs"]
mod selection_set_tests;
