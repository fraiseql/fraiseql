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

    assert_eq!(cache.invalidate_collections_of("Comment").unwrap(), 2);
    assert!(!cached(&cache, 1) && !cached(&cache, 2));
    assert!(cached(&cache, 3), "composes tags, not comments");
}

#[test]
fn a_sweep_of_a_collected_view_evicts_unless_it_keeps_collections() {
    let cache = cache();
    compose(&cache, 1, &["p1"], None);

    let comments = [ViewName::from("tv_comment")];
    assert_eq!(cache.invalidate_views_except_collections(&comments).unwrap(), 0);
    assert!(cached(&cache, 1), "the feed names the comments' collections instead");

    assert_eq!(cache.invalidate_views(&comments).unwrap(), 1);
    assert!(!cached(&cache, 1), "a write FraiseQL ran sweeps it at once");
}

#[test]
fn a_sweep_that_keeps_collections_still_evicts_by_the_entrys_own_views() {
    let cache = cache();
    compose(&cache, 1, &["p1"], None);

    let posts = [ViewName::from("tv_post")];
    assert_eq!(cache.invalidate_views_except_collections(&posts).unwrap(), 1);
    assert!(!cached(&cache, 1));
}

#[test]
fn a_read_that_straddles_a_collection_eviction_is_not_stored() {
    let cache = cache();
    let fence = cache.invalidation_generation();
    assert_eq!(cache.invalidate_collection(&comments_by_post(), "p9").unwrap(), 0);

    compose(&cache, 1, &["p1"], Some(fence));
    assert!(!cached(&cache, 1), "its comments may predate the eviction");
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
    cache.invalidate_views(&[ViewName::from("tv_post")]).unwrap();
    cache.store.run_pending_tasks();

    assert_eq!(cache.invalidate_collection(&comments_by_post(), "p1").unwrap(), 0);
    assert_eq!(cache.invalidate_collections_of("Comment").unwrap(), 0);
    assert_eq!(cache.invalidate_views(&[ViewName::from("tv_comment")]).unwrap(), 0);
}

#[test]
fn a_clear_leaves_no_collection_behind() {
    let cache = cache();
    compose(&cache, 1, &["p1"], None);
    cache.clear().unwrap();

    assert_eq!(cache.invalidate_collections_of("Comment").unwrap(), 0);
    assert_eq!(cache.invalidate_views(&[ViewName::from("tv_comment")]).unwrap(), 0);
}
