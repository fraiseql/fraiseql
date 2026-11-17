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
//! A type's own declaration holds wherever the type is read
//! ([`type_scope`](CompiledSchema::type_scope)), but a root read of a query that returns
//! the type applies the query's `inject_params` alone. So every query that reads a relation
//! and can return the type, by name, as a member of a union or an interface, or as the rows
//! it counts, must be scoped exactly as the type is: the same columns, each from
//! the same source. Anything else, a column the query drops, adds, or takes from another
//! source, or a query with no scope at all, is refused at load, naming the query, the type
//! and both scopes.
//!
//! Agreement is *not* a violation: stating the same scoping in both places is redundant
//! but consistent, and refusing it would reject schemas whose behaviour is unambiguous.

use indexmap::IndexMap;

use super::CompiledSchema;
use crate::schema::InjectedParamSource;

impl CompiledSchema {
    /// Queries that return a type scoped by its own `inject_params` under another scope.
    ///
    /// Returns one human-readable message per violation, empty when the schema is
    /// enforceable. Called by [`CompiledSchema::from_json`] so a schema whose scoping
    /// cannot be honoured as declared never loads. Every query that reads a relation and can
    /// return the type is checked: by name, as a member of the union or interface it
    /// returns, or as the rows it counts. A function root binds its `inject_params` to the
    /// function's parameters, not to rows, and is not. Mutations build their payload in
    /// their function; a level of the type inside it is gated by
    /// [`type_scope`](Self::type_scope).
    #[must_use]
    pub fn type_inject_violations(&self) -> Vec<String> {
        let mut violations = Vec::new();
        for type_def in &self.types {
            let own = &type_def.inject_params;
            if own.is_empty() {
                continue;
            }
            let name = type_def.name.as_str();
            let reads = self.queries.iter().filter(|q| {
                q.reads_relation().is_some()
                    && (q.return_type == name
                        || (self.find_type(&q.return_type).is_none()
                            && self
                                .possible_types(&q.return_type)
                                .is_some_and(|members| members.contains(&name))))
            });
            for query in reads {
                if same_scope(&query.inject_params, own) {
                    continue;
                }
                violations.push(format!(
                    "query '{}' returns type '{}', which is scoped by its own `inject_params` \
                     ({}), but the query is scoped by {}. A type's own scope holds wherever \
                     the type is read, and a root read of the query would apply only its \
                     own: declare the same `inject_params` on both, or drop the type's.",
                    query.name,
                    type_def.name,
                    describe_scope(own),
                    if query.inject_params.is_empty() {
                        "nothing".to_string()
                    } else {
                        describe_scope(&query.inject_params)
                    },
                ));
            }
        }
        violations
    }
}

/// Whether two scopes bind the same columns from the same sources, in any order.
fn same_scope(
    a: &IndexMap<String, InjectedParamSource>,
    b: &IndexMap<String, InjectedParamSource>,
) -> bool {
    a.len() == b.len() && a.iter().all(|(column, source)| b.get(column) == Some(source))
}

/// A scope rendered for an error message: each column with where its value comes from.
fn describe_scope(scope: &IndexMap<String, InjectedParamSource>) -> String {
    scope
        .iter()
        .map(|(column, source)| format!("`{column}` from {}", describe_source(source)))
        .collect::<Vec<_>>()
        .join(", ")
}

/// A source rendered for an error message an author has to act on.
///
/// `InjectedParamSource` is `#[non_exhaustive]`, which binds downstream crates but not
/// this one — so a variant added later breaks this match at compile time here, which is
/// what we want: a new source must be given wording rather than silently rendering as
/// nothing.
pub(super) fn describe_source(source: &InjectedParamSource) -> String {
    match source {
        InjectedParamSource::Jwt(claim) => format!("jwt claim '{claim}'"),
        InjectedParamSource::JwtOptional(claim) => format!("optional jwt claim '{claim}'"),
        InjectedParamSource::Enrichment(field) => format!("enriched field '{field}'"),
    }
}
