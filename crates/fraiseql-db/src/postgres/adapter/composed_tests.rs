//! Unit tests for the composed-read renderer (no live database required).
//!
//! What these pin is *where* each part of a level lands in the statement — which is the
//! whole of the renderer's job. Whether PostgreSQL then answers the right rows is the
//! e2e suites' question (`rest_embedding_*_e2e_pg`).

#![allow(clippy::unwrap_used, clippy::panic)] // Reason: test code, panics are acceptable

use serde_json::json;

use super::composed::build_composed_select_sql;
use crate::{
    WhereOperator,
    traits::{ComposedEmbed, ComposedLevel, EmbedShape, LevelKeys},
    types::{QueryParam, sql_hints::ScalarFieldType},
    where_clause::WhereClause,
};

fn level(view: &str) -> ComposedLevel {
    ComposedLevel {
        view:         view.to_string(),
        projection:   None,
        where_clause: None,
        order_by:     None,
        limit:        None,
        offset:       None,
        keys:         LevelKeys::Whole,
        embeds:       Vec::new(),
    }
}

fn field_eq(key: &str, value: serde_json::Value) -> WhereClause {
    WhereClause::Field {
        path: vec![key.to_string()],
        operator: WhereOperator::Eq,
        value,
    }
}

fn embed(output_key: &str, shape: EmbedShape, level: ComposedLevel) -> ComposedEmbed {
    ComposedEmbed {
        output_key: output_key.to_string(),
        shape,
        target_key: vec!["user_id".to_string()],
        parent_key: vec!["id".to_string()],
        key_type: ScalarFieldType::Text,
        level,
    }
}

/// `params` as `expected` would print — `QueryParam` has no `PartialEq`, and one is not
/// worth adding to a public type for a test.
fn bound(params: &[QueryParam]) -> String {
    format!("{params:?}")
}

/// The text of the `LATERAL` subquery aliased `alias`.
fn lateral<'a>(sql: &'a str, alias: &str) -> &'a str {
    let end = sql.find(&format!(") AS {alias} ON true")).unwrap_or_else(|| {
        panic!("no LATERAL aliased {alias} in: {sql}");
    });
    let mut depth = 0usize;
    for (i, ch) in sql[..end].char_indices().rev() {
        match ch {
            ')' => depth += 1,
            '(' if depth == 0 => return &sql[i + 1..end],
            '(' => depth -= 1,
            _ => {},
        }
    }
    panic!("unbalanced LATERAL {alias} in: {sql}")
}

