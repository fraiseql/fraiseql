//! Schema introspection endpoint.

use axum::{Json, extract::State, response::IntoResponse};
use fraiseql_core::db::traits::DatabaseAdapter;
use serde::Serialize;
use tracing::debug;

use crate::routes::graphql::AppState;

/// Introspection response.
#[derive(Debug, Serialize)]
pub struct IntrospectionResponse {
    /// Schema types.
    pub types: Vec<TypeInfo>,

    /// Schema queries.
    pub queries: Vec<QueryInfo>,

    /// Schema mutations.
    pub mutations: Vec<MutationInfo>,
}

/// Type information.
#[derive(Debug, Serialize)]
pub struct TypeInfo {
    /// Type name.
    pub name: String,

    /// Type description.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,

    /// Field count.
    pub field_count: usize,
}

/// Query information.
#[derive(Debug, Serialize)]
pub struct QueryInfo {
    /// Query name.
    pub name: String,

    /// Return type.
    pub return_type: String,

    /// Returns list.
    pub returns_list: bool,

    /// Query description.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
}

/// Mutation information.
#[derive(Debug, Serialize)]
pub struct MutationInfo {
    /// Mutation name.
    pub name: String,

    /// Return type.
    pub return_type: String,

    /// Mutation description.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
}

/// Introspection handler.
///
/// Returns schema structure for debugging and tooling.
///
/// # Security Note
///
/// In production, this endpoint should be disabled or require authentication.
pub async fn introspection_handler<A: DatabaseAdapter + Clone + Send + Sync + 'static>(
    State(state): State<AppState<A>>,
) -> impl IntoResponse {
    debug!("Introspection requested");

    let schema = state.executor.schema();

    let types: Vec<TypeInfo> = schema
        .types
        .iter()
        .map(|t| TypeInfo {
            name:        t.name.clone(),
            description: t.description.clone(),
            field_count: t.fields.len(),
        })
        .collect();

    let queries: Vec<QueryInfo> = schema
        .queries
        .iter()
        .map(|q| QueryInfo {
            name:         q.name.clone(),
            return_type:  q.return_type.clone(),
            returns_list: q.returns_list,
            description:  q.description.clone(),
        })
        .collect();

    let mutations: Vec<MutationInfo> = schema
        .mutations
        .iter()
        .map(|m| MutationInfo {
            name:        m.name.clone(),
            return_type: m.return_type.clone(),
            description: m.description.clone(),
        })
        .collect();

    Json(IntrospectionResponse {
        types,
        queries,
        mutations,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_type_info_serialization() {
        let type_info = TypeInfo {
            name:        "User".to_string(),
            description: Some("A user in the system".to_string()),
            field_count: 3,
        };

        let json = serde_json::to_string(&type_info).unwrap();
        assert!(json.contains("User"));
        assert!(json.contains("field_count"));
    }
}
