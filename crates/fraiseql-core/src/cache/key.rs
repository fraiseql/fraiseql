//! Cache key generation for query results.
//!
//! # Security Critical
//!
//! This module is **security-critical**. Cache keys MUST include variable values
//! to prevent data leakage between different users or requests. Incorrect key
//! generation could allow User A to see User B's cached data.
//!
//! # Key Composition
//!
//! Cache keys are generated from:
//! 1. Query string + variables (via APQ's security-audited `hash_query_with_variables`)
//! 2. WHERE clause structure (ensures different filters = different keys)
//! 3. Schema version (auto-invalidates on schema changes)
//!
//! # Example
//!
//! ```rust
//! use fraiseql_core::cache::generate_cache_key;
//! use fraiseql_core::db::{WhereClause, WhereOperator};
//! use serde_json::json;
//!
//! // Two different users querying their own data
//! let key1 = generate_cache_key(
//!     "query { user(id: $id) { name } }",
//!     &json!({"id": "alice"}),
//!     None,
//!     "v1"
//! );
//!
//! let key2 = generate_cache_key(
//!     "query { user(id: $id) { name } }",
//!     &json!({"id": "bob"}),
//!     None,
//!     "v1"
//! );
//!
//! // Different variables MUST produce different keys (security requirement)
//! assert_ne!(key1, key2);
//! ```

use serde_json::Value as JsonValue;
use sha2::{Digest, Sha256};

use crate::{
    apq::hasher::hash_query_with_variables, db::where_clause::WhereClause, schema::QueryDefinition,
};

/// Generate cache key for query result.
///
/// # Security Critical
///
/// **DIFFERENT VARIABLE VALUES MUST PRODUCE DIFFERENT KEYS** to prevent data
/// leakage between users. This function leverages APQ's security-audited
/// `hash_query_with_variables()` which correctly handles variable normalization.
///
/// # Key Composition
///
/// The cache key is a SHA-256 hash of:
/// ```text
/// SHA256(
///   hash_query_with_variables(query, variables) +
///   WHERE_clause_structure +
///   schema_version
/// )
/// ```
///
/// This ensures:
/// - Same query + variables = same key (cache hit)
/// - Different variables = different key (security)
/// - Different WHERE clauses = different key (correctness)
/// - Schema changes = different key (validity)
///
/// # Arguments
///
/// * `query` - GraphQL query string
/// * `variables` - Query variables from GraphQL request (optional)
/// * `where_clause` - WHERE filter from auto-params (optional)
/// * `schema_version` - Schema hash from `CompiledSchema`
///
/// # Returns
///
/// 64-character hex string (SHA-256 hash)
///
/// # Security Examples
///
/// ```rust
/// use fraiseql_core::cache::generate_cache_key;
/// use serde_json::json;
///
/// let query = "query getUser($id: ID!) { user(id: $id) { name } }";
///
/// // Different users MUST get different cache keys
/// let alice_key = generate_cache_key(query, &json!({"id": "alice"}), None, "v1");
/// let bob_key = generate_cache_key(query, &json!({"id": "bob"}), None, "v1");
/// assert_ne!(alice_key, bob_key, "Security: different variables must produce different keys");
///
/// // Same user MUST get same key (determinism)
/// let alice_key2 = generate_cache_key(query, &json!({"id": "alice"}), None, "v1");
/// assert_eq!(alice_key, alice_key2, "Determinism: same inputs must produce same key");
/// ```
///
/// # Examples
///
/// ```rust
/// use fraiseql_core::cache::generate_cache_key;
/// use fraiseql_core::db::{WhereClause, WhereOperator};
/// use serde_json::json;
///
/// // Simple query with variables
/// let key = generate_cache_key(
///     "query { users(limit: $limit) { id } }",
///     &json!({"limit": 10}),
///     None,
///     "abc123"
/// );
/// assert_eq!(key.len(), 64); // SHA-256 hex
///
/// // Query with WHERE clause
/// let where_clause = WhereClause::Field {
///     path: vec!["email".to_string()],
///     operator: WhereOperator::Icontains,
///     value: json!("example.com"),
/// };
///
/// let key_with_where = generate_cache_key(
///     "query { users { id } }",
///     &json!({}),
///     Some(&where_clause),
///     "abc123"
/// );
/// assert_eq!(key_with_where.len(), 64);
/// ```
#[must_use]
pub fn generate_cache_key(
    query: &str,
    variables: &JsonValue,
    where_clause: Option<&WhereClause>,
    schema_version: &str,
) -> String {
    // Step 1: Base key from APQ (query + variables)
    // This is security-audited and handles variable ordering correctly
    // Different variables WILL produce different hashes (critical for security)
    let base_key = hash_query_with_variables(query, variables);

    // Step 2: Add WHERE clause structure if present
    // Different WHERE clauses must produce different keys for correctness
    // Using Debug format captures full structure including operators and values
    let where_structure = where_clause.map(|w| format!("{:?}", w)).unwrap_or_default();

    // Step 3: Combine with schema version
    // Schema changes invalidate all cached queries automatically
    let combined = format!("{}:{}:{}", base_key, where_structure, schema_version);

    // Step 4: Hash the combination for final cache key
    // SHA-256 provides collision resistance and fixed-length output
    let mut hasher = Sha256::new();
    hasher.update(combined.as_bytes());
    hex::encode(hasher.finalize())
}

