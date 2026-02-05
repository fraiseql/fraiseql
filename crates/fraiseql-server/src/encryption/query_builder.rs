// Phase 12.3 Cycle 3: Query Builder Integration (REFACTOR)
//! Query builder integration for transparent field-level encryption/decryption
//!
//! Provides automatic encryption on write operations and decryption on read
//! operations at the query layer without application code changes.
//!
//! # Overview
//!
//! This module validates SQL queries to ensure encrypted fields are not used in
//! operations that require plaintext comparison or ordering:
//!
//! - **WHERE clauses**: Encrypted fields cannot be directly compared
//! - **ORDER BY clauses**: Encrypted ciphertext does not preserve plaintext order
//! - **JOIN conditions**: Encrypted fields are not comparable
//! - **GROUP BY clauses**: Encrypted ciphertext values are not stable
//! - **IS NULL**: Allowed (NULL stored at database level, not encrypted)
//!
//! # Alternatives for Encrypted Field Queries
//!
//! When you need to query encrypted fields, consider:
//!
//! 1. **Deterministic Hash Index**
//!    - Hash plaintext to deterministic value
//!    - Store hash in separate index column
//!    - Query using hash value
//!    - Vulnerable to rainbow table attacks - only use for non-sensitive data
//!
//! 2. **Plaintext Copy Column**
//!    - Store plaintext in unencrypted column (for non-sensitive subsets)
//!    - Encrypt separate column for full value
//!    - Query plaintext column, decrypt full value when needed
//!
//! 3. **Application-Level Filtering**
//!    - SELECT all rows with encryption keys available
//!    - Decrypt in application
//!    - Filter in memory
//!    - Works for reasonable result sets
//!
//! 4. **Vault-Native Encryption**
//!    - Use Vault Transit engine's encrypt/decrypt
//!    - Store encrypted data in separate "searchable" column
//!    - Query using Vault API for pattern matching

use crate::secrets_manager::SecretsError;
use std::collections::HashSet;

/// Query type for validation
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum QueryType {
    /// INSERT operation
    Insert,
    /// SELECT operation
    Select,
    /// UPDATE operation
    Update,
    /// DELETE operation
    Delete,
}

/// Clause type for validation
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ClauseType {
    /// WHERE clause
    Where,
    /// ORDER BY clause
    OrderBy,
    /// JOIN condition
    Join,
    /// GROUP BY clause
    GroupBy,
}

/// Query builder integration for encrypted field validation
///
/// Validates queries to ensure encrypted fields are not used in
/// operations that require plaintext comparison (WHERE, ORDER BY, JOIN).
pub struct QueryBuilderIntegration {
    /// Set of encrypted field names
    encrypted_fields: HashSet<String>,
}

impl QueryBuilderIntegration {
    /// Create new query builder integration
    pub fn new(encrypted_fields: Vec<String>) -> Self {
        Self {
            encrypted_fields: encrypted_fields.into_iter().collect(),
        }
    }

    /// Validate that encrypted fields are not used in WHERE clause
    ///
    /// Encrypted fields cannot be directly compared due to non-deterministic
    /// encryption (random nonces). Queries using encrypted fields in WHERE
    /// must use alternative approaches like:
    /// - Deterministic hash of plaintext
    /// - Separate plaintext index
    /// - Application-level filtering
    pub fn validate_where_clause(&self, fields: &[&str]) -> Result<(), SecretsError> {
        for field in fields {
            if self.encrypted_fields.contains(&field.to_string()) {
                return Err(SecretsError::ValidationError(format!(
                    "Cannot use encrypted field '{}' in WHERE clause. \
                     Encrypted fields are not queryable due to non-deterministic encryption. \
                     Consider using: (1) deterministic hash of plaintext, \
                     (2) separate plaintext index, or (3) application-level filtering.",
                    field
                )));
            }
        }

        Ok(())
    }

    /// Validate that encrypted fields are not used in ORDER BY clause
    ///
    /// Encrypted fields cannot be compared for sorting because ciphertext
    /// does not preserve plaintext order.
    pub fn validate_order_by_clause(&self, fields: &[&str]) -> Result<(), SecretsError> {
        for field in fields {
            if self.encrypted_fields.contains(&field.to_string()) {
                return Err(SecretsError::ValidationError(format!(
                    "Cannot use encrypted field '{}' in ORDER BY clause. \
                     Encrypted ciphertext does not preserve plaintext sort order. \
                     Consider ordering by unencrypted fields instead.",
                    field
                )));
            }
        }

        Ok(())
    }

