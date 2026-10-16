//! Reads of one row by id, indexed under that row (#1576).

#![allow(clippy::unwrap_used)] // Reason: test code, panics are acceptable

use std::sync::Arc;

use fraiseql_db::ViewName;
use serde_json::json;

use super::QueryResultCache;
use crate::{cache::CacheConfig, db::types::JsonbValue};

const A: &str = "a";
const B: &str = "b";

fn cache() -> QueryResultCache {
    QueryResultCache::new(CacheConfig::enabled())
}

fn rows(id: &str) -> Arc<Vec<JsonbValue>> {
    Arc::new(vec![JsonbValue::new(json!({"id": id}))])
}

/// `user(id: id)` from `tv_user`, cached under `key`.
fn read_user(cache: &QueryResultCache, key: u64, id: &str, fence: Option<u64>) {
    cache
        .put_by_id(
            key,
            rows(id),
            vec!["tv_user".to_string()],
            None,
            ("User".to_string(), id.to_string()),
            fence,
        )
        .unwrap();
}

/// `users` from `tv_user`, a list, cached under `key`.
fn list_users(cache: &QueryResultCache, key: u64, fence: Option<u64>) {
    cache
        .put_arc(key, rows(A), vec!["tv_user".to_string()], None, Some("User"), fence)
        .unwrap();
}

fn cached(cache: &QueryResultCache, key: u64) -> bool {
    cache.get(key).unwrap().is_some()
}

fn tv_user() -> [ViewName; 1] {
    [ViewName::from("tv_user")]
}

#[test]
fn a_write_reporting_its_rows_sweeps_the_lists_and_not_the_reads_by_id() {
    let cache = cache();
    read_user(&cache, 1, A, None);
    list_users(&cache, 2, None);
    cache.invalidate_views_except_by_id(&tv_user()).unwrap();
    assert!(cached(&cache, 1), "user(id: a) is evicted by its row, not the sweep");
    assert!(!cached(&cache, 2), "the list is swept");
}

#[test]
fn a_full_sweep_evicts_the_reads_by_id_too() {
    let cache = cache();
    read_user(&cache, 1, A, None);
    cache.invalidate_views(&tv_user()).unwrap();
    assert!(!cached(&cache, 1));
}

#[test]
fn a_row_evicts_the_reads_of_that_row_only() {
    let cache = cache();
    read_user(&cache, 1, A, None);
    read_user(&cache, 2, B, None);
    assert_eq!(cache.invalidate_by_id("User", B).unwrap(), 1);
    assert!(cached(&cache, 1));
    assert!(!cached(&cache, 2));
    // The same id of another type is another row.
    assert_eq!(cache.invalidate_by_id("Post", A).unwrap(), 0);
    assert!(cached(&cache, 1));
}

#[test]
fn a_type_evicts_every_read_of_its_rows_by_id() {
    let cache = cache();
    read_user(&cache, 1, A, None);
    read_user(&cache, 2, B, None);
    assert_eq!(cache.invalidate_by_id_type("User").unwrap(), 2);
    assert!(!cached(&cache, 1) && !cached(&cache, 2));
}

/// The entity eviction a write still makes (a view that did not resolve) reaches a read of
/// that entity by id.
#[test]
fn an_entity_eviction_evicts_its_read_by_id() {
    let cache = cache();
    read_user(&cache, 1, A, None);
    read_user(&cache, 2, B, None);
    cache.invalidate_by_entity("User", A).unwrap();
    assert!(!cached(&cache, 1));
    assert!(cached(&cache, 2));
}

#[test]
fn a_clear_forgets_the_rows() {
    let cache = cache();
    read_user(&cache, 1, A, None);
    cache.clear().unwrap();
    read_user(&cache, 2, B, None);
    assert_eq!(cache.invalidate_by_id("User", A).unwrap(), 0, "no stale registration");
    assert_eq!(cache.invalidate_by_id("User", B).unwrap(), 1);
}

/// The fence: a read of A straddling a write to B is stored (under steady writes to other
/// rows, a fence on the view stored nothing); straddling a write to A, or anything reaching
/// every read of A, it is refused.
#[test]
fn a_fill_is_refused_only_by_what_reaches_its_row() {
    type Invalidate = fn(&QueryResultCache);
    let reaches: [(&str, Invalidate); 5] = [
        ("its row", |c| {
            let _ = c.invalidate_by_id("User", A);
        }),
        ("its type", |c| {
            let _ = c.invalidate_by_id_type("User");
        }),
        ("a full sweep of its view", |c| {
            let _ = c.invalidate_views(&tv_user());
        }),
        ("its entity", |c| {
            let _ = c.invalidate_by_entity("User", A);
        }),
        ("a clear", |c| {
            let _ = c.clear();
        }),
    ];
    for (what, invalidate) in reaches {
        let cache = cache();
        let fence = cache.invalidation_generation();
        invalidate(&cache);
        read_user(&cache, 1, A, Some(fence));
        assert!(!cached(&cache, 1), "{what} refuses the fill");
    }
    let misses: [(&str, Invalidate); 4] = [
        ("another row", |c| {
            let _ = c.invalidate_by_id("User", B);
        }),
        ("a write's sweep of its view", |c| {
            let _ = c.invalidate_views_except_by_id(&tv_user());
        }),
        ("a feed's sweep of its view", |c| {
            let _ = c.invalidate_views_except_collections(&tv_user());
        }),
        ("another entity", |c| {
            let _ = c.invalidate_by_entity("User", B);
        }),
    ];
    for (what, invalidate) in misses {
        let cache = cache();
        let fence = cache.invalidation_generation();
        invalidate(&cache);
        read_user(&cache, 1, A, Some(fence));
        assert!(cached(&cache, 1), "{what} does not refuse the fill");
    }
}

/// A write's sweep still refuses a list's fill.
#[test]
fn a_writes_sweep_refuses_a_lists_fill() {
    let cache = cache();
    let fence = cache.invalidation_generation();
    cache.invalidate_views_except_by_id(&tv_user()).unwrap();
    list_users(&cache, 2, Some(fence));
    assert!(!cached(&cache, 2));
}

/// The authoritative check, after the insert: an eviction of the row landing between the
/// pre-check and the insert withdraws the fill.
#[test]
fn a_row_eviction_landing_before_the_insert_withdraws_the_fill() {
    let cache = cache();
    let fence = cache.invalidation_generation();
    cache
        .put_by_id_with(
            1,
            rows(A),
            vec!["tv_user".to_string()],
            None,
            ("User".to_string(), A.to_string()),
            Some(fence),
            || {
                let _ = cache.invalidate_by_id("User", A);
            },
            || {},
        )
        .unwrap();
    assert!(!cached(&cache, 1));
}

/// One write reports every row it rewrote, a user's posts and comments with them: thousands
/// of rows a second under steady writes. Rows have slots of their own, enough that a read of
/// another row is not refused by sharing one (#1576).
#[test]
fn a_fill_survives_the_stamps_of_a_thousand_other_rows() {
    let cache = cache();
    let fence = cache.invalidation_generation();
    for n in 0..1_000 {
        let _ = cache.invalidate_by_id("Post", &format!("00000000-0000-0000-0000-{n:012}"));
    }
    read_user(&cache, 1, A, Some(fence));
    assert!(cached(&cache, 1));
}
