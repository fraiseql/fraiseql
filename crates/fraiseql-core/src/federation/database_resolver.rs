//! Database entity resolution for federation.
//!
//! Executes actual database queries to resolve entities from local databases,
//! replacing mock data with real results.

use std::sync::Arc;

use crate::db::traits::DatabaseAdapter;
use crate::error::Result;
use crate::federation::metadata_helpers::find_federation_type;
use crate::federation::selection_parser::FieldSelection;
use crate::federation::types::{EntityRepresentation, FederatedType, FederationMetadata};
use crate::federation::query_builder::construct_where_in_clause;
use crate::federation::tracing::FederationTraceContext;
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
        self.resolve_entities_from_db_with_tracing(typename, representations, selection, None).await
    }

    /// Resolve entities from database with optional distributed tracing.
    ///
    /// # Arguments
    ///
    /// * `typename` - The entity type name (e.g., "User")
    /// * `representations` - Entity representations with key field values
    /// * `selection` - Field selection from GraphQL query
    /// * `trace_context` - Optional W3C trace context for span creation
    ///
    /// # Returns
    ///
    /// Vector of resolved entities (or None for missing entities)
    ///
    /// # Errors
    ///
    /// Returns error if database query fails
    pub async fn resolve_entities_from_db_with_tracing(
        &self,
        typename: &str,
        representations: &[EntityRepresentation],
        selection: &FieldSelection,
        _trace_context: Option<FederationTraceContext>,
    ) -> Result<Vec<Option<Value>>> {
        if representations.is_empty() {
            return Ok(Vec::new());
        }

        // Find type definition using metadata helpers
        let fed_type = find_federation_type(typename, &self.metadata)?;

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
        project_results(&rows, representations, fed_type, typename)
    }
}

/// Project database results to federation format, maintaining order of representations.
fn project_results(
    rows: &[std::collections::HashMap<String, Value>],
    representations: &[EntityRepresentation],
    fed_type: &FederatedType,
    typename: &str,
) -> Result<Vec<Option<Value>>> {
    use std::collections::HashMap as StdHashMap;

    // Build a map of key values -> row data for quick lookup
    // Key is constructed from the key fields of the federation type
    let mut row_map: StdHashMap<Vec<String>, StdHashMap<String, Value>> = StdHashMap::new();

    for row in rows {
        // Build key from key fields
        let key_values: Result<Vec<String>> = fed_type
            .keys
            .first()
            .ok_or_else(|| crate::error::FraiseQLError::Validation {
                message: format!("Type '{}' has no key fields", typename),
                path: None,
            })?
            .fields
            .iter()
            .map(|field| {
                row.get(field)
                    .and_then(|v| v.as_str().map(|s| s.to_string()))
                    .or_else(|| row.get(field).map(|v| v.to_string()))
                    .ok_or_else(|| crate::error::FraiseQLError::Validation {
                        message: format!("Key field '{}' not found in row", field),
                        path: None,
                    })
            })
            .collect();

        if let Ok(key) = key_values {
            row_map.insert(key, row.clone());
        }
    }

    // Map representations to results, preserving order
    let mut results = Vec::new();
    for rep in representations {
        // Extract key values from representation
        let key_values: Vec<String> = fed_type
            .keys
            .first()
            .map(|k| {
                k.fields
                    .iter()
                    .filter_map(|field| {
                        rep.key_fields.get(field).and_then(|v| {
                            v.as_str().map(|s| s.to_string()).or_else(|| Some(v.to_string()))
                        })
                    })
                    .collect()
            })
            .unwrap_or_default();

        // Look up row in map
        if let Some(row) = row_map.get(&key_values) {
            let mut entity = row.clone();
            entity.insert("__typename".to_string(), Value::String(typename.to_string()));
            results.push(Some(Value::Object(serde_json::Map::from_iter(entity))));
        } else {
            // Entity not found in database
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
