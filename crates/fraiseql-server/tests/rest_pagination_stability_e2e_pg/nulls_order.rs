//! `?sort=`'s NULL placement, `.nullsfirst` and `.nullslast`: a list puts the NULLs where the
//! item asks.
//!
//! **Parallelism:** creates and drops its own `tf_rest_nulls`.

use std::sync::Arc;

use fraiseql_core::{
    db::postgres::PostgresAdapter,
    prelude::DatabaseAdapter as _,
    schema::{CompiledSchema, FieldDefinition, FieldType, RestConfig},
};
use fraiseql_server::server_config::ServerConfig;
use fraiseql_test_support::try_database_url;
use fraiseql_test_utils::schema_builder::{
    TestFieldBuilder, TestQueryBuilder, TestSchemaBuilder, TestTypeBuilder,
};
use serde_json::Value;

use super::{TestServer, get};

const TABLE: &str = "tf_rest_nulls";

/// `(id, note)`: notes holding NULLs. Single-digit ids, so the identity's text order is its
/// numeric order.
const ROWS: [(i64, Option<&str>); 7] = [
    (1, Some("b")),
    (2, None),
    (3, Some("a")),
    (4, Some("c")),
    (5, None),
    (6, Some("b")),
    (7, None),
];

fn schema() -> CompiledSchema {
    let mut list = TestQueryBuilder::new("nullsItems", "RestNullsItem")
        .returns_list(true)
        .with_sql_source(TABLE)
        .build();
    list.rest_path = Some("/nulls-list".to_string());
    let mut schema = TestSchemaBuilder::new()
        .with_type(
            TestTypeBuilder::new("RestNullsItem", TABLE)
                .with_field(TestFieldBuilder::new("id", FieldType::Int).build())
                .with_field(FieldDefinition::nullable("note", FieldType::String))
                .build(),
        )
        .with_query(list)
        .build();
    schema.rest_config = Some(RestConfig {
        enabled: true,
        ..RestConfig::default()
    });
    schema.build_indexes();
    schema
}

async fn start() -> Option<(PostgresAdapter, TestServer)> {
    let url = try_database_url()?;
    let adapter = PostgresAdapter::new(&url).await.expect("connect to the test database");
    let values: Vec<String> = ROWS
        .iter()
        .map(|(id, note)| {
            let note = note.map_or_else(|| "null".to_string(), |n| format!("'{n}'"));
            format!("({id}, jsonb_build_object('id', {id}, 'note', {note}::text))")
        })
        .collect();
    for stmt in [
        format!("DROP TABLE IF EXISTS {TABLE}"),
        format!("CREATE TABLE {TABLE} (id bigint PRIMARY KEY, data jsonb NOT NULL)"),
        format!("INSERT INTO {TABLE} VALUES {}", values.join(", ")),
    ] {
        let _: Vec<std::collections::HashMap<String, Value>> =
            adapter.execute_raw_query(&stmt).await.expect("fixture setup");
    }
    let config = ServerConfig {
        cors_enabled: false,
        ..ServerConfig::default()
    };
    let served = PostgresAdapter::new(&url).await.expect("connect to the test database");
    let server = Box::pin(TestServer::start_with_config(config, schema(), Arc::new(served))).await;
    Some((adapter, server))
}

fn served_ids(body: &str) -> Vec<i64> {
    let parsed: Value =
        serde_json::from_str(body).unwrap_or_else(|e| panic!("not JSON ({e}): {body}"));
    parsed["data"]
        .as_array()
        .unwrap_or_else(|| panic!("no data array: {body}"))
        .iter()
        .map(|row| row["id"].as_i64().unwrap_or_else(|| panic!("row carries no id: {row}")))
        .collect()
}

async fn oracle(adapter: &PostgresAdapter, order: &str) -> Vec<i64> {
    adapter
        .execute_raw_query(&format!("SELECT id FROM {TABLE} ORDER BY {order}, id"))
        .await
        .unwrap()
        .iter()
        .map(|row| row["id"].as_i64().unwrap())
        .collect()
}

#[tokio::test]
async fn a_rest_list_puts_its_nulls_where_the_sort_asks() {
    let Some((adapter, server)) = start().await else {
        return;
    };
    for (sort, order) in [
        ("-note.nullslast", "data->>'note' DESC NULLS LAST"),
        ("note.nullsfirst", "data->>'note' ASC NULLS FIRST"),
    ] {
        let (status, body) = get(&server.url, "nulls-list", &format!("?sort={sort}")).await;
        assert_eq!(status, reqwest::StatusCode::OK, "{sort}: {body}");
        assert_eq!(served_ids(&body), oracle(&adapter, order).await, "{sort}");
    }
}
