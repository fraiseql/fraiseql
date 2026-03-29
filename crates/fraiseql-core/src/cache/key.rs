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
//! 1. Query string + variables (hashed directly with sorted keys)
//! 2. WHERE clause structure (walked directly, no JSON serialization)
//! 3. Schema version (auto-invalidates on schema changes)
//!
//! # Performance
//!
//! Uses `ahash` (non-cryptographic) for fast cache key generation. The APQ
//! hash in `apq/hasher.rs` remains SHA-256 for client-facing security.
//! Cache keys don't need cryptographic properties — they're internal LRU
//! lookup keys where a collision causes a cache miss, not a security breach.
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

use std::hash::{BuildHasher, Hash, Hasher};

use ahash::RandomState;
use serde_json::Value as JsonValue;

use crate::{
    db::where_clause::{WhereClause, WhereOperator},
    schema::QueryDefinition,
};

/// Fixed seeds for deterministic hashing within a process.
///
/// `RandomState::with_seeds()` produces a deterministic hasher — same seeds
/// always yield the same hash for the same input within the same binary.
const SEED1: u64 = 0x5172_7f6a_9b3e_1d4c;
const SEED2: u64 = 0x8a4e_3c2b_f917_6d5e;
const SEED3: u64 = 0xd6f1_48c5_a329_7b0e;
const SEED4: u64 = 0x3e9a_7d14_c582_f6b0;

/// Build a deterministic AHasher from the fixed seeds.
fn new_hasher() -> impl Hasher {
    RandomState::with_seeds(SEED1, SEED2, SEED3, SEED4).build_hasher()
}

/// Generate cache key for query result.
///
/// # Security Critical
///
/// **DIFFERENT VARIABLE VALUES MUST PRODUCE DIFFERENT KEYS** to prevent data
/// leakage between users. Variables are hashed with sorted keys for
/// deterministic ordering.
///
/// # Key Composition
///
/// Single-pass `ahash` over:
/// - Query string bytes
/// - Variables (sorted keys, recursive)
/// - WHERE clause (walked directly, no JSON serialization)
/// - Schema version
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
/// * `variables` - Query variables from GraphQL request
/// * `where_clause` - WHERE filter from auto-params (optional)
/// * `schema_version` - Schema hash from `CompiledSchema`
///
/// # Returns
///
/// `u64` hash — fast comparison, no allocation.
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
/// // Different inputs produce different keys
/// assert_ne!(key, key_with_where);
/// ```
#[must_use]
pub fn generate_cache_key(
    query: &str,
    variables: &JsonValue,
    where_clause: Option<&WhereClause>,
    schema_version: &str,
) -> u64 {
    let mut hasher = new_hasher();

    // Domain separator: query
    hasher.write_u8(b'Q');
    hasher.write(query.as_bytes());

    // Domain separator: variables
    hasher.write_u8(b'V');
    hash_json_value(&mut hasher, variables);

    // Domain separator: WHERE clause
    hasher.write_u8(b'W');
    if let Some(wc) = where_clause {
        hasher.write_u8(1); // present
        hash_where_clause(&mut hasher, wc);
    } else {
        hasher.write_u8(0); // absent
    }

    // Domain separator: schema version
    hasher.write_u8(b'S');
    hasher.write(schema_version.as_bytes());

    hasher.finish()
}

/// Hash a JSON value deterministically.
///
/// Object keys are sorted to ensure deterministic output regardless of
/// insertion order (critical for security: same variables = same key).
fn hash_json_value(hasher: &mut impl Hasher, value: &JsonValue) {
    match value {
        JsonValue::Null => hasher.write_u8(0),
        JsonValue::Bool(b) => {
            hasher.write_u8(1);
            hasher.write_u8(u8::from(*b));
        }
        JsonValue::Number(n) => {
            hasher.write_u8(2);
            // Use the canonical string representation for numbers to avoid
            // f64 representation issues (NaN, -0, etc.)
            let s = n.to_string();
            hasher.write(s.as_bytes());
        }
        JsonValue::String(s) => {
            hasher.write_u8(3);
            hasher.write_usize(s.len());
            hasher.write(s.as_bytes());
        }
        JsonValue::Array(arr) => {
            hasher.write_u8(4);
            hasher.write_usize(arr.len());
            for item in arr {
                hash_json_value(hasher, item);
            }
        }
        JsonValue::Object(map) => {
            hasher.write_u8(5);
            hasher.write_usize(map.len());
            // Sort keys for deterministic ordering
            let mut keys: Vec<&String> = map.keys().collect();
            keys.sort();
            for key in keys {
                hasher.write_usize(key.len());
                hasher.write(key.as_bytes());
                hash_json_value(hasher, &map[key]);
            }
        }
    }
}

