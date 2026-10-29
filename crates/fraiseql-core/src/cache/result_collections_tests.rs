//! Collection tags of composed entries (#1589).

#![allow(clippy::unwrap_used)] // Reason: test code, panics are acceptable

use std::sync::Arc;

use fraiseql_db::ViewName;
use serde_json::json;

use super::{ComposedTags, QueryResultCache};
use crate::{
    cache::{CacheConfig, tviews_feed::Collection},
    db::types::JsonbValue,
};

fn cache() -> QueryResultCache {
    QueryResultCache::new(CacheConfig::enabled())
}

fn rows() -> Arc<Vec<JsonbValue>> {
    Arc::new(vec![JsonbValue::new(
        json!({"d": {"id": "p1"}, "e": {"comments": []}}),
    )])
}

fn comments_by_post() -> Collection {
    Collection::new("Comment", "fk_post")
}

/// An entry composing `parents`' comments from `tv_comment`, read from `tv_post`.
fn compose(cache: &QueryResultCache, key: u64, parents: &[&str], fence: Option<u64>) {
    let tags = ComposedTags {
        collected_views: vec!["tv_comment".to_string()],
        collections:     parents.iter().map(|p| (comments_by_post(), (*p).to_string())).collect(),
        entities:        Vec::new(),
    };
    cache
        .put_composed(key, rows(), vec!["tv_post".to_string()], None, None, tags, fence)
        .unwrap();
}

fn cached(cache: &QueryResultCache, key: u64) -> bool {
    cache.get(key).unwrap().is_some()
}

#[test]
fn a_collection_evicts_the_entries_composing_that_parent_only() {
    let cache = cache();
    compose(&cache, 1, &["p1", "p2"], None);
    compose(&cache, 2, &["p3"], None);

    assert_eq!(cache.invalidate_collection(&comments_by_post(), "p2").unwrap(), 1);
    assert!(!cached(&cache, 1), "composes p2's comments");
    assert!(cached(&cache, 2), "composes p3's only");
}

#[test]
fn another_collection_of_the_same_parent_evicts_nothing() {
    let cache = cache();
    compose(&cache, 1, &["p1"], None);

    let by_author = Collection::new("Comment", "fk_author");
    assert_eq!(cache.invalidate_collection(&by_author, "p1").unwrap(), 0);
    assert!(cached(&cache, 1));
}

#[test]
fn a_child_type_evicts_every_collection_of_it_and_no_other() {
    let cache = cache();
    compose(&cache, 1, &["p1"], None);
    compose(&cache, 2, &["p2"], None);
    let tags = ComposedTags {
        collected_views: vec!["tv_tag".to_string()],
        collections:     vec![(Collection::new("Tag", "fk_post"), "p1".to_string())],
        entities:        Vec::new(),
    };
    cache
        .put_composed(3, rows(), vec!["tv_post".to_string()], None, None, tags, None)
        .unwrap();

    cache.invalidate_collections_of("Comment").unwrap();
    assert!(!cached(&cache, 1) && !cached(&cache, 2));
    assert!(cached(&cache, 3), "composes tags, not comments");
}

#[test]
fn a_sweep_of_a_collected_view_evicts_unless_it_keeps_collections() {
    let cache = cache();
    compose(&cache, 1, &["p1"], None);

    let comments = [ViewName::from("tv_comment")];
    cache.invalidate_views_except_collections(&comments).unwrap();
    assert!(cached(&cache, 1), "the feed names the comments' collections instead");

    cache.invalidate_views(&comments).unwrap();
    assert!(!cached(&cache, 1), "a write FraiseQL ran sweeps it at once");
}

#[test]
fn a_sweep_that_keeps_collections_still_evicts_by_the_entrys_own_views() {
    let cache = cache();
    compose(&cache, 1, &["p1"], None);

    let posts = [ViewName::from("tv_post")];
    cache.invalidate_views_except_collections(&posts).unwrap();
    assert!(!cached(&cache, 1));
}

#[test]
fn a_read_that_straddles_a_sweep_of_a_collected_view_is_not_stored() {
    let cache = cache();
    let fence = cache.invalidation_generation();
    cache.invalidate_views(&[ViewName::from("tv_comment")]).unwrap();

    compose(&cache, 1, &["p1"], Some(fence));
    assert!(!cached(&cache, 1));
}