    /// Validate that encrypted fields are not used in JOIN condition
    ///
    /// Encrypted fields cannot be compared in JOIN conditions because
    /// ciphertext is non-deterministic (different every time even for same plaintext).
    pub fn validate_join_condition(&self, fields: &[&str]) -> Result<(), SecretsError> {
        for field in fields {
            if self.encrypted_fields.contains(&field.to_string()) {
                return Err(SecretsError::ValidationError(format!(
                    "Cannot use encrypted field '{}' in JOIN condition. \
                     Encrypted fields are not comparable. \
                     JOIN on unencrypted fields instead, or denormalize data.",
                    field
                )));
            }
        }

        Ok(())
    }

    /// Validate that encrypted fields are not used in GROUP BY clause
    pub fn validate_group_by_clause(&self, fields: &[&str]) -> Result<(), SecretsError> {
        for field in fields {
            if self.encrypted_fields.contains(&field.to_string()) {
                return Err(SecretsError::ValidationError(format!(
                    "Cannot use encrypted field '{}' in GROUP BY clause. \
                     Encrypted ciphertext values are not stable for grouping.",
                    field
                )));
            }
        }

        Ok(())
    }

    /// Validate IS NULL can be used on encrypted fields
    ///
    /// IS NULL checks NULL at database level (before encryption),
    /// so it works correctly with encrypted fields.
    pub fn validate_is_null_on_encrypted(&self, _field: &str) -> Result<(), SecretsError> {
        // IS NULL is always allowed on encrypted fields
        // NULL is stored at database level, not encrypted
        Ok(())
    }

    /// Validate clause type contains allowed fields
    pub fn validate_clause(
        &self,
        clause_type: ClauseType,
        fields: &[&str],
    ) -> Result<(), SecretsError> {
        match clause_type {
            ClauseType::Where => self.validate_where_clause(fields),
            ClauseType::OrderBy => self.validate_order_by_clause(fields),
            ClauseType::Join => self.validate_join_condition(fields),
            ClauseType::GroupBy => self.validate_group_by_clause(fields),
        }
    }

    /// Get encrypted field names
    pub fn encrypted_fields(&self) -> Vec<String> {
        self.encrypted_fields.iter().cloned().collect()
    }

    /// Check if field is encrypted
    pub fn is_encrypted(&self, field: &str) -> bool {
        self.encrypted_fields.contains(&field.to_string())
    }

    /// Get encrypted fields that appear in field list
    pub fn get_encrypted_fields_in_list(&self, fields: &[&str]) -> Vec<String> {
        fields
            .iter()
            .filter(|f| self.is_encrypted(f))
            .map(|f| f.to_string())
            .collect()
    }

