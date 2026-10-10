//! #1544: a read of more than 50 keys is served, at every site that builds an object from
//! a variable number of key/value pairs.
//!
//! PostgreSQL refuses a function call with more than 100 arguments (`FUNC_MAX_ARGS`,
//! SQLSTATE 54023), and every projection site built its object with one
//! `jsonb_build_object(k1, v1, …)`. So a selection of 51 fields failed with "cannot pass more
//! than 100 arguments to a function", which a client saw as an internal error.
//!
//! Each site is driven here with `WIDE` keys and its SQL executed on PostgreSQL: the
//! generator's three projections and its nested sub-selection, and the composed read's
//! masked keys and embeds. What is asserted is the object PostgreSQL returns.
//!
//! **Execution engine:** `PostgreSQL` · **Infrastructure:** `DATABASE_URL` ·
//! **Parallelism:** drops and recreates its own `p1544_wide` schema → run `--test-threads=1`.
#![cfg(all(feature = "postgres", feature = "test-postgres"))]
#![allow(clippy::unwrap_used, clippy::panic)] // Reason: test code, panics are acceptable

use std::collections::HashMap;

use fraiseql_db::{
    COMPOSED_DOCUMENT_KEY, COMPOSED_EMBEDS_KEY, ComposedEmbed, ComposedLevel, DatabaseAdapter,
    EmbedShape, EmbedSource, FieldKind, LevelKeys, PostgresAdapter, PostgresProjectionGenerator,
    ProjectionField, ScalarFieldType, types::ReadRouting,
};
use serde_json::{Map, Value, json};

const SCHEMA: &str = "p1544_wide";
/// One past the 50 pairs a single call can carry.
const WIDE: usize = 51;

fn key(i: usize) -> String {
    format!("k{i}")
}

fn keys() -> Vec<String> {
    (1..=WIDE).map(key).collect()
}

/// `{k1: "v1", …}`: what each row stores, top-level and under `obj`.
fn wide_object() -> Value {
    Value::Object(keys().into_iter().map(|k| (k.clone(), json!(format!("v-{k}")))).collect())
}

async fn adapter() -> PostgresAdapter {
    let adapter = PostgresAdapter::new(&fraiseql_test_support::database_url()).await.unwrap();
    let mut document = wide_object();
    document["id"] = json!(1);
    document["obj"] = wide_object();
    let document = document.to_string().replace('\'', "''");
    for sql in [
        format!("DROP SCHEMA IF EXISTS {SCHEMA} CASCADE"),
        format!("CREATE SCHEMA {SCHEMA}"),
        format!("CREATE VIEW {SCHEMA}.v_parent AS SELECT 1 AS id, '{document}'::jsonb AS data"),
        format!(
            "CREATE VIEW {SCHEMA}.v_child AS SELECT 1 AS id, '{{\"parent_id\": 1}}'::jsonb AS data"
        ),
    ] {
        let _: Vec<HashMap<String, Value>> = adapter.execute_raw_query(&sql).await.unwrap();
    }
    adapter
}

/// Run a projection expression over the parent view: the object PostgreSQL built.
async fn project(adapter: &PostgresAdapter, projection: &str) -> Map<String, Value> {
    let sql = format!("SELECT {projection} AS out FROM {SCHEMA}.v_parent");
    let rows: Vec<HashMap<String, Value>> = adapter
        .execute_raw_query(&sql)
        .await
        .unwrap_or_else(|e| panic!("the projection must execute: {e}"));
    match rows.into_iter().next().and_then(|mut r| r.remove("out")) {
        Some(Value::Object(object)) => object,
        other => panic!("expected an object, got {other:?}"),
    }
}

fn assert_holds_every_key(object: &Map<String, Value>, what: &str) {
    let missing: Vec<String> = keys().into_iter().filter(|k| !object.contains_key(k)).collect();
    assert!(missing.is_empty(), "{what}: missing {missing:?} of {WIDE}");
}

fn scalars() -> Vec<ProjectionField> {
    keys().into_iter().map(ProjectionField::scalar).collect()
}

#[tokio::test]
async fn the_generators_projections_carry_more_than_fifty_keys() {
    let adapter = adapter().await;
    let generator = PostgresProjectionGenerator::new();

    let plain = generator.generate_projection_sql(&keys()).unwrap();
    assert_holds_every_key(&project(&adapter, &plain).await, "generate_projection_sql");

    let typed = generator.generate_typed_projection_sql(&scalars()).unwrap();
    assert_holds_every_key(&project(&adapter, &typed).await, "generate_typed_projection_sql");

    let merged = generator.generate_merged_projection_sql(&scalars()).unwrap();
    assert_holds_every_key(&project(&adapter, &merged).await, "generate_merged_projection_sql");
}

#[tokio::test]
async fn a_nested_selection_carries_more_than_fifty_keys() {
    let adapter = adapter().await;
    let obj = ProjectionField {
        kind: FieldKind::Composite,
        sub_fields: Some(scalars()),
        ..ProjectionField::scalar("obj")
    };
    let sql = PostgresProjectionGenerator::new()
        .generate_typed_projection_sql(&[obj])
        .unwrap();
    let projected = project(&adapter, &sql).await;
    let nested = projected["obj"].as_object().unwrap_or_else(|| panic!("{projected:?}"));
    assert_holds_every_key(nested, "a sub-selection");
}

fn level(view: &str) -> ComposedLevel {
    ComposedLevel {
        view:         format!("{SCHEMA}.{view}"),
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

async fn compose(adapter: &PostgresAdapter, read: &ComposedLevel) -> Value {
    let rows = adapter
        .execute_composed_with_session(read, &[], ReadRouting::Primary)
        .await
        .unwrap_or_else(|e| panic!("the composed read must execute: {e}"));
    rows.first().expect("one row").as_value().clone()
}

#[tokio::test]
async fn a_composed_document_masks_more_than_fifty_keys() {
    let adapter = adapter().await;
    let read = ComposedLevel {
        keys: LevelKeys::Only {
            kept:   vec!["id".to_string()],
            masked: keys(),
        },
        ..level("v_parent")
    };
    let row = compose(&adapter, &read).await;
    let document = row[COMPOSED_DOCUMENT_KEY].as_object().unwrap_or_else(|| panic!("{row}"));
    assert_holds_every_key(document, "masked keys");
    assert!(keys().iter().all(|k| document[k].is_null()), "a masked key is null: {row}");
}

#[tokio::test]
async fn a_composed_level_carries_more_than_fifty_embeds() {
    let adapter = adapter().await;
    let embeds = keys()
        .into_iter()
        .map(|k| ComposedEmbed {
            output_key: k,
            shape:      EmbedShape::Count,
            source:     EmbedSource::Correlated {
                target_key: vec!["parent_id".to_string()],
                parent_key: vec!["id".to_string()],
                key_type:   ScalarFieldType::Integer,
            },
            level:      level("v_child"),
        })
        .collect();
    let read = ComposedLevel {
        embeds,
        ..level("v_parent")
    };
    let row = compose(&adapter, &read).await;
    let embedded = row[COMPOSED_EMBEDS_KEY].as_object().unwrap_or_else(|| panic!("{row}"));
    assert_holds_every_key(embedded, "embeds");
}
