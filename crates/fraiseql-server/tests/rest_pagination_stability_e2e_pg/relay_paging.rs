//! A REST relay route pages by keyset, as its GraphQL connection does: `first`/`after` and
//! `last`/`before` bound and position the page, `meta` carries the page info and both
//! cursors, and `links.next`/`links.prev` resume it under the request's own `?sort=`. A walk
//! that follows the links returns every row once, in the order PostgreSQL gives; a cursor
//! issued under one `?sort=` is refused under another. The page's rows are charged to the
//! response-bytes ceiling, and an embed, which the keyset page does not compose, is refused.
//!
//! **Parallelism:** creates and drops its own `tf_rest_relay`.

use std::sync::Arc;

use fraiseql_core::{
    db::postgres::PostgresAdapter,
    prelude::DatabaseAdapter as _,
    schema::{
        Cardinality, CompiledSchema, FieldDefinition, FieldType, Relationship, RestConfig,
        ValidationConfig,
    },
};
use fraiseql_server::{Server, server_config::ServerConfig};
use fraiseql_test_support::try_database_url;
use fraiseql_test_utils::schema_builder::{TestQueryBuilder, TestSchemaBuilder, TestTypeBuilder};
use serde_json::Value;

use super::get;

const TABLE: &str = "tf_rest_relay";
const ROUTE: &str = "relay-items";

/// `(pk, note)`: notes holding NULLs and ties.
const ROWS: [(i64, Option<&str>); 9] = [
    (1, Some("b")),
    (2, None),
    (3, Some("a")),
    (4, Some("c")),
    (5, None),
    (6, Some("b")),
    (7, Some("a")),
    (8, None),
    (9, Some("c")),
];

fn schema() -> CompiledSchema {
    let mut connection = TestQueryBuilder::new("relayItems", "RestRelayItem")
        .returns_list(true)
        .with_sql_source(TABLE)
        .relay_cursor_column("pk")
        .build();
    connection.auto_params.has_order_by = true;
    connection.rest_path = Some(format!("/{ROUTE}"));
    let mut schema = TestSchemaBuilder::new()
        .with_type(
            TestTypeBuilder::new("RestRelayItem", TABLE)
                .relay_node()
                .with_implements(&["Node"])
                .with_simple_field("id", FieldType::Id)
                .with_simple_field("pk", FieldType::Int)
                .with_field(FieldDefinition::nullable("note", FieldType::String))
                .build(),
        )
        .with_query(connection)
        .build();
    schema.interfaces.push(
        fraiseql_core::schema::InterfaceDefinition::new("Node")
            .with_field(FieldDefinition::new("id", FieldType::Id)),
    );
    schema.rest_config = Some(RestConfig {
        enabled: true,
        ..RestConfig::default()
    });
    schema.build_indexes();
    schema
}

/// A served instance: its base URL, and the channel whose drop stops it.
struct Served {
    url:       String,
    _shutdown: tokio::sync::oneshot::Sender<()>,
}

/// Booted as the server binary boots a schema that declares a relay query: relay-capable.
async fn start() -> Option<(PostgresAdapter, Served)> {
    start_with(|_| {}).await
}

