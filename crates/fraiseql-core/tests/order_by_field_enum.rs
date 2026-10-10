#![allow(clippy::unwrap_used, clippy::panic, clippy::print_stderr)] // Reason: test code — panics and skip diagnostics are acceptable

//! #1159: an `orderBy` item's `field` is an enum of exactly the keys the query accepts, so a
//! client gets completion and the schema never advertises a key the engine refuses.
//!
//! The enum and the acceptance are one set: a query's sortable fields (its type's declared
//! fields that order meaningfully, so not an object or a list) and its native columns. When
//! every query returning an entity accepts the same set, the entity has one
//! `{Entity}OrderByField`; when they differ, each query has its own, and each refuses the
//! other's key. A relay connection is advertised `orderBy` like a list, and its `where` too
//! (#1535).
//!
//! **Execution engine:** `PostgreSQL` · **Infrastructure:** `DATABASE_URL` ·
//! **Parallelism:** creates and drops its own `tv_obf_*` tables.

use std::{collections::BTreeMap, sync::Arc};

use fraiseql_core::{
    db::{DatabaseAdapter, postgres::PostgresAdapter},
    runtime::Executor,
    schema::{CompiledSchema, FieldDefinition, FieldType, LocaleConfig},
};
use fraiseql_test_utils::{
    failing_adapter::FailingAdapter,
    schema_builder::{TestQueryBuilder, TestSchemaBuilder, TestTypeBuilder},
};
use serde_json::{Value, json};

const ITEM: &str = "tv_obf_item";

/// `Item { id, name, rank, label (localized), tags: [String], meta: Object }`, read by
/// `items` (a list) and `itemsConnection` (relay), both sortable.
fn item_type() -> fraiseql_core::schema::TypeDefinition {
    let mut label = FieldDefinition::nullable("label", FieldType::String);
    label.localized = true;
    TestTypeBuilder::new("Item", ITEM)
        .relay_node()
        .with_implements(&["Node"])
        .with_simple_field("id", FieldType::Id)
        .with_simple_field("pk", FieldType::Int)
        .with_simple_field("name", FieldType::String)
        .with_simple_field("rank", FieldType::Int)
        .with_field(label)
        .with_field(FieldDefinition::nullable("tags", FieldType::List(Box::new(FieldType::String))))
        .with_field(FieldDefinition::nullable("meta", FieldType::Json))
        .build()
}

fn list(name: &str) -> fraiseql_core::schema::QueryDefinition {
    let mut q = TestQueryBuilder::new(name, "Item")
        .returns_list(true)
        .with_sql_source(ITEM)
        .build();
    q.auto_params.has_order_by = true;
    q.auto_params.has_limit = true;
    q
}

fn connection(name: &str) -> fraiseql_core::schema::QueryDefinition {
    let mut q = TestQueryBuilder::new(name, "Item")
        .returns_list(true)
        .with_sql_source(ITEM)
        .relay_cursor_column("pk")
        .build();
    q.auto_params.has_order_by = true;
    q
}

fn with_node(mut schema: CompiledSchema) -> CompiledSchema {
    schema.interfaces.push(
        fraiseql_core::schema::InterfaceDefinition::new("Node")
            .with_field(FieldDefinition::new("id", FieldType::Id)),
    );
    schema.locale = Some(
        LocaleConfig::new("en-US", vec!["en-US".into(), "fr-FR".into()], BTreeMap::new(), vec![])
            .unwrap(),
    );
    schema.build_indexes();
    schema
}

/// Every query returning `Item` accepts the same keys: one entity-level enum.
fn uniform_schema() -> CompiledSchema {
    with_node(
        TestSchemaBuilder::new()
            .with_type(item_type())
            .with_query(list("items"))
            .with_query(connection("itemsConnection"))
            .build(),
    )
}

/// `itemsByRank` also accepts its native `rank_n` column, so the sets differ.
fn diverging_schema() -> CompiledSchema {
    let mut by_rank = list("itemsByRank");
    by_rank
        .native_columns
        .insert("rank_n".to_string(), fraiseql_core::schema::NativeColumn::nullable("integer"));
    // A key no spelling reaches: the engine resolves a sort key to a native column through
    // `snake_case`, which never yields `rankAlt`. Neither listed nor accepted.
    by_rank
        .native_columns
        .insert("rankAlt".to_string(), fraiseql_core::schema::NativeColumn::nullable("integer"));
    with_node(
        TestSchemaBuilder::new()
            .with_type(item_type())
            .with_query(list("items"))
            .with_query(by_rank)
            .build(),
    )
}

