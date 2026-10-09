#![allow(clippy::unwrap_used, clippy::panic)] // Reason: test code, panics are acceptable

//! #1513: the index `compile` reports for a localized field is the one the query the
//! executor generates uses.
//!
//! 100k rows on PostgreSQL 18; the reported DDL is applied as reported; the SQL is captured
//! from the executor (its `SQL with projection` debug event), not rebuilt by the test, and
//! planned with `EXPLAIN (GENERIC_PLAN)`: the plan must read the reported index, for an `eq`
//! filter and for `ORDER BY … LIMIT`.
//!
//! **Execution engine:** `PostgreSQL` · **Infrastructure:** `DATABASE_URL` ·
//! **Parallelism:** creates and drops its own `tv_locale_index_product` table.

use std::{
    collections::BTreeMap,
    sync::{Arc, Mutex},
};

use fraiseql_core::{
    db::{DatabaseAdapter, postgres::PostgresAdapter},
    runtime::{Executor, with_request_locale},
    schema::{CompiledSchema, FieldDefinition, FieldType, LocaleConfig, LocaleSource},
};
use fraiseql_test_utils::schema_builder::{TestQueryBuilder, TestSchemaBuilder, TestTypeBuilder};
use serde_json::Value;
use tracing_subscriber::layer::SubscriberExt as _;

const TABLE: &str = "tv_locale_index_product";
const ROWS: u32 = 100_000;

fn schema() -> CompiledSchema {
    let mut name = FieldDefinition::nullable("name", FieldType::String);
    name.localized = true;
    let product = TestTypeBuilder::new("Product", TABLE)
        .with_simple_field("id", FieldType::Id)
        .with_field(name)
        .build();
    let mut products = TestQueryBuilder::new("products", "Product")
        .returns_list(true)
        .with_sql_source(TABLE)
        .build();
    products.auto_params.has_where = true;
    products.auto_params.has_order_by = true;
    products.auto_params.has_limit = true;
    let mut schema = TestSchemaBuilder::new().with_type(product).with_query(products).build();
    schema.locale = Some(
        LocaleConfig::new(
            "en-US",
            ["en-US", "fr-FR", "sv-SE"].map(String::from).to_vec(),
            BTreeMap::new(),
            vec![LocaleSource::Header {
                header: "accept-language".to_string(),
            }],
        )
        .unwrap(),
    );
    schema
}

/// The `SQL with projection` statements the adapter logs while `f` runs.
struct SqlCapture(Arc<Mutex<Vec<String>>>);

impl<S: tracing::Subscriber> tracing_subscriber::Layer<S> for SqlCapture {
    fn on_event(&self, event: &tracing::Event<'_>, _: tracing_subscriber::layer::Context<'_, S>) {
        struct Message(String);
        impl tracing::field::Visit for Message {
            fn record_debug(&mut self, field: &tracing::field::Field, value: &dyn std::fmt::Debug) {
                if field.name() == "message" {
                    self.0 = format!("{value:?}");
                }
            }
        }
        let mut message = Message(String::new());
        event.record(&mut message);
        if let Some(sql) = message.0.strip_prefix("SQL with projection = ") {
            self.0.lock().unwrap().push(sql.to_string());
        }
    }
}

/// The SQL the executor runs for `query` in `locale`.
async fn executed_sql(executor: &Executor, locale: &str, query: &str) -> String {
    let captured = Arc::new(Mutex::new(Vec::new()));
    let subscriber = tracing_subscriber::registry().with(SqlCapture(Arc::clone(&captured)));
    let _guard = tracing::subscriber::set_default(subscriber);
    with_request_locale(locale, executor.execute(query, None))
        .await
        .unwrap_or_else(|e| panic!("{query}: {e}"));
    let mut statements = captured.lock().unwrap().clone();
    assert_eq!(statements.len(), 1, "one read for {query}: {statements:?}");
    statements.remove(0)
}

/// The names of every index the generic plan of `sql` reads.
async fn indexes_read(adapter: &PostgresAdapter, sql: &str) -> Vec<String> {
    let quoted = sql.replace('\'', "''");
    let rows = adapter
        .execute_raw_query(&format!("SELECT fraiseql_test_explain('{quoted}') AS data"))
        .await
        .unwrap();
    let mut names = Vec::new();
    collect_index_names(&rows[0]["data"], &mut names);
    names
}

fn collect_index_names(plan: &Value, out: &mut Vec<String>) {
    match plan {
        Value::Object(map) => {
            if let Some(Value::String(name)) = map.get("Index Name") {
                out.push(name.clone());
            }
            map.values().for_each(|v| collect_index_names(v, out));
        },
        Value::Array(items) => items.iter().for_each(|v| collect_index_names(v, out)),
        _ => {},
    }
}

