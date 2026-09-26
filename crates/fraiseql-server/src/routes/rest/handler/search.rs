//! What `?search=` means: the rows it matches, and the order they come back in.
//!
//! Both halves are built here, from one call to `TypeDefinition::searchable_fields`,
//! because they describe the *same* document. A rank computed over a different set
//! of fields from the predicate that matched would order rows by something nothing
//! searched — and there would be no error to notice, only a plausible wrong order.

use fraiseql_core::{
    db::{RelevanceOrder, to_snake_case},
    runtime::can_reference_field,
    schema::{SecurityConfig, TypeDefinition},
    security::SecurityContext,
};
use serde_json::json;

/// The WHERE clause and the ORDER BY a `?search=` request implies.
///
/// `None` when no searchable field is one the caller may reference. The extractor already
/// refuses `?search=` on a type with no searchable field at all, so `None` here means every
/// one is gated for this caller — which the caller must refuse, not read as "no search":
/// that would answer with the unfiltered relation.
pub(super) struct SearchPlan {
    /// `{"_or": [{"field": {"websearch_query": "query"}}, …]}`, or the single
    /// clause when the type has one searchable field.
    pub where_clause: serde_json::Value,
    /// The `ts_rank` ordering the same query implies (#1284), for a request that
    /// named no `?sort=` of its own.
    pub relevance:    RelevanceOrder,
}

/// Build the full-text plan for a search query string against a type.
///
/// # Why the relevance carries storage keys
///
/// The predicate's field names are lowered to `snake_case` JSONB storage keys by
/// `WhereClause::from_graphql_json`, and rendered as `data->>'key'`. The rank has
/// to extract the same expression, so it carries the keys already lowered.
///
/// # Only what the caller may read
///
/// A field the caller may not read may not influence the response (ruling AA 3): which rows
/// match a word, and in what order, answers a question about every field searched. So both
/// halves run over the searchable fields [`can_reference_field`] allows this caller — the
/// same rule the engine applies to a filter — and a caller holding the scope still searches
/// the scoped field.
pub(super) fn plan_search(
    query: &str,
    type_def: Option<&TypeDefinition>,
    security: Option<&SecurityConfig>,
    security_context: Option<&SecurityContext>,
) -> Option<SearchPlan> {
    let td = type_def?;
    let fields: Vec<_> = td
        .searchable_fields()
        .into_iter()
        .filter(|f| can_reference_field(security, f, security_context))
        .collect();
    if fields.is_empty() {
        return None;
    }

    let clauses: Vec<serde_json::Value> = fields
        .iter()
        .map(|f| json!({ f.name.as_str(): { "websearch_query": query } }))
        .collect();

    let where_clause = if clauses.len() == 1 {
        // Reason: len == 1 checked above; iterator always yields Some on a non-empty vec.
        clauses.into_iter().next().expect("len checked above")
    } else {
        json!({ "_or": clauses })
    };

    Some(SearchPlan {
        where_clause,
        relevance: RelevanceOrder {
            fields: fields.iter().map(|f| to_snake_case(f.name.as_str())).collect(),
            query:  query.to_string(),
        },
    })
}

#[cfg(test)]
mod tests;
