//! A REST relay route embeds: `?select=pk,parts(pk),kind(pk)` on a keyset page serves each
//! row with its related rows, and the page is the one the route serves without the embed.
//! The page is cut before any embedded level is joined to it, so a walk that follows the
//! links with an embed returns the same rows, cursors, `meta` and links as the flat walk, in
//! both directions and under every `?sort=`, whatever number of related rows a row has. The
//! page stays bounded (the default page, the page-size ceiling) and is charged for what it
//! embeds; `Prefer: count=exact` counts the page's rows, not its embedded ones.
//!
//! **Parallelism:** creates and drops its own `tf_rest_relay_embed_*` tables.

use std::sync::Arc;

use fraiseql_core::{
    db::postgres::PostgresAdapter,
    prelude::DatabaseAdapter as _,
    schema::{
        Cardinality, CompiledSchema, CostBudgetConfig, FieldDefinition, FieldType, Relationship,
        RestConfig, SecurityConfig, ValidationConfig,
    },
};
use fraiseql_server::{Server, server_config::ServerConfig};
use fraiseql_test_support::try_database_url;
use fraiseql_test_utils::schema_builder::{TestQueryBuilder, TestSchemaBuilder, TestTypeBuilder};
use serde_json::Value;

use super::get;

const ITEMS: &str = "tf_rest_relay_embed_item";
const PARTS: &str = "tf_rest_relay_embed_part";
const KINDS: &str = "tf_rest_relay_embed_kind";
const ROUTE: &str = "relay-embed-items";
const ROWS: i64 = 25;

/// A row's note: ties and NULLs, so a `?sort=` on it resumes past equal keys.
fn note(pk: i64) -> Option<&'static str> {
    [Some("b"), None, Some("a"), Some("c"), None][usize::try_from(pk % 5).unwrap()]
}

/// A row's kind: none for every fourth row, so a to-one embed is `null` there.
fn kind(pk: i64) -> Option<i64> {
    (pk % 4 != 0).then_some(pk % 3 + 1)
}

/// A row's parts: none, one or three, so a join that fanned out or dropped a row would
/// move the page's bounds.
fn parts(pk: i64) -> Vec<i64> {
    let count = [0, 1, 3][usize::try_from(pk % 3).unwrap()];
    (0..count).map(|j| pk * 10 + j).collect()
}

fn relationship(name: &str, target: &str, cardinality: Cardinality, fk: &str) -> Relationship {
    Relationship {
        name: name.to_string(),
        target_type: target.to_string(),
        cardinality,
        foreign_key: fk.to_string(),
        referenced_key: "pk".to_string(),
        native_join: None,
    }
}