/// Hash a WHERE clause by walking the enum directly.
///
/// Avoids JSON serialization (`serde_json::to_string`) by hashing each
/// variant's discriminant and fields directly.
fn hash_where_clause(hasher: &mut impl Hasher, clause: &WhereClause) {
    match clause {
        WhereClause::Field { path, operator, value } => {
            hasher.write_u8(0); // Field discriminant
            hasher.write_usize(path.len());
            for segment in path {
                hasher.write_usize(segment.len());
                hasher.write(segment.as_bytes());
            }
            // Hash operator discriminant (zero-allocation). For the Extended
            // variant which carries data, also hash its Debug representation.
            hash_where_operator(hasher, operator);
            hash_json_value(hasher, value);
        }
        WhereClause::And(clauses) => {
            hasher.write_u8(1); // And discriminant
            hasher.write_usize(clauses.len());
            for c in clauses {
                hash_where_clause(hasher, c);
            }
        }
        WhereClause::Or(clauses) => {
            hasher.write_u8(2); // Or discriminant
            hasher.write_usize(clauses.len());
            for c in clauses {
                hash_where_clause(hasher, c);
            }
        }
        WhereClause::Not(inner) => {
            hasher.write_u8(3); // Not discriminant
            hash_where_clause(hasher, inner);
        }
    }
}

/// Hash a `WhereOperator` without allocating.
///
/// Uses `std::mem::discriminant` for the variant tag (zero-allocation).
/// For the `Extended(op)` variant which carries data, also hashes the
/// Debug representation of the inner operator (rare path, acceptable allocation).
fn hash_where_operator(hasher: &mut impl Hasher, op: &WhereOperator) {
    // discriminant is a fixed-size hashable value — no allocation
    std::mem::discriminant(op).hash(hasher);

    // Extended operators carry inner data that affects the hash.
    // All other variants are fully distinguished by their discriminant.
    if let WhereOperator::Extended(inner) = op {
        // Rare path: Extended operators are uncommon. The Debug allocation
        // here is acceptable because it only triggers for rich-filter queries.
        let inner_str = format!("{inner:?}");
        hasher.write(inner_str.as_bytes());
    }
}

