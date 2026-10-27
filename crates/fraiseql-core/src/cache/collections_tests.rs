#![allow(clippy::unwrap_used)] // Reason: test code, panics are acceptable

use serde_json::{Value, json};

use super::*;
use crate::{
    backend::{ComposedEmbed, ComposedLevel, EmbedShape, EmbedSource, LevelKeys, ScalarFieldType},
    cache::tviews_feed::Collection,
    db::types::JsonbValue,
};

fn level(view: &str) -> ComposedLevel {
    ComposedLevel {
        view:         view.to_string(),
        projection:   None,
        where_clause: None,
        order_by:     None,
        limit:        None,
        offset:       None,
        keyset:       None,
        keys:         LevelKeys::Whole,
        embeds:       Vec::new(),
        decimals:     None,
    }
}

fn correlated() -> EmbedSource {
    EmbedSource::Correlated {
        target_key: vec!["post_id".to_string()],
        parent_key: vec!["id".to_string()],
        key_type:   ScalarFieldType::Text,
        native:     None,
    }
}

fn embed(
    output_key: &str,
    shape: EmbedShape,
    source: EmbedSource,
    level: ComposedLevel,
) -> ComposedEmbed {
    ComposedEmbed {
        output_key: output_key.to_string(),
        shape,
        source,
        level,
    }
}

/// Joined on the columns `compile --database` proved: the comment's `fk_post`, the post's
/// `pk_post`.
fn by_fk_post() -> EmbedSource {
    EmbedSource::Correlated {
        target_key: vec!["post_id".to_string()],
        parent_key: vec!["id".to_string()],
        key_type:   ScalarFieldType::Text,
        native:     Some(crate::backend::NativeCorrelation {
            target_column: "fk_post".to_string(),
            parent_column: "pk_post".to_string(),
        }),
    }
}

/// `tv_post`, embedding its comments under `comments` as `shape`.
fn posts(shape: EmbedShape) -> ComposedLevel {
    let mut posts = level("tv_post");
    posts.embeds.push(embed("comments", shape, by_fk_post(), level("tv_comment")));
    posts
}

fn row(document: Value, embeds: Value) -> Value {
    Value::Object(serde_json::Map::from_iter([
        ("d".to_string(), document),
        ("e".to_string(), embeds),
    ]))
}

fn rows(values: Vec<Value>) -> Vec<JsonbValue> {
    values.into_iter().map(JsonbValue::new).collect()
}

/// A map in which `tv_comment` rows are a collection of `tv_post` by `fk_post`.
fn comments_of_posts() -> CollectionMap {
    CollectionMap::from_levels([(
        ("tv_comment", "tv_post", "fk_post", "pk_post"),
        Collection::new("Comment", "fk_post"),
    )])
}

fn views_of(view: &str) -> Vec<String> {
    vec![view.to_string()]
}

fn sorted(mut v: Vec<String>) -> Vec<String> {
    v.sort();
    v
}

#[test]
fn without_a_map_every_level_is_indexed_by_its_view() {
    let read = posts(EmbedShape::Many);
    let result = rows(vec![row(json!({"id": "p1"}), json!({"comments": []}))]);
    let (views, tags) = composed_tags(&read, &result, None, views_of);
    assert_eq!(sorted(views), vec!["tv_comment", "tv_post"]);
    assert!(tags.collected_views.is_empty() && tags.collections.is_empty(), "{tags:?}");
}

#[test]
fn a_mapped_list_is_tagged_by_the_collection_of_each_parent() {
    let read = posts(EmbedShape::Many);
    let result = rows(vec![
        row(json!({"id": "p1"}), json!({"comments": [row(json!({"id": "c1"}), json!({}))]})),
        row(json!({"id": 2}), json!({"comments": []})),
    ]);
    let (views, tags) = composed_tags(&read, &result, Some(&comments_of_posts()), views_of);
    assert_eq!(views, vec!["tv_post"]);
    assert_eq!(tags.collected_views, vec!["tv_comment"]);
    let by_post = Collection::new("Comment", "fk_post");
    assert_eq!(
        tags.collections,
        vec![
            (by_post.clone(), "p1".to_string()),
            (by_post, "2".to_string())
        ]
    );
}

#[test]
fn a_mapped_count_is_tagged_by_collection_too() {
    let read = posts(EmbedShape::Count);
    let result = rows(vec![row(json!({"id": "p1"}), json!({"comments": 3}))]);
    let (views, tags) = composed_tags(&read, &result, Some(&comments_of_posts()), views_of);
    assert_eq!(views, vec!["tv_post"]);
    assert_eq!(tags.collections.len(), 1, "{tags:?}");
}

