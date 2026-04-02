//! Mutation-targeted tests for cache key generation.
//!
//! These tests are explicitly designed to **kill surviving mutants** — code
//! transformations that typical happy-path tests would not detect. Each test
//! targets a specific mutation operator applied to `cache/key.rs`.
//!
//! ## Targeted mutations
//!
//! | Mutant | What cargo-mutants would change | Killed by |
//! |--------|----------------------------------|-----------|
//! | M1 | Remove `where_structure` from `combined` | `where_clause_component_contributes_independently` |
//! | M2 | Remove `schema_version` from `combined` | `schema_version_component_contributes_independently` |
//! | M3 | Remove `base_key` from `combined` | `query_component_contributes_independently` |
//! | M4 | Skip `additional_views.extend(...)` | `extract_views_includes_additional_views` |
//! | M5 | Return empty from `sql_source` branch | `extract_views_includes_primary_sql_source` |
//! | M6 | Negate `is_some()` on `where_clause` | `none_and_some_where_clause_produce_different_keys` |
//!
//! **Do not merge tests** — each test targets exactly one mutation.

#![allow(clippy::unwrap_used)] // Reason: test code, panics acceptable
#![allow(clippy::missing_panics_doc)] // Reason: test code
#![allow(clippy::missing_errors_doc)] // Reason: test code

use fraiseql_core::{
    cache::{extract_accessed_views, generate_cache_key},
    db::{WhereClause, WhereOperator},
    schema::QueryDefinition,
};
use serde_json::json;

// ─── Shared fixtures ──────────────────────────────────────────────────────────

const FIXED_QUERY: &str = "query { users { id } }";
const FIXED_VERSION: &str = "schema-v42";

fn fixed_vars() -> serde_json::Value {
    json!({"limit": 5})
}

fn simple_where() -> WhereClause {
    WhereClause::Field {
        path: vec!["status".to_string()],
        operator: WhereOperator::Eq,
        value: json!("active"),
    }
}

fn other_where() -> WhereClause {
    WhereClause::Field {
        path: vec!["status".to_string()],
        operator: WhereOperator::Eq,
        value: json!("inactive"),
    }
}

// ─── M1: WHERE clause component ───────────────────────────────────────────────

/// If the WHERE clause were omitted from `combined`, changing only the WHERE
/// clause while keeping everything else constant would produce the same key.
#[test]
fn where_clause_component_contributes_independently() {
    let vars = fixed_vars();
    let key_none = generate_cache_key(FIXED_QUERY, &vars, None, FIXED_VERSION);
    let key_some = generate_cache_key(FIXED_QUERY, &vars, Some(&simple_where()), FIXED_VERSION);
    let key_other = generate_cache_key(FIXED_QUERY, &vars, Some(&other_where()), FIXED_VERSION);

    assert_ne!(key_none, key_some, "M1: WHERE clause must contribute to key");
    assert_ne!(key_some, key_other, "M1: Different WHERE values must produce different keys");
    assert_ne!(key_none, key_other, "M1: Absent vs present WHERE must differ");
}

// ─── M2: Schema version component ─────────────────────────────────────────────

/// If the schema version were omitted from `combined`, two calls with different
/// schema versions but the same query+vars+WHERE would collide.
#[test]
fn schema_version_component_contributes_independently() {
    let vars = fixed_vars();
    let k_v1 = generate_cache_key(FIXED_QUERY, &vars, Some(&simple_where()), "schema-v1");
    let k_v2 = generate_cache_key(FIXED_QUERY, &vars, Some(&simple_where()), "schema-v2");
    let k_v3 = generate_cache_key(FIXED_QUERY, &vars, Some(&simple_where()), "schema-v3");

    assert_ne!(k_v1, k_v2, "M2: Schema version must contribute to key (v1 vs v2)");
    assert_ne!(k_v2, k_v3, "M2: Schema version must contribute to key (v2 vs v3)");
    assert_ne!(k_v1, k_v3, "M2: Schema version must contribute to key (v1 vs v3)");
}

// ─── M3: Query + variables component (base_key) ───────────────────────────────

/// If the `base_key` (query + variables) were omitted, queries that differ only
/// in the query string would collide.
#[test]
fn query_component_contributes_independently() {
    let key_users = generate_cache_key("query { users { id } }", &json!({}), None, FIXED_VERSION);
    let key_posts = generate_cache_key("query { posts { id } }", &json!({}), None, FIXED_VERSION);
    assert_ne!(key_users, key_posts, "M3: Query string must contribute to key");
}

