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
//! Cache keys are generated from a single-pass ahash over:
//! 1. Query string bytes
//! 2. Recursively hashed variable values (canonical ordering)
//! 3. WHERE clause structure (hashed structurally, not via serde)
//! 4. Schema version string
//!
//! The hasher uses fixed seeds so that keys are deterministic across restarts.
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
    db::{OrderByClause, WhereOperator, where_clause::WhereClause},
    schema::{QueryDefinition, SqlProjectionHint},
};

// Fixed seeds for deterministic hashing across process restarts.
// These are arbitrary constants — changing them invalidates all cached entries.
const SEED_K0: u64 = 0x5241_4953_454F_4E31; // "RAISEON1"
const SEED_K1: u64 = 0x4652_4149_5345_514C; // "FRAISEQL"
const SEED_K2: u64 = 0x4341_4348_454B_4559; // "CACHEKEY"
const SEED_K3: u64 = 0x5632_5F43_4143_4845; // "V2_CACHE"

/// Create a new hasher from the fixed-seed `RandomState`.
fn new_hasher() -> impl Hasher {
    RandomState::with_seeds(SEED_K0, SEED_K1, SEED_K2, SEED_K3).build_hasher()
}

/// Generate cache key for query result.
///
/// # Security Critical
///
/// **DIFFERENT VARIABLE VALUES MUST PRODUCE DIFFERENT KEYS** to prevent data
/// leakage between users. This function feeds the full query, variables, WHERE
/// clause, and schema version into a single-pass ahash for a fast, deterministic
/// `u64` key.
///
/// # Key Composition
///
/// The cache key is a single ahash pass over:
/// ```text
/// ahash(
///   query_bytes          +
///   hash(variables)      +   ← recursive, canonical key ordering
///   hash(WHERE_clause)   +   ← structural, not serde-dependent
///   schema_version_bytes
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
/// A `u64` cache key suitable for use as a hash-map key.
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
#[must_use]
pub fn generate_cache_key(
    query: &str,
    variables: &JsonValue,
    where_clause: Option<&WhereClause>,
    schema_version: &str,
) -> u64 {
    let mut h = new_hasher();

    // Domain-separate the four sections with unique tags so that, e.g.,
    // a query ending with "v1" and an empty schema_version can never
    // collide with a shorter query and schema_version = "v1".
    h.write(b"q:");
    h.write(query.as_bytes());

    h.write(b"\0v:");
    hash_json_value(&mut h, variables);

    h.write(b"\0w:");
    if let Some(wc) = where_clause {
        h.write_u8(1);
        hash_where_clause(&mut h, wc);
    } else {
        h.write_u8(0);
    }

    h.write(b"\0s:");
    h.write(schema_version.as_bytes());

    h.finish()
}

/// Fast cache key for a view query — **zero heap allocations**.
///
/// Hashes `view + where_clause + limit + offset + schema_version` directly
/// without constructing an intermediate `String` or `serde_json::Value`.
/// Use this instead of [`generate_cache_key`] in the cache adapter hot path.
///
/// Domain tag `"v:"` separates these keys from projection keys (`"p:"`) and
/// generic query keys (`"q:"`), preventing cross-path collisions.
///
/// # Arguments
///
/// * `view` - Database view / table name
/// * `where_clause` - Optional WHERE filter (e.g. from RLS injection)
/// * `limit` - Optional row limit
/// * `offset` - Optional row offset
/// * `schema_version` - Schema hash from `CompiledSchema::content_hash()`
#[must_use]
pub fn generate_view_query_key(
    view: &str,
    where_clause: Option<&WhereClause>,
    limit: Option<u32>,
    offset: Option<u32>,
    order_by: Option<&[OrderByClause]>,
    schema_version: &str,
) -> u64 {
    let mut h = new_hasher();
    h.write(b"v:");
    h.write(view.as_bytes());
    h.write(b"\0w:");
    if let Some(wc) = where_clause {
        h.write_u8(1);
        hash_where_clause(&mut h, wc);
    } else {
        h.write_u8(0);
    }
    h.write(b"\0l:");
    match limit {
        Some(l) => {
            h.write_u8(1);
            h.write_u32(l);
        },
        None => h.write_u8(0),
    }
    h.write(b"\0o:");
    match offset {
        Some(o) => {
            h.write_u8(1);
            h.write_u32(o);
        },
        None => h.write_u8(0),
    }
    h.write(b"\0b:");
    hash_order_by(&mut h, order_by);
    h.write(b"\0s:");
    h.write(schema_version.as_bytes());
    h.finish()
}

