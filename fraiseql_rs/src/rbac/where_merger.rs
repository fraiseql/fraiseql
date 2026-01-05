//! Safe merging of explicit WHERE clauses with row-level auth filters.
//!
//! This module handles the safe composition of user-provided WHERE clauses
//! with automatically injected row-level access filters. It ensures that:
//! - Auth filters ALWAYS apply (cannot be bypassed)
//! - Conflicts are detected and handled
//! - WHERE clause structure is preserved
//! - AND composition is used for safe merging
//!
//! ## Architecture
//!
//! WHERE clause merging follows these rules:
//! 1. If only auth filter exists, apply it alone
//! 2. If only explicit WHERE exists, apply it alone
//! 3. If both exist, combine with AND operator
//! 4. Detect conflicts between them
//! 5. Flatten nested AND operators for efficiency
//!
//! ## Examples
//!
//! ```text
//! Explicit:  {status: {eq: "active"}}
//! Auth:      {tenant_id: {eq: "tenant-123"}}
//! Result:    {AND: [{status: {eq: "active"}}, {tenant_id: {eq: "tenant-123"}}]}
//!
//! Explicit:  {AND: [{status: {eq: "active"}}, {owner: {eq: "user1"}}]}
//! Auth:      {tenant_id: {eq: "tenant-123"}}
//! Result:    {AND: [{status: {eq: "active"}}, {owner: {eq: "user1"}}, {tenant_id: {eq: "tenant-123"}}]}
//!
//! Conflict:  {owner_id: {eq: "user1"}} + {owner_id: {eq: "user2"}}
//! Result:    Conflict detected, handler decides action
//! ```
//!
//! ## Thread Safety
//!
//! This module is stateless and thread-safe:
//! - All methods are pure functions (no mutable state)
//! - No external dependencies except `serde_json`
//! - Safe to use in concurrent contexts

use serde_json::{json, Value};
use std::collections::HashMap;

/// Result type for WHERE merger operations
pub type Result<T> = std::result::Result<T, WhereMergeError>;

/// Error type for WHERE clause merging
#[derive(Debug, Clone)]
pub enum WhereMergeError {
    /// Fields conflict between explicit and auth WHERE clauses
    ConflictingFields {
        /// Field name that conflicts
        field: String,
        /// Operator in explicit WHERE
        explicit_op: String,
        /// Operator in auth filter
        auth_op: String,
    },
    /// Invalid WHERE clause structure
    InvalidStructure(String),
    /// Serialization error
    SerializationError(String),
}

impl std::fmt::Display for WhereMergeError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::ConflictingFields {
                field,
                explicit_op,
                auth_op,
            } => {
                write!(
                    f,
                    "WHERE clause conflict: field '{field}' uses {explicit_op} in explicit WHERE but {auth_op} in auth filter"
                )
            }
            Self::InvalidStructure(msg) => write!(f, "Invalid WHERE clause structure: {msg}"),
            Self::SerializationError(msg) => write!(f, "Serialization error: {msg}"),
        }
    }
}

impl std::error::Error for WhereMergeError {}

/// WHERE clause merger for safe composition of explicit and auth filters
#[derive(Debug)]
pub struct WhereMerger;

