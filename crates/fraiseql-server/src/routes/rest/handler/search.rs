//! What `?search=` means: the rows it matches, and the order they come back in.
//!
//! Both halves are built here, from one call to `TypeDefinition::searchable_fields`,
//! because they describe the *same* document. A rank computed over a different set
//! of fields from the predicate that matched would order rows by something nothing
//! searched — and there would be no error to notice, only a plausible wrong order.

use fraiseql_core::{
    db::{RelevanceOrder, utils::to_snake_case},
    schema::TypeDefinition,
};
use serde_json::json;

/// The WHERE clause and the ORDER BY a `?search=` request implies.
///
/// `None` when the type has no searchable fields — the extractor already refuses
/// `?search=` in that case, so this is the shape of "nothing to search", not a
/// silent drop.
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
pub(super) fn plan_search(query: &str, type_def: Option<&TypeDefinition>) -> Option<SearchPlan> {
    let td = type_def?;
    let fields = td.searchable_fields();
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
