//! Type-level `inject_params`: the tenant/owner scoping a federation entity declares on
//! itself, and rejection of the declaration pairs the runtime cannot honour.
//!
//! # What was missing (#1142)
//!
//! `inject_params` — the mechanism carrying tenant/owner scoping into a read — lived only
//! on an *operation*. An entity that no query returns, reachable only through `_entities`
//! with its relation riding on the type-level `sql_source` (#507), therefore had nowhere
//! to declare scoping at all. Its `requires_role` half *was* honoured from the type
//! (#1030), so an author could reasonably read the surrounding annotation as covering the
//! whole entity; on that path the tenancy half silently covered nothing, with no
//! compile-time or load-time signal. That is the "declared but unenforced" shape this
//! project refuses on principle.
//!
//! [`TypeDefinition::inject_params`](crate::schema::TypeDefinition::inject_params) gives
//! the declaration a home, and both `_entities` consumers — the deny gate and the per-row
//! predicate builder — read it behind the backing query.
//!
//! # The shape this rejects
//!
//! Because the two declarations *merge*, a column declared by both a query and its return
//! type from two different sources is a contradiction the merge cannot resolve: picking
//! either one enforces a rule the author did not necessarily intend, and does so silently.
//! Such a schema is refused at load, naming the query, the type, the column and both
//! sources — the same treatment
//! [`type_role_violations`](CompiledSchema::type_role_violations) gives a conflicting
//! role pair.
//!
//! Agreement is *not* a violation: stating the same scoping in both places is redundant
//! but consistent, and refusing it would reject schemas whose behaviour is unambiguous.

use super::CompiledSchema;
use crate::schema::InjectedParamSource;

impl CompiledSchema {
    /// Scoping declarations a type and its backing query make that contradict each other.
    ///
    /// Returns one human-readable message per violation, empty when the schema is
    /// enforceable. Called by [`CompiledSchema::from_json`] so a schema whose scoping
    /// cannot be honoured as declared never loads.
    ///
    /// Scoped to the query the `_entities` path would actually consult — the first
    /// returning the type with a `sql_source` — so this can never refuse a pair the
    /// runtime would have ignored. Mutations are deliberately out of scope: entity
    /// resolution merges no mutation's declaration, so no mutation/type pair can
    /// contradict one another in a way that changes what a read enforces.
    #[must_use]
    pub fn type_inject_violations(&self) -> Vec<String> {
        let mut violations = Vec::new();

        for type_def in &self.types {
            if type_def.inject_params.is_empty() {
                continue;
            }
            let Some(query) = self
                .queries
                .iter()
                .find(|q| q.return_type == type_def.name.as_str() && q.sql_source.is_some())
            else {
                continue;
            };

            for (column, type_source) in &type_def.inject_params {
                let Some(query_source) = query.inject_params.get(column) else {
                    continue;
                };
                if query_source == type_source {
                    continue;
                }
                violations.push(format!(
                    "query '{}' and type '{}' both scope column '{column}', but from \
                     different sources — the query from {}, the type from {}. The \
                     _entities path merges both declarations and cannot choose between \
                     them; give them the same source, or drop one.",
                    query.name,
                    type_def.name,
                    describe_source(query_source),
                    describe_source(type_source),
                ));
            }
        }

        violations
    }
}

/// A source rendered for an error message an author has to act on.
///
/// `InjectedParamSource` is `#[non_exhaustive]`, which binds downstream crates but not
/// this one — so a variant added later breaks this match at compile time here, which is
/// what we want: a new source must be given wording rather than silently rendering as
/// nothing.
fn describe_source(source: &InjectedParamSource) -> String {
    match source {
        InjectedParamSource::Jwt(claim) => format!("jwt claim '{claim}'"),
        InjectedParamSource::Enrichment(field) => format!("enriched field '{field}'"),
    }
}