fn offline(schema: CompiledSchema) -> Executor {
    Executor::new(schema, Arc::new(FailingAdapter::new()))
}

async fn introspect(executor: &Executor, type_name: &str) -> Value {
    let response = executor
        .execute(
            &format!(
                r#"{{ __type(name: "{type_name}") {{ name kind enumValues {{ name }}
                   inputFields {{ name type {{ kind name ofType {{ kind name }} }} }} }} }}"#
            ),
            None,
        )
        .await
        .unwrap();
    response["data"]["__type"].clone()
}

fn enum_values(t: &Value) -> Vec<String> {
    t["enumValues"]
        .as_array()
        .unwrap_or_else(|| panic!("not an enum: {t}"))
        .iter()
        .map(|v| v["name"].as_str().unwrap().to_string())
        .collect()
}

/// The type `orderBy` publishes on root field `field`, unwrapped to the item's name.
async fn order_by_item(executor: &Executor, field: &str) -> Option<String> {
    argument_type(executor, field, "orderBy").await
}

/// The named type root field `field`'s argument `argument` publishes, under its wrappers.
async fn argument_type(executor: &Executor, field: &str, argument: &str) -> Option<String> {
    let response = executor
        .execute(
            r#"{ __type(name: "Query") { fields { name args { name type { kind name ofType { kind name ofType { name } } } } } } }"#,
            None,
        )
        .await
        .unwrap();
    let fields = response["data"]["__type"]["fields"]
        .as_array()
        .unwrap_or_else(|| panic!("{response}"))
        .clone();
    let root = fields.iter().find(|f| f["name"] == field).unwrap_or_else(|| panic!("{field}"));
    let arg = root["args"].as_array().unwrap().iter().find(|a| a["name"] == argument)?;
    // A list of the item input: the first named type under the wrappers.
    let mut t = &arg["type"];
    while t["name"].is_null() {
        t = &t["ofType"];
    }
    Some(t["name"].as_str().unwrap().to_string())
}

/// Whether `query` accepts `key`, asked of **both** readers, which must agree: the
/// document validator (GraphQL, against the enum) and the engine itself (what REST and gRPC
/// reach, with no document). A refused key says so before any statement; an accepted one
/// reaches the (failing) adapter.
async fn accepts(executor: &Executor, query: &str, key: &str) -> bool {
    let document = format!("{{ {query}(orderBy: [{{field: {key}}}]) {{ id }} }}");
    let by_document = match executor.execute(&document, None).await {
        Ok(_) => true,
        Err(e) => {
            let e = e.to_string();
            !(e.contains("not one of its members") || e.contains("Cannot sort by"))
        },
    };

    let query_def = executor.schema().queries.iter().find(|q| q.name == query).unwrap().clone();
    let mut arguments = std::collections::HashMap::new();
    arguments.insert("orderBy".to_string(), json!([{ "field": key }]));
    let direct = fraiseql_core::runtime::QueryMatch {
        query_def,
        fields: vec!["id".to_string()],
        selections: vec![],
        arguments,
        operation_name: None,
        scope_where: None,
        search_relevance: None,
        parsed_query: fraiseql_core::graphql::ParsedQuery::default(),
    };
    let by_engine = match executor.execute_query_direct(&direct, None, None, None).await {
        Ok(_) => true,
        Err(e) => !e.to_string().contains("Cannot sort by"),
    };
    assert_eq!(by_document, by_engine, "{query}.{key}: the document and the engine disagree");
    by_engine
}

#[tokio::test]
async fn one_entity_wide_enum_lists_exactly_the_sortable_fields() {
    let executor = offline(uniform_schema());
    let field = introspect(&executor, "ItemOrderByField").await;
    assert_eq!(field["kind"], json!("ENUM"), "{field}");
    assert_eq!(enum_values(&field), ["id", "pk", "name", "rank", "label"], "{field}");

    let input = introspect(&executor, "ItemOrderByInput").await;
    let typed = input["inputFields"].as_array().unwrap().iter().find(|f| f["name"] == "field");
    let typed = typed.unwrap_or_else(|| panic!("{input}"));
    assert_eq!(typed["type"]["kind"], json!("NON_NULL"), "{input}");
    assert_eq!(typed["type"]["ofType"], json!({"kind": "ENUM", "name": "ItemOrderByField"}));

    assert_eq!(order_by_item(&executor, "items").await.as_deref(), Some("ItemOrderByInput"));
    assert_eq!(
        order_by_item(&executor, "itemsConnection").await.as_deref(),
        Some("ItemOrderByInput"),
        "a relay connection is advertised `orderBy` too"
    );
}