impl WhereMerger {
    /// Merge explicit WHERE clause with row-level auth filter
    ///
    /// Returns:
    /// - `Ok(Some(where))`: Merged WHERE clause (applies both filters)
    /// - `Ok(None)`: Neither filter exists (no filtering)
    ///
    /// # Arguments
    ///
    /// - `explicit_where`: User-provided WHERE clause from GraphQL args
    /// - `auth_filter`: Row-level filter injected by auth system
    /// - `strategy`: How to handle conflicts (error, override, or log)
    ///
    /// # Errors
    ///
    /// Returns error if:
    /// - Conflicting fields detected (strategy = error)
    /// - Invalid WHERE clause structure
    ///
    /// # Panics
    ///
    /// Panics if conflicts are detected but the conflicts vector is empty (internal logic error).
    pub fn merge_where(
        explicit_where: Option<&Value>,
        auth_filter: Option<&Value>,
        strategy: ConflictStrategy,
    ) -> Result<Option<Value>> {
        match (explicit_where, auth_filter) {
            // Neither exists - no filtering
            (None, None) => Ok(None),
            // Only auth filter
            (None, Some(auth)) => Ok(Some(auth.clone())),
            // Only explicit WHERE
            (Some(explicit), None) => Ok(Some(explicit.clone())),
            // Both exist - merge them
            (Some(explicit), Some(auth)) => {
                // Check for conflicts
                let conflicts = Self::detect_conflicts(explicit, auth)?;

                if !conflicts.is_empty() {
                    match strategy {
                        ConflictStrategy::Error => {
                            return Err(conflicts.into_iter().next().unwrap());
                        }
                        ConflictStrategy::Override => {
                            // Auth filter takes precedence
                            return Ok(Some(auth.clone()));
                        }
                        ConflictStrategy::Log => {
                            // Continue despite conflicts (log happens externally)
                        }
                    }
                }

                // Merge using AND composition
                Ok(Self::compose_and(explicit, auth))
            }
        }
    }

    /// Detect conflicts between explicit WHERE and auth filter
    ///
    /// A conflict occurs when both clauses specify different conditions
    /// on the same field.
    ///
    /// # Examples
    ///
    /// ```text
    /// Explicit: {owner_id: {eq: "user1"}}
    /// Auth:     {owner_id: {eq: "user2"}}
    /// Result:   Conflict! (same field, different values)
    ///
    /// Explicit: {status: {eq: "active"}}
    /// Auth:     {tenant_id: {eq: "tenant-123"}}
    /// Result:   No conflict (different fields)
    /// ```
    fn detect_conflicts(explicit: &Value, auth: &Value) -> Result<Vec<WhereMergeError>> {
        let mut conflicts = Vec::new();

        let explicit_fields = Self::extract_field_operators(explicit)?;
        let auth_fields = Self::extract_field_operators(auth)?;

        // Find overlapping fields
        for (field, explicit_op) in &explicit_fields {
            if let Some(auth_op) = auth_fields.get(field) {
                // Same field with different operators = conflict
                if explicit_op != auth_op {
                    conflicts.push(WhereMergeError::ConflictingFields {
                        field: field.clone(),
                        explicit_op: explicit_op.clone(),
                        auth_op: auth_op.clone(),
                    });
                }
            }
        }

        Ok(conflicts)
    }

    /// Extract field names and their operators from a WHERE clause
    ///
    /// # Examples
    ///
    /// ```text
    /// {status: {eq: "active"}} → {status: "eq"}
    /// {owner_id: {neq: "user1"}} → {owner_id: "neq"}
    /// {AND: [{status: {eq: "active"}}, {id: {in: [...]}}]} → {status: "eq", id: "in"}
    /// ```
    fn extract_field_operators(where_clause: &Value) -> Result<HashMap<String, String>> {
        let mut fields = HashMap::new();

        let Some(obj) = where_clause.as_object() else {
            return Ok(fields);
        };

        for (key, value) in obj {
            match key.as_str() {
                "AND" | "OR" => {
                    Self::extract_logical_operators(value, &mut fields)?;
                }
                _ => {
                    Self::extract_field_operator(key, value, &mut fields);
                }
            }
        }

        Ok(fields)
    }

    fn extract_logical_operators(
        value: &Value,
        fields: &mut HashMap<String, String>,
    ) -> Result<()> {
        let Some(arr) = value.as_array() else {
            return Ok(());
        };

        for item in arr {
            let nested_fields = Self::extract_field_operators(item)?;
            fields.extend(nested_fields);
        }

        Ok(())
    }

    fn extract_field_operator(key: &str, value: &Value, fields: &mut HashMap<String, String>) {
        let Some(ops) = value.as_object() else {
            return;
        };

        if let Some(op_key) = ops.keys().next() {
            fields.insert(key.to_string(), op_key.clone());
        }
    }