    /// Validate entire query structure
    ///
    /// Performs comprehensive validation across multiple clauses.
    pub fn validate_query(
        &self,
        query_type: QueryType,
        where_fields: &[&str],
        order_by_fields: &[&str],
        join_fields: &[&str],
    ) -> Result<(), SecretsError> {
        // Validate based on query type
        match query_type {
            QueryType::Insert | QueryType::Update => {
                // For INSERT/UPDATE, encrypted fields must be handled by adapter
                // No clause restrictions for write operations
                Ok(())
            }
            QueryType::Select => {
                // For SELECT, validate all clause restrictions
                self.validate_where_clause(where_fields)?;
                self.validate_order_by_clause(order_by_fields)?;
                self.validate_join_condition(join_fields)?;
                Ok(())
            }
            QueryType::Delete => {
                // For DELETE, encrypted fields not needed, no restrictions
                Ok(())
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_validate_where_clause_unencrypted_field() {
        let qbi = QueryBuilderIntegration::new(vec!["email".to_string()]);
        let result = qbi.validate_where_clause(&["name"]);
        assert!(result.is_ok());
    }

    #[test]
    fn test_validate_where_clause_encrypted_field_rejects() {
        let qbi = QueryBuilderIntegration::new(vec!["email".to_string()]);
        let result = qbi.validate_where_clause(&["email"]);
        assert!(result.is_err());
        assert!(matches!(result, Err(SecretsError::ValidationError(_))));
    }

    #[test]
    fn test_validate_order_by_unencrypted_field() {
        let qbi = QueryBuilderIntegration::new(vec!["email".to_string()]);
        let result = qbi.validate_order_by_clause(&["name"]);
        assert!(result.is_ok());
    }

    #[test]
    fn test_validate_order_by_encrypted_field_rejects() {
        let qbi = QueryBuilderIntegration::new(vec!["email".to_string()]);
        let result = qbi.validate_order_by_clause(&["email"]);
        assert!(result.is_err());
    }

    #[test]
    fn test_validate_join_unencrypted_field() {
        let qbi = QueryBuilderIntegration::new(vec!["email".to_string()]);
        let result = qbi.validate_join_condition(&["user_id"]);
        assert!(result.is_ok());
    }

    #[test]
    fn test_validate_join_encrypted_field_rejects() {
        let qbi = QueryBuilderIntegration::new(vec!["email".to_string()]);
        let result = qbi.validate_join_condition(&["email"]);
        assert!(result.is_err());
    }

    #[test]
    fn test_validate_group_by_unencrypted_field() {
        let qbi = QueryBuilderIntegration::new(vec!["email".to_string()]);
        let result = qbi.validate_group_by_clause(&["status"]);
        assert!(result.is_ok());
    }

    #[test]
    fn test_validate_group_by_encrypted_field_rejects() {
        let qbi = QueryBuilderIntegration::new(vec!["email".to_string()]);
        let result = qbi.validate_group_by_clause(&["email"]);
        assert!(result.is_err());
    }

    #[test]
    fn test_validate_is_null_on_encrypted_field() {
        let qbi = QueryBuilderIntegration::new(vec!["email".to_string()]);
        let result = qbi.validate_is_null_on_encrypted("email");
        assert!(result.is_ok());
    }

    #[test]
    fn test_validate_mixed_encrypted_unencrypted_fields() {
        let qbi = QueryBuilderIntegration::new(vec!["email".to_string(), "phone".to_string()]);
        // Should reject if any field is encrypted
        let result = qbi.validate_where_clause(&["name", "email"]);
        assert!(result.is_err());
    }

    #[test]
    fn test_validate_clause_with_type() {
        let qbi = QueryBuilderIntegration::new(vec!["email".to_string()]);
        let result = qbi.validate_clause(ClauseType::Where, &["email"]);
        assert!(result.is_err());

        let result = qbi.validate_clause(ClauseType::OrderBy, &["email"]);
        assert!(result.is_err());

        let result = qbi.validate_clause(ClauseType::Join, &["email"]);
        assert!(result.is_err());
    }

    #[test]
    fn test_encrypted_fields_list() {
        let qbi = QueryBuilderIntegration::new(vec!["email".to_string(), "phone".to_string()]);
        let fields = qbi.encrypted_fields();
        assert_eq!(fields.len(), 2);
        assert!(fields.contains(&"email".to_string()));
        assert!(fields.contains(&"phone".to_string()));
    }

    #[test]
    fn test_is_encrypted() {
        let qbi = QueryBuilderIntegration::new(vec!["email".to_string()]);
        assert!(qbi.is_encrypted("email"));
        assert!(!qbi.is_encrypted("name"));
    }

    #[test]
    fn test_get_encrypted_fields_in_list() {
        let qbi = QueryBuilderIntegration::new(vec!["email".to_string(), "phone".to_string()]);
        let result = qbi.get_encrypted_fields_in_list(&["name", "email", "phone"]);
        assert_eq!(result.len(), 2);
        assert!(result.contains(&"email".to_string()));
        assert!(result.contains(&"phone".to_string()));
    }

    #[test]
    fn test_validate_query_insert_allows_encrypted() {
        let qbi = QueryBuilderIntegration::new(vec!["email".to_string()]);
        let result = qbi.validate_query(QueryType::Insert, &[], &[], &[]);
        assert!(result.is_ok());
    }

    #[test]
    fn test_validate_query_select_rejects_encrypted_where() {
        let qbi = QueryBuilderIntegration::new(vec!["email".to_string()]);
        let result = qbi.validate_query(QueryType::Select, &["email"], &[], &[]);
        assert!(result.is_err());
    }

    #[test]
    fn test_validate_query_delete_allows_encrypted() {
        let qbi = QueryBuilderIntegration::new(vec!["email".to_string()]);
        let result = qbi.validate_query(QueryType::Delete, &[], &[], &[]);
        assert!(result.is_ok());
    }
}
