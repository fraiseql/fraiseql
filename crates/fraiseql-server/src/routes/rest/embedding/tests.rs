//! Tests for the `embedding` module.

#![allow(clippy::unwrap_used)] // Reason: test code, panics are acceptable

use std::collections::{BTreeMap, HashMap};

use serde_json::json;

use super::{SubSelect, default_embed_page, selections};
use crate::routes::rest::params::{EmbeddedSpec, SelectEntry};

fn embedded(name: &str) -> EmbeddedSpec {
    EmbeddedSpec {
        relationship: name.to_string(),
        rename:       None,
        fields:       vec![],
    }
}

// ---------------------------------------------------------------------------
// #1267 — every kind of sub-select entry is collected
//
// These run on the required workspace leg. The end-to-end proof of the defect
// lives in `rest_embedding_safety_e2e_pg`, which needs a database and therefore
// only runs in the integration shard — so without these, a regression would
// reach `dev` through a green required leg.
// ---------------------------------------------------------------------------

#[test]
fn a_sub_select_separates_fields_embeds_and_counts() {
    let split = SubSelect::split(&[
        SelectEntry::Field("id".to_string()),
        SelectEntry::Embedded(embedded("author")),
        SelectEntry::Count("comments".to_string()),
        SelectEntry::Field("title".to_string()),
    ]);

    assert_eq!(split.fields, vec!["id".to_string(), "title".to_string()]);
    assert_eq!(split.embeds.len(), 1);
    assert_eq!(split.embeds[0].relationship, "author");
    // The half #864 left in the wildcard: before #1267 this was empty for every
    // input, so the count reached no executor and the key never appeared.
    assert_eq!(split.counts, vec!["comments".to_string()]);
}

#[test]
fn a_count_only_sub_select_yields_a_count_and_no_fields() {
    let split = SubSelect::split(&[SelectEntry::Count("comments".to_string())]);

    assert!(split.fields.is_empty());
    assert!(split.embeds.is_empty());
    assert_eq!(split.counts, vec!["comments".to_string()]);
}

#[test]
fn an_empty_sub_select_yields_nothing() {
    assert_eq!(SubSelect::split(&[]), SubSelect::default());
}

// ---------------------------------------------------------------------------
// The request handed to the engine
// ---------------------------------------------------------------------------

/// `?select=id,posts(id,comments(id),comments.count),posts.count`
fn posts_with_comments() -> (Vec<EmbeddedSpec>, Vec<String>) {
    let posts = EmbeddedSpec {
        relationship: "posts".to_string(),
        rename:       None,
        fields:       vec![
            SelectEntry::Field("id".to_string()),
            SelectEntry::Embedded(EmbeddedSpec {
                relationship: "comments".to_string(),
                rename:       None,
                fields:       vec![SelectEntry::Field("id".to_string())],
            }),
            SelectEntry::Count("comments".to_string()),
        ],
    };
    (vec![posts], vec!["posts".to_string()])
}

#[test]
fn a_nested_selection_reaches_the_engine_whole() {
    let (embeds, counts) = posts_with_comments();

    let (embeds, counts) = selections(&embeds, &counts, &HashMap::new(), &BTreeMap::new(), 50);

    assert_eq!(embeds.len(), 1);
    let posts = &embeds[0];
    assert_eq!(posts.fields, vec!["id".to_string()]);
    assert_eq!(posts.embeds.len(), 1, "the nested embed (#864)");
    assert_eq!(posts.embeds[0].relationship, "comments");
    assert_eq!(posts.counts.len(), 1, "the nested count (#1267)");
    assert_eq!(posts.counts[0].output_key, "comments_count");
    assert_eq!(counts.len(), 1);
    assert_eq!(counts[0].output_key, "posts_count");
    assert_eq!(
        (posts.limit, posts.embeds[0].limit),
        (Some(50), Some(50)),
        "a level no page names gets the default"
    );
}

#[test]
fn a_top_level_filter_narrows_the_embed_and_its_count_alike() {
    // #1285: one filter, read by every reader of the relation it names.
    let (embeds, counts) = posts_with_comments();
    let filters = HashMap::from([("posts".to_string(), json!({"status": {"eq": "published"}}))]);

    let (embeds, counts) = selections(&embeds, &counts, &filters, &BTreeMap::new(), 50);

    assert_eq!(embeds[0].filter, Some(json!({"status": {"eq": "published"}})));
    assert_eq!(counts[0].filter, embeds[0].filter);
}

#[test]
fn a_nested_level_is_given_no_filter_even_one_named_like_it() {
    // The syntax is one segment deep, so `?comments.x=` can only mean a top-level
    // `comments`. Handing it to a nested `comments` would filter a relation the
    // client did not name.
    let (embeds, counts) = posts_with_comments();
    let filters = HashMap::from([("comments".to_string(), json!({"id": {"eq": 1}}))]);

    let (embeds, _) = selections(&embeds, &counts, &filters, &BTreeMap::new(), 50);

    assert_eq!(embeds[0].embeds[0].filter, None);
    assert_eq!(embeds[0].counts[0].filter, None);
}

#[test]
fn a_rename_is_the_output_key() {
    let spec = EmbeddedSpec {
        relationship: "fk_user".to_string(),
        rename:       Some("author".to_string()),
        fields:       Vec::new(),
    };

    let (embeds, _) = selections(&[spec], &[], &HashMap::new(), &BTreeMap::new(), 50);

    assert_eq!(embeds[0].relationship, "fk_user");
    assert_eq!(embeds[0].output_key, "author");
}

/// A page reaches the level its dotted path names, and no other.
///
/// `posts.comments` pages the nested level alone; `posts` pages the top level alone; and a
/// top-level name that happens to match a nested relationship (`comments`) is not the
/// nested level's path, so it pages nothing here.
#[test]
fn a_page_reaches_only_the_level_its_path_names() {
    let limits = |pages: &[(&str, u32)]| {
        let (embeds, counts) = posts_with_comments();
        let pages = pages.iter().map(|(p, n)| ((*p).to_string(), *n)).collect();
        let (embeds, _) = selections(&embeds, &counts, &HashMap::new(), &pages, 50);
        (embeds[0].limit, embeds[0].embeds[0].limit)
    };

    assert_eq!(limits(&[("posts.comments", 5)]), (Some(50), Some(5)));
    assert_eq!(limits(&[("posts", 200)]), (Some(200), Some(50)));
    assert_eq!(limits(&[("comments", 7)]), (Some(50), Some(50)));
}

/// An unpaged embed gets `default_embed_page_size`, not the ceiling — and never more than
/// the ceiling, when a deployment sets the ceiling lower.
#[test]
fn the_default_embed_page_is_its_own_setting_capped_by_the_ceiling() {
    use fraiseql_core::schema::RestConfig;

    assert_eq!(default_embed_page(&RestConfig::default()), 50, "not max_page_size (1 000)");
    let low_ceiling = RestConfig {
        max_page_size: 20,
        ..RestConfig::default()
    };
    assert_eq!(default_embed_page(&low_ceiling), 20);
}