#[test]
fn a_flat_root_is_paged_ordered_and_returned_in_its_order() {
    let mut root = level("v_user");
    root.limit = Some(10);
    root.offset = Some(20);

    let (sql, params) = build_composed_select_sql(&root).unwrap();

    assert!(sql.starts_with("SELECT _r.data FROM ("), "{sql}");
    assert!(sql.ends_with(r#") AS _r ORDER BY _r."_o""#), "the root's order is kept: {sql}");
    assert!(sql.contains(r#"FROM "v_user" ORDER BY "_o" LIMIT $1 OFFSET $2"#), "{sql}");
    assert_eq!(bound(&params), bound(&[QueryParam::BigInt(10), QueryParam::BigInt(20)]));
}

#[test]
fn an_embeds_page_is_taken_inside_its_lateral_and_the_parents_outside_it() {
    // The property the composition exists for: each parent row gets its own page of
    // related rows. A LIMIT outside the LATERAL would page the join, not the embed.
    let mut orders = level("v_order");
    orders.limit = Some(7);
    let mut root = level("v_user");
    root.limit = Some(2);
    root.embeds.push(embed("orders", EmbedShape::Many, orders));

    let (sql, params) = build_composed_select_sql(&root).unwrap();
    let inner = lateral(&sql, "_l0_e0");

    assert_eq!(bound(&params), bound(&[QueryParam::BigInt(2), QueryParam::BigInt(7)]));
    assert!(inner.contains(r#"FROM "v_order""#), "{inner}");
    assert!(inner.contains("LIMIT $2"), "the embed's page is inside its lateral: {inner}");
    assert!(!inner.contains("LIMIT $1"), "the parent's page is not: {inner}");
    let root_page = sql.find(r#"FROM "v_user" ORDER BY "_o" LIMIT $1)"#).unwrap();
    assert!(
        root_page < sql.find("LEFT JOIN LATERAL").unwrap(),
        "the parent is paged before anything is joined to it: {sql}"
    );
    assert!(
        inner.contains(r#"COALESCE(jsonb_agg(_c.data ORDER BY _c."_o"), '[]'::jsonb)"#),
        "an empty collection is [], in the embed's own order: {inner}"
    );
}

#[test]
fn each_level_keeps_its_own_predicate_and_is_correlated_to_its_parent_only() {
    let mut orders = level("v_order");
    orders.where_clause = Some(field_eq("tenant_id", json!("t-order")));
    let mut root = level("v_user");
    root.where_clause = Some(field_eq("tenant_id", json!("t-user")));
    root.embeds.push(embed("orders", EmbedShape::Many, orders));

    let (sql, params) = build_composed_select_sql(&root).unwrap();
    let inner = lateral(&sql, "_l0_e0");

    assert_eq!(
        bound(&params),
        bound(&[
            QueryParam::Text("t-user".into()),
            QueryParam::Text("t-order".into())
        ])
    );
    assert!(
        inner.contains("WHERE data->>'tenant_id' = $2 AND data->>'user_id' = _l0.data->>'id'"),
        "the embed's own predicate, unqualified, and the correlation naming the parent: {inner}"
    );
    assert!(!inner.contains("$1"), "the parent's predicate stays on the parent: {inner}");
}

#[test]
fn a_correlation_names_its_own_parent_at_every_depth() {
    let items = level("v_item");
    let mut orders = level("v_order");
    orders.embeds.push(ComposedEmbed {
        target_key: vec!["order_id".to_string()],
        ..embed("items", EmbedShape::Many, items)
    });
    let mut root = level("v_user");
    root.embeds.push(embed("orders", EmbedShape::Many, orders));

    let (sql, _) = build_composed_select_sql(&root).unwrap();

    assert!(sql.contains("data->>'user_id' = _l0.data->>'id'"), "{sql}");
    assert!(
        sql.contains("data->>'order_id' = _l1.data->>'id'"),
        "the second level correlates to the first, not to the root: {sql}"
    );
}

#[test]
fn a_to_one_embed_is_one_row_and_a_count_is_a_count() {
    let mut root = level("v_post");
    let mut author = level("v_user");
    author.limit = Some(1000);
    root.embeds.push(ComposedEmbed {
        target_key: vec!["id".to_string()],
        parent_key: vec!["fk_author".to_string()],
        ..embed("author", EmbedShape::One, author)
    });
    let mut comments = level("v_comment");
    comments.limit = Some(1000);
    comments.where_clause = Some(field_eq("status", json!("published")));
    root.embeds.push(embed("comments_count", EmbedShape::Count, comments));

    let (sql, params) = build_composed_select_sql(&root).unwrap();
    let one = lateral(&sql, "_l0_e0");
    let count = lateral(&sql, "_l0_e1");

    assert!(one.ends_with(r#"ORDER BY _c."_o" LIMIT 1"#), "{one}");
    assert!(one.contains("data->>'id' = _l0.data->>'fk_author'"), "{one}");
    assert_eq!(
        count,
        r#"SELECT COUNT(*) AS v FROM "v_comment" WHERE data->>'status' = $2 AND data->>'user_id' = _l0.data->>'id'"#,
        "a count reads its predicate and correlation and no page"
    );
    assert_eq!(
        bound(&params),
        bound(&[
            QueryParam::BigInt(1000),
            QueryParam::Text("published".into())
        ]),
        "the count's page is not bound"
    );
    assert!(
        sql.contains("jsonb_build_object('author', _l0_e0.v, 'comments_count', _l0_e1.v)"),
        "{sql}"
    );
}

#[test]
fn an_embedded_level_returns_only_its_kept_keys_and_nulls_its_masked_ones() {
    let mut orders = level("v_order");
    orders.keys = LevelKeys::Only {
        kept:   vec!["id".to_string(), "total".to_string()],
        masked: vec!["margin".to_string()],
    };
    let mut root = level("v_user");
    root.embeds.push(embed("orders", EmbedShape::Many, orders));

    let (sql, _) = build_composed_select_sql(&root).unwrap();
    let inner = lateral(&sql, "_l0_e0");

    assert!(
        inner.contains("FROM jsonb_each(_l1.data) AS _k WHERE _k.key IN ('id', 'total'))"),
        "{inner}"
    );
    assert!(
        inner.contains("|| jsonb_build_object('margin', NULL))"),
        "a masked value never leaves the database: {inner}"
    );
    assert!(
        sql.contains("jsonb_build_object('d', _l0.data, 'e',"),
        "the root reads whole: {sql}"
    );
}

#[test]
fn a_typed_key_is_cast_on_both_sides() {
    let mut root = level("v_user");
    root.embeds.push(ComposedEmbed {
        key_type: ScalarFieldType::Integer,
        ..embed("orders", EmbedShape::Many, level("v_order"))
    });

    let (sql, _) = build_composed_select_sql(&root).unwrap();

    assert!(sql.contains("(data->>'user_id')::bigint = (_l0.data->>'id')::bigint"), "{sql}");
}

#[test]
fn keys_and_output_names_are_quoted_as_literals() {
    let mut orders = level("v_order");
    orders.keys = LevelKeys::Only {
        kept:   vec!["o'k".to_string()],
        masked: Vec::new(),
    };
    let mut root = level("v_user");
    root.embeds.push(embed("it's", EmbedShape::Many, orders));

    let (sql, _) = build_composed_select_sql(&root).unwrap();

    assert!(sql.contains("IN ('o''k')"), "{sql}");
    assert!(sql.contains("jsonb_build_object('it''s', _l0_e0.v)"), "{sql}");
}
