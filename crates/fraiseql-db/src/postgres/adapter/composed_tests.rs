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
    traits::{
        ComposedEmbed, ComposedKeyset, ComposedLevel, CursorValue, EmbedShape, EmbedSource,
        LevelKeys,
    },
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
        keyset:       None,
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

fn correlated(target_key: &str, parent_key: &str, key_type: ScalarFieldType) -> EmbedSource {
    EmbedSource::Correlated {
        target_key: vec![target_key.to_string()],
        parent_key: vec![parent_key.to_string()],
        key_type,
        native: None,
    }
}

fn embed(output_key: &str, shape: EmbedShape, level: ComposedLevel) -> ComposedEmbed {
    ComposedEmbed {
        output_key: output_key.to_string(),
        shape,
        source: correlated("user_id", "id", ScalarFieldType::Text),
        level,
    }
}

/// `embed`, read from the parent's own document under `keys` instead of a view.
fn materialised(
    output_key: &str,
    shape: EmbedShape,
    keys: &[&str],
    level: ComposedLevel,
) -> ComposedEmbed {
    ComposedEmbed {
        source: EmbedSource::Materialised {
            keys: keys.iter().map(|k| (*k).to_string()).collect(),
        },
        ..embed(output_key, shape, level)
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
        inner.contains(r#"COALESCE(json_agg(_c.data ORDER BY _c."_o"), '[]'::json)"#),
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
        source: correlated("order_id", "id", ScalarFieldType::Text),
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
        source: correlated("id", "fk_author", ScalarFieldType::Text),
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
        sql.contains(
            "JSON_OBJECT('author' VALUE _l0_e0.v, 'comments_count' VALUE _l0_e1.v RETURNING json)"
        ),
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
        sql.contains("JSON_OBJECT('d' VALUE _l0.data, 'e' VALUE"),
        "the root reads whole: {sql}"
    );
}

#[test]
fn a_typed_key_is_cast_on_both_sides() {
    let mut root = level("v_user");
    root.embeds.push(ComposedEmbed {
        source: correlated("user_id", "id", ScalarFieldType::Integer),
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
    assert!(sql.contains("JSON_OBJECT('it''s' VALUE _l0_e0.v RETURNING json)"), "{sql}");
}

// ---------------------------------------------------------------------------
// A level read from its parent's document
// ---------------------------------------------------------------------------

/// The elements under the parent's key are the level's rows, named `data` like a view's,
/// so the level's own unqualified predicate binds to each element — and the parent is
/// named only in the source expression, the one qualified reference.
#[test]
fn a_materialised_level_filters_its_parents_elements_as_its_own_rows() {
    let mut orders = level("v_order");
    orders.where_clause = Some(field_eq("owner", json!("u-alice")));
    orders.keys = LevelKeys::Only {
        kept:   vec!["id".to_string()],
        masked: vec!["margin".to_string()],
    };
    let mut root = level("v_user");
    root.embeds.push(materialised("orders", EmbedShape::Many, &["orders"], orders));

    let (sql, params) = build_composed_select_sql(&root).unwrap();
    let inner = lateral(&sql, "_l0_e0");

    assert!(
        inner.contains(
            "jsonb_array_elements(CASE WHEN jsonb_typeof(COALESCE(_l0.data->'orders')) = 'array' \
             THEN COALESCE(_l0.data->'orders') ELSE '[]'::jsonb END) WITH ORDINALITY AS \
             _l2(data, \"_o\")"
        ),
        "{inner}"
    );
    assert!(
        inner.contains(
            "WHERE data->>'owner' = $1 AND jsonb_typeof(data) = 'object' ORDER BY \"_o\""
        ),
        "the level's predicate, unqualified, over each object element: {inner}"
    );
    assert!(!inner.contains("\"v_order\""), "the level's view is not read: {inner}");
    assert!(inner.contains("'margin', NULL"), "masked as for a view's rows: {inner}");
    assert_eq!(bound(&params), r#"[Text("u-alice")]"#);
}

/// Every stored spelling of the key is tried, in order.
#[test]
fn a_materialised_to_one_is_the_parents_object_under_the_first_key_it_holds() {
    let mut root = level("v_order");
    root.embeds
        .push(materialised("user", EmbedShape::One, &["user", "User"], level("v_user")));

    let (sql, _) = build_composed_select_sql(&root).unwrap();
    let inner = lateral(&sql, "_l0_e0");

    assert!(
        inner.contains(
            "FROM (SELECT COALESCE(_l0.data->'user', _l0.data->'User') AS data, 1::bigint AS \
             \"_o\") AS _l2 WHERE jsonb_typeof(data) = 'object'"
        ),
        "{inner}"
    );
    assert!(inner.ends_with(r#"ORDER BY _c."_o" LIMIT 1"#), "{inner}");
}

/// A native column is not in a document, and an unqualified column name there would
/// resolve outward: refused, never rendered.
#[test]
fn a_materialised_level_refuses_a_predicate_on_a_column() {
    let mut orders = level("v_order");
    orders.where_clause = Some(WhereClause::And(vec![
        field_eq("owner", json!("u-alice")),
        WhereClause::NativeField {
            column:   "tenant_id".to_string(),
            pg_cast:  String::new(),
            operator: WhereOperator::Eq,
            value:    json!("A"),
        },
    ]));
    let mut root = level("v_user");
    root.embeds.push(materialised("orders", EmbedShape::Many, &["orders"], orders));

    let error = build_composed_select_sql(&root).unwrap_err().to_string();
    assert!(error.contains("may only read the document"), "{error}");
}

/// The same for a document field its types map to a native column (#1561): the
/// comparison would read the column, which the document does not have.
#[test]
fn a_materialised_level_refuses_a_field_its_types_read_from_a_column() {
    let types = crate::where_clause::FieldTypeMap::from_pairs([(
        "owner",
        crate::types::sql_hints::ScalarFieldType::Text,
    )])
    .with_columns([("owner".to_string(), "owner".to_string())]);
    let mut orders = level("v_order");
    orders.where_clause =
        Some(field_eq("owner", json!("u-alice")).typed(std::sync::Arc::new(types)));
    let mut root = level("v_user");
    root.embeds.push(materialised("orders", EmbedShape::Many, &["orders"], orders));

    let error = build_composed_select_sql(&root).unwrap_err().to_string();
    assert!(error.contains("may only read the document"), "{error}");
}

#[test]
fn a_materialised_level_cannot_be_counted_ordered_or_projected() {
    let refused = |embed: ComposedEmbed| {
        let mut root = level("v_user");
        root.embeds.push(embed);
        build_composed_select_sql(&root).unwrap_err().to_string()
    };
    assert!(
        refused(materialised("n", EmbedShape::Count, &["orders"], level("v_order")))
            .contains("counted")
    );

    let mut ordered = level("v_order");
    ordered.order_by = Some(Vec::new());
    assert!(refused(materialised("o", EmbedShape::Many, &["orders"], ordered)).contains("order"));

    let mut projected = level("v_order");
    projected.projection = Some("data".to_string());
    assert!(
        refused(materialised("p", EmbedShape::Many, &["orders"], projected)).contains("projection")
    );
}

/// The root, less the stored keys its embeds replace: the value the parent's view stored
/// there was never filtered by the embedded level's predicate.
#[test]
fn a_root_can_return_its_document_less_the_keys_embedded_in_their_place() {
    let mut root = level("v_user");
    root.keys = LevelKeys::Without(vec!["orders".to_string(), "it's".to_string()]);
    root.embeds
        .push(materialised("orders", EmbedShape::Many, &["orders"], level("v_order")));

    let (sql, _) = build_composed_select_sql(&root).unwrap();

    assert!(
        sql.contains(
            "JSON_OBJECT('d' VALUE (_l0.data - ARRAY['orders', 'it''s']::text[]), 'e' VALUE"
        ),
        "{sql}"
    );
}

// ── A root paged by keyset (a Relay connection) ──────────────────────────────────────

fn keyset_root(cursor: Option<CursorValue>, forward: bool) -> ComposedLevel {
    ComposedLevel {
        where_clause: Some(field_eq("tenant_id", json!("A"))),
        limit: Some(6),
        keyset: Some(ComposedKeyset {
            cursor_column: "pk_user".to_string(),
            cursor: cursor.map(crate::traits::RelayCursor::at),
            forward,
        }),
        embeds: vec![embed("orders", EmbedShape::Many, level("v_order"))],
        ..level("v_user")
    }
}

/// Forward: past the cursor on its native column, numbered in that column's order, and
/// cut before anything is joined — the relay page, as a composed root.
#[test]
fn a_forward_keyset_root_is_paged_past_its_cursor_before_it_is_joined() {
    let (sql, params) =
        build_composed_select_sql(&keyset_root(Some(CursorValue::Int64(41)), true)).unwrap();

    let page_end = sql.find(") AS _l0").unwrap();
    let page = &sql[..page_end];
    assert!(page.contains(r#"row_number() OVER (ORDER BY "pk_user" ASC) AS "_o""#), "{sql}");
    assert!(
        page.contains(r#"AND ("pk_user" > $2)"#),
        "the cursor, AND-ed after the predicate: {sql}"
    );
    assert!(page.contains(r#"ORDER BY "_o" LIMIT $3"#), "{sql}");
    assert!(
        !sql[page_end..].contains("LIMIT $3"),
        "the page is cut inside, not after the join"
    );
    assert!(bound(&params).starts_with(r#"[Text("A"), BigInt(41), BigInt(6)"#), "{params:?}");
}

/// Backward: read descending past a `uuid` cursor, and numbered in ascending cursor order —
/// the relay page's re-sort — so the statement returns the page in the order it does.
#[test]
fn a_backward_keyset_root_is_read_descending_and_returned_ascending() {
    let cursor = CursorValue::Uuid("00000000-0000-0000-0000-000000000007".to_string());
    let (sql, _) = build_composed_select_sql(&keyset_root(Some(cursor), false)).unwrap();

    assert!(sql.contains(r#""pk_user" < $2::text::uuid"#), "{sql}");
    assert!(sql.contains(r#"ORDER BY "pk_user" DESC LIMIT $3"#), "{sql}");
    assert!(sql.contains(r#"row_number() OVER (ORDER BY _k._cursor ASC) AS "_o""#), "{sql}");
}

/// No cursor: the first page, ordered and cut the same way.
#[test]
fn a_keyset_root_with_no_cursor_is_the_first_page() {
    let (sql, _) = build_composed_select_sql(&keyset_root(None, true)).unwrap();
    assert!(!sql.contains(r#""pk_user" >"#), "{sql}");
    assert!(sql.contains(r#"ORDER BY "pk_user" ASC"#), "{sql}");
}

/// Keyset paging is the root's alone, and never with an offset.
#[test]
fn a_keyset_is_refused_below_the_root_and_beside_an_offset() {
    let with_offset = ComposedLevel {
        offset: Some(5),
        ..keyset_root(None, true)
    };
    assert!(build_composed_select_sql(&with_offset).is_err());

    let nested = ComposedLevel {
        embeds: vec![embed("orders", EmbedShape::Many, keyset_root(None, true))],
        ..level("v_user")
    };
    assert!(build_composed_select_sql(&nested).is_err());
}

/// With an ordering, the cursor column follows the sort key directly — nothing between
/// them (`Tiebreak::None`): the next page resumes from exactly that pair (#1287), as the
/// relay page does.
#[test]
fn a_keyset_roots_order_is_its_sort_key_then_the_cursor_column() {
    let root = ComposedLevel {
        order_by: Some(vec![crate::OrderByClause::new(
            "name".to_string(),
            crate::OrderDirection::Asc,
        )]),
        ..keyset_root(None, true)
    };
    let (sql, _) = build_composed_select_sql(&root).unwrap();
    let rendered = crate::order_by::render_order_by_columns(
        root.order_by.as_deref(),
        crate::DatabaseType::PostgreSQL,
        1,
        crate::order_by::Tiebreak::None,
    )
    .unwrap()
    .unwrap();
    let window = format!(r#"row_number() OVER (ORDER BY {}, "pk_user" ASC)"#, rendered.columns);
    assert!(sql.contains(&window), "expected {window} in: {sql}");
}

/// `correlated`, joined on the columns the database proved (#1587).
fn native(target_column: &str, parent_column: &str) -> EmbedSource {
    EmbedSource::Correlated {
        target_key: vec!["post_id".to_string()],
        parent_key: vec!["id".to_string()],
        key_type:   ScalarFieldType::Uuid,
        native:     Some(crate::traits::NativeCorrelation {
            target_column: target_column.to_string(),
            parent_column: parent_column.to_string(),
        }),
    }
}

#[test]
fn a_native_join_compares_the_columns_through_the_parents_page() {
    let mut root = level("tv_post");
    root.limit = Some(20);
    root.embeds.push(ComposedEmbed {
        source: native("fk_post", "pk_post"),
        ..embed("comments", EmbedShape::Many, level("tv_comment"))
    });

    let (sql, _) = build_composed_select_sql(&root).unwrap();
    let inner = lateral(&sql, "_l0_e0");

    assert!(
        sql.contains(r#"SELECT data AS data, "pk_post" AS "_j0", row_number()"#),
        "the parent's page carries its key column: {sql}"
    );
    assert!(inner.contains(r#"WHERE "fk_post" = _l0."_j0""#), "{inner}");
    assert!(!inner.contains("->>"), "no stored key is read to correlate: {inner}");
}

#[test]
fn a_native_count_compares_the_columns_too() {
    let mut root = level("tv_post");
    root.embeds.push(ComposedEmbed {
        source: native("fk_post", "pk_post"),
        ..embed("comments_count", EmbedShape::Count, level("tv_comment"))
    });

    let (sql, _) = build_composed_select_sql(&root).unwrap();
    let inner = lateral(&sql, "_l0_e0");

    assert!(
        inner.contains(r#"SELECT COUNT(*) AS v FROM "tv_comment" WHERE "fk_post" = _l0."_j0""#),
        "{inner}"
    );
}

#[test]
fn each_embed_carries_its_own_parent_column() {
    let mut root = level("tv_comment");
    root.embeds.push(ComposedEmbed {
        source: native("pk_user", "fk_author"),
        ..embed("author", EmbedShape::One, level("tv_user"))
    });
    root.embeds.push(ComposedEmbed {
        source: native("pk_post", "fk_post"),
        ..embed("post", EmbedShape::One, level("tv_post"))
    });

    let (sql, _) = build_composed_select_sql(&root).unwrap();

    assert!(sql.contains(r#""fk_author" AS "_j0", "fk_post" AS "_j1""#), "{sql}");
    assert!(lateral(&sql, "_l0_e0").contains(r#"WHERE "pk_user" = _l0."_j0""#), "{sql}");
    assert!(lateral(&sql, "_l0_e1").contains(r#"WHERE "pk_post" = _l0."_j1""#), "{sql}");
}

#[test]
fn a_keyset_root_carries_its_key_column_too() {
    let mut root = level("tv_post");
    root.limit = Some(20);
    root.keyset = Some(ComposedKeyset {
        cursor_column: "pk_post".to_string(),
        cursor:        None,
        forward:       true,
    });
    root.embeds.push(ComposedEmbed {
        source: native("fk_post", "pk_post"),
        ..embed("comments", EmbedShape::Many, level("tv_comment"))
    });

    let (sql, _) = build_composed_select_sql(&root).unwrap();

    assert!(sql.contains(r#""pk_post" AS "_j0""#), "{sql}");
    assert!(lateral(&sql, "_l0_e0").contains(r#"WHERE "fk_post" = _l0."_j0""#), "{sql}");
}

#[test]
fn under_a_level_read_from_its_parents_document_the_stored_keys_correlate() {
    // A materialised level's rows are documents: it has no column to carry.
    let mut tags = level("v_tag");
    tags.keys = LevelKeys::Only {
        kept:   vec!["id".to_string()],
        masked: Vec::new(),
    };
    tags.embeds.push(ComposedEmbed {
        source: native("fk_tag", "pk_tag"),
        ..embed("uses", EmbedShape::Many, level("tv_use"))
    });
    let mut root = level("tv_post");
    root.embeds.push(materialised("tags", EmbedShape::Many, &["tags"], tags));

    let (sql, _) = build_composed_select_sql(&root).unwrap();

    assert!(sql.contains("data->>'post_id' = _l1.data->>'id'"), "{sql}");
    assert!(!sql.contains("_j0"), "{sql}");
}

#[test]
fn a_backward_keyset_root_carries_its_key_column_through_its_reversed_read() {
    let mut root = level("tv_post");
    root.limit = Some(20);
    root.keyset = Some(ComposedKeyset {
        cursor_column: "pk_post".to_string(),
        cursor:        None,
        forward:       false,
    });
    root.embeds.push(ComposedEmbed {
        source: native("fk_post", "pk_post"),
        ..embed("comments", EmbedShape::Many, level("tv_comment"))
    });

    let (sql, _) = build_composed_select_sql(&root).unwrap();

    assert!(
        sql.contains(r#"SELECT _k.data AS data, _k."_j0" AS "_j0", row_number()"#),
        "{sql}"
    );
    assert!(sql.contains(r#""pk_post" AS "_j0" FROM "tv_post""#), "{sql}");
    assert!(lateral(&sql, "_l0_e0").contains(r#"WHERE "fk_post" = _l0."_j0""#), "{sql}");
}

/// #1588: the tree is assembled as `json`, which concatenates the stored documents' text,
/// rather than rebuilt as `jsonb` at every level (1.8 µs per composed element against 0.46).
#[test]
fn levels_are_assembled_as_json_not_rebuilt_as_jsonb() {
    let mut comments = level("tv_comment");
    comments.embeds.push(embed("author", EmbedShape::One, level("tv_user")));
    let mut root = level("tv_post");
    root.embeds.push(embed("comments", EmbedShape::Many, comments));

    let (sql, _) = build_composed_select_sql(&root).unwrap();

    assert!(!sql.contains("jsonb_build_object"), "{sql}");
    assert!(!sql.contains("jsonb_agg"), "{sql}");
    assert!(sql.contains("JSON_OBJECT('d' VALUE _l0.data, 'e' VALUE JSON_OBJECT("), "{sql}");
    assert!(
        lateral(&sql, "_l0_e0")
            .contains(r#"COALESCE(json_agg(_c.data ORDER BY _c."_o"), '[]'::json)"#),
        "{sql}"
    );
}