/// [`start`] over the fixture schema as `edit` leaves it.
async fn start_with(edit: impl FnOnce(&mut CompiledSchema)) -> Option<(PostgresAdapter, Served)> {
    let url = try_database_url()?;
    let adapter = PostgresAdapter::new(&url).await.expect("connect to the test database");
    let values: Vec<String> = ROWS
        .iter()
        .map(|(pk, note)| {
            let note = note.map_or_else(|| "null".to_string(), |n| format!("'{n}'"));
            format!("({pk}, jsonb_build_object('id', '{pk}', 'pk', {pk}, 'note', {note}::text))")
        })
        .collect();
    for stmt in [
        format!("DROP TABLE IF EXISTS {TABLE}"),
        format!("CREATE TABLE {TABLE} (pk bigint PRIMARY KEY, data jsonb NOT NULL)"),
        format!("INSERT INTO {TABLE} VALUES {}", values.join(", ")),
    ] {
        let _: Vec<std::collections::HashMap<String, Value>> =
            adapter.execute_raw_query(&stmt).await.expect("fixture setup");
    }
    let config = ServerConfig {
        cors_enabled: false,
        ..ServerConfig::default()
    };
    let served = Arc::new(PostgresAdapter::new(&url).await.expect("connect to the test database"));
    let mut schema = schema();
    edit(&mut schema);
    let server = Box::pin(Server::with_relay_pagination(config, schema, served, None))
        .await
        .expect("Server::with_relay_pagination");
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.expect("bind");
    let port = listener.local_addr().expect("local addr").port();
    let (tx, rx) = tokio::sync::oneshot::channel::<()>();
    tokio::spawn(async move {
        server
            .serve_on_listener(listener, async {
                let _ = rx.await;
            })
            .await
            .expect("server task failed");
    });
    tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    let served = Served {
        url:       format!("http://127.0.0.1:{port}"),
        _shutdown: tx,
    };
    Some((adapter, served))
}

/// The pks in the order PostgreSQL gives `order`, ties broken by `pk`.
async fn oracle(adapter: &PostgresAdapter, order: &str) -> Vec<i64> {
    adapter
        .execute_raw_query(&format!("SELECT pk FROM {TABLE} ORDER BY {order}, pk"))
        .await
        .unwrap()
        .iter()
        .map(|row| row["pk"].as_i64().unwrap())
        .collect()
}

/// One page: its pks, its `meta`, its `links`.
async fn page(server: &Served, query: &str) -> (Vec<i64>, Value, Value) {
    let (status, body) = get(&server.url, ROUTE, query).await;
    assert_eq!(status, reqwest::StatusCode::OK, "{query}: {body}");
    let parsed: Value =
        serde_json::from_str(&body).unwrap_or_else(|e| panic!("not JSON ({e}): {body}"));
    let pks = parsed["data"]
        .as_array()
        .unwrap_or_else(|| panic!("{query}: no data array: {body}"))
        .iter()
        .map(|row| row["pk"].as_i64().unwrap_or_else(|| panic!("{query}: no pk: {row}")))
        .collect();
    (pks, parsed["meta"].clone(), parsed["links"].clone())
}

/// The route and query string a link names, as `get` takes them.
fn followed(link: &Value) -> String {
    let link = link.as_str().unwrap_or_else(|| panic!("a link is a string: {link}"));
    let query = link
        .strip_prefix(&format!("/rest/v1/{ROUTE}"))
        .unwrap_or_else(|| panic!("a link names the route: {link}"));
    query.to_string()
}

/// Every row, walking from `query` by the link `follow` names until a page has none.
async fn walk(server: &Served, query: &str, size: usize, follow: &str) -> Vec<i64> {
    let mut pages = Vec::new();
    let mut query = query.to_string();
    for _ in 0..=ROWS.len() {
        let (pks, meta, links) = page(server, &query).await;
        assert!(pks.len() <= size, "{query}: a page of {size} served {pks:?}");
        pages.push(pks);
        if links[follow].is_null() {
            let more = if follow == "next" {
                "hasNextPage"
            } else {
                "hasPreviousPage"
            };
            assert_eq!(meta[more], Value::Bool(false), "{query}: no `{follow}` link: {meta}");
            if follow == "prev" {
                pages.reverse();
            }
            return pages.into_iter().flatten().collect();
        }
        query = followed(&links[follow]);
    }
    panic!("the walk from {query} did not end: {pages:?}");
}

#[tokio::test]
async fn a_rest_relay_route_walks_every_row_once_in_its_order_both_ways() {
    let Some((adapter, server)) = start().await else {
        return;
    };
    for (sort, order) in [
        ("", "pk"),
        ("&sort=-note.nullslast", "data->>'note' DESC NULLS LAST"),
        ("&sort=note.nullsfirst", "data->>'note' ASC NULLS FIRST"),
    ] {
        let expected = oracle(&adapter, order).await;
        for size in [2, 4] {
            let forward = walk(&server, &format!("?first={size}{sort}"), size, "next").await;
            assert_eq!(forward, expected, "forward {sort} by {size}");
            let backward = walk(&server, &format!("?last={size}{sort}"), size, "prev").await;
            assert_eq!(backward, expected, "backward {sort} by {size}");
        }
    }
}

