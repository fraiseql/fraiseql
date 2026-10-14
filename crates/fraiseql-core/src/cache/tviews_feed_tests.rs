#![allow(clippy::unwrap_used, clippy::panic)] // Reason: test code, panics are acceptable

use std::{
    collections::{BTreeMap, BTreeSet, HashMap},
    sync::Arc,
};

use fraiseql_db::ViewName;
use serde_json::json;

use super::*;
use crate::{
    cache::{CacheConfig, ComposedTags, QueryResultCache},
    db::types::JsonbValue,
};

fn set(items: &[&str]) -> BTreeSet<String> {
    items.iter().map(ToString::to_string).collect()
}

fn change(payload: &str) -> TviewsChange {
    match decode(payload) {
        TviewsMessage::Change(change) => change,
        TviewsMessage::Everything => panic!("evicts everything: {payload}"),
    }
}

#[test]
fn rows_inserted_updated_and_deleted_are_read_by_type() {
    let c = change(r#"{"v":2,"u":{"User":["u1"],"Comment":["c1","c2"]},"d":{"Comment":["c3"]}}"#);
    assert_eq!(
        c.rows,
        BTreeMap::from([
            ("Comment".to_string(), set(&["c1", "c2", "c3"])),
            ("User".to_string(), set(&["u1"])),
        ])
    );
    assert!(c.collections.is_empty() && c.types.is_empty(), "{c:?}");
}

#[test]
fn an_integer_id_is_read_as_its_decimal_text() {
    let c = change(r#"{"v":2,"u":{"Tag":[42]}}"#);
    assert_eq!(c.rows["Tag"], set(&["42"]));
}

#[test]
fn collections_are_read_by_child_type_and_column() {
    let c = change(r#"{"v":2,"c":{"Comment.fk_post":["p1","p2"],"Comment.fk_author":[7]}}"#);
    assert_eq!(
        c.collections,
        BTreeMap::from([
            (Collection::new("Comment", "fk_author"), set(&["7"])),
            (Collection::new("Comment", "fk_post"), set(&["p1", "p2"])),
        ])
    );
}

#[test]
fn a_reference_column_may_hold_a_dot() {
    let c = change(r#"{"v":2,"c":{"Comment.fk.post":["p1"]}}"#);
    assert_eq!(c.collections.keys().next().unwrap(), &Collection::new("Comment", "fk.post"));
}

#[test]
fn types_are_read() {
    let c = change(r#"{"v":2,"t":["Audit","Post"]}"#);
    assert_eq!(c.types, set(&["Audit", "Post"]));
}

#[test]
fn an_id_that_is_neither_text_nor_a_number_evicts_its_whole_type() {
    let c = change(r#"{"v":2,"u":{"User":["u1",null]},"d":{"Tag":[{"x":1}]}}"#);
    assert_eq!(c.types, set(&["Tag", "User"]));
}

#[test]
fn a_collection_whose_parent_cannot_be_read_evicts_its_child_type() {
    let c = change(r#"{"v":2,"c":{"Comment.fk_post":[true]}}"#);
    assert!(c.collections.is_empty(), "{c:?}");
    assert_eq!(c.types, set(&["Comment"]));
}

#[test]
fn an_empty_message_evicts_nothing() {
    assert_eq!(change(r#"{"v":2}"#), TviewsChange::default());
}

#[test]
fn another_version_evicts_everything() {
    for payload in [
        r#"{"v":1,"u":{"User":["u1"]}}"#,
        r#"{"v":3}"#,
        r#"{"u":{}}"#,
    ] {
        assert_eq!(decode(payload), TviewsMessage::Everything, "{payload}");
    }
}

#[test]
fn an_unreadable_message_evicts_everything() {
    for payload in [
        "",
        "not json",
        r#"{"v":2,"u":["User"]}"#,
        r#"{"v":2,"t":"User"}"#,
        r#"{"v":2,"c":{"no_dot":["p1"]}}"#,
        r#"{"v":2,"c":{".fk_post":["p1"]}}"#,
        r#"{"v":2,"c":{"Comment.":["p1"]}}"#,
    ] {
        assert_eq!(decode(payload), TviewsMessage::Everything, "{payload:?}");
    }
}

fn cache() -> Arc<QueryResultCache> {
    Arc::new(QueryResultCache::new(CacheConfig::enabled()))
}

fn rows() -> Arc<Vec<JsonbValue>> {
    Arc::new(vec![JsonbValue::new(json!({"id": "x"}))])
}

fn cached(cache: &QueryResultCache, key: u64) -> bool {
    cache.get(key).unwrap().is_some()
}

/// `Comment` rows are read by `tv_comment` and `v_comment_stats`, `Post` rows by `tv_post`.
fn evictor(cache: &Arc<QueryResultCache>) -> TviewsEvictor {
    TviewsEvictor::new(
        Arc::clone(cache),
        HashMap::from([
            (
                "Comment".to_string(),
                vec![
                    ViewName::from("tv_comment"),
                    ViewName::from("v_comment_stats"),
                ],
            ),
            ("Post".to_string(), vec![ViewName::from("tv_post")]),
        ]),
    )
}

/// Entry 1: a list of comments. Entry 2: posts composing the comments of p1 by collection.
/// Entry 3: comment statistics. Entry 4: a list of tags.
fn fill(cache: &QueryResultCache) {
    cache.put(1, vec![], vec!["tv_comment".to_string()], None, None, None).unwrap();
    let tags = ComposedTags {
        collected_views: vec!["tv_comment".to_string()],
        collections:     vec![(Collection::new("Comment", "fk_post"), "p1".to_string())],
    };
    cache
        .put_composed(2, rows(), vec!["tv_post".to_string()], None, None, tags, None)
        .unwrap();
    cache
        .put(3, vec![], vec!["v_comment_stats".to_string()], None, None, None)
        .unwrap();
    cache.put(4, vec![], vec!["tv_tag".to_string()], None, None, None).unwrap();
}

#[test]
fn rows_of_a_type_sweep_its_views_but_not_the_levels_indexed_by_collection() {
    let cache = cache();
    fill(&cache);
    evictor(&cache).apply(&decode(r#"{"v":2,"u":{"Comment":["c9"]}}"#)).unwrap();
    assert!(!cached(&cache, 1) && !cached(&cache, 3), "lists and statistics of comments");
    assert!(cached(&cache, 2), "c9 is in no collection of p1");
    assert!(cached(&cache, 4));
}

#[test]
fn a_collection_evicts_the_entries_composing_it() {
    let cache = cache();
    fill(&cache);
    evictor(&cache)
        .apply(&decode(r#"{"v":2,"u":{"Comment":["c1"]},"c":{"Comment.fk_post":["p1"]}}"#))
        .unwrap();
    assert!(!cached(&cache, 2), "c1 joined, left or changed in p1's comments");
}

#[test]
fn deleted_rows_sweep_their_type_too() {
    let cache = cache();
    fill(&cache);
    evictor(&cache).apply(&decode(r#"{"v":2,"d":{"Post":["p1"]}}"#)).unwrap();
    assert!(!cached(&cache, 2), "the composed read lists p1");
    assert!(cached(&cache, 1));
}

#[test]
fn a_whole_type_evicts_its_views_and_every_collection_of_it() {
    let cache = cache();
    fill(&cache);
    evictor(&cache).apply(&decode(r#"{"v":2,"t":["Comment"]}"#)).unwrap();
    assert!(!cached(&cache, 1) && !cached(&cache, 2) && !cached(&cache, 3));
    assert!(cached(&cache, 4));
}

#[test]
fn a_type_no_view_reads_evicts_nothing() {
    let cache = cache();
    fill(&cache);
    evictor(&cache)
        .apply(&decode(r#"{"v":2,"u":{"Audit":["a1"]},"t":["Audit"]}"#))
        .unwrap();
    assert!((1..=4).all(|key| cached(&cache, key)));
}

#[test]
fn an_unreadable_message_clears_the_cache() {
    let cache = cache();
    fill(&cache);
    evictor(&cache).apply(&decode(r#"{"v":1}"#)).unwrap();
    assert!((1..=4).all(|key| !cached(&cache, key)));
}

#[test]
fn a_catalog_maps_each_type_to_the_views_reading_it() {
    let catalog = FeedCatalog::from_rows(
        [
            ("Post", "tv_post"),
            ("Post", "v_post_full"),
            ("User", "tv_user"),
        ],
        [],
    );
    assert_eq!(
        catalog.type_views,
        HashMap::from([
            (
                "Post".to_string(),
                vec![ViewName::from("tv_post"), ViewName::from("v_post_full")]
            ),
            ("User".to_string(), vec![ViewName::from("tv_user")]),
        ])
    );
}

#[test]
fn a_catalog_knows_a_collection_by_its_one_reference() {
    let catalog = FeedCatalog::from_rows(
        [],
        [
            ("tv_comment", "tv_post", "Comment", "fk_post"),
            ("tv_comment", "tv_user", "Comment", "fk_author"),
            ("tv_comment", "tv_user", "Comment", "reviewed_by"),
        ],
    );
    assert_eq!(
        catalog.collections.collection("tv_comment", "tv_post"),
        Some(&Collection::new("Comment", "fk_post"))
    );
    assert_eq!(
        catalog.collections.collection("tv_comment", "tv_user"),
        None,
        "two references to users: which one a level follows is not known"
    );
}
