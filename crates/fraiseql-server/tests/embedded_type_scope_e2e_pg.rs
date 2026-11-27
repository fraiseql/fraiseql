//! A type scoped by the `inject_params` it declares on itself is scoped wherever it is read:
//! embedded in another type's document, delivered by a subscription, or grouped by an
//! aggregate, whether or not a query returns it.
//!
//! `Shipment` declares `inject_params { tenant_id: jwt }` and publishes `tenant_id` in its
//! document; it is a federation entity, and no query returns it unless a test adds one.
//! `Order`, read by a scoped `orders`, embeds it. Tenants `A` and `B`, one shipment each:
//! order 1 (tenant `A`) embeds shipment `A`; order 2 (tenant `A`) embeds shipment `B`, the
//! discriminating row, which tenant `A` reads as `null`.
//!
//! The runtime tests serve the schema as built, without the load checks, so each shows the
//! gate on its own; the load tests show what a schema of each shape may declare.
//!
//! **Execution engine:** `PostgreSQL` · **Infrastructure:** `DATABASE_URL` ·
//! **Parallelism:** creates and drops its own schema and fact table → run
//! `--test-threads=1`.
#![cfg(feature = "federation")]
#![allow(clippy::unwrap_used, clippy::panic, clippy::print_stderr)] // Reason: test code

use std::{collections::HashMap, sync::Arc};

use fraiseql_core::{
    compiler::fact_table::{
        Additivity, DimensionColumn, DimensionPath, FactRowScope, FactTableMetadata, FilterColumn,
        MeasureColumn, SqlType,
    },
    db::postgres::PostgresAdapter,
    error::FraiseQLError,
    prelude::{DatabaseAdapter as _, UserId},
    runtime::{Executor, QueryMatch, RuntimeConfig},
    schema::{
        ArgumentDefinition, CompiledSchema, FederationConfig, FederationEntity, FieldDefinition,
        FieldType, InjectedParamSource, MutationDefinition, MutationOperation, NativeColumn,
        QueryDefinition, SubscriptionDefinition, TypeDefinition,
    },
    security::SecurityContext,
    types::TenantId,
};
use fraiseql_test_support::try_database_url;
use futures::StreamExt as _;
use serde_json::{Value, json};

const SCHEMA: &str = "p_embedded_type_scope";
const FACT: &str = "tf_embedded_type_scope";
const TENANT_A: &str = "00000000-0000-0000-0000-00000000000a";
const TENANT_B: &str = "00000000-0000-0000-0000-00000000000b";

