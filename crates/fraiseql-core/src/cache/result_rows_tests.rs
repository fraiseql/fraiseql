//! The row index: entity rows, rows read by id and collections of one parent (#1778).

#![allow(clippy::unwrap_used)] // Reason: test code, panics are acceptable

use std::sync::Arc;

use serde_json::json;

use super::{ComposedTags, QueryResultCache};
use crate::{
    cache::{CacheConfig, tviews_feed::Collection},
    db::types::JsonbValue,
};

fn cache() -> QueryResultCache {
    QueryResultCache::new(CacheConfig::enabled())
}

fn rows(ids: &[&str]) -> Arc<Vec<JsonbValue>> {
    Arc::new(ids.iter().map(|id| JsonbValue::new(json!({"id": id}))).collect())
}

/// A list of two users, a read of one by id and a composed read of one post's comments.
fn fill(cache: &QueryResultCache) {
    cache
        .put_arc(1, rows(&["a", "b"]), vec!["tv_user".to_string()], None, Some("User"), None)
        .unwrap();
    cache
        .put_by_id(
            2,
            rows(&["a"]),
            vec!["tv_user".to_string()],
            None,
            ("User".to_string(), "a".to_string()),
            None,
        )
        .unwrap();
    let tags = ComposedTags {
        collected_views: vec!["tv_comment".to_string()],
        collections:     vec![(Collection::new("Comment", "fk_post"), "p".to_string())],
        entities:        Vec::new(),
    };
    cache
        .put_composed(3, rows(&["p"]), vec!["tv_post".to_string()], None, None, tags, None)
        .unwrap();
}

#[test]
fn an_evicted_entry_leaves_no_row_key_behind() {
    let cache = cache();
    fill(&cache);
    assert!(cache.row_key_count() > 0);

    for key in [1, 2, 3] {
        cache.store.invalidate(&key);
    }
    cache.store.run_pending_tasks();

    assert_eq!(cache.row_key_count(), 0, "a row no entry holds is not kept");
}

#[test]
fn a_row_still_held_by_an_entry_keeps_its_key() {
    let cache = cache();
    fill(&cache);

    cache.store.invalidate(&2);
    cache.store.run_pending_tasks();

    assert_eq!(cache.invalidate_by_entity("User", "a").unwrap(), 1, "the list still holds a");
}

#[test]
fn a_replaced_entry_leaves_its_replacement_reachable_by_each_of_its_rows() {
    let cache = cache();
    fill(&cache);
    fill(&cache);
    cache.store.run_pending_tasks();

    assert_eq!(cache.invalidate_by_entity("User", "b").unwrap(), 1);
    assert_eq!(cache.invalidate_by_id("User", "a").unwrap(), 1);
    assert_eq!(
        cache
            .invalidate_collection(&Collection::new("Comment", "fk_post"), "p")
            .unwrap(),
        1
    );
}

/// A fill whose row is evicted after its pre-check and before its insert is withdrawn by
/// its post-insert check. Between the two it is in the store, and a reader there must not
/// be served it: the eviction has returned, so its rows are a write too old.
#[test]
fn a_fill_whose_row_is_evicted_while_it_inserts_is_never_served() {
    type Evict = fn(&QueryResultCache);
    let evictions: [(&str, Evict); 3] = [
        ("entity", |cache| {
            cache.invalidate_by_entity("User", "a").unwrap();
        }),
        ("row read by id", |cache| {
            cache.invalidate_by_id("User", "a").unwrap();
        }),
        ("collection", |cache| {
            cache
                .invalidate_collection(&Collection::new("Comment", "fk_post"), "p")
                .unwrap();
        }),
    ];
    for (row, evict) in evictions {
        let cache = cache();
        let fence = Some(cache.invalidation_generation());
        let served_mid_fill = || cache.get(1).unwrap().is_some();
        let mut seen = false;
        let views = vec!["tv_user".to_string()];
        match row {
            "entity" => cache
                .put_arc_with(
                    1,
                    rows(&["a"]),
                    views,
                    None,
                    Some("User"),
                    fence,
                    || evict(&cache),
                    || {
                        seen = served_mid_fill();
                    },
                )
                .unwrap(),
            "row read by id" => cache
                .put_by_id_with(
                    1,
                    rows(&["a"]),
                    views,
                    None,
                    ("User".to_string(), "a".to_string()),
                    fence,
                    || evict(&cache),
                    || seen = served_mid_fill(),
                )
                .unwrap(),
            _ => {
                let tags = ComposedTags {
                    collections: vec![(Collection::new("Comment", "fk_post"), "p".to_string())],
                    ..ComposedTags::default()
                };
                cache
                    .put_tagged(
                        1,
                        rows(&["p"]),
                        views,
                        None,
                        None,
                        tags,
                        None,
                        fence,
                        || evict(&cache),
                        || seen = served_mid_fill(),
                    )
                    .unwrap();
            },
        }
        assert!(!seen, "{row}: the fill was served between its insert and its withdrawal");
        assert!(!served_mid_fill(), "{row}: the fill was left in the store");
    }
}
