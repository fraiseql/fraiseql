//! Federation mutation execution.
//!
//! Executes GraphQL mutations on federation entities, handling both
//! local mutations (owned entities) and extended mutations (non-owned).

use std::sync::Arc;

use serde_json::Value;

use crate::{
    db::traits::DatabaseAdapter,
    error::Result,
    federation::{
        metadata_helpers::find_federation_type,
        mutation_query_builder::{build_delete_query, build_insert_query, build_update_query},
        types::FederationMetadata,
    },
};

/// Type of mutation being performed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum MutationType {
    /// CREATE mutation (INSERT)
    Create,
    /// UPDATE mutation
    Update,
    /// DELETE mutation
    Delete,
}

/// Determine the mutation type from the operation name.
fn determine_mutation_type(mutation_name: &str) -> Result<MutationType> {
    let lower = mutation_name.to_lowercase();

    if lower.starts_with("create") || lower.starts_with("add") {
        Ok(MutationType::Create)
    } else if lower.starts_with("update") || lower.starts_with("modify") {
        Ok(MutationType::Update)
    } else if lower.starts_with("delete") || lower.starts_with("remove") {
        Ok(MutationType::Delete)
    } else {
        // Default to UPDATE for mutations without clear type indicator
        Ok(MutationType::Update)
    }
}

/// Executes federation mutations.
#[derive(Clone)]
pub struct FederationMutationExecutor<A: DatabaseAdapter> {
    /// Database adapter for executing mutations
    #[allow(dead_code)]
    adapter:  Arc<A>,
    /// Federation metadata
    metadata: FederationMetadata,
}

impl<A: DatabaseAdapter> FederationMutationExecutor<A> {
    /// Create a new mutation executor.
    #[must_use]
    pub fn new(adapter: Arc<A>, metadata: FederationMetadata) -> Self {
        Self { adapter, metadata }
    }

    /// Execute a mutation on a locally-owned entity.
    ///
    /// # Arguments
    ///
    /// * `typename` - The entity type name
    /// * `mutation_name` - The mutation operation name (e.g., "updateUser", "createUser",
    ///   "deleteUser")
    /// * `variables` - Mutation variables/input
    ///
    /// # Returns
    ///
    /// The updated entity in federation format
    ///
    /// # Errors
    ///
    /// Returns error if mutation execution fails
    pub async fn execute_local_mutation(
        &self,
        typename: &str,
        mutation_name: &str,
        variables: &Value,
    ) -> Result<Value> {
        // Find entity type
        let fed_type = find_federation_type(typename, &self.metadata)?;

        // Determine mutation type from operation name
        let mutation_type = determine_mutation_type(mutation_name)?;

        // Build and execute SQL based on mutation type
        let sql = match mutation_type {
            MutationType::Create => build_insert_query(typename, variables, &self.metadata)?,
            MutationType::Update => build_update_query(typename, variables, &self.metadata)?,
            MutationType::Delete => build_delete_query(typename, variables, &self.metadata)?,
        };

        // Execute the mutation
        let _rows = self.adapter.execute_raw_query(&sql).await?;

        // Build response entity with key fields and updated values
        let mut response = serde_json::Map::new();
        response.insert("__typename".to_string(), Value::String(typename.to_string()));

        // Add key fields to response
        if let Some(key_directive) = fed_type.keys.first() {
            for key_field in &key_directive.fields {
                if let Some(value) = variables.get(key_field) {
                    response.insert(key_field.clone(), value.clone());
                }
            }
        }

        // Add updated fields to response
        if let Some(obj) = variables.as_object() {
            for (field, value) in obj {
                response.insert(field.clone(), value.clone());
            }
        }

        Ok(Value::Object(response))
    }

    /// Execute a mutation on an extended (non-owned) entity.
    ///
    /// Extended mutations are propagated to the authoritative subgraph that owns the entity.
    /// In Phase 6A, this returns a properly-structured response. Full implementation
    /// will add HTTP client support for actual remote subgraph communication.
    ///
    /// # Arguments
    ///
    /// * `typename` - The entity type name being mutated
    /// * `mutation_name` - The mutation operation name
    /// * `variables` - Mutation variables/input
    ///
    /// # Returns
    ///
    /// Federation-formatted response with:
    /// - `__typename`: The entity type
    /// - Key fields and mutated fields from variables
    /// - Mutation status indicator
    ///
    /// In full implementation, would:
    /// 1. Find the authoritative subgraph for the entity from federation metadata
    /// 2. Build GraphQL mutation query with proper field selection
    /// 3. Execute via HTTP to remote subgraph
    /// 4. Return the mutation response with resolved entity fields
    pub async fn execute_extended_mutation(
        &self,
        typename: &str,
        mutation_name: &str,
        variables: &Value,
    ) -> Result<Value> {
        // Build response entity with key fields and updated values
        let mut response = serde_json::Map::new();
        response.insert("__typename".to_string(), Value::String(typename.to_string()));

        // Add key fields from metadata if available
        if let Some(fed_type) = self.metadata.types.iter().find(|t| t.name == typename) {
            if let Some(key_directive) = fed_type.keys.first() {
                for key_field in &key_directive.fields {
                    if let Some(value) = variables.get(key_field) {
                        response.insert(key_field.clone(), value.clone());
                    }
                }
            }
        }

        // Add all variables to response (represents updated fields)
        if let Some(obj) = variables.as_object() {
            for (field, value) in obj {
                response.insert(field.clone(), value.clone());
            }
        }

        // Add mutation metadata
        response.insert("_mutation".to_string(), Value::String(mutation_name.to_string()));
        response.insert(
            "_remote_execution".to_string(),
            Value::Bool(true), // Indicates this would be executed on remote subgraph
        );

        Ok(Value::Object(response))
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn test_mutation_executor_creation() {
        // Test that executor can be created (mock adapter would be used)
        // Actual mutation tests are in integration tests
    }
}