async fn provision(adapter: &PostgresAdapter) {
    let stmts = [
        "CREATE SCHEMA IF NOT EXISTS app".to_string(),
        "DO $$ BEGIN CREATE TYPE app.mutation_error_class AS ENUM ('validation','conflict',\
         'not_found','unauthorized','forbidden','internal','transaction_failed','timeout',\
         'rate_limited','service_unavailable'); EXCEPTION WHEN duplicate_object THEN NULL; END $$;"
            .to_string(),
        "DO $$ BEGIN CREATE TYPE app.mutation_response AS (succeeded BOOLEAN, state_changed \
         BOOLEAN, error_class app.mutation_error_class, status_detail TEXT, http_status \
         SMALLINT, message TEXT, entity_id UUID, entity_type TEXT, entity JSONB, \
         updated_fields TEXT[], cascade JSONB, error_detail JSONB, metadata JSONB); \
         EXCEPTION WHEN duplicate_object THEN NULL; END $$;"
            .to_string(),
        format!("DROP SCHEMA IF EXISTS {SCHEMA} CASCADE"),
        format!("DROP TABLE IF EXISTS public.{FACT}"),
        format!("CREATE SCHEMA {SCHEMA}"),
        format!(
            "CREATE TABLE {SCHEMA}.tb_shipment (id int PRIMARY KEY, tenant_id uuid NOT NULL, \
             label text NOT NULL)"
        ),
        format!(
            "INSERT INTO {SCHEMA}.tb_shipment VALUES (1, '{TENANT_A}', 'a-shipment'), (2, \
             '{TENANT_B}', 'b-shipment')"
        ),
        format!(
            "CREATE TABLE {SCHEMA}.tb_order (id int PRIMARY KEY, tenant_id uuid NOT NULL, \
             fk_shipment int NOT NULL)"
        ),
        format!("INSERT INTO {SCHEMA}.tb_order VALUES (1, '{TENANT_A}', 1), (2, '{TENANT_A}', 2)"),
        format!(
            "CREATE VIEW {SCHEMA}.v_shipment AS SELECT id, tenant_id, jsonb_build_object('id', \
             id, 'tenant_id', tenant_id, 'label', label) AS data FROM {SCHEMA}.tb_shipment"
        ),
        format!(
            "CREATE VIEW {SCHEMA}.v_order AS SELECT o.id, o.tenant_id, jsonb_build_object('id', \
             o.id, 'tenant_id', o.tenant_id, 'shipment', s.data) AS data FROM {SCHEMA}.tb_order \
             o JOIN {SCHEMA}.v_shipment s ON s.id = o.fk_shipment"
        ),
        // One fact row per order, all tenant `A`'s, each carrying the order's shipment.
        format!(
            "CREATE TABLE public.{FACT} (revenue numeric NOT NULL, tenant_id text NOT NULL, data \
             jsonb NOT NULL)"
        ),
        format!(
            "INSERT INTO public.{FACT} SELECT 1, o.tenant_id::text, jsonb_build_object('shipment', \
             s.data) FROM {SCHEMA}.tb_order o JOIN {SCHEMA}.v_shipment s ON s.id = o.fk_shipment"
        ),
        format!("CREATE TABLE {SCHEMA}.tb_write (id int)"),
        format!(
            "CREATE FUNCTION {SCHEMA}.fn_touch_order(p_id int) RETURNS app.mutation_response \
             LANGUAGE plpgsql AS $$ DECLARE v app.mutation_response; BEGIN INSERT INTO \
             {SCHEMA}.tb_write VALUES (p_id); v.succeeded := true; v.state_changed := true; \
             v.entity_type := 'Order'; v.entity := (SELECT data FROM {SCHEMA}.v_order WHERE id \
             = p_id); RETURN v; END $$"
        ),
    ];
    for stmt in stmts {
        adapter.execute_raw_query(&stmt).await.unwrap_or_else(|e| panic!("{stmt}: {e}"));
    }
}

fn jwt(claim: &str) -> InjectedParamSource {
    InjectedParamSource::Jwt(claim.to_string())
}

/// A list query over `view`, scoped by `scope` (column → source).
fn read(
    name: &str,
    return_type: &str,
    view: &str,
    scope: &[(&str, InjectedParamSource)],
) -> QueryDefinition {
    let mut query = QueryDefinition::new(name, return_type)
        .returning_list()
        .with_sql_source(format!("{SCHEMA}.{view}"));
    for (column, source) in scope {
        query.inject_params.insert((*column).to_string(), source.clone());
        query.native_columns.insert(
            (*column).to_string(),
            NativeColumn {
                pg_type:  "uuid".to_string(),
                not_null: false,
            },
        );
    }
    query.auto_params.has_where = true;
    query.auto_params.has_order_by = true;
    query
}

/// Which query, if any, returns `Shipment`.
#[derive(Clone, Copy)]
enum Shipments {
    /// None does.
    NoQuery,
    /// `shipments`, scoped as the type is.
    Agreeing,
    /// `shipments`, unscoped.
    Unscoped,
    /// `shipments`, scoped by another column.
    OtherColumn,
}

/// How a fact table read as `OrderFact` reaches the shipment its rows carry.
#[derive(Clone, Copy, PartialEq, Eq)]
enum FactDimension {
    /// No fact table.
    None,
    /// A dimension named `shipment`, at `data->>'shipment'`: the embedded document whole.
    Whole,
    /// A dimension named `lbl`, at `data->'shipment'->>'label'`: a key inside it, under
    /// another name. `OrderFact` declares both `shipment` and `lbl`.
    Inside,
}

