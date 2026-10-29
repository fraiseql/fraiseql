//! A hit is served only while no name its entry reads has moved since its read (#1778).
//!
//! These stamp the fence alone, evicting nothing, so a refused entry is still in the store:
//! the hit check is what refuses it.

#![allow(clippy::unwrap_used)] // Reason: test code, panics are acceptable

use std::sync::Arc;

use fraiseql_db::ViewName;
use serde_json::json;

use super::{ComposedTags, QueryResultCache};
use crate::{
    cache::{CacheConfig, tviews_feed::Collection},
    db::types::JsonbValue,
};

const LIST: u64 = 1;
const BY_ID: u64 = 2;
const COMPOSED: u64 = 3;

fn cache() -> QueryResultCache {
    QueryResultCache::new(CacheConfig::enabled())
}

fn rows() -> Arc<Vec<JsonbValue>> {
    Arc::new(vec![JsonbValue::new(json!({"id": "a"}))])
}

/// `users` from `tv_user`.
fn list_users(cache: &QueryResultCache, fence: Option<u64>) {
    cache
        .put_arc(LIST, rows(), vec!["tv_user".to_string()], None, Some("User"), fence)
        .unwrap();
}

/// `user(id: a)` from `tv_user`.
fn read_user(cache: &QueryResultCache) {
    cache
        .put_by_id(
            BY_ID,
            rows(),
            vec!["tv_user".to_string()],
            None,
            ("User".to_string(), "a".to_string()),
            None,
        )
        .unwrap();
}

/// `posts { comments }` from `tv_post`, its comments a level tagged by collection.
fn compose_posts(cache: &QueryResultCache) {
    let tags = ComposedTags {
        collected_views: vec!["tv_comment".to_string()],
        collections:     vec![(Collection::new("Comment", "fk_post"), "p".to_string())],
        entities:        Vec::new(),
    };
    cache
        .put_composed(COMPOSED, rows(), vec!["tv_post".to_string()], None, None, tags, None)
        .unwrap();
}

fn served(cache: &QueryResultCache, key: u64) -> bool {
    cache.get(key).unwrap().is_some()
}

/// Refused on a hit, though nothing evicted it.
fn refused(cache: &QueryResultCache, key: u64) -> bool {
    cache.store.contains_key(&key) && !served(cache, key)
}

#[test]
fn a_full_sweep_of_a_view_refuses_every_entry_reading_it() {
    let cache = cache();
    list_users(&cache, None);
    read_user(&cache);
    compose_posts(&cache);

    cache.fence.stamp_views(["tv_user", "tv_comment"]);

    assert!(refused(&cache, LIST), "a list of the view");
    assert!(refused(&cache, BY_ID), "a read of one row of it by id");
    assert!(refused(&cache, COMPOSED), "a composed level tagged by collection");
}

#[test]
fn a_feed_sweep_refuses_the_lists_of_a_view_and_not_its_levels_tagged_by_collection() {
    let cache = cache();
    list_users(&cache, None);
    read_user(&cache);
    compose_posts(&cache);

    cache.fence.stamp_feed_views(["tv_user", "tv_comment"]);

    assert!(refused(&cache, LIST));
    assert!(served(&cache, BY_ID), "a read by id is evicted by its row");
    assert!(served(&cache, COMPOSED), "a level tagged by collection is evicted by it");
}

#[test]
fn a_write_sweep_refuses_lists_and_levels_tagged_by_collection_and_not_reads_by_id() {
    let cache = cache();
    list_users(&cache, None);
    read_user(&cache);
    compose_posts(&cache);

    cache.fence.stamp_write_views(["tv_user", "tv_comment"]);

    assert!(refused(&cache, LIST));
    assert!(served(&cache, BY_ID), "a read by id is evicted by its row");
    assert!(refused(&cache, COMPOSED));
}

#[test]
fn a_stamp_of_a_type_read_by_id_refuses_those_reads_and_nothing_else() {
    let cache = cache();
    list_users(&cache, None);
    read_user(&cache);

    cache.fence.stamp_by_id_type("User");

    assert!(refused(&cache, BY_ID));
    assert!(served(&cache, LIST));
}

#[test]
fn a_stamp_of_a_collection_type_refuses_the_entries_composing_one() {
    let cache = cache();
    list_users(&cache, None);
    compose_posts(&cache);

    cache.fence.stamp_collection_type("Comment");

    assert!(refused(&cache, COMPOSED));
    assert!(served(&cache, LIST));
}

