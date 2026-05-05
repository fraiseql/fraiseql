//! Full-text search WHERE clause builder.

use fraiseql_core::schema::TypeDefinition;
use serde_json::json;

/// Build a FTS WHERE clause from a search query string and the type's searchable fields.
///
/// Produces `{"_or": [{"field": {"websearch_query": "query"}}, ...]}` for each
/// searchable field.  Returns `None` if the type has no searchable fields.
pub(super) fn build_fts_where_clause(
    query: &str,
    type_def: Option<&TypeDefinition>,
) -> Option<serde_json::Value> {
    let td = type_def?;
    let fields = td.searchable_fields();
    if fields.is_empty() {
        return None;
    }

    let clauses: Vec<serde_json::Value> = fields
        .iter()
        .map(|f| json!({ f.name.as_str(): { "websearch_query": query } }))
        .collect();

    if clauses.len() == 1 {
        // Reason: len == 1 checked above; iterator always yields Some on a non-empty vec.
        Some(clauses.into_iter().next().expect("len checked above"))
    } else {
        Some(json!({ "_or": clauses }))
    }
}
