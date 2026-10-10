#![allow(clippy::unwrap_used, clippy::panic, clippy::print_stderr)] // Reason: test code — panics and skip diagnostics are acceptable

//! #1524: an aggregate grouped by a localized dimension groups by its **label** in the request
//! locale, through the locale's fallback chain, and filters and sorts by the same label.
//!
//! Rows 1 and 2 share the French label `Pomme` and differ in English (`Apple`, `Crab apple`),
//! so the same `groupBy: { name }` is two groups in `fr-FR` and three in `en-US`. A measure
//! over a localized field is refused at load: a label is text, and text has no sum.
//!
//! **Execution engine:** `PostgreSQL` · **Infrastructure:** `DATABASE_URL` ·
//! **Parallelism:** creates and drops its own `tf_localized_sale` table.

use std::{
    collections::{BTreeMap, HashMap},
    sync::Arc,
};

use fraiseql_core::{
    compiler::fact_table::{
        DimensionColumn, DimensionPath, FactTableMetadata, MeasureColumn, SqlType,
    },
    db::{DatabaseAdapter, postgres::PostgresAdapter},
    runtime::{Executor, with_request_locale},
    schema::{CompiledSchema, FieldDefinition, FieldType, LocaleConfig, LocaleSource},
};
use fraiseql_test_utils::schema_builder::{TestSchemaBuilder, TestTypeBuilder};
use serde_json::{Value, json};

const TABLE: &str = "tf_localized_sale";

fn fact_table() -> FactTableMetadata {
    FactTableMetadata {
        table_name:               TABLE.to_string(),
        type_name:                Some("Sale".to_string()),
        measures:                 vec![MeasureColumn {
            name:     "qty".to_string(),
            sql_type: SqlType::BigInt,
            nullable: false,
        }],
        dimensions:               DimensionColumn {
            name:  "data".to_string(),
            paths: vec![DimensionPath {
                name:      "name".to_string(),
                json_path: "data->>'name'".to_string(),
                data_type: "string".to_string(),
            }],
        },
        denormalized_filters:     vec![],
        calendar_dimensions:      vec![],
        native_measures:          HashMap::new(),
        native_dimension_mapping: HashMap::new(),
    }
}

fn schema() -> CompiledSchema {
    let mut name = FieldDefinition::nullable("name", FieldType::String);
    name.localized = true;
    let sale = TestTypeBuilder::new("Sale", TABLE)
        .with_simple_field("id", FieldType::Id)
        .with_field(name)
        .with_simple_field("qty", FieldType::Int)
        .build();
    let mut schema = TestSchemaBuilder::new().with_type(sale).build();
    schema.fact_tables.insert(TABLE.to_string(), fact_table());
    schema.locale = Some(
        LocaleConfig::new(
            "en-US",
            vec!["en-US".into(), "fr-FR".into()],
            BTreeMap::new(),
            vec![LocaleSource::Header {
                header: "accept-language".to_string(),
            }],
        )
        .unwrap(),
    );
    schema
}

async fn executor() -> Option<Executor> {
    let url = fraiseql_test_support::try_database_url()?;
    let adapter = PostgresAdapter::new(&url).await.unwrap();
    for ddl in [
        format!("DROP TABLE IF EXISTS {TABLE}"),
        format!("CREATE TABLE {TABLE} (id bigint, qty bigint, data jsonb)"),
        format!(
            "INSERT INTO {TABLE} VALUES \
             (1, 1, '{{\"name\": {{\"fr-FR\": \"Pomme\", \"en-US\": \"Apple\"}}}}'), \
             (2, 2, '{{\"name\": {{\"fr-FR\": \"Pomme\", \"en-US\": \"Crab apple\"}}}}'), \
             (3, 4, '{{\"name\": {{\"fr-FR\": \"Poire\", \"en-US\": \"Pear\"}}}}')"
        ),
    ] {
        adapter.execute_raw_query(&ddl).await.unwrap();
    }
    Some(Executor::new(schema(), Arc::new(adapter)))
}

/// `(name, count)` of each group, in the order returned.
async fn groups(executor: &Executor, locale: &str, query: &Value) -> Vec<(Value, Value)> {
    let response = with_request_locale(
        locale,
        executor.execute_aggregate_query(query, "sales_aggregate", &fact_table()),
    )
    .await
    .unwrap_or_else(|e| panic!("{locale}: {e}"));
    response["data"]["sales_aggregate"]
        .as_array()
        .unwrap_or_else(|| panic!("{response}"))
        .iter()
        .map(|g| (g["name"].clone(), g["count"].clone()))
        .collect()
}

#[tokio::test]
async fn a_localized_dimension_groups_by_its_label_in_the_request_locale() {
    let Some(executor) = executor().await else {
        eprintln!("skipping #1524: DATABASE_URL not set");
        return;
    };
    let query = json!({
        "table": TABLE,
        "groupBy": { "name": true },
        "aggregates": [{ "count": {} }],
        "orderBy": { "name": "ASC" }
    });
    assert_eq!(
        groups(&executor, "fr-FR", &query).await,
        vec![(json!("Poire"), json!(1)), (json!("Pomme"), json!(2))],
        "one group per French label"
    );
    assert_eq!(
        groups(&executor, "en-US", &query).await,
        vec![
            (json!("Apple"), json!(1)),
            (json!("Crab apple"), json!(1)),
            (json!("Pear"), json!(1))
        ],
        "one group per English label"
    );
}