/// How `orders`, the read of `Order`, is scoped.
#[derive(Clone, Copy)]
enum OrdersScope {
    /// By `tenant_id` from the token, as `Shipment` is.
    Tenant,
    /// Not at all.
    Unscoped,
    /// By `owner_id`, a column `Shipment` is not scoped by.
    Owner,
}

/// What a test varies about the schema.
#[derive(Clone, Copy)]
struct Shape {
    shipments:    Shipments,
    orders_scope: OrdersScope,
    /// `Order` embeds `shipment`.
    order_embeds: bool,
    /// `Order.shipment` declared `scoped_like_parent`.
    declared:     bool,
    fact_table:   FactDimension,
}

const SERVED: Shape = Shape {
    shipments:    Shipments::NoQuery,
    orders_scope: OrdersScope::Tenant,
    order_embeds: true,
    declared:     true,
    fact_table:   FactDimension::None,
};

const WITH_FACT_TABLE: Shape = Shape {
    fact_table: FactDimension::Whole,
    ..SERVED
};

const WITH_A_KEY_INSIDE: Shape = Shape {
    fact_table: FactDimension::Inside,
    ..SERVED
};

fn order_fact_table(dimension: FactDimension) -> FactTableMetadata {
    let (name, json_path) = match dimension {
        FactDimension::Whole => ("shipment", "data->>'shipment'"),
        _ => ("lbl", "data->'shipment'->>'label'"),
    };
    FactTableMetadata {
        table_name:               FACT.to_string(),
        type_name:                Some("OrderFact".to_string()),
        row_scope:                FactRowScope::Server,
        inject_params:            fraiseql_core::indexmap::IndexMap::default(),
        measures:                 vec![MeasureColumn {
            name:       "revenue".to_string(),
            sql_type:   SqlType::Decimal,
            nullable:   false,
            additivity: Additivity::Additive,
            column:     None,
        }],
        dimensions:               DimensionColumn {
            name:  "data".to_string(),
            paths: vec![DimensionPath {
                name:      name.to_string(),
                json_path: json_path.to_string(),
                data_type: "string".to_string(),
            }],
        },
        denormalized_filters:     vec![FilterColumn {
            name:      "tenant_id".to_string(),
            sql_type:  SqlType::Text,
            indexed:   true,
            hierarchy: None,
        }],
        calendar_dimensions:      vec![],
        native_dimension_mapping: HashMap::new(),
        grain:                    None,
    }
}