#[test]
fn a_to_one_embed_is_indexed_by_its_view() {
    let read = posts(EmbedShape::One);
    let result = rows(vec![row(json!({"id": "p1"}), json!({"comments": null}))]);
    let (views, tags) = composed_tags(&read, &result, Some(&comments_of_posts()), views_of);
    assert_eq!(sorted(views), vec!["tv_comment", "tv_post"]);
    assert!(tags.collections.is_empty(), "{tags:?}");
}

#[test]
fn a_parent_without_a_readable_id_indexes_the_level_by_its_view() {
    let read = posts(EmbedShape::Many);
    let result = rows(vec![
        row(json!({"id": "p1"}), json!({"comments": []})),
        row(json!({"title": "no id"}), json!({"comments": []})),
    ]);
    let (views, tags) = composed_tags(&read, &result, Some(&comments_of_posts()), views_of);
    assert_eq!(sorted(views), vec!["tv_comment", "tv_post"]);
    assert!(tags.collections.is_empty() && tags.collected_views.is_empty(), "{tags:?}");
}

#[test]
fn an_unmapped_list_is_indexed_by_its_view() {
    let mut read = level("tv_post");
    read.embeds.push(embed("tags", EmbedShape::Many, correlated(), level("tv_tag")));
    let result = rows(vec![row(json!({"id": "p1"}), json!({"tags": []}))]);
    let (views, tags) = composed_tags(&read, &result, Some(&comments_of_posts()), views_of);
    assert_eq!(sorted(views), vec!["tv_post", "tv_tag"]);
    assert!(tags.collections.is_empty(), "{tags:?}");
}

#[test]
fn levels_beneath_a_list_are_tagged_by_their_own_rows() {
    // posts → comments (a collection) → each comment's author (to-one: its view).
    let mut comments = level("tv_comment");
    comments
        .embeds
        .push(embed("author", EmbedShape::One, correlated(), level("tv_user")));
    let mut read = level("tv_post");
    read.embeds.push(embed("comments", EmbedShape::Many, by_fk_post(), comments));
    let result = rows(vec![row(
        json!({"id": "p1"}),
        json!({"comments": [row(json!({"id": "c1"}), json!({"author": row(json!({"id": "u1"}), json!({}))}))]}),
    )]);
    let (views, tags) = composed_tags(&read, &result, Some(&comments_of_posts()), views_of);
    assert_eq!(sorted(views), vec!["tv_post", "tv_user"]);
    assert_eq!(tags.collected_views, vec!["tv_comment"]);
}

#[test]
fn a_level_read_from_its_parents_document_reads_no_view() {
    let mut read = level("tv_post");
    let source = EmbedSource::Materialised {
        keys: vec!["tags".to_string()],
    };
    read.embeds.push(embed("tags", EmbedShape::Many, source, level("tv_tag")));
    let result = rows(vec![row(json!({"id": "p1"}), json!({"tags": []}))]);
    let (views, _) = composed_tags(&read, &result, Some(&comments_of_posts()), views_of);
    assert_eq!(views, vec!["tv_post"]);
}

#[test]
fn every_level_is_indexed_by_its_secondary_views_too() {
    let read = posts(EmbedShape::Many);
    let result = rows(vec![row(json!({"id": "p1"}), json!({"comments": []}))]);
    let with_secondary = |view: &str| vec![view.to_string(), format!("{view}_extra")];
    let (views, tags) = composed_tags(&read, &result, Some(&comments_of_posts()), with_secondary);
    assert_eq!(sorted(views), vec!["tv_post", "tv_post_extra"]);
    assert_eq!(sorted(tags.collected_views), vec!["tv_comment", "tv_comment_extra"]);
}

#[test]
fn a_list_correlated_on_stored_keys_is_indexed_by_its_view() {
    // The registry proves `fk_post`; a level joining on a key in the documents may follow
    // another relationship to the same parent (pinned comments): no collection is proven.
    let mut read = level("tv_post");
    read.embeds
        .push(embed("comments", EmbedShape::Many, correlated(), level("tv_comment")));
    let result = rows(vec![row(json!({"id": "p1"}), json!({"comments": []}))]);
    let (views, tags) = composed_tags(&read, &result, Some(&comments_of_posts()), views_of);
    assert_eq!(sorted(views), vec!["tv_comment", "tv_post"]);
    assert!(tags.collections.is_empty(), "{tags:?}");
}