/// Fast cache key for a projection query — **zero heap allocations**.
///
/// Like [`generate_view_query_key`] but also hashes the projection template.
/// Domain tag `"p:"` separates these keys from plain view keys.
///
/// # Arguments
///
/// * `view` - Database view / table name
/// * `projection` - Optional SQL projection hint (column subset)
/// * `where_clause` - Optional WHERE filter
/// * `limit` - Optional row limit
/// * `offset` - Optional row offset
/// * `schema_version` - Schema hash from `CompiledSchema::content_hash()`
#[must_use]
pub fn generate_projection_query_key(
    view: &str,
    projection: Option<&SqlProjectionHint>,
    where_clause: Option<&WhereClause>,
    limit: Option<u32>,
    offset: Option<u32>,
    order_by: Option<&[OrderByClause]>,
    schema_version: &str,
) -> u64 {
    let mut h = new_hasher();
    h.write(b"p:");
    h.write(view.as_bytes());
    h.write(b"\0j:");
    match projection {
        Some(p) => {
            h.write_u8(1);
            h.write(p.projection_template.as_bytes());
        },
        None => h.write_u8(0),
    }
    h.write(b"\0w:");
    if let Some(wc) = where_clause {
        h.write_u8(1);
        hash_where_clause(&mut h, wc);
    } else {
        h.write_u8(0);
    }
    h.write(b"\0l:");
    match limit {
        Some(l) => {
            h.write_u8(1);
            h.write_u32(l);
        },
        None => h.write_u8(0),
    }
    h.write(b"\0o:");
    match offset {
        Some(o) => {
            h.write_u8(1);
            h.write_u32(o);
        },
        None => h.write_u8(0),
    }
    h.write(b"\0b:");
    hash_order_by(&mut h, order_by);
    h.write(b"\0s:");
    h.write(schema_version.as_bytes());
    h.finish()
}

/// Recursively hash a `serde_json::Value` into the given hasher.
///
/// Object keys are sorted before hashing so that insertion order does not
/// affect the output (critical for variable-order independence).
fn hash_json_value(h: &mut impl Hasher, value: &JsonValue) {
    // Write a type discriminant so that `null`, `false`, `0`, `""`, `[]`, and `{}`
    // all produce distinct hashes.
    match value {
        JsonValue::Null => h.write_u8(0),
        JsonValue::Bool(b) => {
            h.write_u8(1);
            b.hash(h);
        },
        JsonValue::Number(n) => {
            h.write_u8(2);
            // Use the canonical string form so that 1.0 and 1 hash identically
            // when serde represents them the same way.
            h.write(n.to_string().as_bytes());
        },
        JsonValue::String(s) => {
            h.write_u8(3);
            h.write(s.as_bytes());
        },
        JsonValue::Array(arr) => {
            h.write_u8(4);
            h.write_usize(arr.len());
            for item in arr {
                hash_json_value(h, item);
            }
        },
        JsonValue::Object(map) => {
            h.write_u8(5);
            h.write_usize(map.len());
            // Sort keys for canonical ordering.
            let mut keys: Vec<&String> = map.keys().collect();
            keys.sort_unstable();
            for key in keys {
                h.write(key.as_bytes());
                hash_json_value(h, &map[key]);
            }
        },
    }
}

/// Hash a `WhereClause` tree structurally.
///
/// Uses discriminant tags and recursion so that structurally different clauses
/// always produce different hash contributions.
fn hash_where_clause(h: &mut impl Hasher, clause: &WhereClause) {
    match clause {
        WhereClause::Field {
            path,
            operator,
            value,
        } => {
            h.write_u8(b'F');
            h.write_usize(path.len());
            for segment in path {
                h.write(segment.as_bytes());
                h.write_u8(0); // separator
            }
            hash_where_operator(h, operator);
            hash_json_value(h, value);
        },
        WhereClause::And(clauses) => {
            h.write_u8(b'A');
            h.write_usize(clauses.len());
            for c in clauses {
                hash_where_clause(h, c);
            }
        },
        WhereClause::Or(clauses) => {
            h.write_u8(b'O');
            h.write_usize(clauses.len());
            for c in clauses {
                hash_where_clause(h, c);
            }
        },
        WhereClause::Not(inner) => {
            h.write_u8(b'N');
            hash_where_clause(h, inner);
        },
        // WhereClause is #[non_exhaustive]; unknown variants get a distinct tag
        // plus their Debug representation as a conservative fallback.
        _ => {
            h.write_u8(b'?');
            h.write(format!("{clause:?}").as_bytes());
        },
    }
}

/// Hash a `WhereOperator` by its `Debug` representation.
///
/// `WhereOperator` is `#[non_exhaustive]` with 40+ variants (including
/// `Extended(ExtendedOperator)`). Using the `Debug` string is stable across
/// refactors and automatically covers new variants without maintenance.
/// Hash a `WhereOperator` without allocating.
///
/// Uses `std::mem::discriminant` for the variant tag (zero-allocation).
/// For the `Extended(op)` variant which carries data, also hashes the
/// Debug representation of the inner operator (rare path, acceptable allocation).
fn hash_where_operator(h: &mut impl Hasher, op: &WhereOperator) {
    // discriminant is a fixed-size hashable value — no allocation
    std::mem::discriminant(op).hash(h);

    // Extended operators carry inner data that affects the hash.
    // All other variants are fully distinguished by their discriminant.
    if let WhereOperator::Extended(inner) = op {
        // Rare path: Extended operators are uncommon. The Debug allocation
        // here is acceptable because it only triggers for rich-filter queries.
        let inner_str = format!("{inner:?}");
        h.write(inner_str.as_bytes());
    }
}

/// Hash an optional `OrderByClause` slice into the given hasher.
///
/// Hashes each clause's `storage_key()` (`snake_case`) and `direction` discriminant,
/// ensuring that different orderings produce different cache keys.
fn hash_order_by(h: &mut impl Hasher, order_by: Option<&[OrderByClause]>) {
    match order_by.filter(|c| !c.is_empty()) {
        Some(clauses) => {
            h.write_u8(1);
            h.write_usize(clauses.len());
            for clause in clauses {
                let key = clause.storage_key();
                h.write(key.as_bytes());
                h.write_u8(clause.direction as u8);
            }
        },
        None => h.write_u8(0),
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