fn schema() -> CompiledSchema {
    let mut connection = TestQueryBuilder::new("relayEmbedItems", "RestRelayEmbedItem")
        .returns_list(true)
        .with_sql_source(ITEMS)
        .relay_cursor_column("pk")
        .build();
    connection.auto_params.has_order_by = true;
    connection.rest_path = Some(format!("/{ROUTE}"));
    let list = |name: &str, ty: &str, table: &str| {
        TestQueryBuilder::new(name, ty)
            .returns_list(true)
            .with_sql_source(table)
            .build()
    };
    let mut item = TestTypeBuilder::new("RestRelayEmbedItem", ITEMS)
        .relay_node()
        .with_implements(&["Node"])
        .with_simple_field("id", FieldType::Id)
        .with_simple_field("pk", FieldType::Int)
        .with_field(FieldDefinition::nullable("note", FieldType::String))
        .with_field(FieldDefinition::nullable("kind_pk", FieldType::Int))
        .build();
    item.relationships = vec![
        relationship("parts", "RestRelayEmbedPart", Cardinality::OneToMany, "item_pk"),
        relationship("kind", "RestRelayEmbedKind", Cardinality::ManyToOne, "kind_pk"),
    ];
    let mut part = TestTypeBuilder::new("RestRelayEmbedPart", PARTS)
        .with_simple_field("id", FieldType::Id)
        .with_simple_field("pk", FieldType::Int)
        .with_simple_field("item_pk", FieldType::Int)
        .build();
    part.relationships = vec![relationship(
        "item",
        "RestRelayEmbedItem",
        Cardinality::ManyToOne,
        "item_pk",
    )];
    let kind = TestTypeBuilder::new("RestRelayEmbedKind", KINDS)
        .with_simple_field("id", FieldType::Id)
        .with_simple_field("pk", FieldType::Int)
        .build();
    let mut schema = TestSchemaBuilder::new()
        .with_type(item)
        .with_type(part)
        .with_type(kind)
        // The list query an embedded item level is read through, ahead of the connection.
        .with_query(list("relayEmbedItemList", "RestRelayEmbedItem", ITEMS))
        .with_query(connection)
        .with_query(list("relayEmbedParts", "RestRelayEmbedPart", PARTS))
        .with_query(list("relayEmbedKinds", "RestRelayEmbedKind", KINDS))
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

async fn run(adapter: &PostgresAdapter, statement: &str) {
    let _: Vec<std::collections::HashMap<String, Value>> =
        adapter.execute_raw_query(statement).await.expect("fixture setup");
}

/// The fixture's rows, and a relay-capable server over the schema as `edit` leaves it.
async fn start_with(edit: impl FnOnce(&mut CompiledSchema)) -> Option<Served> {
    let url = try_database_url()?;
    let adapter = PostgresAdapter::new(&url).await.expect("connect to the test database");
    for table in [ITEMS, PARTS, KINDS] {
        run(&adapter, &format!("DROP TABLE IF EXISTS {table}")).await;
        // `id`: the column an embedded list level is ordered by.
        run(
            &adapter,
            &format!(
                "CREATE TABLE {table} (pk bigint PRIMARY KEY, id bigint GENERATED ALWAYS AS (pk) \
                 STORED, data jsonb NOT NULL)"
            ),
        )
        .await;
    }
    let items: Vec<String> = (1..=ROWS)
        .map(|pk| {
            let note = note(pk).map_or_else(|| "null".to_string(), |n| format!("'{n}'"));
            let kind = kind(pk).map_or_else(|| "null".to_string(), |k| k.to_string());
            format!(
                "({pk}, jsonb_build_object('id', '{pk}', 'pk', {pk}, 'note', {note}::text, \
                 'kind_pk', {kind}::bigint))"
            )
        })
        .collect();
    run(&adapter, &format!("INSERT INTO {ITEMS} (pk, data) VALUES {}", items.join(", "))).await;
    let parts: Vec<String> = (1..=ROWS)
        .flat_map(|item| {
            parts(item).into_iter().map(move |pk| {
                format!("({pk}, jsonb_build_object('id', '{pk}', 'pk', {pk}, 'item_pk', {item}))")
            })
        })
        .collect();
    run(&adapter, &format!("INSERT INTO {PARTS} (pk, data) VALUES {}", parts.join(", "))).await;
    run(
        &adapter,
        &format!(
            "INSERT INTO {KINDS} (pk, data) SELECT k, jsonb_build_object('id', k::text, 'pk', k) FROM \
             generate_series(1, 3) AS k"
        ),
    )
    .await;

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
    Some(Served {
        url:       format!("http://127.0.0.1:{port}"),
        _shutdown: tx,
    })
}

async fn start() -> Option<Served> {
    start_with(|_| {}).await
}

/// One page as a client compares it: the rows' pks, `meta`, and each link's parameters
/// other than `select=`, sorted.
#[derive(Debug, PartialEq)]
struct Page {
    pks:   Vec<i64>,
    meta:  Value,
    links: Vec<(String, Option<Vec<(String, String)>>)>,
}

/// A link's parameters other than `select=`, sorted; `None` for no link.
fn link_parameters(link: &Value) -> Option<Vec<(String, String)>> {
    let link = link.as_str()?;
    let query = link.split_once('?').map_or("", |(_, q)| q);
    let mut parameters: Vec<(String, String)> = url::form_urlencoded::parse(query.as_bytes())
        .filter(|(k, _)| k != "select")
        .map(|(k, v)| (k.into_owned(), v.into_owned()))
        .collect();
    parameters.sort();
    Some(parameters)
}

/// `query` answered `200`: the page, and the body it came in.
async fn page(server: &Served, query: &str) -> (Page, Value) {
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
    let links = ["next", "prev"]
        .into_iter()
        .map(|l| (l.to_string(), link_parameters(&parsed["links"][l])))
        .collect();
    (
        Page {
            pks,
            meta: parsed["meta"].clone(),
            links,
        },
        parsed,
    )
}

/// The route and query string a link names, as `get` takes them.
fn followed(link: &Value) -> String {
    let link = link.as_str().unwrap_or_else(|| panic!("a link is a string: {link}"));
    link.strip_prefix(&format!("/rest/v1/{ROUTE}"))
        .unwrap_or_else(|| panic!("a link names the route: {link}"))
        .to_string()
}

/// Every page from `query`, following the link `follow` names until a page has none, and
/// every body served on the way.
async fn walk(server: &Served, query: &str, follow: &str) -> (Vec<Page>, Vec<Value>) {
    let mut pages = Vec::new();
    let mut bodies = Vec::new();
    let mut query = query.to_string();
    for _ in 0..=ROWS {
        let (page, body) = page(server, &query).await;
        let next = body["links"][follow].clone();
        pages.push(page);
        bodies.push(body);
        if next.is_null() {
            return (pages, bodies);
        }
        query = followed(&next);
    }
    panic!("the walk from {query} did not end: {pages:?}");
}

/// Each served row carries exactly its own related rows: its parts, and its kind or `null`.
fn assert_embedded(bodies: &[Value]) {
    for body in bodies {
        for row in body["data"].as_array().unwrap() {
            let pk = row["pk"].as_i64().unwrap();
            let mut served: Vec<i64> = row["parts"]
                .as_array()
                .unwrap_or_else(|| panic!("row {pk} carries its parts: {row}"))
                .iter()
                .map(|p| p["pk"].as_i64().unwrap())
                .collect();
            served.sort_unstable();
            assert_eq!(served, parts(pk), "row {pk}'s parts: {row}");
            assert_eq!(row["kind"]["pk"].as_i64(), kind(pk), "row {pk}'s kind: {row}");
            assert!(kind(pk).is_some() || row["kind"].is_null(), "row {pk}: {row}");
        }
    }
}

const FLAT: &str = "select=pk,note";
const EMBEDDED: &str = "select=pk,note,parts(pk),kind(pk)";

#[tokio::test]
async fn a_walk_with_an_embed_pages_as_the_flat_walk_both_ways() {
    let Some(server) = start().await else {
        return;
    };
    for sort in ["", "&sort=-note.nullslast", "&sort=note.nullsfirst"] {
        for (window, follow) in [("first=4", "next"), ("last=4", "prev"), ("first=7", "next")] {
            let (flat, _) = walk(&server, &format!("?{window}&{FLAT}{sort}"), follow).await;
            let (embedded, bodies) =
                walk(&server, &format!("?{window}&{EMBEDDED}{sort}"), follow).await;
            assert_eq!(embedded, flat, "{window} {sort}: the embed changed the walk");
            let served: usize = flat.iter().map(|p| p.pks.len()).sum();
            assert_eq!(served, usize::try_from(ROWS).unwrap(), "{window} {sort}: every row");
            assert_embedded(&bodies);
        }
    }
}

/// A cursor issued with an embed resumes the flat page, and one issued without resumes the
/// page with an embed, in both directions: an embed is not part of the ordering a cursor
/// belongs to.
#[tokio::test]
async fn a_cursor_resumes_across_the_embed_both_ways() {
    let Some(server) = start().await else {
        return;
    };
    let sort = "&sort=-note.nullslast";
    let encode = |c: &Value| -> String {
        url::form_urlencoded::byte_serialize(c.as_str().unwrap().as_bytes()).collect()
    };
    let (embedded, _) = page(&server, &format!("?first=4&{EMBEDDED}{sort}")).await;
    let (flat, _) = page(&server, &format!("?first=4&{FLAT}{sort}")).await;
    let after = encode(&embedded.meta["endCursor"]);
    let before = encode(&flat.meta["endCursor"]);
    for (resumed, from) in [
        (
            format!("?first=4&after={after}&{FLAT}{sort}"),
            format!("?first=4&after={after}&{EMBEDDED}{sort}"),
        ),
        (
            format!("?last=2&before={before}&{EMBEDDED}{sort}"),
            format!("?last=2&before={before}&{FLAT}{sort}"),
        ),
    ] {
        let (a, _) = page(&server, &resumed).await;
        let (b, _) = page(&server, &from).await;
        assert_eq!(a.pks, b.pks, "{resumed} against {from}");
        assert!(!a.pks.is_empty(), "{resumed}");
    }
}

/// A nested embed is read below the page as the page's own embeds are.
#[tokio::test]
async fn a_nested_embed_is_read_below_the_page() {
    let Some(server) = start().await else {
        return;
    };
    let (flat, _) = page(&server, &format!("?first=5&{FLAT}")).await;
    let (nested, body) = page(&server, "?first=5&select=pk,note,parts(pk,item(pk))").await;
    assert_eq!(nested.pks, flat.pks);
    for row in body["data"].as_array().unwrap() {
        let pk = row["pk"].as_i64().unwrap();
        let parts = row["parts"].as_array().unwrap();
        assert_eq!(parts.len(), self::parts(pk).len(), "row {pk}: {row}");
        for part in parts {
            assert_eq!(part["item"]["pk"].as_i64(), Some(pk), "row {pk}'s part: {part}");
        }
    }
}

/// `Prefer: count=exact` counts the rows the route pages, not the rows it embeds.
#[tokio::test]
async fn the_total_counts_the_pages_rows_with_an_embed() {
    let Some(server) = start().await else {
        return;
    };
    let client = reqwest::Client::new();
    for select in [FLAT, EMBEDDED] {
        let response = client
            .get(format!("{}/rest/v1/{ROUTE}?first=3&{select}", server.url))
            .header("Prefer", "count=exact")
            .send()
            .await
            .expect("request");
        assert_eq!(response.status(), reqwest::StatusCode::OK);
        let body: Value = response.json().await.expect("JSON");
        assert_eq!(body["meta"]["total"].as_i64(), Some(ROWS), "{select}: {body}");
    }
}

/// `[rest] default_page_size` and `max_page_size` bound a page with an embed as they bound
/// the flat page: with no `first` or `last` it is the default page, and a `first` over the
/// maximum is clamped to it. An embed does not make the page unbounded.
#[tokio::test]
async fn the_rest_page_bounds_hold_with_an_embed() {
    let Some(server) = start_with(|schema| {
        schema.rest_config = Some(RestConfig {
            enabled: true,
            default_page_size: 6,
            max_page_size: 9,
            ..RestConfig::default()
        });
    })
    .await
    else {
        return;
    };
    for (window, rows) in [("", 6), ("first=50&", 9)] {
        let (flat, _) = page(&server, &format!("?{window}{FLAT}")).await;
        let (embedded, body) = page(&server, &format!("?{window}{EMBEDDED}")).await;
        assert_eq!(embedded.pks.len(), rows, "{window}: {:?}", embedded.pks);
        assert_eq!(embedded, flat, "{window}");
        assert_eq!(embedded.meta["hasNextPage"], Value::Bool(true), "{window}");
        assert_embedded(std::slice::from_ref(&body));
    }
}

/// A window over `[validation] max_page_size` is refused with an embed exactly as without.
#[tokio::test]
async fn a_window_over_the_page_ceiling_is_refused_with_an_embed_as_without() {
    let Some(server) = start_with(|schema| {
        schema.validation_config = Some(ValidationConfig {
            max_page_size: Some(5),
            ..ValidationConfig::default()
        });
    })
    .await
    else {
        return;
    };
    let (status, body) = get(&server.url, ROUTE, &format!("?first=6&{FLAT}")).await;
    assert_eq!(status, reqwest::StatusCode::BAD_REQUEST, "{body}");
    // The embed's own page is within the ceiling, so what is refused is the route's window.
    let (embedded_status, embedded_body) =
        get(&server.url, ROUTE, "?first=6&select=pk,note,parts(pk)&parts.limit=2").await;
    assert_eq!((embedded_status, embedded_body), (status, body));
    let (served, _) = page(&server, "?first=5&select=pk,note,parts(pk)&parts.limit=2").await;
    assert_eq!(served.pks.len(), 5, "a window at the ceiling is served");
}

/// A page whose embed puts it over `[security.cost_budget] per_request_max` is refused as
/// a flat page over it is; the same page without the embed is served.
#[tokio::test]
async fn an_embed_over_the_cost_ceiling_is_refused_as_a_flat_page_over_it_is() {
    let Some(server) = start_with(|schema| {
        // `pk, note` over `first: 2` scores 1 + 2 × 2.
        schema.security = Some(SecurityConfig {
            cost_budget: Some(CostBudgetConfig {
                per_request_max: Some(5),
                ..CostBudgetConfig::default()
            }),
            ..SecurityConfig::default()
        });
    })
    .await
    else {
        return;
    };
    let (served, _) = page(&server, &format!("?first=2&{FLAT}")).await;
    assert_eq!(served.pks.len(), 2, "the flat page at the ceiling is served");
    let (flat_status, flat_body) = get(&server.url, ROUTE, &format!("?first=3&{FLAT}")).await;
    assert_ne!(
        flat_status,
        reqwest::StatusCode::OK,
        "a flat page over the ceiling: {flat_body}"
    );
    let (status, body) = get(&server.url, ROUTE, &format!("?first=2&{EMBEDDED}")).await;
    assert_eq!(status, flat_status, "{body}");
    // Refused by the cost ceiling, as the flat page is, and not for another reason.
    for refused in [&flat_body, &body] {
        assert!(refused.contains("[security.cost_budget] per_request_max"), "{refused}");
    }
}
