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
    backend::{OrderByClause, ProjectionRequest, WhereOperator, where_clause::WhereClause},
    schema::QueryDefinition,
    security::SecurityContext,
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
#[allow(clippy::too_many_arguments)] // Reason: one argument per key section; a struct would only rename them
pub fn generate_view_query_key(
    view: &str,
    where_clause: Option<&WhereClause>,
    limit: Option<u32>,
    offset: Option<u32>,
    order_by: Option<&[OrderByClause]>,
    session_vars: &[(&str, &str)],
    locale: Option<&str>,
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
    h.write(b"\0sv:");
    hash_session_vars(&mut h, session_vars);
    hash_locale(&mut h, locale);

    h.write(b"\0s:");
    h.write(schema_version.as_bytes());
    h.finish()
}

/// Fast cache key for a projection query — **zero heap allocations**.
///
/// Like [`generate_view_query_key`] but also hashes the projection template.
/// Domain tag `"p:"` separates these keys from plain view keys.
///
/// The request is destructured without `..`, so a field added to
/// [`ProjectionRequest`] does not compile until it is either hashed here or
/// argued out of the key.
///
/// # Arguments
///
/// * `request` - The read: view, projection, WHERE, ORDER BY, limit and offset
/// * `session_vars` - The session variables the read runs under (#1373)
/// * `schema_version` - Schema hash from `CompiledSchema::content_hash()`
#[must_use]
pub fn generate_projection_query_key(
    request: &ProjectionRequest<'_>,
    session_vars: &[(&str, &str)],
    locale: Option<&str>,
    schema_version: &str,
) -> u64 {
    let ProjectionRequest {
        view,
        projection,
        where_clause,
        order_by,
        limit,
        offset,
    } = *request;
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
    h.write(b"\0sv:");
    hash_session_vars(&mut h, session_vars);
    hash_locale(&mut h, locale);

    h.write(b"\0s:");
    h.write(schema_version.as_bytes());
    h.finish()
}

/// Hash of the principal a cached result belongs to.
///
/// Covers the fields that can change which rows a request is entitled to:
/// `user_id`, roles, `tenant_id`, scopes and `attributes` (custom RLS policies can
/// key on any attribute, e.g. "department" or "region"). Roles and scopes are
/// sorted and attribute keys ordered, so the order a token lists them in does not
/// fork the entry; every collection is length-prefixed, so a value cannot slide
/// from one field into the next.
///
/// Not hashed: `request_id`, `ip_address`, `authenticated_at`, `expires_at`,
/// `issuer`, `audience` — they do not change what the principal may see.
///
/// Returns `0` when no security context is present (all anonymous requests share
/// one scope).
#[must_use]
pub fn hash_security_context(ctx: Option<&SecurityContext>) -> u64 {
    let Some(ctx) = ctx else {
        return 0;
    };

    let mut h = new_hasher();
    h.write(b"u:");
    ctx.user_id.hash(&mut h);

    let mut roles: Vec<&String> = ctx.roles.iter().collect();
    roles.sort_unstable();
    h.write(b"r:");
    h.write_usize(roles.len());
    for role in roles {
        role.hash(&mut h);
    }

    h.write(b"t:");
    ctx.tenant_id.hash(&mut h);

    let mut scopes: Vec<&String> = ctx.scopes.iter().collect();
    scopes.sort_unstable();
    h.write(b"s:");
    h.write_usize(scopes.len());
    for scope in scopes {
        scope.hash(&mut h);
    }

    let mut attribute_keys: Vec<&String> = ctx.attributes.keys().collect();
    attribute_keys.sort_unstable();
    h.write(b"a:");
    h.write_usize(attribute_keys.len());
    for key in attribute_keys {
        key.hash(&mut h);
        hash_json_value(&mut h, &ctx.attributes[key]);
    }

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
        // The declared field types are a function of the query's return type,
        // which the key already covers, so only the annotated subtree needs
        // hashing. The tag keeps an annotated clause distinct from a bare one.
        WhereClause::Typed { inner, .. } => {
            h.write_u8(b'T');
            hash_where_clause(h, inner);
        },
        // The guard carries the caller's row predicate (ruling AH): two callers whose
        // predicates differ must not share an entry, so it is hashed in full.
        WhereClause::Guarded {
            under,
            guard,
            inner,
        } => {
            h.write_u8(b'G');
            h.write_usize(under.len());
            for segment in under {
                h.write(segment.as_bytes());
                h.write_u8(0);
            }
            hash_where_clause(h, guard);
            hash_where_clause(h, inner);
        },
        // Ruling AL: every part, the predicate included — it is the caller's.
        WhereClause::KeyIn {
            path,
            key_type,
            view,
            target_key,
            predicate,
        } => {
            h.write_u8(b'K');
            for segments in [path, target_key] {
                h.write_usize(segments.len());
                for segment in segments {
                    h.write(segment.as_bytes());
                    h.write_u8(0);
                }
            }
            h.write(format!("{key_type:?}").as_bytes());
            h.write_u8(0);
            h.write(view.as_bytes());
            h.write_u8(0);
            hash_where_clause(h, predicate);
        },
        // WhereClause is #[non_exhaustive]; unknown variants get a distinct tag
        // plus their Debug representation as a conservative fallback.
        _ => {
            h.write_u8(b'?');
            h.write(format!("{clause:?}").as_bytes());
        },
    }
}

/// Hash a `WhereOperator` without allocating.
///
/// Uses `std::mem::discriminant` for the variant tag (zero-allocation).
/// Every variant is a unit variant, so the discriminant fully distinguishes it.
fn hash_where_operator(h: &mut impl Hasher, op: &WhereOperator) {
    // discriminant is a fixed-size hashable value — no allocation
    std::mem::discriminant(op).hash(h);
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

/// The request locale a read runs in, as its own section (#1512).
///
/// A localized read's rows depend on it whatever carries it to SQL (a literal in the
/// projection, the `fraiseql.locale` setting), so the key names it outright rather than
/// relying on either.
fn hash_locale(h: &mut impl std::hash::Hasher, locale: Option<&str>) {
    h.write(b"\0loc:");
    match locale {
        Some(tag) => {
            h.write_u8(1);
            h.write(tag.as_bytes());
        },
        None => h.write_u8(0),
    }
}

/// Hash the session variables a read runs under, independent of the order they arrive
/// in (#1373).
///
/// A view may read request context with `current_setting()`, so the rows a read
/// returns can depend on any session variable it sets, and only the database knows
/// whether one does. Every pair is therefore part of the key: two callers share an
/// entry only when they would have run the read under identical settings. Names are
/// unique per request, so sorting by name is a total order.
fn hash_session_vars(h: &mut impl std::hash::Hasher, session_vars: &[(&str, &str)]) {
    let mut sorted: Vec<&(&str, &str)> = session_vars.iter().collect();
    sorted.sort_unstable_by(|a, b| a.0.cmp(b.0).then_with(|| a.1.cmp(b.1)));
    h.write_usize(sorted.len());
    for (name, value) in sorted {
        h.write(name.as_bytes());
        h.write_u8(0);
        h.write(value.as_bytes());
        h.write_u8(0);
    }
}