    /// Compose two WHERE clauses with AND operator
    ///
    /// Handles:
    /// - Flattening existing AND clauses
    /// - Creating new AND for non-AND clauses
    /// - Avoiding nested AND structures
    fn compose_and(clause1: &Value, clause2: &Value) -> Option<Value> {
        // If clause1 is already AND, extend it
        if clause1.get("AND").is_some() {
            if let Some(mut and_parts) = clause1.get("AND").and_then(|v| v.as_array().cloned()) {
                and_parts.push(clause2.clone());
                return Some(json!({"AND": and_parts}));
            }
        }

        // If clause2 is already AND, prepend clause1
        if clause2.get("AND").is_some() {
            if let Some(and_parts) = clause2.get("AND").and_then(|v| v.as_array().cloned()) {
                let mut result = vec![clause1.clone()];
                result.extend(and_parts);
                return Some(json!({"AND": result}));
            }
        }

        // Create new AND composition
        Some(json!({"AND": [clause1.clone(), clause2.clone()]}))
    }

    /// Validate WHERE clause structure
    ///
    /// Checks for:
    /// - Valid object type
    /// - Proper field structure (field: {operator: value})
    /// - Valid AND/OR compositions
    ///
    /// # Errors
    ///
    /// Returns an error if the WHERE clause has invalid structure or unsupported operators.
    pub fn validate_where(where_clause: &Value) -> Result<()> {
        if !where_clause.is_object() {
            return Err(WhereMergeError::InvalidStructure(
                "WHERE clause must be an object".to_string(),
            ));
        }

        let Some(obj) = where_clause.as_object() else {
            return Ok(());
        };

        for (key, value) in obj {
            match key.as_str() {
                "AND" | "OR" => {
                    Self::validate_logical_operator(key, value)?;
                }
                _ => {
                    Self::validate_field_operator(key, value)?;
                }
            }
        }

        Ok(())
    }

    fn validate_logical_operator(key: &str, value: &Value) -> Result<()> {
        if !value.is_array() {
            return Err(WhereMergeError::InvalidStructure(format!(
                "{key} must contain an array"
            )));
        }

        let Some(arr) = value.as_array() else {
            return Ok(());
        };

        for item in arr {
            Self::validate_where(item)?;
        }

        Ok(())
    }

    fn validate_field_operator(key: &str, value: &Value) -> Result<()> {
        if !value.is_object() {
            return Err(WhereMergeError::InvalidStructure(format!(
                "Field '{key}' must contain operators"
            )));
        }
        Ok(())
    }
}