async fn seeded() -> Option<(PostgresAdapter, CompiledSchema)> {
    let pg = fraiseql_test_support::postgres().await?;
    let adapter = PostgresAdapter::new(pg.url()).await.unwrap();
    for ddl in [
        format!("DROP TABLE IF EXISTS {TABLE}"),
        format!("CREATE TABLE {TABLE} (pk bigint, data jsonb)"),
        format!(
            "INSERT INTO {TABLE} SELECT i, jsonb_build_object('id', i::text, 'name', \
             jsonb_build_object('fr-FR', 'Nom ' || i, 'en-US', 'Name ' || i, 'sv-SE', 'Namn ' \
             || i)) FROM generate_series(1, {ROWS}) AS i"
        ),
        // `EXPLAIN` is a utility statement, so it is read through a function.
        "CREATE OR REPLACE FUNCTION fraiseql_test_explain(q text) RETURNS jsonb LANGUAGE plpgsql \
         AS $$ DECLARE plan jsonb; BEGIN EXECUTE 'EXPLAIN (GENERIC_PLAN, FORMAT JSON) ' || q \
         INTO plan; RETURN plan; END $$"
            .to_string(),
    ] {
        adapter.execute_raw_query(&ddl).await.unwrap();
    }
    let schema = schema();
    for advice in schema.localized_index_report() {
        if let Some(index) = &advice.index {
            adapter.execute_raw_query(&index.ddl).await.unwrap();
        }
    }
    adapter.execute_raw_query(&format!("ANALYZE {TABLE}")).await.unwrap();
    Some((adapter, schema))
}

/// One index per (field, allowed locale), and the generated `eq` filter and `ORDER BY
/// … LIMIT` under `fr-FR` read the `fr-FR` one.
#[tokio::test]
async fn the_reported_index_is_the_one_the_query_reads() {
    let Some((adapter, schema)) = seeded().await else {
        return;
    };
    let report = schema.localized_index_report();
    assert_eq!(report.len(), 3, "one per allowed locale: {report:?}");
    let fr = report
        .iter()
        .find(|a| a.locale == "fr-FR" && a.field == "name")
        .and_then(|a| a.index.as_ref())
        .unwrap_or_else(|| panic!("{report:?}"));

    let executor = Executor::new(
        schema.clone(),
        Arc::new(PostgresAdapter::new(&fraiseql_test_support::database_url()).await.unwrap()),
    );
    let filter = executed_sql(
        &executor,
        "fr-FR",
        r#"{ products(where: { name: { eq: "Nom 500" } }) { id } }"#,
    )
    .await;
    assert!(indexes_read(&adapter, &filter).await.contains(&fr.name), "eq: {filter}");

    let sort =
        executed_sql(&executor, "fr-FR", "{ products(orderBy: { name: ASC }, limit: 10) { id } }")
            .await;
    assert!(indexes_read(&adapter, &sort).await.contains(&fr.name), "order: {sort}");

    // Projection proof: the index, as the catalog holds it, depends on no session setting.
    let definition = adapter
        .execute_raw_query(&format!(
            "SELECT jsonb_build_object('def', pg_get_indexdef('{}'::regclass)) AS data",
            fr.name
        ))
        .await
        .unwrap();
    let definition = definition[0]["data"]["def"].as_str().unwrap().to_string();
    assert!(definition.contains("fr-FR-x-icu"), "{definition}");
    assert!(!definition.contains("current_setting"), "{definition}");
}

/// Index names are deterministic and fit PostgreSQL's 63-byte identifiers.
#[test]
fn index_names_are_deterministic_and_bounded() {
    let mut long = schema();
    let long_table = format!("tv_{}", "a_very_long_product_catalogue_table".repeat(2));
    long.types[0].sql_source = long_table.into();
    let names: Vec<String> = long
        .localized_index_report()
        .iter()
        .map(|a| a.index.as_ref().unwrap().name.clone())
        .collect();
    let again: Vec<String> = long
        .localized_index_report()
        .iter()
        .map(|a| a.index.as_ref().unwrap().name.clone())
        .collect();
    assert_eq!(names, again, "deterministic");
    assert!(names.iter().all(|n| n.len() <= 63), "{names:?}");
    let distinct: std::collections::BTreeSet<_> = names.iter().collect();
    assert_eq!(distinct.len(), names.len(), "distinct per locale: {names:?}");

    // A view gets advice and no DDL: the index belongs on its base table.
    let mut view = schema();
    view.types[0].sql_source = "v_product".to_string().into();
    assert!(view.localized_index_report().iter().all(|a| a.index.is_none()));
}
