//! Adversarial + behavioural test suite ported verbatim from #242
//! (`routes/enrichment.rs`, `v2.2.1`). These are the hard-to-get-right part of
//! the primitive and they are already correct; they are ported first (DESIGN
//! §8, P00) and pinned as a fixed point before any new resolver logic lands.

#![allow(clippy::unwrap_used)] // Reason: test code, panics acceptable

use std::{
    collections::HashMap,
    time::{Duration, Instant},
};

use super::{
    cache::{CacheEntry, EnrichmentCache},
    query::prepare_enrichment_query,
};

// ── prepare_enrichment_query ─────────────────────────────────────────────

#[test]
fn rewrites_single_param() {
    let mut claims = HashMap::new();
    claims.insert("sub".to_owned(), serde_json::json!("user-123"));

    let bound =
        prepare_enrichment_query("SELECT role FROM users WHERE sub = $sub", &claims).unwrap();

    assert_eq!(bound.sql, "SELECT role FROM users WHERE sub = $1");
    assert_eq!(bound.binds.len(), 1);
    assert_eq!(bound.binds[0], serde_json::json!("user-123"));
}

#[test]
fn rewrites_multiple_params() {
    let mut claims = HashMap::new();
    claims.insert("sub".to_owned(), serde_json::json!("u1"));
    claims.insert("email".to_owned(), serde_json::json!("a@b.com"));

    let bound = prepare_enrichment_query(
        "SELECT role FROM users WHERE sub = $sub AND email = $email",
        &claims,
    )
    .unwrap();

    assert_eq!(bound.sql, "SELECT role FROM users WHERE sub = $1 AND email = $2");
    assert_eq!(bound.binds.len(), 2);
}

#[test]
fn reuses_position_for_repeated_param() {
    let mut claims = HashMap::new();
    claims.insert("sub".to_owned(), serde_json::json!("u1"));

    let bound =
        prepare_enrichment_query("SELECT * FROM users WHERE sub = $sub OR alt_sub = $sub", &claims)
            .unwrap();

    assert_eq!(bound.sql, "SELECT * FROM users WHERE sub = $1 OR alt_sub = $1");
    assert_eq!(bound.binds.len(), 1);
}

#[test]
fn missing_param_returns_error() {
    let claims = HashMap::new();

    let err = prepare_enrichment_query("SELECT 1 WHERE sub = $sub", &claims).unwrap_err();

    assert!(err.contains("$sub"));
    assert!(err.contains("not in the JWT claims"));
}

#[test]
fn no_params_passes_through() {
    let claims = HashMap::new();

    let bound = prepare_enrichment_query("SELECT 1 AS one", &claims).unwrap();

    assert_eq!(bound.sql, "SELECT 1 AS one");
    assert!(bound.binds.is_empty());
}

#[test]
fn preserves_dollar_followed_by_digit() {
    let claims = HashMap::new();

    let bound = prepare_enrichment_query("SELECT $1", &claims).unwrap();

    // $1 is NOT a named param (digit after $) — passed through as-is.
    assert_eq!(bound.sql, "SELECT $1");
}

// ── EnrichmentCache ──────────────────────────────────────────────────────

#[test]
fn cache_hit_returns_value() {
    let cache = EnrichmentCache::new();
    let mut map = serde_json::Map::new();
    map.insert("role".to_owned(), serde_json::json!("admin"));

    cache.insert("user-1".to_owned(), map, Duration::from_secs(60));

    let result = cache.get("user-1");
    assert!(result.is_some());
    assert_eq!(result.unwrap()["role"], "admin");
}

#[test]
fn cache_miss_returns_none() {
    let cache = EnrichmentCache::new();
    assert!(cache.get("nonexistent").is_none());
}

#[test]
fn expired_entry_returns_none() {
    let cache = EnrichmentCache::new();
    let map = serde_json::Map::new();

    // Insert with an already-expired timestamp.
    cache.entries.insert(
        "user-1".to_owned(),
        CacheEntry {
            value:      map,
            expires_at: Instant::now().checked_sub(Duration::from_secs(1)).unwrap(),
        },
    );

    assert!(cache.get("user-1").is_none());
}

// ── Security: adversarial inputs ─────────────────────────────────────────

#[test]
fn sql_injection_in_claim_value_is_bound_not_interpolated() {
    let mut claims = HashMap::new();
    claims.insert("email".to_owned(), serde_json::json!("'; DROP TABLE users; --"));

    let bound =
        prepare_enrichment_query("SELECT role FROM users WHERE email = $email", &claims).unwrap();

    // The malicious value must appear as a bind parameter, not in the SQL.
    assert_eq!(bound.sql, "SELECT role FROM users WHERE email = $1");
    assert_eq!(bound.binds[0], serde_json::json!("'; DROP TABLE users; --"));
    assert!(!bound.sql.contains("DROP"));
}

#[test]
fn sql_comment_in_claim_value_is_bound_not_interpolated() {
    let mut claims = HashMap::new();
    claims.insert("sub".to_owned(), serde_json::json!("user /* */ OR 1=1"));

    let bound =
        prepare_enrichment_query("SELECT role FROM users WHERE sub = $sub", &claims).unwrap();

    assert_eq!(bound.sql, "SELECT role FROM users WHERE sub = $1");
    assert_eq!(bound.binds[0], serde_json::json!("user /* */ OR 1=1"));
    assert!(!bound.sql.contains("/*"));
}

#[test]
fn overlapping_param_names_are_distinguished() {
    // $email vs $email_verified — ensure the greedy match doesn't treat
    // $email_verified as "$email" + "verified".
    let mut claims = HashMap::new();
    claims.insert("email".to_owned(), serde_json::json!("a@b.com"));
    claims.insert("email_verified".to_owned(), serde_json::json!(true));

    let bound = prepare_enrichment_query(
        "SELECT * FROM users WHERE email = $email AND verified = $email_verified",
        &claims,
    )
    .unwrap();

    assert_eq!(bound.sql, "SELECT * FROM users WHERE email = $1 AND verified = $2");
    assert_eq!(bound.binds.len(), 2);
    assert_eq!(bound.binds[0], serde_json::json!("a@b.com"));
    assert_eq!(bound.binds[1], serde_json::json!(true));
}

#[test]
fn param_at_end_of_query() {
    let mut claims = HashMap::new();
    claims.insert("sub".to_owned(), serde_json::json!("u1"));

    let bound = prepare_enrichment_query("SELECT * FROM users WHERE sub = $sub", &claims).unwrap();

    assert_eq!(bound.sql, "SELECT * FROM users WHERE sub = $1");
}

#[test]
fn unicode_claim_value_is_bound() {
    let mut claims = HashMap::new();
    claims.insert("sub".to_owned(), serde_json::json!("用户-émoji-🍓"));

    let bound =
        prepare_enrichment_query("SELECT role FROM users WHERE sub = $sub", &claims).unwrap();

    assert_eq!(bound.binds[0], serde_json::json!("用户-émoji-🍓"));
}
