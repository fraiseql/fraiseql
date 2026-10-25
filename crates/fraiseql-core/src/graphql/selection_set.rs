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
/// the document is circular, or nesting exceeds `max_depth` — the executor's
/// effective `max_query_depth`, so the resolver never refuses a document the
/// depth gate admits, and never admits one it would refuse by more than the
/// level or two it counts from.
pub fn resolve(
    selections: &[FieldSelection],
    fragments: &[FragmentDefinition],
    max_depth: u32,
) -> Result<Vec<FieldSelection>, SelectionError> {
    Ok(FragmentResolver::new(fragments)
        .with_max_depth(max_depth)
        .resolve_spreads(selections)?)
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
    Ok(merge_by_response_key(DirectiveEvaluator::filter_selections(
        selections, variables,
    )?))
}

/// Merge the selections that share a response key into one, at every level (GraphQL
/// § 6.3.2, CollectFields): the first occurrence keeps its position and takes the later
/// ones' sub-selections.
///
/// Runs after `@skip`/`@include`, which apply to each occurrence on its own. Only
/// occurrences of the same field with the same arguments merge. Two that differ are a
/// § 5.3.2 conflict, which is decided per parent type, and named spreads lose their type
/// condition at expansion, so they are left as written.
fn merge_by_response_key(selections: Vec<FieldSelection>) -> Vec<FieldSelection> {
    let mut merged: Vec<FieldSelection> = Vec::with_capacity(selections.len());
    for selection in selections {
        let same_field = merged.iter_mut().find(|m| {
            m.response_key() == selection.response_key()
                && m.name == selection.name
                && m.arguments == selection.arguments
        });
        match same_field {
            Some(first) => first.nested_fields.extend(selection.nested_fields),
            None => merged.push(selection),
        }
    }
    for field in &mut merged {
        if !field.nested_fields.is_empty() {
            field.nested_fields = merge_by_response_key(std::mem::take(&mut field.nested_fields));
        }
    }
    merged
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
    max_depth: u32,
) -> Result<Vec<FieldSelection>, SelectionError> {
    filter(&resolve(selections, fragments, max_depth)?, variables)
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