/// If variable values were dropped from the hash, two requests with different
/// variables would collide — this is a security regression.
#[test]
fn variables_component_contributes_independently() {
    let key_alice =
        generate_cache_key(FIXED_QUERY, &json!({"userId": "alice"}), None, FIXED_VERSION);
    let key_bob = generate_cache_key(FIXED_QUERY, &json!({"userId": "bob"}), None, FIXED_VERSION);
    assert_ne!(key_alice, key_bob, "M3: Variable values must contribute to key");
}

// ─── M4: additional_views in extract_accessed_views ───────────────────────────

/// If `views.extend(additional_views)` were removed, secondary views would be
/// absent — cache invalidation for `JOINed` views would silently break.
#[test]
fn extract_views_includes_additional_views() {
    let query_def = QueryDefinition {
        name: "usersWithPosts".to_string(),
        return_type: "UserWithPosts".to_string(),
        returns_list: true,
        sql_source: Some("v_user_with_posts".to_string()),
        additional_views: vec!["v_post".to_string(), "v_comment".to_string()],
        ..QueryDefinition::new("usersWithPosts", "UserWithPosts")
    };

    let views = extract_accessed_views(&query_def);

    assert!(views.contains(&"v_post".to_string()), "M4: additional_views must be included");
    assert!(
        views.contains(&"v_comment".to_string()),
        "M4: all additional_views must be included"
    );
    assert_eq!(views.len(), 3, "M4: primary + 2 additional = 3 views total");
}

// ─── M5: primary sql_source in extract_accessed_views ─────────────────────────

/// If the `sql_source` push were removed, the primary view would be absent —
/// mutation invalidation would silently skip the main view.
#[test]
fn extract_views_includes_primary_sql_source() {
    let query_def =
        QueryDefinition::new("users", "User").returning_list().with_sql_source("v_user");

    let views = extract_accessed_views(&query_def);

    assert_eq!(views, vec!["v_user"], "M5: primary sql_source must be in views");
    assert_eq!(views.len(), 1, "M5: no phantom entries");
}

#[test]
fn extract_views_with_no_sql_source_returns_empty() {
    let query_def = QueryDefinition::new("custom", "Custom");
    let views = extract_accessed_views(&query_def);
    assert!(views.is_empty(), "M5: no sql_source → empty views");
}

// ─── M6: None vs Some WHERE clause distinguishability ─────────────────────────

/// Catches a mutation that negates `is_some()` or unconditionally returns `""`.
#[test]
fn none_and_some_where_clause_produce_different_keys() {
    let k_none = generate_cache_key(FIXED_QUERY, &json!({}), None, "v1");
    let k_some = generate_cache_key(FIXED_QUERY, &json!({}), Some(&simple_where()), "v1");
    assert_ne!(k_none, k_some, "M6: None WHERE must differ from Some WHERE");
}

// ─── Determinism: all components stay stable across two calls ─────────────────

#[test]
fn key_is_fully_deterministic_with_all_components() {
    let vars = fixed_vars();
    let run1 = generate_cache_key(FIXED_QUERY, &vars, Some(&simple_where()), FIXED_VERSION);
    let run2 = generate_cache_key(FIXED_QUERY, &vars, Some(&simple_where()), FIXED_VERSION);
    assert_eq!(run1, run2, "Key must be deterministic across repeated calls");
}

/// A single-character difference in the WHERE value must change the entire hash.
#[test]
fn single_char_difference_in_where_value_changes_entire_key() {
    let w1 = WhereClause::Field {
        path: vec!["email".to_string()],
        operator: WhereOperator::Eq,
        value: json!("alice@example.com"),
    };
    let w2 = WhereClause::Field {
        path: vec!["email".to_string()],
        operator: WhereOperator::Eq,
        value: json!("blice@example.com"), // one char changed
    };

    let k1 = generate_cache_key(FIXED_QUERY, &json!({}), Some(&w1), FIXED_VERSION);
    let k2 = generate_cache_key(FIXED_QUERY, &json!({}), Some(&w2), FIXED_VERSION);

    assert_ne!(k1, k2, "Single-char WHERE difference must change key");
}