/// Extract accessed views from query definition.
///
/// In Phase 2, we track which database views/tables a query accesses for
/// view-based cache invalidation. When a mutation modifies a view, we can
/// invalidate all cached queries that read from that view.
///
/// # Phase 2 Scope
///
/// Currently extracts only the primary SQL source from the query definition.
/// Does not analyze:
/// - JOIN clauses (requires compiled SQL - Phase 4)
/// - Resolver chains (requires runtime context - Phase 5)
/// - Nested queries (requires query analyzer - Phase 4)
///
/// # Future Enhancements (Phase 4+)
///
/// - Extract views from JOIN clauses in compiled SQL
/// - Extract views from resolver chains
/// - Support for custom resolver view tracking
/// - Entity-level tracking (extract IDs from results)
///
/// # Arguments
///
/// * `query_def` - The compiled query definition from schema
///
/// # Returns
///
/// List of view/table names accessed by this query
///
/// # Examples
///
/// ```ignore
/// use fraiseql_core::cache::extract_accessed_views;
/// use fraiseql_core::schema::{QueryDefinition, AutoParams};
///
/// let query_def = QueryDefinition {
///     name: "users".to_string(),
///     return_type: "User".to_string(),
///     returns_list: true,
///     nullable: false,
///     arguments: vec![],
///     sql_source: Some("v_user".to_string()),
///     auto_params: AutoParams {
///         has_where: true,
///         has_order_by: false,
///         has_limit: true,
///         has_offset: false,
///     },
/// };
///
/// let views = extract_accessed_views(&query_def);
/// assert_eq!(views, vec!["v_user"]);
/// ```
#[must_use]
pub fn extract_accessed_views(query_def: &QueryDefinition) -> Vec<String> {
    let mut views = Vec::new();

    // Add primary SQL source
    // Note: FraiseQL uses single-table compiled templates (no JOINs or resolver chains),
    // so the sql_source is the complete set of accessed views for cache invalidation.
    if let Some(sql_source) = &query_def.sql_source {
        views.push(sql_source.clone());
    }

    views
}