/// Acceptance is the enum: every value is accepted, an object or a list is not, and neither
/// is a `…Translations` sibling.
#[tokio::test]
async fn the_engine_accepts_exactly_the_enum() {
    let executor = offline(uniform_schema());
    for key in ["id", "pk", "name", "rank", "label"] {
        assert!(accepts(&executor, "items", key).await, "{key}");
    }
    for key in ["tags", "meta", "labelTranslations", "nope"] {
        assert!(!accepts(&executor, "items", key).await, "{key}");
    }
}

/// Two queries over one entity accepting different keys: each its own enum and input, and
/// each refuses the other's key.
#[tokio::test]
async fn queries_that_disagree_each_get_their_own_enum() {
    let executor = offline(diverging_schema());
    assert!(introspect(&executor, "ItemOrderByField").await.is_null(), "no entity-wide enum");
    assert_eq!(
        enum_values(&introspect(&executor, "ItemsOrderByField").await),
        ["id", "pk", "name", "rank", "label"]
    );
    assert_eq!(
        enum_values(&introspect(&executor, "ItemsByRankOrderByField").await),
        ["id", "pk", "name", "rank", "label", "rank_n"]
    );
    assert_eq!(order_by_item(&executor, "items").await.as_deref(), Some("ItemsOrderByInput"));
    assert_eq!(
        order_by_item(&executor, "itemsByRank").await.as_deref(),
        Some("ItemsByRankOrderByInput")
    );
    assert!(accepts(&executor, "itemsByRank", "rank_n").await);
    assert!(!accepts(&executor, "itemsByRank", "rankAlt").await, "an unreachable native key");
    assert!(!accepts(&executor, "items", "rank_n").await, "the sibling's native key");
}

/// The SDL agrees with introspection.
#[test]
fn the_sdl_types_the_sort_key_with_the_enum() {
    let sdl = uniform_schema().raw_schema();
    assert!(sdl.contains("enum ItemOrderByField {"), "{sdl}");
    assert!(sdl.contains("field: ItemOrderByField!"), "{sdl}");
    let connection = sdl.lines().find(|l| l.trim_start().starts_with("itemsConnection("));
    let connection = connection.unwrap_or_else(|| panic!("{sdl}"));
    assert!(connection.contains("orderBy: [ItemOrderByInput"), "{connection}");
}

/// #1535: a connection publishes the `where` its runner reads, typed as a list's is, in
/// introspection and the SDL; a variable typed by it passes validation.
#[tokio::test]
async fn a_connection_publishes_the_where_it_reads() {
    let mut filtered = connection("itemsConnection");
    filtered.auto_params.has_where = true;
    let schema =
        with_node(TestSchemaBuilder::new().with_type(item_type()).with_query(filtered).build());

    let sdl = schema.raw_schema();
    let line = sdl.lines().find(|l| l.trim_start().starts_with("itemsConnection("));
    let line = line.unwrap_or_else(|| panic!("{sdl}"));
    assert!(line.contains("where: ItemWhereInput"), "{line}");

    let executor = offline(schema);
    assert_eq!(
        argument_type(&executor, "itemsConnection", "where").await.as_deref(),
        Some("ItemWhereInput")
    );
    assert_eq!(introspect(&executor, "ItemWhereInput").await["kind"], json!("INPUT_OBJECT"));

    let typed = executor
        .execute(
            "query($w: ItemWhereInput) { itemsConnection(first: 1, where: $w) { edges { node { id } } } }",
            Some(&json!({ "w": { "name": { "eq": "a" } } })),
        )
        .await;
    // The offline adapter has no relay path, so the read fails past variable validation;
    // what must not happen is the variable's type being called unknown.
    let refusal = typed.expect_err("the offline adapter serves no page").to_string();
    assert!(!refusal.contains("declares unknown type"), "{refusal}");
}

/// #1535: a connection over a type the schema cannot adjudicate publishes `where: JSON`, and
/// the SDL declares the scalar it references.
#[test]
fn an_unadjudicable_connection_publishes_where_as_json() {
    let opaque = TestTypeBuilder::new("Blob", "tv_obf_blob").relay_node().build();
    let mut filtered = TestQueryBuilder::new("blobsConnection", "Blob")
        .returns_list(true)
        .with_sql_source("tv_obf_blob")
        .relay_cursor_column("pk")
        .build();
    filtered.auto_params.has_where = true;
    let sdl = with_node(TestSchemaBuilder::new().with_type(opaque).with_query(filtered).build())
        .raw_schema();
    let line = sdl.lines().find(|l| l.trim_start().starts_with("blobsConnection("));
    assert!(line.unwrap_or_else(|| panic!("{sdl}")).contains("where: JSON"), "{sdl}");
    assert!(sdl.contains("scalar JSON"), "{sdl}");
}