#[test]
fn a_sweep_of_a_view_no_entry_reads_leaves_every_hit_served() {
    let cache = cache();
    list_users(&cache, None);
    read_user(&cache);
    compose_posts(&cache);

    cache.fence.stamp_views(["tv_order"]);

    assert!(served(&cache, LIST));
    assert!(served(&cache, BY_ID));
    assert!(served(&cache, COMPOSED));
}

#[test]
fn a_clear_refuses_an_entry_left_in_the_store() {
    let cache = cache();
    list_users(&cache, None);

    cache.fence.stamp_everything();

    assert!(refused(&cache, LIST));
}

#[test]
fn a_fill_straddling_a_sweep_of_its_view_is_never_served() {
    let cache = cache();
    let snapshot = cache.invalidation_generation();
    cache.invalidate_views(&[ViewName::from("tv_user")]).unwrap();

    list_users(&cache, Some(snapshot));

    assert!(!served(&cache, LIST));
}

#[test]
fn a_fill_after_a_sweep_is_served() {
    let cache = cache();
    cache.invalidate_views(&[ViewName::from("tv_user")]).unwrap();
    let snapshot = cache.invalidation_generation();

    list_users(&cache, Some(snapshot));

    assert!(served(&cache, LIST));
}

#[test]
fn invalidations_of_names_no_entry_reads_register_none() {
    let cache = cache();

    cache.invalidate_views(&[ViewName::from("v_from_a_request")]).unwrap();
    cache
        .invalidate_views_except_by_id(&[ViewName::from("v_from_a_request")])
        .unwrap();
    cache
        .invalidate_views_except_collections(&[ViewName::from("v_from_a_request")])
        .unwrap();
    cache.invalidate_by_id_type("FromARequest").unwrap();
    cache.invalidate_collections_of("FromARequest").unwrap();

    assert_eq!(cache.fence.name_count(), 0);
}

#[test]
fn a_clear_drops_every_name_counter() {
    let cache = cache();
    list_users(&cache, None);
    read_user(&cache);
    compose_posts(&cache);
    assert!(cache.fence.name_count() > 0);

    cache.clear().unwrap();

    assert_eq!(cache.fence.name_count(), 0);
}

#[test]
fn a_fill_after_a_clear_is_refused_by_a_later_sweep() {
    let cache = cache();
    list_users(&cache, None);
    cache.clear().unwrap();
    list_users(&cache, Some(cache.invalidation_generation()));
    assert!(served(&cache, LIST));

    cache.fence.stamp_views(["tv_user"]);

    assert!(refused(&cache, LIST));
}

#[test]
fn a_fill_straddling_a_sweep_of_another_view_is_served() {
    let cache = cache();
    let snapshot = cache.invalidation_generation();
    cache.invalidate_views(&[ViewName::from("tv_order")]).unwrap();

    list_users(&cache, Some(snapshot));

    assert!(served(&cache, LIST), "the first fill of a name starts its counter at its floor");
}

#[test]
fn a_counter_created_after_its_name_was_stamped_starts_past_the_stamp_for_every_kind() {
    use super::Reach;

    type Stamp = fn(&QueryResultCache);
    let kinds: [(&str, Stamp, Reach<'static>); 5] = [
        (
            "full sweep",
            |c| c.fence.stamp_views(["v"]),
            Reach {
                views: vec!["v"],
                ..Reach::default()
            },
        ),
        (
            "feed sweep",
            |c| c.fence.stamp_feed_views(["v"]),
            Reach {
                views: vec!["v"],
                ..Reach::default()
            },
        ),
        (
            "write sweep",
            |c| c.fence.stamp_write_views(["v"]),
            Reach {
                collected_views: vec!["v"],
                ..Reach::default()
            },
        ),
        (
            "type read by id",
            |c| c.fence.stamp_by_id_type("T"),
            Reach {
                by_id: Some((("T", "a"), Vec::new())),
                ..Reach::default()
            },
        ),
        (
            "collection type",
            |c| c.fence.stamp_collection_type("T"),
            Reach {
                collections: vec![("T", "fk", "p")],
                ..Reach::default()
            },
        ),
    ];
    for (kind, stamp, reach) in kinds {
        let cache = cache();
        let snapshot = cache.invalidation_generation();
        stamp(&cache);

        let counters = cache.fence.counters(&reach);

        assert!(!cache.fence.current(snapshot, &counters), "{kind}");
    }
}

#[test]
fn a_clear_refuses_an_entry_reading_no_name() {
    let cache = cache();
    cache.put_arc(LIST, rows(), Vec::new(), None, None, None).unwrap();
    assert!(served(&cache, LIST));

    cache.fence.stamp_everything();

    assert!(refused(&cache, LIST));
}
