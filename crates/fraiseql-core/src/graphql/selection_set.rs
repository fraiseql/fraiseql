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

/// A document's expanded root selection set, and the fields it collects under each set
/// of `@skip`/`@include` outcomes a request has produced.
///
/// What [`filter`] returns depends on the written set and on which fields its directives
/// keep, nothing else: the walk below takes the same path `filter` does and records each
/// directive-bearing field's outcome, so two requests recording the same outcomes collect
/// the same fields and the first one's result answers both. A document with no directives
/// has one outcome set, the empty one, and collects its fields once.
#[derive(Debug)]
pub struct CollectedFields {
    written:   Vec<FieldSelection>,
    collected: std::sync::Mutex<Vec<(Outcomes, std::sync::Arc<Collected>)>>,
}

/// The fields one set of directive outcomes collects, and what is kept of them.
///
/// Whether a check that depends on those fields alone has passed on them, and the
/// projection SQL they read through in the locale chain that first asked for it.
#[derive(Debug)]
pub struct Collected {
    fields:     std::sync::Arc<[FieldSelection]>,
    checked:    std::sync::OnceLock<()>,
    projection: std::sync::OnceLock<KeptProjection>,
}

/// A projection, and the request locale chain it was built in.
type KeptProjection = (Option<Vec<String>>, std::sync::Arc<crate::schema::SqlProjectionHint>);

impl Collected {
    fn new(fields: Vec<FieldSelection>) -> std::sync::Arc<Self> {
        std::sync::Arc::new(Self {
            fields:     fields.into(),
            checked:    std::sync::OnceLock::new(),
            projection: std::sync::OnceLock::new(),
        })
    }

    /// The projection these fields read through in the request locale chain `chain`,
    /// built by `build` the first time it is asked for in that chain. Only for a projection
    /// that depends on the fields and the chain alone. The first chain's projection is
    /// kept; one built in another chain serves its own request and is not kept, nor is a
    /// failure, so a request that cannot be projected fails again.
    ///
    /// # Errors
    ///
    /// Whatever `build` returns.
    pub fn projection_once<E>(
        &self,
        chain: Option<&[String]>,
        build: impl FnOnce(&[FieldSelection]) -> Result<crate::schema::SqlProjectionHint, E>,
    ) -> Result<std::sync::Arc<crate::schema::SqlProjectionHint>, E> {
        if let Some((kept_chain, hint)) = self.projection.get() {
            if kept_chain.as_deref() == chain {
                return Ok(std::sync::Arc::clone(hint));
            }
            return build(&self.fields).map(std::sync::Arc::new);
        }
        let hint = std::sync::Arc::new(build(&self.fields)?);
        let (kept_chain, kept) = self
            .projection
            .get_or_init(|| (chain.map(<[String]>::to_vec), std::sync::Arc::clone(&hint)));
        Ok(if kept_chain.as_deref() == chain {
            std::sync::Arc::clone(kept)
        } else {
            hint
        })
    }

    /// The collected fields.
    #[must_use]
    pub const fn fields(&self) -> &std::sync::Arc<[FieldSelection]> {
        &self.fields
    }

    /// Run `check` on these fields unless it has passed on them before. Only for a check
    /// whose verdict depends on the fields alone: a failure is not kept, so a refused
    /// request is refused again.
    ///
    /// # Errors
    ///
    /// Whatever `check` returns.
    pub fn check_once<E>(
        &self,
        check: impl FnOnce(&[FieldSelection]) -> Result<(), E>,
    ) -> Result<(), E> {
        if self.checked.get().is_none() {
            check(&self.fields)?;
            let _ = self.checked.set(());
        }
        Ok(())
    }
}

/// The `@skip`/`@include` outcomes of one request, in the order [`filter`] evaluates
/// them: bit `i` is whether the `i`th directive-bearing field it reached is kept.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Outcomes {
    bits:  u64,
    count: u32,
}

impl CollectedFields {
    /// Outcome sets kept per document; a request producing another one collects afresh.
    const KEPT: usize = 16;

    /// A document's root selection set, spreads expanded, directives not yet evaluated.
    #[must_use]
    pub const fn new(written: Vec<FieldSelection>) -> Self {
        Self {
            written,
            collected: std::sync::Mutex::new(Vec::new()),
        }
    }

    /// The root selection set as written, spreads expanded.
    #[must_use]
    pub fn written(&self) -> &[FieldSelection] {
        &self.written
    }

    /// The fields this request collects: [`filter`] over the written set, shared with
    /// every earlier request whose directives had the same outcomes.
    ///
    /// # Errors
    ///
    /// Returns [`SelectionError::Directive`] exactly when [`filter`] would.
    #[allow(clippy::implicit_hasher)]
    // Reason: as on `filter`, which this answers for.
    pub fn collect(
        &self,
        variables: &HashMap<String, JsonValue>,
    ) -> Result<std::sync::Arc<Collected>, SelectionError> {
        let mut outcomes = Some(Outcomes { bits: 0, count: 0 });
        record_outcomes(&self.written, variables, &mut outcomes)?;
        let Some(outcomes) = outcomes else {
            return Ok(Collected::new(filter(&self.written, variables)?));
        };
        let mut collected =
            self.collected.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
        if let Some((_, entry)) = collected.iter().find(|(kept, _)| *kept == outcomes) {
            return Ok(entry.clone());
        }
        let entry = Collected::new(filter(&self.written, variables)?);
        if collected.len() < Self::KEPT {
            collected.push((outcomes, entry.clone()));
        }
        Ok(entry)
    }
}

/// Walk `selections` as [`DirectiveEvaluator::filter_selections`] does, evaluating the
/// same directives in the same order and descending only into kept fields, and append
/// each directive-bearing field's outcome. `None` once there are more than fit.
fn record_outcomes(
    selections: &[FieldSelection],
    variables: &HashMap<String, JsonValue>,
    outcomes: &mut Option<Outcomes>,
) -> Result<(), DirectiveError> {
    for selection in selections {
        if !selection.directives.is_empty() {
            let kept = DirectiveEvaluator::evaluate_directives(selection, variables)?;
            if let Some(o) = outcomes {
                if o.count == u64::BITS {
                    *outcomes = None;
                } else {
                    o.bits |= u64::from(kept) << o.count;
                    o.count += 1;
                }
            }
            if !kept {
                continue;
            }
        }
        record_outcomes(&selection.nested_fields, variables, outcomes)?;
    }
    Ok(())
}

#[cfg(test)]
#[path = "selection_set_tests.rs"]
mod selection_set_tests;