/// Verify cache key generation is deterministic.
///
/// Used in testing to ensure cache hits work correctly.
/// Same inputs must always produce the same key.
///
/// # Arguments
///
/// * `query` - GraphQL query string
/// * `variables` - Query variables
/// * `schema_version` - Schema version hash
///
/// # Returns
///
/// `true` if two sequential key generations produce identical keys
///
/// # Example
///
/// ```rust
/// use fraiseql_core::cache::verify_deterministic;
/// use serde_json::json;
///
/// assert!(verify_deterministic(
///     "query { users { id } }",
///     &json!({}),
///     "v1"
/// ));
/// ```
#[cfg(test)]
#[must_use]
pub fn verify_deterministic(query: &str, variables: &JsonValue, schema_version: &str) -> bool {
    let key1 = generate_cache_key(query, variables, None, schema_version);
    let key2 = generate_cache_key(query, variables, None, schema_version);
    key1 == key2
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;
    use crate::db::WhereOperator;

    // ========================================================================
    // Security Tests (CRITICAL)
    // ========================================================================

    #[test]
    fn test_different_variables_produce_different_keys() {
        // SECURITY CRITICAL: Different variables MUST produce different keys
        // to prevent User A from seeing User B's cached data
        let query = "query getUser($id: ID!) { user(id: $id) { name email } }";

        let key_alice = generate_cache_key(query, &json!({"id": "alice"}), None, "v1");
        let key_bob = generate_cache_key(query, &json!({"id": "bob"}), None, "v1");

        assert_ne!(
            key_alice, key_bob,
            "SECURITY: Different variables MUST produce different cache keys"
        );
    }

    #[test]
    fn test_different_variable_values_produce_different_keys() {
        let query = "query getUsers($limit: Int!) { users(limit: $limit) { id } }";

        let key_10 = generate_cache_key(query, &json!({"limit": 10}), None, "v1");
        let key_20 = generate_cache_key(query, &json!({"limit": 20}), None, "v1");

        assert_ne!(
            key_10, key_20,
            "SECURITY: Different variable values MUST produce different keys"
        );
    }

    #[test]
    fn test_empty_vs_non_empty_variables() {
        let query = "query { users { id } }";

        let key_empty = generate_cache_key(query, &json!({}), None, "v1");
        let key_with_vars = generate_cache_key(query, &json!({"limit": 10}), None, "v1");

        assert_ne!(
            key_empty, key_with_vars,
            "Empty variables must produce different key than non-empty"
        );
    }

    #[test]
    fn test_variable_order_independence() {
        // APQ handles variable ordering, so this should be deterministic
        let query = "query($a: Int, $b: Int) { users { id } }";

        // Note: serde_json maintains insertion order, so we can't easily test
        // reordering without custom JSON construction. This test documents
        // the expectation that APQ handles this correctly.
        let key1 = generate_cache_key(query, &json!({"a": 1, "b": 2}), None, "v1");
        let key2 = generate_cache_key(query, &json!({"a": 1, "b": 2}), None, "v1");

        assert_eq!(key1, key2, "Same variables must produce same key");
    }

    // ========================================================================
    // Determinism Tests
    // ========================================================================

    #[test]
    fn test_cache_key_deterministic() {
        // Same inputs must always produce same output
        let query = "query { users { id } }";
        let vars = json!({"limit": 10});

        let key1 = generate_cache_key(query, &vars, None, "v1");
        let key2 = generate_cache_key(query, &vars, None, "v1");

        assert_eq!(key1, key2, "Cache keys must be deterministic");
    }

    #[test]
    fn test_verify_deterministic_helper() {
        assert!(
            verify_deterministic("query { users }", &json!({}), "v1"),
            "Helper should verify determinism"
        );
    }

    // ========================================================================
    // WHERE Clause Tests
    // ========================================================================

    #[test]
    fn test_different_where_clauses_produce_different_keys() {
        let query = "query { users { id } }";

        let where1 = WhereClause::Field {
            path:     vec!["email".to_string()],
            operator: WhereOperator::Eq,
            value:    json!("alice@example.com"),
        };

        let where2 = WhereClause::Field {
            path:     vec!["email".to_string()],
            operator: WhereOperator::Eq,
            value:    json!("bob@example.com"),
        };

        let key1 = generate_cache_key(query, &json!({}), Some(&where1), "v1");
        let key2 = generate_cache_key(query, &json!({}), Some(&where2), "v1");

        assert_ne!(key1, key2, "Different WHERE clauses must produce different keys");
    }

    #[test]
    fn test_different_where_operators_produce_different_keys() {
        let query = "query { users { id } }";

        let where_eq = WhereClause::Field {
            path:     vec!["age".to_string()],
            operator: WhereOperator::Eq,
            value:    json!(30),
        };

        let where_gt = WhereClause::Field {
            path:     vec!["age".to_string()],
            operator: WhereOperator::Gt,
            value:    json!(30),
        };

        let key_eq = generate_cache_key(query, &json!({}), Some(&where_eq), "v1");
        let key_gt = generate_cache_key(query, &json!({}), Some(&where_gt), "v1");

        assert_ne!(key_eq, key_gt, "Different operators must produce different keys");
    }

    #[test]
    fn test_with_and_without_where_clause() {
        let query = "query { users { id } }";

        let where_clause = WhereClause::Field {
            path:     vec!["active".to_string()],
            operator: WhereOperator::Eq,
            value:    json!(true),
        };

        let key_without = generate_cache_key(query, &json!({}), None, "v1");
        let key_with = generate_cache_key(query, &json!({}), Some(&where_clause), "v1");

        assert_ne!(key_without, key_with, "Presence of WHERE clause must change key");
    }

    #[test]
    fn test_complex_where_clause() {
        let query = "query { users { id } }";

        let where_clause = WhereClause::And(vec![
            WhereClause::Field {
                path:     vec!["age".to_string()],
                operator: WhereOperator::Gte,
                value:    json!(18),
            },
            WhereClause::Field {
                path:     vec!["active".to_string()],
                operator: WhereOperator::Eq,
                value:    json!(true),
            },
        ]);

        let key = generate_cache_key(query, &json!({}), Some(&where_clause), "v1");

        assert_eq!(key.len(), 64, "Should produce valid SHA-256 hex");
    }

    // ========================================================================
    // Schema Version Tests
    // ========================================================================

    #[test]
    fn test_different_schema_versions_produce_different_keys() {
        let query = "query { users { id } }";

        let key_v1 = generate_cache_key(query, &json!({}), None, "v1");
        let key_v2 = generate_cache_key(query, &json!({}), None, "v2");

        assert_ne!(key_v1, key_v2, "Different schema versions must produce different keys");
    }

    #[test]
    fn test_schema_version_invalidation() {
        // When schema changes, all cache keys change (automatic invalidation)
        let query = "query { users { id } }";

        let old_schema = "abc123";
        let new_schema = "def456";

        let key_old = generate_cache_key(query, &json!({}), None, old_schema);
        let key_new = generate_cache_key(query, &json!({}), None, new_schema);

        assert_ne!(key_old, key_new, "Schema changes should invalidate cache");
    }

    // ========================================================================
    // Output Format Tests
    // ========================================================================

    #[test]
    fn test_cache_key_length() {
        let key = generate_cache_key("query { users }", &json!({}), None, "v1");
        assert_eq!(key.len(), 64, "SHA-256 hex should be 64 characters");
    }

    #[test]
    fn test_cache_key_format() {
        let key = generate_cache_key("query { users }", &json!({}), None, "v1");

        // Verify it's valid hexadecimal
        assert!(key.chars().all(|c| c.is_ascii_hexdigit()), "Cache key should be hexadecimal");
    }

    // ========================================================================
    // Extract Views Tests
    // ========================================================================

    #[test]
    fn test_extract_accessed_views_with_sql_source() {
        use crate::schema::AutoParams;

        let query_def = QueryDefinition {
            name:         "users".to_string(),
            return_type:  "User".to_string(),
            returns_list: true,
            nullable:     false,
            arguments:    vec![],
            sql_source:   Some("v_user".to_string()),
            description:  None,
            auto_params:  AutoParams {
                has_where:    true,
                has_order_by: false,
                has_limit:    true,
                has_offset:   false,
            },
            deprecation:  None,
        };

        let views = extract_accessed_views(&query_def);
        assert_eq!(views, vec!["v_user"]);
    }

    #[test]
    fn test_extract_accessed_views_without_sql_source() {
        use crate::schema::AutoParams;

        let query_def = QueryDefinition {
            name:         "customQuery".to_string(),
            return_type:  "Custom".to_string(),
            returns_list: false,
            nullable:     false,
            arguments:    vec![],
            sql_source:   None, // No SQL source (custom resolver)
            description:  None,
            auto_params:  AutoParams {
                has_where:    false,
                has_order_by: false,
                has_limit:    false,
                has_offset:   false,
            },
            deprecation:  None,
        };

        let views = extract_accessed_views(&query_def);
        assert_eq!(views, Vec::<String>::new());
    }

    // ========================================================================
    // Edge Case Tests
    // ========================================================================

    #[test]
    fn test_empty_query_string() {
        let key = generate_cache_key("", &json!({}), None, "v1");
        assert_eq!(key.len(), 64, "Empty query should still produce valid key");
    }

    #[test]
    fn test_null_variables() {
        let key = generate_cache_key("query { users }", &json!(null), None, "v1");
        assert_eq!(key.len(), 64, "Null variables should produce valid key");
    }

    #[test]
    fn test_large_variable_object() {
        let large_vars = json!({
            "filter": {
                "age": 30,
                "active": true,
                "tags": ["rust", "graphql", "database"],
                "metadata": {
                    "created_after": "2024-01-01",
                    "updated_before": "2024-12-31"
                }
            }
        });

        let key = generate_cache_key("query { users }", &large_vars, None, "v1");
        assert_eq!(key.len(), 64, "Large variables should produce valid key");
    }

    #[test]
    fn test_special_characters_in_query() {
        let query = r#"query { user(email: "test@example.com") { name } }"#;
        let key = generate_cache_key(query, &json!({}), None, "v1");
        assert_eq!(key.len(), 64, "Special characters should be handled");
    }
}