/// The schema as built, before any load check.
fn built(shape: Shape) -> CompiledSchema {
    let tenant = [("tenant_id", jwt("tenant_id"))];
    let mut schema = CompiledSchema::new();
    schema.federation = Some(FederationConfig {
        enabled: true,
        version: Some("v2".to_string()),
        entities: vec![FederationEntity {
            name: "Shipment".to_string(),
            key_fields: vec!["id".to_string()],
            ..Default::default()
        }],
        ..Default::default()
    });
    let mut shipment = TypeDefinition::new("Shipment", format!("{SCHEMA}.v_shipment"));
    shipment.fields = vec![
        FieldDefinition::new("id", FieldType::Int),
        FieldDefinition::new("tenantId", FieldType::Uuid),
        FieldDefinition::new("label", FieldType::String),
    ];
    shipment.inject_params.insert("tenant_id".to_string(), jwt("tenant_id"));
    let embeds = || {
        let mut field = FieldDefinition::nullable("shipment", FieldType::Object("Shipment".into()));
        field.scoped_like_parent = shape.declared;
        field
    };
    // `OrderFact` is read only through its fact table, no query's document: nothing to declare.
    let fact_embeds =
        || FieldDefinition::nullable("shipment", FieldType::Object("Shipment".into()));
    let mut order = TypeDefinition::new("Order", format!("{SCHEMA}.v_order"));
    order.fields = vec![
        FieldDefinition::new("id", FieldType::Int),
        FieldDefinition::new("tenantId", FieldType::Uuid),
    ];
    if shape.order_embeds {
        order.fields.push(embeds());
    }
    schema.types.extend([shipment, order]);
    let owner = [("owner_id", jwt("sub"))];
    let orders_scope: &[(&str, InjectedParamSource)] = match shape.orders_scope {
        OrdersScope::Tenant => &tenant,
        OrdersScope::Unscoped => &[],
        OrdersScope::Owner => &owner,
    };
    schema.queries.push(read("orders", "Order", "v_order", orders_scope));
    match shape.shipments {
        Shipments::NoQuery => {},
        Shipments::Agreeing => {
            schema.queries.push(read("shipments", "Shipment", "v_shipment", &tenant));
        },
        Shipments::Unscoped => {
            schema.queries.push(read("shipments", "Shipment", "v_shipment", &[]));
        },
        Shipments::OtherColumn => {
            schema.queries.push(read(
                "shipments",
                "Shipment",
                "v_shipment",
                &[("owner_id", jwt("sub"))],
            ));
        },
    }
    let mut touch = MutationDefinition::new("touchOrder", "Order");
    touch.sql_source = Some(format!("{SCHEMA}.fn_touch_order"));
    touch.operation = MutationOperation::Update {
        table: "tb_order".to_string(),
    };
    touch.arguments = vec![ArgumentDefinition::new("id", FieldType::Int)];
    schema.mutations.push(touch);
    schema
        .subscriptions
        .push(SubscriptionDefinition::new("shipmentChanged", "Shipment"));
    schema.subscriptions.push(SubscriptionDefinition::new("orderChanged", "Order"));
    if shape.fact_table != FactDimension::None {
        let mut fact = TypeDefinition::new("OrderFact", FACT);
        fact.fields = vec![
            FieldDefinition::new("revenue", FieldType::Float),
            FieldDefinition::new("tenant_id", FieldType::String),
        ];
        match shape.fact_table {
            FactDimension::Whole => fact.fields.push(fact_embeds()),
            FactDimension::Inside => {
                fact.fields.push(fact_embeds());
                fact.fields.push(FieldDefinition::nullable("lbl", FieldType::String));
            },
            FactDimension::None => {},
        }
        fact.inject_params.insert("tenant_id".to_string(), jwt("tenant_id"));
        schema.types.push(fact);
        schema.add_fact_table(FACT.to_string(), order_fact_table(shape.fact_table));
    }
    schema.build_indexes();
    schema
}

/// Load `shape` as a server loads a compiled artifact.
fn load(shape: Shape) -> Result<CompiledSchema, FraiseQLError> {
    CompiledSchema::from_json(&serde_json::to_string(&built(shape)).unwrap(), false)
}

fn tenant_a() -> SecurityContext {
    SecurityContext {
        user_id:          UserId::from("u-a"),
        roles:            vec![],
        tenant_id:        Some(TenantId::from(TENANT_A)),
        scopes:           vec![],
        attributes:       HashMap::from([
            ("tenant_id".to_string(), json!(TENANT_A)),
            ("sub".to_string(), json!("u-a")),
        ]),
        request_id:       "req-embedded-type-scope".to_string(),
        ip_address:       None,
        authenticated_at: fraiseql_time::Timestamp::now(),
        expires_at:       fraiseql_time::Timestamp::now()
            + fraiseql_time::jiff::SignedDuration::from_hours(1),
        issuer:           None,
        audience:         None,
        email:            None,
        display_name:     None,
    }
}

fn executor_over<A: fraiseql_core::db::Writer + 'static>(
    schema: CompiledSchema,
    adapter: Arc<A>,
) -> Executor {
    let config = RuntimeConfig::from_compiled_schema(&schema).unwrap();
    Executor::with_config(schema, adapter, config)
}

async fn rig(shape: Shape) -> Option<(Executor, Arc<PostgresAdapter>)> {
    let url = try_database_url()?;
    let adapter = Arc::new(PostgresAdapter::new(&url).await.unwrap());
    provision(&adapter).await;
    Some((executor_over(built(shape), adapter.clone()), adapter))
}

