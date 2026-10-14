//! The key of a composed read (#1589).

#![allow(clippy::unwrap_used)] // Reason: test code, panics are acceptable

use serde_json::json;

use super::generate_composed_query_key;
use crate::{
    backend::{
        ComposedEmbed, ComposedLevel, EmbedShape, EmbedSource, LevelKeys, ScalarFieldType,
        WhereOperator,
    },
    db::WhereClause,
};

fn level(view: &str) -> ComposedLevel {
    ComposedLevel {
        view:         view.to_string(),
        projection:   None,
        where_clause: None,
        order_by:     None,
        limit:        Some(20),
        offset:       None,
        keyset:       None,
        keys:         LevelKeys::Whole,
        embeds:       Vec::new(),
    }
}

/// Posts, each with its comments by `author`.
fn posts_with_comments_by(author: &str) -> ComposedLevel {
    let mut comments = level("tv_comment");
    comments.where_clause = Some(WhereClause::Field {
        path:     vec!["author".to_string()],
        operator: WhereOperator::Eq,
        value:    json!(author),
    });
    let mut posts = level("tv_post");
    posts.embeds.push(ComposedEmbed {
        output_key: "comments".to_string(),
        shape:      EmbedShape::Many,
        source:     EmbedSource::Correlated {
            target_key: vec!["post_id".to_string()],
            parent_key: vec!["id".to_string()],
            key_type:   ScalarFieldType::Text,
        },
        level:      comments,
    });
    posts
}

fn key(read: &ComposedLevel, vars: &[(&str, &str)], locale: Option<&str>, version: &str) -> u64 {
    generate_composed_query_key(read, vars, locale, version)
}

#[test]
fn equal_reads_share_a_key() {
    assert_eq!(
        key(&posts_with_comments_by("a"), &[("app.tenant", "t1")], None, "v1"),
        key(&posts_with_comments_by("a"), &[("app.tenant", "t1")], None, "v1"),
    );
}

#[test]
fn a_value_in_an_embedded_level_splits_the_key() {
    assert_ne!(
        key(&posts_with_comments_by("a"), &[], None, "v1"),
        key(&posts_with_comments_by("b"), &[], None, "v1"),
    );
}

#[test]
fn the_shape_of_an_embed_splits_the_key() {
    let many = posts_with_comments_by("a");
    let mut count = many.clone();
    count.embeds[0].shape = EmbedShape::Count;
    let mut renamed = many.clone();
    renamed.embeds[0].output_key = "replies".to_string();
    let k = |r: &ComposedLevel| key(r, &[], None, "v1");
    assert_ne!(k(&many), k(&count));
    assert_ne!(k(&many), k(&renamed));
}

#[test]
fn session_variables_split_the_key_in_any_order() {
    let read = posts_with_comments_by("a");
    let ab = key(&read, &[("a", "1"), ("b", "2")], None, "v1");
    assert_eq!(ab, key(&read, &[("b", "2"), ("a", "1")], None, "v1"));
    assert_ne!(ab, key(&read, &[("a", "1"), ("b", "3")], None, "v1"));
}

#[test]
fn the_locale_and_the_schema_version_split_the_key() {
    let read = posts_with_comments_by("a");
    let base = key(&read, &[], None, "v1");
    assert_ne!(base, key(&read, &[], Some("fr"), "v1"));
    assert_ne!(base, key(&read, &[], None, "v2"));
}

/// A level whose predicate carries its field types, built afresh: a map whose iteration
/// order differs from one instance to the next.
fn typed_read() -> ComposedLevel {
    let mut read = posts_with_comments_by("a");
    let types = crate::db::where_clause::FieldTypeMap::from_pairs(
        (0..32).map(|i| (format!("field_{i}"), ScalarFieldType::Text)),
    );
    read.embeds[0].level.where_clause = Some(WhereClause::Typed {
        types: std::sync::Arc::new(types),
        inner: Box::new(read.embeds[0].level.where_clause.take().unwrap()),
    });
    read
}

#[test]
fn equal_reads_share_a_key_whatever_order_their_maps_iterate_in() {
    let first = key(&typed_read(), &[], None, "v1");
    for _ in 0..20 {
        assert_eq!(key(&typed_read(), &[], None, "v1"), first);
    }
}
