//! Bulk mutation execution — CQRS-compliant batch operations.
//!
//! All writes go through mutation functions (`fn_create_*`, `fn_update_*`,
//! `fn_delete_*`).  The bulk executor never issues raw `INSERT`/`UPDATE`/
//! `DELETE` SQL.

use crate::db::traits::{DatabaseAdapter, MutationCapable};
use crate::error::{FraiseQLError, Result};
use crate::security::SecurityContext;

use super::Executor;

/// Result of a bulk operation (batch insert, filter-based update/delete).
#[derive(Debug, Clone)]
pub struct BulkResult {
    /// Number of rows affected by the operation.
    pub affected_rows: u64,
    /// Entity representations from each mutation (when `return=representation`).
    /// `None` when `return=minimal` was requested.
    pub entities: Option<Vec<serde_json::Value>>,
}

impl<A: DatabaseAdapter + MutationCapable> Executor<A> {
    /// Execute a mutation for each item in a batch.
    ///
    /// Calls [`Executor::execute_mutation_with_security`] (or
    /// [`Executor::execute_mutation`]) per item.  Stops on first error.
    ///
    /// # Errors
    ///
    /// Returns the first error encountered.  Items processed before the error
    /// are already committed (each mutation is its own DB transaction).
    pub async fn execute_mutation_batch(
        &self,
        mutation_name: &str,
        items: &[serde_json::Value],
        security_context: Option<&SecurityContext>,
    ) -> Result<Vec<String>> {
        let mut results = Vec::with_capacity(items.len());
        for item in items {
            let result = self
                .execute_mutation_routed(mutation_name, Some(item), security_context)
                .await?;
            results.push(result);
        }
        Ok(results)
    }

    /// CQRS bulk update/delete: query the read view for matching IDs, then
    /// mutate per row.
    ///
    /// Flow:
    /// 1. Query `v_{entity}` (read view) with `where_clause` + RLS to get IDs
    /// 2. Count check: if count > `max_affected`, return error
    /// 3. Call mutation function per ID
    /// 4. Collect results
    ///
    /// # Errors
    ///
    /// Returns [`FraiseQLError::Validation`] if affected rows exceed
    /// `max_affected`.
    /// Returns the first mutation error encountered.
    pub async fn execute_bulk_by_filter(
        &self,
        query_match: &crate::runtime::matcher::QueryMatch,
        mutation_name: &str,
        body_variables: &serde_json::Value,
        id_field: &str,
        max_affected: u64,
        security_context: Option<&SecurityContext>,
    ) -> Result<BulkResult> {
        // 1. Execute the read view query to get matching rows
        let result = self
            .execute_query_direct(query_match, None, security_context)
            .await?;

        // 2. Parse result and extract IDs
        let parsed: serde_json::Value = serde_json::from_str(&result)?;
        let rows = extract_rows_from_result(&parsed);

        let count = rows.len() as u64;
        if count > max_affected {
            return Err(FraiseQLError::Validation {
                message: format!(
                    "Would affect {count} rows, exceeding max-affected limit of {max_affected}"
                ),
                path: None,
            });
        }

        if rows.is_empty() {
            return Ok(BulkResult {
                affected_rows: 0,
                entities: Some(Vec::new()),
            });
        }

        // 3. Call mutation per row
        let mut entities = Vec::with_capacity(rows.len());
        for row in &rows {
            let id = row.get(id_field).cloned().unwrap_or(serde_json::Value::Null);

            // Merge body variables with the row's ID
            let mut vars = serde_json::Map::new();
            vars.insert(id_field.to_string(), id);
            if let Some(obj) = body_variables.as_object() {
                for (k, v) in obj {
                    vars.insert(k.clone(), v.clone());
                }
            }
            let vars_json = serde_json::Value::Object(vars);

            let mutation_result = self
                .execute_mutation_routed(mutation_name, Some(&vars_json), security_context)
                .await?;
            entities.push(serde_json::Value::String(mutation_result));
        }

        Ok(BulkResult {
            affected_rows: count,
            entities: Some(entities),
        })
    }

    /// Route a single mutation through security context when available.
    async fn execute_mutation_routed(
        &self,
        mutation_name: &str,
        variables: Option<&serde_json::Value>,
        security_context: Option<&SecurityContext>,
    ) -> Result<String> {
        if let Some(ctx) = security_context {
            self.execute_mutation_with_security(mutation_name, variables, ctx)
                .await
        } else {
            self.execute_mutation(mutation_name, variables).await
        }
    }
}

/// Extract row data from an executor result envelope.
///
/// Handles both `{ "data": { "queryName": [...] } }` and
/// `{ "data": { "queryName": { "edges": [...] } } }` (Relay) formats.
fn extract_rows_from_result(parsed: &serde_json::Value) -> Vec<&serde_json::Value> {
    let Some(data) = parsed.get("data") else {
        return Vec::new();
    };

    // Get the first (and typically only) field in the data object
    let Some(inner) = data.as_object().and_then(|m| m.values().next()) else {
        return Vec::new();
    };

    // Direct array
    if let Some(arr) = inner.as_array() {
        return arr.iter().collect();
    }

    // Relay connection: { edges: [{ node: ... }] }
    if let Some(edges) = inner.get("edges").and_then(|e| e.as_array()) {
        return edges
            .iter()
            .filter_map(|edge| edge.get("node"))
            .collect();
    }

    Vec::new()
}

#[cfg(test)]
#[allow(clippy::unwrap_used)] // Reason: test code
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn extract_rows_array_format() {
        let result = json!({
            "data": {
                "users": [
                    {"pk_user_id": 1, "name": "Alice"},
                    {"pk_user_id": 2, "name": "Bob"},
                ]
            }
        });
        let rows = extract_rows_from_result(&result);
        assert_eq!(rows.len(), 2);
        assert_eq!(rows[0]["pk_user_id"], 1);
    }

    #[test]
    fn extract_rows_relay_format() {
        let result = json!({
            "data": {
                "users": {
                    "edges": [
                        {"cursor": "a", "node": {"pk_user_id": 1}},
                        {"cursor": "b", "node": {"pk_user_id": 2}},
                    ]
                }
            }
        });
        let rows = extract_rows_from_result(&result);
        assert_eq!(rows.len(), 2);
        assert_eq!(rows[0]["pk_user_id"], 1);
    }

    #[test]
    fn extract_rows_empty_data() {
        let result = json!({ "data": { "users": [] } });
        let rows = extract_rows_from_result(&result);
        assert!(rows.is_empty());
    }

    #[test]
    fn extract_rows_no_data() {
        let result = json!({ "errors": [{"message": "fail"}] });
        let rows = extract_rows_from_result(&result);
        assert!(rows.is_empty());
    }

    #[test]
    fn bulk_result_default() {
        let result = BulkResult {
            affected_rows: 0,
            entities: None,
        };
        assert_eq!(result.affected_rows, 0);
        assert!(result.entities.is_none());
    }
}
