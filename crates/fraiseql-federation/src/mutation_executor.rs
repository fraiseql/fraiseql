//! Federation mutation execution.
//!
//! Executes GraphQL mutations on federation entities, handling both
//! local mutations (owned entities) and extended mutations (non-owned).

use std::{collections::HashMap, sync::Arc};

use fraiseql_db::{traits::DatabaseAdapter, utils::to_snake_case};
use fraiseql_error::Result;
use serde_json::Value;

use crate::{
    metadata_helpers::find_federation_type,
    mutation_query_builder::{build_delete_query, build_insert_query, build_update_query},
    types::{FederatedType, FederationMetadata},
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
///
/// # Errors
///
/// Returns `FraiseQLError::Validation` when the operation name does not begin
/// with a recognised verb. The previous behaviour defaulted an unrecognised name
/// to `Update` (M-fed-mut-executor), so a typo or an unsupported operation
/// silently issued an `UPDATE` against the entity table. It now fails loud.
fn determine_mutation_type(mutation_name: &str) -> Result<MutationType> {
    let lower = mutation_name.to_lowercase();

    if lower.starts_with("create") || lower.starts_with("add") {
        Ok(MutationType::Create)
    } else if lower.starts_with("update") || lower.starts_with("modify") {
        Ok(MutationType::Update)
    } else if lower.starts_with("delete") || lower.starts_with("remove") {
        Ok(MutationType::Delete)
    } else {
        Err(fraiseql_error::FraiseQLError::Validation {
            message: format!(
                "Cannot determine mutation type from operation name '{mutation_name}': \
                 expected a name beginning with create/add, update/modify, or delete/remove"
            ),
            path:    None,
        })
    }
}

/// Build the federation entity response from a read-back row: `__typename`
/// followed by every column the database returned.
fn build_entity_response(typename: &str, row: HashMap<String, Value>) -> Value {
    let mut map = serde_json::Map::with_capacity(row.len() + 1);
    map.insert("__typename".to_string(), Value::String(typename.to_string()));
    map.extend(row);
    Value::Object(map)
}

/// Best-effort identifier for a not-found error: the value of the entity's first
/// key field from the input variables, or `<unknown>` if absent.
fn key_identifier(fed_type: &FederatedType, variables: &Value) -> String {
    fed_type
        .keys
        .first()
        .and_then(|k| k.fields.first())
        .and_then(|field| variables.get(field))
        .map_or_else(
            || "<unknown>".to_string(),
            |v| v.as_str().map_or_else(|| v.to_string(), ToString::to_string),
        )
}

/// Recase a mutation's input variable keys to their canonical `snake_case` column
/// names when the GraphQL surface is camelCase (`recase` = true).
///
/// The federation mutation builders treat each input key as a SQL column
/// identifier and the `@key` field names as already-canonical, so a camelCase
/// surface (`s3Key`, `dns1Id`) must be reversed to the stored column name
/// (`s3_key`, `dns_1_id`) — otherwise the generated `INSERT`/`UPDATE` quotes a
/// column that does not exist and the write silently misses (#400). Uses the same
/// acronym-aware [`to_snake_case`] as the read path, so `s3Key` → `s3_key`
/// (acronym kept whole) while `dns1Id` → `dns_1_id`.
///
/// Federation mutations are scalar-only (`value_to_sql_literal` rejects objects),
/// so only the top-level keys are recased; values are left untouched. Idempotent
/// on already-`snake_case` keys, and a no-op under `Preserve` (`recase` = false).
fn canonicalize_input_keys(variables: &Value, recase: bool) -> Value {
    match variables.as_object() {
        Some(obj) if recase => {
            let recased: serde_json::Map<String, Value> =
                obj.iter().map(|(k, v)| (to_snake_case(k), v.clone())).collect();
            Value::Object(recased)
        },
        _ => variables.clone(),
    }
}

/// Executes federation mutations.
#[derive(Clone)]
pub struct FederationMutationExecutor<A: DatabaseAdapter> {
    /// Database adapter for executing mutations
    adapter:           Arc<A>,
    /// Federation metadata
    metadata:          FederationMetadata,
    /// Recase mutation input keys to canonical `snake_case` before they become SQL
    /// column identifiers. Set from the schema's `naming_convention == CamelCase`
    /// (#400); when false (the `Preserve` default) keys pass through verbatim.
    recase_input_keys: bool,
}

impl<A: DatabaseAdapter> FederationMutationExecutor<A> {
    /// Create a new mutation executor.
    ///
    /// `recase_input_keys` should be set when the schema's GraphQL surface is
    /// camelCase (`naming_convention == CamelCase`) so mutation input keys are
    /// reversed to canonical `snake_case` column names before SQL generation;
    /// pass `false` for a `Preserve`-convention schema.
    #[must_use]
    pub const fn new(
        adapter: Arc<A>,
        metadata: FederationMetadata,
        recase_input_keys: bool,
    ) -> Self {
        Self {
            adapter,
            metadata,
            recase_input_keys,
        }
    }

    /// The federation metadata this executor resolves entity types against.
    ///
    /// Used by the saga remote-dispatch path
    /// ([`SagaExecutor::dispatch_step`](crate::saga_executor::SagaExecutor)) to
    /// build the outgoing GraphQL mutation and project its response — the same
    /// metadata the local path uses to locate the entity table.
    #[cfg(feature = "unstable-saga")]
    #[must_use]
    pub(crate) const fn metadata(&self) -> &FederationMetadata {
        &self.metadata
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
    /// The mutated row read back from the database, in federation format
    /// (`__typename` plus every returned column).
    ///
    /// The mutation SQL uses `RETURNING *`, so the response reflects the actual
    /// database state — including DB-computed defaults — rather than echoing the
    /// input (#430). A `0`-row `UPDATE`/`DELETE` means the targeted entity does
    /// not exist and returns `FraiseQLError::NotFound` instead of a fabricated
    /// success. Unknown operation names fail loud (`determine_mutation_type`).
    ///
    /// # Errors
    ///
    /// Returns error if the operation name is unrecognised, the entity type is
    /// unknown, query construction fails, mutation execution fails, or an
    /// `UPDATE`/`DELETE` matched no row (`FraiseQLError::NotFound`).
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

        // Recase the input keys to canonical snake_case column names before the
        // builders turn them into SQL identifiers and look up the `@key` field
        // (which is already canonical). No-op under the `Preserve` default (#400).
        let recased = canonicalize_input_keys(variables, self.recase_input_keys);
        let variables = &recased;

        // Build and execute SQL based on mutation type
        let sql = match mutation_type {
            MutationType::Create => build_insert_query(typename, variables, &self.metadata)?,
            MutationType::Update => build_update_query(typename, variables, &self.metadata)?,
            MutationType::Delete => build_delete_query(typename, variables, &self.metadata)?,
        };

        // Execute the mutation and read the affected row back (RETURNING *).
        let returned = self.adapter.execute_raw_query(&sql).await?.into_iter().next();

        let row = match (mutation_type, returned) {
            // A row came back — use it verbatim (the real post-mutation state).
            (_, Some(row)) => row,
            // An INSERT that returns no row is a backend contract violation.
            (MutationType::Create, None) => {
                return Err(fraiseql_error::FraiseQLError::Database {
                    message:   format!("INSERT into '{typename}' returned no row from RETURNING *"),
                    sql_state: None,
                });
            },
            // A 0-row UPDATE/DELETE means the targeted entity does not exist.
            (MutationType::Update | MutationType::Delete, None) => {
                return Err(fraiseql_error::FraiseQLError::not_found(
                    typename,
                    key_identifier(fed_type, variables),
                ));
            },
        };

        Ok(build_entity_response(typename, row))
    }

    /// Execute a mutation on an extended (non-owned) entity.
    ///
    /// Extended mutations are propagated to the authoritative subgraph that owns the entity.
    /// Currently returns a mock response. Remote subgraph communication via HTTP is not yet
    /// implemented.
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
    ///
    /// # Errors
    ///
    /// Returns `FraiseQLError::Validation` if the variables are not a JSON object.
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
mod tests;