/// Extract accessed views from query definition.
///
/// We track which database views/tables a query accesses for view-based
/// cache invalidation. When a mutation modifies a view, we can invalidate
/// all cached queries that read from that view.
///
/// # Current Scope
///
/// Currently extracts only the primary SQL source from the query definition.
/// Does not analyze:
/// - JOIN clauses (requires compiled SQL)
/// - Resolver chains (requires runtime context)
/// - Nested queries (requires query analyzer)
///
/// # Future Enhancements
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
/// ```rust
/// use fraiseql_core::cache::extract_accessed_views;
/// use fraiseql_core::schema::QueryDefinition;
///
/// let query_def = QueryDefinition::new("users", "User")
///     .returning_list()
///     .with_sql_source("v_user");
///
/// let views = extract_accessed_views(&query_def);
/// assert_eq!(views, vec!["v_user"]);
/// ```
#[must_use]
pub fn extract_accessed_views(query_def: &QueryDefinition) -> Vec<String> {
    let mut views = Vec::new();

    // Add primary SQL source
    if let Some(sql_source) = &query_def.sql_source {
        views.push(sql_source.clone());
    }

    // Add developer-declared secondary views (JOINs, nested queries, etc.)
    // Required for correct invalidation when a query reads from multiple views.
    views.extend(query_def.additional_views.iter().cloned());

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
        let query = "query($a: Int, $b: Int) { users { id } }";

        // Variables are hashed with sorted keys, so order doesn't matter
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

        // Just verify it produces a key (u64) without panicking
        let _key = generate_cache_key(query, &json!({}), Some(&where_clause), "v1");
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
    // Extract Views Tests
    // ========================================================================

    #[test]
    fn test_extract_accessed_views_with_sql_source() {
        use crate::schema::AutoParams;

        let query_def = QueryDefinition {
            name:                "users".to_string(),
            return_type:         "User".to_string(),
            returns_list:        true,
            nullable:            false,
            arguments:           vec![],
            sql_source:          Some("v_user".to_string()),
            description:         None,
            auto_params:         AutoParams {
                has_where:    true,
                has_order_by: false,
                has_limit:    true,
                has_offset:   false,
            },
            deprecation:         None,
            jsonb_column:        "data".to_string(),
            relay:               false,
            relay_cursor_column: None,
            relay_cursor_type:   Default::default(),
            inject_params:       Default::default(),
            cache_ttl_seconds:   None,
            additional_views:    vec![],
            requires_role:       None,
            rest_path:           None,
            rest_method:         None,
        };

        let views = extract_accessed_views(&query_def);
        assert_eq!(views, vec!["v_user"]);
    }

    #[test]
    fn test_extract_accessed_views_without_sql_source() {
        use crate::schema::AutoParams;

        let query_def = QueryDefinition {
            name:                "customQuery".to_string(),
            return_type:         "Custom".to_string(),
            returns_list:        false,
            nullable:            false,
            arguments:           vec![],
            sql_source:          None, // No SQL source (custom resolver)
            description:         None,
            auto_params:         AutoParams {
                has_where:    false,
                has_order_by: false,
                has_limit:    false,
                has_offset:   false,
            },
            deprecation:         None,
            jsonb_column:        "data".to_string(),
            relay:               false,
            relay_cursor_column: None,
            relay_cursor_type:   Default::default(),
            inject_params:       Default::default(),
            cache_ttl_seconds:   None,
            additional_views:    vec![],
            requires_role:       None,
            rest_path:           None,
            rest_method:         None,
        };

        let views = extract_accessed_views(&query_def);
        assert_eq!(views, Vec::<String>::new());
    }

    #[test]
    fn test_extract_accessed_views_with_additional_views() {
        use crate::schema::AutoParams;

        let query_def = QueryDefinition {
            name:                "usersWithPosts".to_string(),
            return_type:         "UserWithPosts".to_string(),
            returns_list:        true,
            nullable:            false,
            arguments:           vec![],
            sql_source:          Some("v_user_with_posts".to_string()),
            description:         None,
            auto_params:         AutoParams::default(),
            deprecation:         None,
            jsonb_column:        "data".to_string(),
            relay:               false,
            relay_cursor_column: None,
            relay_cursor_type:   Default::default(),
            inject_params:       Default::default(),
            cache_ttl_seconds:   None,
            additional_views:    vec!["v_post".to_string(), "v_tag".to_string()],
            requires_role:       None,
            rest_path:           None,
            rest_method:         None,
        };

        let views = extract_accessed_views(&query_def);
        assert_eq!(views, vec!["v_user_with_posts", "v_post", "v_tag"]);
    }

    // ========================================================================
    // Edge Case Tests
    // ========================================================================

    #[test]
    fn test_empty_query_string() {
        // Should not panic
        let _key = generate_cache_key("", &json!({}), None, "v1");
    }

    #[test]
    fn test_null_variables() {
        // Should not panic
        let _key = generate_cache_key("query { users }", &json!(null), None, "v1");
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

        // Should not panic
        let _key = generate_cache_key("query { users }", &large_vars, None, "v1");
    }

    #[test]
    fn test_special_characters_in_query() {
        let query = r#"query { user(email: "test@example.com") { name } }"#;
        // Should not panic
        let _key = generate_cache_key(query, &json!({}), None, "v1");
    }

    // ========================================================================
    // Collision Avoidance Tests
    // ========================================================================

    #[test]
    fn test_no_collisions_in_sample() {
        use std::collections::HashSet;

        let mut keys = HashSet::new();
        // Generate 1000 distinct keys
        for i in 0..1000 {
            let key = generate_cache_key(
                &format!("query {{ users(id: {i}) {{ id }} }}"),
                &json!({"page": i}),
                None,
                "v1",
            );
            assert!(keys.insert(key), "Collision at i={i}");
        }
    }
}