#[tokio::test]
async fn a_filter_on_a_localized_dimension_reads_the_label() {
    let Some(executor) = executor().await else {
        return;
    };
    let query = json!({
        "table": TABLE,
        "groupBy": { "name": true },
        "aggregates": [{ "count": {} }],
        "where": { "name_eq": "Pomme" }
    });
    assert_eq!(groups(&executor, "fr-FR", &query).await, vec![(json!("Pomme"), json!(2))]);
}

/// The compiled schema loads with a localized dimension; a measure over one is refused,
/// naming why.
#[test]
fn a_localized_dimension_loads_and_a_localized_measure_does_not() {
    let json = serde_json::to_string(&schema()).unwrap();
    CompiledSchema::from_json(&json, false).expect("a localized dimension loads");

    let mut measured = schema();
    let mut table = fact_table();
    table.measures.push(MeasureColumn {
        name:     "name".to_string(),
        sql_type: SqlType::Text,
        nullable: true,
    });
    measured.fact_tables.insert(TABLE.to_string(), table);
    let err = CompiledSchema::from_json(&serde_json::to_string(&measured).unwrap(), false)
        .unwrap_err()
        .to_string();
    assert!(err.contains("name") && err.contains("measure"), "{err}");
}

/// A window query selects, partitions and orders a localized dimension by its label.
#[tokio::test]
async fn a_window_reads_a_localized_dimension_as_its_label() {
    let Some(executor) = executor().await else {
        return;
    };
    let query = json!({
        "table": TABLE,
        "select": [{ "type": "dimension", "path": "name", "alias": "name" }],
        "windows": [{
            "function": { "type": "row_number" },
            "alias": "position",
            "partitionBy": [{ "type": "dimension", "path": "name" }],
            "orderBy": [{ "field": "name", "direction": "ASC" }]
        }],
        "orderBy": [{ "field": "name", "direction": "ASC" }]
    });
    let response = with_request_locale(
        "fr-FR",
        executor.execute_window_query(&query, "sales_window", &fact_table()),
    )
    .await
    .unwrap();
    let rows: Vec<(Value, Value)> = response["data"]["sales_window"]
        .as_array()
        .unwrap_or_else(|| panic!("{response}"))
        .iter()
        .map(|r| (r["name"].clone(), r["position"].clone()))
        .collect();
    assert_eq!(
        rows,
        vec![
            (json!("Poire"), json!(1)),
            (json!("Pomme"), json!(1)),
            (json!("Pomme"), json!(2)),
        ],
        "{response}"
    );
}

/// A window query's filter on a localized dimension compares the label.
#[tokio::test]
async fn a_window_filter_on_a_localized_dimension_reads_the_label() {
    let Some(executor) = executor().await else {
        return;
    };
    let query = json!({
        "table": TABLE,
        "select": [{ "type": "dimension", "path": "name", "alias": "name" }],
        "windows": [{ "function": { "type": "row_number" }, "alias": "position" }],
        "where": { "name_eq": "Pomme" }
    });
    let response = with_request_locale(
        "fr-FR",
        executor.execute_window_query(&query, "sales_window", &fact_table()),
    )
    .await
    .unwrap();
    let names: Vec<Value> = response["data"]["sales_window"]
        .as_array()
        .unwrap_or_else(|| panic!("{response}"))
        .iter()
        .map(|r| r["name"].clone())
        .collect();
    assert_eq!(names, vec![json!("Pomme"), json!("Pomme")], "{response}");
}

/// A window ordered by a localized dimension it does not select sorts by the label (by the
/// stored map, `{"en-US": "Apple", …}` sorts before `{"en-US": "Pear", …}`).
#[tokio::test]
async fn a_window_ordered_by_an_unselected_localized_dimension_sorts_by_its_label() {
    let Some(executor) = executor().await else {
        return;
    };
    let query = json!({
        "table": TABLE,
        "select": [{ "type": "measure", "name": "qty", "alias": "qty" }],
        "windows": [{ "function": { "type": "row_number" }, "alias": "position" }],
        "orderBy": [{ "field": "name", "direction": "ASC" }, { "field": "qty", "direction": "ASC" }]
    });
    let response = with_request_locale(
        "fr-FR",
        executor.execute_window_query(&query, "sales_window", &fact_table()),
    )
    .await
    .unwrap();
    let quantities: Vec<Value> = response["data"]["sales_window"]
        .as_array()
        .unwrap_or_else(|| panic!("{response}"))
        .iter()
        .map(|r| r["qty"].clone())
        .collect();
    assert_eq!(quantities, vec![json!(4), json!(1), json!(2)], "Poire, then Pomme: {response}");
}

/// The suite's schema loads with no database.
#[test]
fn the_document_loads_without_a_database() {
    CompiledSchema::from_json(&serde_json::to_string(&schema()).unwrap(), false)
        .unwrap_or_else(|e| panic!("the aggregate suite's schema must load: {e}"));
}
