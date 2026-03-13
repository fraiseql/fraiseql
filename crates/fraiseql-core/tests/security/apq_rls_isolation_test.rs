//! APQ / RLS cache key isolation — pure unit tests.
//!
//! Verifies that `generate_cache_key` produces distinct cache keys for
//! distinct RLS contexts.  These are pure in-process tests: no database,
//! no network.
//!
//! # Security Invariant
//!
//! APQ caches query results keyed by (query, variables, WHERE clause,
//! schema version).  RLS generates a per-user `WhereClause`, so two users
//! always get distinct keys and can never see each other's cached data.
//! A future refactor of `generate_cache_key` or `RlsPolicy::evaluate` that
//! breaks this invariant will cause these tests to fail immediately.

#![allow(clippy::tests_outside_test_module)] // Reason: integration test file, not a lib crate

use fraiseql_core::{
    cache::generate_cache_key,
    db::{WhereClause, WhereOperator},
};
use serde_json::json;

// ---------------------------------------------------------------------------
// Test 1 — Different users produce different cache keys
// ---------------------------------------------------------------------------

/// Two users with identical queries but different per-user RLS clauses must
/// receive different cache keys, preventing cross-user cache hits.
#[test]
fn test_apq_cache_key_differs_per_user() {
    let query = "{ posts { id title } }";
    let vars = json!({});
    let version = "abc123";

    let rls_alice = WhereClause::Field {
        path:     vec!["author_id".to_string()],
        operator: WhereOperator::Eq,
        value:    json!("alice"),
    };
    let rls_bob = WhereClause::Field {
        path:     vec!["author_id".to_string()],
        operator: WhereOperator::Eq,
        value:    json!("bob"),
    };

    let key_alice = generate_cache_key(query, &vars, Some(&rls_alice), version);
    let key_bob = generate_cache_key(query, &vars, Some(&rls_bob), version);

    assert_ne!(
        key_alice, key_bob,
        "Users with different RLS contexts must get different APQ cache keys"
    );
}

// ---------------------------------------------------------------------------
// Test 2 — Admin bypass (no RLS) differs from any regular-user key
// ---------------------------------------------------------------------------

/// An admin query with no RLS clause (`None`) must not share a cache entry
/// with a per-user filtered query.  Admins see all rows; users see a subset.
#[test]
fn test_apq_cache_key_admin_differs_from_user() {
    let query = "{ posts { id title } }";
    let vars = json!({});
    let version = "abc123";

    let rls_user = WhereClause::Field {
        path:     vec!["author_id".to_string()],
        operator: WhereOperator::Eq,
        value:    json!("alice"),
    };

    let key_admin = generate_cache_key(query, &vars, None, version);
    let key_user = generate_cache_key(query, &vars, Some(&rls_user), version);

    assert_ne!(
        key_admin, key_user,
        "Admin bypass (no RLS) must not share cache with per-user filtered results"
    );
}

// ---------------------------------------------------------------------------
// Test 3 — Multi-tenant: same user in different tenants → different keys
// ---------------------------------------------------------------------------

/// The same user ID appearing in two different tenants must produce distinct
/// cache keys.  This prevents a cross-tenant data leak in multi-tenant deployments.
#[test]
fn test_apq_cache_key_tenant_isolation() {
    let query = "{ orders { id amount } }";
    let vars = json!({});
    let version = "abc123";

    // Tenant ACME: tenant_id = 'acme' AND author_id = 'alice'
    let rls_acme = WhereClause::And(vec![
        WhereClause::Field {
            path:     vec!["tenant_id".to_string()],
            operator: WhereOperator::Eq,
            value:    json!("acme"),
        },
        WhereClause::Field {
            path:     vec!["author_id".to_string()],
            operator: WhereOperator::Eq,
            value:    json!("alice"),
        },
    ]);

    // Tenant GLOBEX: tenant_id = 'globex' AND author_id = 'alice'
    let rls_globex = WhereClause::And(vec![
        WhereClause::Field {
            path:     vec!["tenant_id".to_string()],
            operator: WhereOperator::Eq,
            value:    json!("globex"),
        },
        WhereClause::Field {
            path:     vec!["author_id".to_string()],
            operator: WhereOperator::Eq,
            value:    json!("alice"),
        },
    ]);

    let key_acme = generate_cache_key(query, &vars, Some(&rls_acme), version);
    let key_globex = generate_cache_key(query, &vars, Some(&rls_globex), version);

    assert_ne!(
        key_acme, key_globex,
        "Same user in different tenants must get different APQ cache keys"
    );
}

// ---------------------------------------------------------------------------
// Test 4 — Determinism: same context always yields the same key
// ---------------------------------------------------------------------------

/// Identical inputs must produce identical keys on every call.  This is
/// required for cache hits to work and proves the hash is not randomised.
#[test]
fn test_apq_cache_key_stable_for_same_context() {
    let query = "{ posts { id title } }";
    let vars = json!({"limit": 10});
    let version = "abc123";

    let rls = WhereClause::Field {
        path:     vec!["author_id".to_string()],
        operator: WhereOperator::Eq,
        value:    json!("alice"),
    };

    let key1 = generate_cache_key(query, &vars, Some(&rls), version);
    let key2 = generate_cache_key(query, &vars, Some(&rls), version);

    assert_eq!(key1, key2, "Cache key must be deterministic for identical inputs");
}

// ---------------------------------------------------------------------------
// Test 5 — Schema version change invalidates all cache entries
// ---------------------------------------------------------------------------

/// A change in the compiled schema version (e.g., after re-deploying) must
/// produce a different cache key even if the query, variables, and RLS clause
/// are identical.  This prevents stale-schema cache hits after deploys.
#[test]
fn test_apq_cache_key_changes_on_schema_version() {
    let query = "{ posts { id title } }";
    let vars = json!({});

    let rls = WhereClause::Field {
        path:     vec!["author_id".to_string()],
        operator: WhereOperator::Eq,
        value:    json!("alice"),
    };

    let key_v1 = generate_cache_key(query, &vars, Some(&rls), "schema_v1");
    let key_v2 = generate_cache_key(query, &vars, Some(&rls), "schema_v2");

    assert_ne!(
        key_v1, key_v2,
        "Schema version change must invalidate all cached entries"
    );
}