#[tokio::test]
async fn a_page_carries_its_page_info_and_cursors() {
    let Some((_adapter, server)) = start().await else {
        return;
    };
    let (pks, meta, links) = page(&server, "?first=2").await;
    assert_eq!(pks, [1, 2]);
    assert_eq!(meta["hasNextPage"], Value::Bool(true), "{meta}");
    assert_eq!(meta["hasPreviousPage"], Value::Bool(false), "{meta}");
    for cursor in ["startCursor", "endCursor"] {
        assert!(meta[cursor].is_string(), "`{cursor}`: {meta}");
    }
    assert!(links["next"].is_string(), "{links}");
    assert!(links["prev"].is_null(), "{links}");

    // What it serves is what its OpenAPI document publishes for the route.
    let (status, spec) = get(&server.url, "openapi.json", "").await;
    assert_eq!(status, reqwest::StatusCode::OK, "{spec}");
    let spec: Value = serde_json::from_str(&spec).unwrap();
    let published = &spec["paths"][format!("/{ROUTE}")]["get"]["responses"]["200"]["content"]["application/json"]
        ["schema"]["properties"];
    for (member, served) in [("meta", &meta), ("links", &links)] {
        for key in served.as_object().unwrap().keys() {
            assert!(
                published[member]["properties"][key].is_object(),
                "`{member}.{key}` is served but not published: {published}"
            );
        }
    }
}

#[tokio::test]
async fn a_cursor_is_refused_under_another_sort() {
    let Some((_adapter, server)) = start().await else {
        return;
    };
    let (_, meta, _) = page(&server, "?first=2&sort=note.nullslast").await;
    let cursor = meta["endCursor"].as_str().unwrap_or_else(|| panic!("{meta}"));
    let cursor: String = url::form_urlencoded::byte_serialize(cursor.as_bytes()).collect();
    let (status, body) =
        get(&server.url, ROUTE, &format!("?first=2&sort=note.nullsfirst&after={cursor}")).await;
    assert_eq!(status, reqwest::StatusCode::BAD_REQUEST, "{body}");
    assert!(body.contains("invalid relay cursor for `after`"), "{body}");
}

#[tokio::test]
async fn a_page_over_the_response_ceiling_is_refused() {
    let Some((_adapter, server)) = start_with(|schema| {
        schema.validation_config = Some(ValidationConfig {
            max_response_bytes: Some(60),
            ..ValidationConfig::default()
        });
    })
    .await
    else {
        return;
    };
    let (pks, _, _) = page(&server, "?first=1").await;
    assert_eq!(pks, [1], "one row is under the ceiling");
    let (status, body) = get(&server.url, ROUTE, "?first=9").await;
    assert_eq!(status, reqwest::StatusCode::PAYLOAD_TOO_LARGE, "{body}");
    assert!(body.contains("max_response_bytes"), "{body}");
}

#[tokio::test]
async fn an_embed_on_a_relay_route_is_refused() {
    let Some((_adapter, server)) = start_with(|schema| {
        schema.types[0].relationships.push(Relationship {
            name:           "peer".to_string(),
            target_type:    "RestRelayItem".to_string(),
            cardinality:    Cardinality::ManyToOne,
            foreign_key:    "pk".to_string(),
            referenced_key: "pk".to_string(),
            native_join:    None,
        });
        schema.build_indexes();
    })
    .await
    else {
        return;
    };
    let (status, body) = get(&server.url, ROUTE, "?first=2&select=pk,peer(pk)").await;
    assert_eq!(status, reqwest::StatusCode::BAD_REQUEST, "{body}");
    assert!(body.contains("cannot embed a relationship on a relay (cursor) route"), "{body}");
}
