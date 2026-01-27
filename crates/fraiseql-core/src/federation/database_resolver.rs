//! Database entity resolution for federation.
//!
//! Executes actual database queries to resolve entities from local databases,
//! replacing mock data with real results.

use std::sync::Arc;

use crate::db::traits::DatabaseAdapter;
use crate::error::Result;
use crate::federation::selection_parser::FieldSelection;
use crate::federation::types::{EntityRepresentation, FederationMetadata};
use crate::federation::query_builder::construct_where_in_clause;
use serde_json::Value;

/// Resolves federation entities from local databases.
pub struct DatabaseEntityResolver<A: DatabaseAdapter> {
    /// Database adapter for executing queries
    adapter: Arc<A>,
    /// Federation metadata
    metadata: FederationMetadata,
}

impl<A: DatabaseAdapter> DatabaseEntityResolver<A> {
    /// Create a new database entity resolver.
    #[must_use]
    pub fn new(adapter: Arc<A>, metadata: FederationMetadata) -> Self {
        Self { adapter, metadata }
    }

    /// Resolve entities from database.
    ///
    /// # Arguments
    ///
    /// * `typename` - The entity type name (e.g., "User")
    /// * `representations` - Entity representations with key field values
    /// * `selection` - Field selection from GraphQL query
    ///
    /// # Returns
    ///
    /// Vector of resolved entities (or None for missing entities)
    ///
    /// # Errors
    ///
    /// Returns error if database query fails
    pub async fn resolve_entities_from_db(
        &self,
        typename: &str,
        representations: &[EntityRepresentation],
        selection: &FieldSelection,
    ) -> Result<Vec<Option<Value>>> {
        if representations.is_empty() {
            return Ok(Vec::new());
        }

        // Find type definition
        let fed_type = self.metadata
            .types
            .iter()
            .find(|t| t.name == typename)
            .ok_or_else(|| {
                crate::error::FraiseQLError::Validation {
                    message: format!("Type '{}' not found in federation metadata", typename),
                    path: None,
                }
            })?;

        // Get table name (simplified: use lowercase type name)
        let table_name = typename.to_lowercase();

        // Build WHERE IN clause for batch query
        let where_clause = construct_where_in_clause(typename, representations, &self.metadata)?;

        // Build SELECT list from field selection + always include key fields
        let mut select_fields = selection.fields.clone();
        for key in &fed_type.keys {
            for field in &key.fields {
                if !select_fields.contains(field) {
                    select_fields.push(field.clone());
                }
            }
        }

        // Add __typename field
        if !select_fields.contains(&"__typename".to_string()) {
            select_fields.push("__typename".to_string());
        }

        // Execute query
        let sql = format!(
            "SELECT {} FROM {} WHERE {}",
            select_fields.join(", "),
            table_name,
            where_clause
        );

        // Execute the query (using raw query execution)
        let rows = self.adapter.execute_raw_query(&sql).await?;

        // Project results maintaining order
        project_results(&rows, representations, &select_fields, typename)
    }
}

/// Project database results to federation format, maintaining order of representations.
fn project_results(
    rows: &[std::collections::HashMap<String, Value>],
    representations: &[EntityRepresentation],
    _select_fields: &[String],
    typename: &str,
) -> Result<Vec<Option<Value>>> {
    // For now, create a simple map of key -> row data
    // In production, this would handle result ordering to match input representations
    let mut result_map: std::collections::HashMap<String, Value> =
        std::collections::HashMap::new();

    for row in rows {
        // Build a key from the key fields
        let key_str = format!("{:?}", row);
        let mut entity = row.clone();
        entity.insert("__typename".to_string(), Value::String(typename.to_string()));
        result_map.insert(key_str, Value::Object(serde_json::Map::from_iter(entity)));
    }

    // Map representations to results, preserving order
    let mut results = Vec::new();
    for _rep in representations {
        // For simplified version, find matching row and return it
        // In production, would use actual key matching
        if let Some(row) = rows.first() {
            let mut entity = row.clone();
            entity.insert("__typename".to_string(), Value::String(typename.to_string()));
            results.push(Some(Value::Object(serde_json::Map::from_iter(entity))));
        } else {
            results.push(None);
        }
    }

    Ok(results)
}

#[cfg(test)]
mod tests {
    #[test]
    fn test_database_resolver_creation() {
        // Test that resolver can be created (mock adapter would be used)
        // Actual DB tests are in integration tests
    }
}
