//! What a successful mutation invalidates, decided once and from the schema.
//!
//! # Why this is a sweep and not a classification
//!
//! The invalidation this replaced tried to be precise: a CREATE evicted only
//! entries flagged as "list queries", an UPDATE with an `entity_id` evicted only
//! entries whose rows already contained that UUID. Precision is worth having,
//! but only if the entries it *keeps* are provably unaffected by the write —
//! and neither signal proved that:
//!
//! - **Row count is not query shape (#742).** `is_list_query` was `result.len() > 1`, so a filtered
//!   list that currently matches nothing, and one that matches a single row, were not lists. Those
//!   are precisely the entries a CREATE is most likely to change.
//! - **`returns_list` would not have been sound either.** A query that returns a single object —
//!   `currentUser`, `latestPost`, `userByEmail` — is still affected by a CREATE or by an UPDATE to
//!   a row it does not currently return. "Not a list" does not mean "not affected"; the compiled
//!   schema has no flag that does.
//! - **Containing the mutated id is not the affected set (#763).** An UPDATE that moves a row
//!   *into* a filtered list changes an entry that never mentioned the row. Entity-aware eviction
//!   cannot see it by construction.
//! - **The payload's `entity_id` is not the operation kind (#741).** A CREATE that stamps the new
//!   row's id took the UPDATE branch and evicted nothing: no entry cached before the row existed
//!   can contain its id.
//!
//! So the plan is: **every successful mutation evicts every cached entry for the
//! views it touches.** Over-eviction costs a re-read; under-eviction serves a
//! wrong answer for the entry's whole TTL — forever for the
//! `cache_ttl_seconds = 0` entries documented as "mutation-invalidated only".
//! Entity-aware eviction is kept as a *supplement* for the case where no view
//! resolves, never as a substitute for the sweep.

use fraiseql_db::ViewName;

use super::resolve_cascade_views;
use crate::{
    runtime::mutation_result::MutationOutcome,
    schema::{CompiledSchema, MutationDefinition},
};

/// Everything a single successful mutation must evict.
#[derive(Debug, Default, PartialEq, Eq)]
pub(super) struct InvalidationPlan {
    /// Views to sweep: every cached entry reading one of these is stale.
    ///
    /// De-duplicated, in discovery order.
    pub views: Vec<ViewName>,

    /// `(entity_type, entity_id)` for entity-precise eviction, when the payload
    /// carried both. Always a *subset* of what the view sweep covers when the
    /// entity's view resolves — it exists for the case where it does not.
    pub entity: Option<(String, String)>,
}

/// Append `view` unless an equal name is already planned.
fn push_view(views: &mut Vec<ViewName>, view: ViewName) {
    if !views.iter().any(|v| v.as_str() == view.as_str()) {
        views.push(view);
    }
}

/// The view a type reads from, if it declares one.
fn view_of_type(schema: &CompiledSchema, type_name: &str) -> Option<ViewName> {
    schema
        .types
        .iter()
        .find(|t| t.name == type_name)
        .filter(|t| !t.sql_source.as_str().is_empty())
        .map(|t| ViewName::from(t.sql_source.as_str()))
}

/// Decide what a mutation outcome invalidates.
///
/// A pure function of `(mutation_def, outcome, schema)`: no cache, no adapter,
/// no I/O — so the decision is testable on its own, which is what the three
/// defects above needed and did not have.
///
/// Views are collected from every declaration that can name one:
///
/// 1. `invalidates_views` — the developer said so explicitly.
/// 2. The mutation's return type, and the entity type it wraps if it is a payload
///    (`CreateUserSuccess` → `User`).
/// 3. The `entity_type` the function stamped on `mutation_response`, which is the only source when
///    the return type is an unbacked payload.
/// 4. Cascade side-effects on other entity types, for a `cascade` mutation.
///
/// A failed mutation wrote nothing, so it invalidates nothing.
pub(super) fn plan_invalidation(
    mutation_def: &MutationDefinition,
    outcome: &MutationOutcome,
    schema: &CompiledSchema,
) -> InvalidationPlan {
    let MutationOutcome::Success {
        entity_type,
        entity_id,
        cascade,
        ..
    } = outcome
    else {
        return InvalidationPlan::default();
    };

    let mut views: Vec<ViewName> = Vec::new();

    // 1. Declared.
    for declared in &mutation_def.invalidates_views {
        push_view(&mut views, ViewName::from(declared.as_str()));
    }

    // 2. The return type, and the entity a payload type wraps.
    if let Some(view) = view_of_type(schema, &mutation_def.return_type) {
        push_view(&mut views, view);
    }
    if let Some(inner) = super::payload_entity_type(&mutation_def.return_type, schema) {
        if let Some(view) = view_of_type(schema, &inner) {
            push_view(&mut views, view);
        }
    }

    // 3. The entity type the function stamped. For a mutation returning an unbacked payload this is
    //    the only thing that names the written view.
    if let Some(etype) = entity_type.as_deref() {
        if let Some(view) = view_of_type(schema, etype) {
            push_view(&mut views, view);
        }
    }

    // 4. Cascade side-effects on unrelated entity types.
    if mutation_def.cascade {
        if let Some(cascade_json) = cascade.as_ref() {
            for view in resolve_cascade_views(cascade_json, schema) {
                push_view(&mut views, view);
            }
        }
    }

    let entity = match (entity_type.as_deref(), entity_id.as_deref()) {
        (Some(etype), Some(eid)) => Some((etype.to_string(), eid.to_string())),
        _ => None,
    };

    InvalidationPlan { views, entity }
}

#[cfg(test)]
#[path = "invalidation_tests.rs"]
mod invalidation_tests;