#[test]
fn a_read_that_straddles_another_child_types_eviction_is_stored() {
    let cache = cache();
    let fence = cache.invalidation_generation();
    cache.invalidate_collections_of("Tag").unwrap();

    compose(&cache, 1, &["p1"], Some(fence));
    assert!(cached(&cache, 1), "a tag write cannot make comments stale (#1584)");
}

#[test]
fn an_evicted_entry_leaves_no_collection_behind() {
    let cache = cache();
    compose(&cache, 1, &["p1"], None);
    cache.store.invalidate(&1);
    cache.store.run_pending_tasks();

    assert_eq!(cache.invalidate_collection(&comments_by_post(), "p1").unwrap(), 0);
}

#[test]
fn a_clear_leaves_no_collection_behind() {
    let cache = cache();
    compose(&cache, 1, &["p1"], None);
    cache.clear().unwrap();

    assert_eq!(cache.invalidate_collection(&comments_by_post(), "p1").unwrap(), 0);
}

#[test]
fn a_read_that_straddles_the_feeds_sweep_of_a_collected_view_is_stored() {
    // The feed names the collections a comment write changed: its sweep of tv_comment does
    // not reach the levels tagged by collection, so it must not refuse their fill either.
    let cache = cache();
    let fence = cache.invalidation_generation();
    cache
        .invalidate_views_except_collections(&[ViewName::from("tv_comment")])
        .unwrap();

    compose(&cache, 1, &["p1"], Some(fence));
    assert!(cached(&cache, 1));
}

#[test]
fn a_read_that_straddles_another_parents_collection_eviction_is_stored() {
    let cache = cache();
    let fence = cache.invalidation_generation();
    cache.invalidate_collection(&comments_by_post(), "p9").unwrap();

    compose(&cache, 1, &["p1"], Some(fence));
    assert!(cached(&cache, 1), "a comment under p9 cannot make p1's comments stale");
}

#[test]
fn a_read_that_straddles_its_own_parents_collection_eviction_is_not_stored() {
    let cache = cache();
    let fence = cache.invalidation_generation();
    cache.invalidate_collection(&comments_by_post(), "p1").unwrap();

    compose(&cache, 1, &["p1"], Some(fence));
    assert!(!cached(&cache, 1));
}

#[test]
fn a_read_that_straddles_its_child_types_eviction_is_not_stored() {
    let cache = cache();
    let fence = cache.invalidation_generation();
    cache.invalidate_collections_of("Comment").unwrap();

    compose(&cache, 1, &["p1"], Some(fence));
    assert!(!cached(&cache, 1));
}

#[test]
fn a_plain_read_that_straddles_the_feeds_sweep_of_its_view_is_not_stored() {
    let cache = cache();
    let fence = cache.invalidation_generation();
    cache.invalidate_views_except_collections(&[ViewName::from("tv_post")]).unwrap();

    compose(&cache, 1, &["p1"], Some(fence));
    assert!(!cached(&cache, 1), "tv_post is the entry's own view");
}

/// #1605: a composed read holds its comments' rows by `(type, id)`. An eviction of a comment
/// it holds refuses its fill; an eviction of another comment, a write on another post, does
/// not: under steady comment writes a per-type fence stored no composed read at all.
#[test]
fn a_composed_fill_is_fenced_by_the_rows_it_holds_not_by_their_type() {
    let compose_holding = |cache: &QueryResultCache, fence: Option<u64>| {
        let tags = ComposedTags {
            collected_views: vec!["tv_comment".to_string()],
            collections:     vec![(comments_by_post(), "p1".to_string())],
            entities:        vec![
                ("Post".to_string(), "p1".to_string()),
                ("Comment".to_string(), "c1".to_string()),
            ],
        };
        cache
            .put_composed(1, rows(), vec!["tv_post".to_string()], None, None, tags, fence)
            .unwrap();
    };
    for (evicted, stored) in [("c1", false), ("c9", true)] {
        let cache = cache();
        let fence = cache.invalidation_generation();
        cache.invalidate_by_entity("Comment", evicted).unwrap();
        compose_holding(&cache, Some(fence));
        assert_eq!(cached(&cache, 1), stored, "a straddled eviction of Comment {evicted}");
    }
}