macro_rules! rig_or_skip {
    ($shape:expr) => {
        match rig($shape).await {
            Some(rig) => rig,
            None => {
                eprintln!("skipping: DATABASE_URL not set");
                return;
            },
        }
    };
}

async fn as_tenant_a(executor: &Executor, document: &str) -> Result<Value, FraiseQLError> {
    executor.execute_with_security(document, None, &tenant_a()).await
}

const ORDERS_WITH_SHIPMENT: &str = "{ orders { id shipment { label } } }";

/// Order 1's shipment is tenant `A`'s; order 2's is tenant `B`'s, and reads as `null`.
fn as_tenant_a_reads_them() -> Value {
    json!([
        { "id": 1, "shipment": { "label": "a-shipment" } },
        { "id": 2, "shipment": null }
    ])
}

fn aggregate_root() -> String {
    format!("{}_aggregate", FACT.trim_start_matches("tf_"))
}

fn window_root() -> String {
    format!("{}_window", FACT.trim_start_matches("tf_"))
}

// ── Reads ─────────────────────────────────────────────────────────────────────────────

#[tokio::test]
async fn a_nested_read_serves_another_tenants_embedded_document_as_null() {
    let (executor, _) = rig_or_skip!(SERVED);
    let out = as_tenant_a(&executor, ORDERS_WITH_SHIPMENT).await.unwrap();
    assert_eq!(out["data"]["orders"], as_tenant_a_reads_them(), "{out}");
}

/// A query reading the type without its scope does not drop the type's own.
#[tokio::test]
async fn a_nested_read_keeps_the_types_own_scope_beside_an_unscoped_query() {
    let (executor, _) = rig_or_skip!(Shape {
        shipments: Shipments::Unscoped,
        ..SERVED
    });
    let out = as_tenant_a(&executor, ORDERS_WITH_SHIPMENT).await.unwrap();
    assert_eq!(out["data"]["orders"], as_tenant_a_reads_them(), "{out}");
}

/// Control: with a query scoped as the type is, the same read is gated.
#[tokio::test]
async fn control_a_scoped_query_gates_the_nested_read() {
    let (executor, _) = rig_or_skip!(Shape {
        shipments: Shipments::Agreeing,
        ..SERVED
    });
    let out = as_tenant_a(&executor, ORDERS_WITH_SHIPMENT).await.unwrap();
    assert_eq!(out["data"]["orders"], as_tenant_a_reads_them(), "{out}");
}

#[tokio::test]
async fn a_write_payload_serves_another_tenants_embedded_document_as_null() {
    let (executor, adapter) = rig_or_skip!(SERVED);
    let out = as_tenant_a(&executor, "mutation { touchOrder(id: 2) { id shipment { label } } }")
        .await
        .unwrap();
    assert_eq!(out["data"]["touchOrder"], json!({ "id": 2, "shipment": null }), "{out}");
    let writes = adapter
        .execute_raw_query(&format!("SELECT id FROM {SCHEMA}.tb_write"))
        .await
        .unwrap()
        .len();
    assert_eq!(writes, 1, "the write ran once");
}