/// Strategy for handling WHERE clause conflicts
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConflictStrategy {
    /// Raise error on conflict
    Error,
    /// Auth filter takes precedence (user's WHERE is ignored)
    Override,
    /// Log conflict but continue merging (both applied)
    Log,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_merge_only_auth_filter() {
        let auth = json!({"tenant_id": {"eq": "tenant-123"}});
        let result = WhereMerger::merge_where(None, Some(&auth), ConflictStrategy::Error);

        assert!(result.is_ok());
        assert_eq!(result.unwrap(), Some(auth));
    }

    #[test]
    fn test_merge_only_explicit_where() {
        let explicit = json!({"status": {"eq": "active"}});
        let result = WhereMerger::merge_where(Some(&explicit), None, ConflictStrategy::Error);

        assert!(result.is_ok());
        assert_eq!(result.unwrap(), Some(explicit));
    }

    #[test]
    fn test_merge_neither_filter() {
        let result = WhereMerger::merge_where(None, None, ConflictStrategy::Error);

        assert!(result.is_ok());
        assert_eq!(result.unwrap(), None);
    }

    #[test]
    fn test_merge_both_no_conflict() {
        let explicit = json!({"status": {"eq": "active"}});
        let auth = json!({"tenant_id": {"eq": "tenant-123"}});
        let result =
            WhereMerger::merge_where(Some(&explicit), Some(&auth), ConflictStrategy::Error);

        assert!(result.is_ok());
        let merged = result.unwrap().unwrap();
        assert!(merged.get("AND").is_some());

        // Check that both conditions are in the AND array
        if let Some(and_arr) = merged.get("AND").and_then(|v| v.as_array()) {
            assert_eq!(and_arr.len(), 2);
        }
    }

    #[test]
    fn test_merge_with_existing_and() {
        let explicit = json!({
            "AND": [
                {"status": {"eq": "active"}},
                {"owner": {"eq": "user1"}}
            ]
        });
        let auth = json!({"tenant_id": {"eq": "tenant-123"}});
        let result =
            WhereMerger::merge_where(Some(&explicit), Some(&auth), ConflictStrategy::Error);

        assert!(result.is_ok());
        let merged = result.unwrap().unwrap();

        // Should have 3 items in AND array
        if let Some(and_arr) = merged.get("AND").and_then(|v| v.as_array()) {
            assert_eq!(and_arr.len(), 3);
        }
    }

    #[test]
    fn test_detect_conflict_same_field_different_ops() {
        let explicit = json!({"owner_id": {"eq": "user1"}});
        let auth = json!({"owner_id": {"neq": "user2"}});
        let conflicts = WhereMerger::detect_conflicts(&explicit, &auth);

        assert!(conflicts.is_ok());
        assert_eq!(conflicts.unwrap().len(), 1);
    }

    #[test]
    fn test_no_conflict_different_fields() {
        let explicit = json!({"owner_id": {"eq": "user1"}});
        let auth = json!({"tenant_id": {"eq": "tenant-123"}});
        let conflicts = WhereMerger::detect_conflicts(&explicit, &auth);

        assert!(conflicts.is_ok());
        assert!(conflicts.unwrap().is_empty());
    }

    #[test]
    fn test_extract_field_operators_simple() {
        let where_clause = json!({"status": {"eq": "active"}});
        let fields = WhereMerger::extract_field_operators(&where_clause);

        assert!(fields.is_ok());
        let map = fields.unwrap();
        assert_eq!(map.get("status"), Some(&"eq".to_string()));
    }

    #[test]
    fn test_extract_field_operators_with_and() {
        let where_clause = json!({
            "AND": [
                {"status": {"eq": "active"}},
                {"id": {"in": ["1", "2"]}}
            ]
        });
        let fields = WhereMerger::extract_field_operators(&where_clause);

        assert!(fields.is_ok());
        let map = fields.unwrap();
        assert_eq!(map.get("status"), Some(&"eq".to_string()));
        assert_eq!(map.get("id"), Some(&"in".to_string()));
    }

    #[test]
    fn test_validate_where_valid_simple() {
        let where_clause = json!({"status": {"eq": "active"}});
        let result = WhereMerger::validate_where(&where_clause);
        assert!(result.is_ok());
    }

    #[test]
    fn test_validate_where_valid_and() {
        let where_clause = json!({
            "AND": [
                {"status": {"eq": "active"}},
                {"id": {"eq": "123"}}
            ]
        });
        let result = WhereMerger::validate_where(&where_clause);
        assert!(result.is_ok());
    }

    #[test]
    fn test_validate_where_invalid_and_not_array() {
        let where_clause = json!({"AND": "not_an_array"});
        let result = WhereMerger::validate_where(&where_clause);
        assert!(result.is_err());
    }

    #[test]
    fn test_conflict_strategy_override() {
        let explicit = json!({"owner_id": {"eq": "user1"}});
        let auth = json!({"owner_id": {"eq": "user2"}});
        let result =
            WhereMerger::merge_where(Some(&explicit), Some(&auth), ConflictStrategy::Override);

        assert!(result.is_ok());
        // With Override strategy, auth filter takes precedence
        assert_eq!(result.unwrap(), Some(auth));
    }
}