/// Served on PostgreSQL: a sort by an enum value orders the rows.
#[tokio::test]
async fn a_sort_by_an_enum_value_is_served() {
    let Some(url) = fraiseql_test_support::try_database_url() else {
        eprintln!("skipping #1159: DATABASE_URL not set");
        return;
    };
    let adapter = PostgresAdapter::new(&url).await.unwrap();
    for ddl in [
        format!("DROP TABLE IF EXISTS {ITEM}"),
        format!("CREATE TABLE {ITEM} (pk bigint, data jsonb)"),
        format!(
            "INSERT INTO {ITEM} SELECT k, jsonb_build_object('id', k::text, 'pk', k, 'name', \
             'n' || (4 - k), 'rank', k) FROM generate_series(1, 3) k"
        ),
    ] {
        adapter.execute_raw_query(&ddl).await.unwrap();
    }
    let executor = Executor::new(uniform_schema(), Arc::new(adapter));
    let response = executor
        .execute("{ items(orderBy: [{field: name, direction: ASC}]) { pk } }", None)
        .await
        .unwrap();
    assert_eq!(
        response["data"]["items"],
        json!([{"pk": 3}, {"pk": 2}, {"pk": 1}]),
        "{response}"
    );
}

/// A query whose type has no key that orders meaningfully publishes no `orderBy`, and one
/// sent is an unknown argument, not a sort the engine cannot serve.
#[tokio::test]
async fn a_query_with_no_sortable_key_publishes_no_order_by() {
    let bag = TestTypeBuilder::new("Bag", "tv_obf_bag")
        .with_field(FieldDefinition::nullable("tags", FieldType::List(Box::new(FieldType::String))))
        .with_field(FieldDefinition::nullable("meta", FieldType::Json))
        .build();
    let mut bags = TestQueryBuilder::new("bags", "Bag")
        .returns_list(true)
        .with_sql_source("tv_obf_bag")
        .build();
    bags.auto_params.has_order_by = true;
    let executor =
        offline(with_node(TestSchemaBuilder::new().with_type(bag).with_query(bags).build()));
    assert_eq!(order_by_item(&executor, "bags").await, None, "no orderBy argument");
    assert!(introspect(&executor, "BagOrderByField").await.is_null());
    let err = executor
        .execute(r#"{ bags(orderBy: [{field: "tags"}]) { tags } }"#, None)
        .await
        .expect_err("refused")
        .to_string();
    assert!(err.contains("orderBy"), "{err}");
}

/// A declaration that takes the name a query's sort-key enum needs would hand the query
/// someone else's keys: the artifact is refused at load, naming the query.
#[test]
fn a_sort_key_enum_name_another_declaration_takes_is_refused_at_load() {
    let mut artifact = serde_json::to_value(diverging_schema()).unwrap();
    let enums = artifact["enums"].as_array_mut().unwrap();
    enums.retain(|e| e["name"] != "ItemsOrderByField");
    enums.push(json!({ "name": "ItemsOrderByField", "values": [{ "name": "elsewhere" }] }));
    let err = CompiledSchema::from_json(&artifact.to_string(), false)
        .expect_err("refused at load")
        .to_string();
    assert!(err.contains("ItemsOrderByField") && err.contains("`items`"), "{err}");

    // Another kind of type taking the name, beside the right enum, is refused as well.
    let mut artifact = serde_json::to_value(diverging_schema()).unwrap();
    artifact["input_types"]
        .as_array_mut()
        .unwrap()
        .push(json!({ "name": "ItemsOrderByField", "fields": [] }));
    let err = CompiledSchema::from_json(&artifact.to_string(), false)
        .expect_err("refused at load")
        .to_string();
    assert!(err.contains("ItemsOrderByField") && err.contains("`items`"), "{err}");
}

/// The suite's schemas load with no database.
#[test]
fn the_document_loads_without_a_database() {
    for schema in [uniform_schema(), diverging_schema()] {
        CompiledSchema::from_json(&serde_json::to_string(&schema).unwrap(), false)
            .unwrap_or_else(|e| panic!("the sort-key suite's schema must load: {e}"));
    }
}