#[tokio::test]
async fn a_relation_filter_cannot_match_another_tenants_embedded_document() {
    let (executor, _) = rig_or_skip!(SERVED);
    for (label, expected) in [
        ("a-shipment", json!([{ "id": 1 }])),
        ("b-shipment", json!([])),
    ] {
        let out = as_tenant_a(
            &executor,
            &format!(r#"{{ orders(where: {{shipment: {{label: {{eq: "{label}"}}}}}}) {{ id }} }}"#),
        )
        .await
        .unwrap();
        assert_eq!(out["data"]["orders"], expected, "{label}: {out}");
    }
}

/// An embedded document is not a sort key, so no ordering reads another tenant's shipment.
#[tokio::test]
async fn an_embedded_document_is_not_a_sort_key() {
    let (executor, _) = rig_or_skip!(SERVED);
    for document in [
        "{ orders(orderBy: {shipment: ASC}) { id } }",
        r#"{ orders(orderBy: [{field: "shipment.label", direction: DESC}]) { id } }"#,
    ] {
        let out = as_tenant_a(&executor, document).await;
        assert!(matches!(out, Err(FraiseQLError::Validation { .. })), "{document}: {out:?}");
    }
}

/// Control: `_entities` reads the type under its own scope.
#[tokio::test]
async fn control_entities_reads_the_type_under_its_own_scope() {
    let (executor, _) = rig_or_skip!(SERVED);
    let document =
        "query($r: [_Any!]!) { _entities(representations: $r) { ... on Shipment { id label } } }";
    for (id, expected) in [
        (2, json!([null])),
        (1, json!([{ "id": 1, "label": "a-shipment", "__typename": "Shipment" }])),
    ] {
        let vars = json!({
            "r": [{ "__typename": "Shipment", "id": id }],
            "representations": [{ "__typename": "Shipment", "id": id }],
        });
        let out = executor
            .execute_with_security(document, Some(&vars), &tenant_a())
            .await
            .unwrap();
        assert_eq!(out["data"]["_entities"], expected, "{id}: {out}");
    }
}

// ── REST ──────────────────────────────────────────────────────────────────────────────

fn rest_orders(executor: &Executor) -> QueryMatch {
    let schema = executor.schema();
    let definition = schema.queries.iter().find(|q| q.name == "orders").unwrap().clone();
    QueryMatch::from_operation(
        definition,
        vec!["id".to_string(), "shipment".to_string()],
        HashMap::new(),
        schema.find_type("Order"),
    )
    .unwrap()
}

#[tokio::test]
async fn a_rest_selection_serves_another_tenants_embedded_document_as_null() {
    let (executor, _) = rig_or_skip!(SERVED);
    let out = executor
        .execute_query_direct(&rest_orders(&executor), None, Some(&tenant_a()), None)
        .await
        .unwrap();
    let labels: Vec<Value> = out["data"]["orders"]
        .as_array()
        .unwrap()
        .iter()
        .map(|o| o["shipment"]["label"].clone())
        .collect();
    assert_eq!(labels, [json!("a-shipment"), Value::Null], "{out}");
    assert_eq!(out["data"]["orders"][1]["shipment"], Value::Null, "{out}");
}

/// A streamed read cannot gate a nested level, so it refuses one it would have to.
#[tokio::test]
async fn a_streamed_rest_selection_of_an_embedded_scoped_document_is_refused() {
    let (executor, _) = rig_or_skip!(SERVED);
    match executor
        .stream_query_direct(rest_orders(&executor), None, Some(tenant_a()))
        .await
    {
        Err(FraiseQLError::Unsupported { .. }) => {},
        Err(other) => panic!("refused, but not as unsupported: {other:?}"),
        Ok(stream) => {
            let rows: Vec<_> = stream.collect().await;
            panic!("streamed: {rows:?}");
        },
    }
}

// ── Subscriptions ─────────────────────────────────────────────────────────────────────

/// Each subscription below is served as a subscriber is: from its own read of the row an
/// event names, under its scope. Shipment `1` is tenant `A`'s, `2` tenant `B`'s; order `2`
/// is tenant `A`'s and embeds shipment `2`.
#[tokio::test]
async fn a_subscription_to_the_type_suppresses_another_tenants_event() {
    let (executor, _) = rig_or_skip!(SERVED);
    let document =
        fraiseql_core::graphql::parse_query("subscription { shipmentChanged { id label } }")
            .unwrap();
    let plan = executor.plan_subscription(&document, None, Some(&tenant_a())).unwrap();
    assert!(plan.read("2").await.unwrap().is_none(), "read tenant B's shipment");
    let own = plan.read("1").await.unwrap().expect("tenant A reads its own shipment");
    assert!(plan.deliver(&own, false).is_some(), "tenant A's own shipment is delivered");
}

#[tokio::test]
async fn a_type_stream_suppresses_another_tenants_event() {
    let (executor, _) = rig_or_skip!(SERVED);
    let plan = executor.plan_type_stream("Shipment", Some(&tenant_a())).unwrap();
    assert!(plan.read("2").await.unwrap().is_none(), "streamed tenant B's shipment");
    assert!(plan.read("1").await.unwrap().is_some(), "tenant A's own shipment is streamed");
}

#[tokio::test]
async fn a_subscription_serves_another_tenants_embedded_document_as_null() {
    let (executor, _) = rig_or_skip!(SERVED);
    let document = fraiseql_core::graphql::parse_query(
        "subscription { orderChanged { id shipment { label } } }",
    )
    .unwrap();
    let plan = executor.plan_subscription(&document, None, Some(&tenant_a())).unwrap();
    let order_2 = plan.read("2").await.unwrap().expect("tenant A reads its order 2");
    let served = plan.deliver(&order_2, false).expect("tenant A's order is delivered");
    assert_eq!(served.data, json!({ "id": 2, "shipment": null }));
}

// ── Aggregates and windows ────────────────────────────────────────────────────────────

/// A group key is a value, not a row: there is no row to filter, so grouping by an embedded
/// scoped document is refused.
#[tokio::test]
async fn an_aggregate_cannot_group_by_an_embedded_scoped_document() {
    let (executor, _) = rig_or_skip!(WITH_FACT_TABLE);
    let vars = json!({ "groupBy": { "shipment": true }, "aggregates": [{ "count": true }] });
    let out = executor
        .execute_with_security(
            &format!("{{ {} {{ shipment count }} }}", aggregate_root()),
            Some(&vars),
            &tenant_a(),
        )
        .await;
    assert!(matches!(out, Err(FraiseQLError::Authorization { .. })), "{out:?}");
}

#[tokio::test]
async fn a_window_cannot_select_an_embedded_scoped_document() {
    let (executor, _) = rig_or_skip!(WITH_FACT_TABLE);
    let vars = json!({
        "select": [{ "type": "dimension", "path": "shipment", "alias": "shipment" }],
        "windows": [{ "function": { "type": "row_number" }, "alias": "n" }]
    });
    let out = executor
        .execute_with_security(&format!("{{ {} }}", window_root()), Some(&vars), &tenant_a())
        .await;
    assert!(matches!(out, Err(FraiseQLError::Authorization { .. })), "{out:?}");
}

/// A key inside the embedded document, whatever the dimension is called.
#[tokio::test]
async fn an_aggregate_cannot_group_by_a_key_inside_an_embedded_scoped_document() {
    let (executor, _) = rig_or_skip!(WITH_A_KEY_INSIDE);
    let vars = json!({ "groupBy": { "lbl": true }, "aggregates": [{ "count": true }] });
    let out = executor
        .execute_with_security(
            &format!("{{ {} {{ lbl count }} }}", aggregate_root()),
            Some(&vars),
            &tenant_a(),
        )
        .await;
    assert!(matches!(out, Err(FraiseQLError::Authorization { .. })), "{out:?}");
}

#[tokio::test]
async fn a_window_cannot_select_a_key_inside_an_embedded_scoped_document() {
    let (executor, _) = rig_or_skip!(WITH_A_KEY_INSIDE);
    let vars = json!({
        "select": [{ "type": "dimension", "path": "lbl", "alias": "lbl" }],
        "windows": [{ "function": { "type": "row_number" }, "alias": "n" }]
    });
    let out = executor
        .execute_with_security(&format!("{{ {} }}", window_root()), Some(&vars), &tenant_a())
        .await;
    assert!(matches!(out, Err(FraiseQLError::Authorization { .. })), "{out:?}");
}

/// Control: the same fact table, read without the embedded document, is served.
#[tokio::test]
async fn control_an_aggregate_over_the_fact_table_is_served() {
    let (executor, _) = rig_or_skip!(WITH_FACT_TABLE);
    let vars = json!({ "aggregates": [{ "count": true }] });
    let out = executor
        .execute_with_security(
            &format!("{{ {} {{ count }} }}", aggregate_root()),
            Some(&vars),
            &tenant_a(),
        )
        .await
        .unwrap();
    assert_eq!(out["data"][aggregate_root()][0]["count"], json!(2), "{out}");
}

// ── Loading ───────────────────────────────────────────────────────────────────────────

fn refusal(shape: Shape) -> String {
    match load(shape) {
        Ok(_) => panic!("the schema loaded"),
        Err(error) => error.to_string(),
    }
}

fn assert_names(refusal: &str, needles: &[&str]) {
    for needle in needles {
        assert!(refusal.contains(needle), "names `{needle}`: {refusal}");
    }
}

/// The database tests rig the schema as built. Every shape a server can serve loads; the
/// others (an unscoped `shipments`, a fact table reaching the shipment) are refused at
/// load, pinned below, and rigged only to drive the request-time refusal behind that.
#[test]
fn the_document_loads_without_a_database() {
    let agreeing = Shape {
        shipments: Shipments::Agreeing,
        ..SERVED
    };
    for shape in [SERVED, agreeing] {
        load(shape).unwrap_or_else(|e| panic!("refused: {e}"));
    }
    let unscoped = Shape {
        shipments: Shipments::Unscoped,
        ..SERVED
    };
    for shape in [unscoped, WITH_FACT_TABLE, WITH_A_KEY_INSIDE] {
        assert!(load(shape).is_err(), "a shape served only as built must not load");
    }
}

#[test]
fn a_declared_embedding_under_a_parent_scoped_alike_loads() {
    load(SERVED).unwrap_or_else(|e| panic!("refused: {e}"));
}

#[test]
fn an_undeclared_embedding_of_the_type_is_refused() {
    let refusal = refusal(Shape {
        declared: false,
        ..SERVED
    });
    assert_names(&refusal, &["Order.shipment", "tenant_id", "scoped_like_parent"]);
}

#[test]
fn an_embedding_under_an_unscoped_parent_is_refused() {
    for declared in [false, true] {
        let refusal = refusal(Shape {
            orders_scope: OrdersScope::Unscoped,
            declared,
            ..SERVED
        });
        assert_names(&refusal, &["Order.shipment", "tenant_id"]);
    }
}

/// The declaration is a claim the parent's scope has to bear out: `orders` scoped by
/// another column does not carry `Shipment`'s.
#[test]
fn a_declared_embedding_under_a_parent_scoped_otherwise_is_refused_naming_both() {
    let refusal = refusal(Shape {
        orders_scope: OrdersScope::Owner,
        ..SERVED
    });
    assert_names(&refusal, &["Order.shipment", "Shipment", "orders", "tenant_id"]);
}

#[test]
fn a_query_reading_the_type_without_its_scope_is_refused_naming_both() {
    let refusal = refusal(Shape {
        shipments: Shipments::Unscoped,
        order_embeds: false,
        ..SERVED
    });
    assert_names(&refusal, &["shipments", "Shipment", "tenant_id"]);
}

#[test]
fn a_query_reading_the_type_under_another_scope_is_refused_naming_both() {
    let refusal = refusal(Shape {
        shipments: Shipments::OtherColumn,
        order_embeds: false,
        ..SERVED
    });
    assert_names(&refusal, &["shipments", "Shipment", "tenant_id", "owner_id"]);
}

#[test]
fn a_query_reading_the_type_under_its_own_scope_loads() {
    load(Shape {
        shipments: Shipments::Agreeing,
        order_embeds: false,
        ..SERVED
    })
    .unwrap_or_else(|e| panic!("refused: {e}"));
}

#[test]
fn a_fact_table_dimension_on_an_embedded_scoped_document_is_refused() {
    let refusal = refusal(Shape {
        order_embeds: false,
        ..WITH_FACT_TABLE
    });
    assert_names(&refusal, &[FACT, "shipment", "Shipment"]);
}

/// Keyed on where the dimension reads, not what it is called.
#[test]
fn a_fact_table_dimension_inside_an_embedded_scoped_document_is_refused() {
    let refusal = refusal(Shape {
        order_embeds: false,
        ..WITH_A_KEY_INSIDE
    });
    assert_names(&refusal, &[FACT, "lbl", "shipment", "Shipment"]);
}
