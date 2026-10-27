#![allow(clippy::unwrap_used, clippy::panic, clippy::print_stderr)] // Reason: test code — panics and skip diagnostics are acceptable

//! An aggregate of a `Decimal` measure is answered as the digits PostgreSQL computed.
//!
//! `SUM`, `AVG`, `MIN`, `MAX`, `STDDEV` and `VARIANCE` over a `numeric` column are `numeric`
//! themselves; decoded to a JSON number they went through an `f64` and lost every digit
//! past the fifteenth. A `Decimal` is a string on every path, so these are too. A count, and
//! an aggregate of a measure that is not a `Decimal`, stay numbers, and ordering by an
//! aggregate still sorts by its value, not by its text.
//!
//! **Execution engine:** `PostgreSQL` · **Infrastructure:** `DATABASE_URL` ·
//! **Parallelism:** each test creates and drops its own `tf_decimal_digits_*` table.

use std::{collections::HashMap, sync::Arc};

use fraiseql_core::{
    compiler::fact_table::{
        Additivity, DimensionColumn, DimensionPath, FactTableMetadata, FilterColumn, MeasureColumn,
        SemiAdditiveReduction, SqlType,
    },
    db::{DatabaseAdapter, postgres::PostgresAdapter},
    runtime::Executor,
    schema::CompiledSchema,
};
use serde_json::{Value, json};

fn measure(name: &str, sql_type: SqlType) -> MeasureColumn {
    MeasureColumn {
        name: name.to_string(),
        sql_type,
        nullable: false,
        additivity: Additivity::Additive,
    }
}

fn fact_table(table: &str) -> FactTableMetadata {
    FactTableMetadata {
        table_name:               table.to_string(),
        type_name:                None,
        measures:                 vec![
            measure("amount", SqlType::Decimal),
            measure("qty", SqlType::BigInt),
            measure("score", SqlType::Float),
        ],
        dimensions:               DimensionColumn {
            name:  "data".to_string(),
            paths: vec![DimensionPath {
                name:      "region".to_string(),
                json_path: "data->>'region'".to_string(),
                data_type: "string".to_string(),
            }],
        },
        denormalized_filters:     vec![FilterColumn {
            name:      "id".to_string(),
            sql_type:  SqlType::BigInt,
            indexed:   false,
            hierarchy: None,
        }],
        calendar_dimensions:      vec![],
        native_measures:          HashMap::new(),
        native_dimension_mapping: HashMap::new(),
    }
}

/// The sales `table`, and an adapter on the database holding it; `None` without a database.
async fn seeded(table: &str) -> Option<PostgresAdapter> {
    let Some(url) = fraiseql_test_support::try_database_url() else {
        eprintln!("skipping: DATABASE_URL not set");
        return None;
    };
    let adapter = PostgresAdapter::new(&url).await.unwrap();
    for ddl in [
        format!("DROP TABLE IF EXISTS {table}"),
        format!(
            "CREATE TABLE {table} (id bigint, amount numeric(30, 10), qty bigint, score numeric, data jsonb)"
        ),
        format!(
            "INSERT INTO {table} VALUES \
             (1, 12345678901234567.89, 3, 1.5, '{{\"region\": \"big\"}}'), \
             (2, 0.01, 4, 2.25, '{{\"region\": \"big\"}}'), \
             (3, 9, 1, 0, '{{\"region\": \"nine\"}}'), \
             (4, 10, 1, 0, '{{\"region\": \"ten\"}}')"
        ),
    ] {
        adapter.execute_raw_query(&ddl).await.unwrap();
    }
    Some(adapter)
}

/// What PostgreSQL prints for `expr` over the `big` region.
async fn computed(adapter: &PostgresAdapter, table: &str, expr: &str) -> String {
    let rows = adapter
        .execute_raw_query(&format!(
            "SELECT ({expr})::text AS v FROM {table} WHERE data->>'region' = 'big'"
        ))
        .await
        .unwrap();
    rows[0]["v"].as_str().unwrap().to_string()
}

async fn groups(adapter: PostgresAdapter, table: &str, mut request: Value) -> Vec<Value> {
    request["table"] = json!(table);
    let executor = Executor::new(CompiledSchema::new(), Arc::new(adapter));
    let response = executor
        .execute_aggregate_query(&request, "decimal_digits_sales_aggregate", &fact_table(table))
        .await
        .unwrap();
    response["data"]["decimal_digits_sales_aggregate"]
        .as_array()
        .unwrap_or_else(|| panic!("{response}"))
        .clone()
}

#[tokio::test]
async fn an_aggregate_of_a_decimal_measure_keeps_every_digit() {
    let table = "tf_decimal_digits_every";
    let Some(adapter) = seeded(table).await else {
        return;
    };
    let mut expected = HashMap::new();
    for function in ["sum", "avg", "min", "max", "stddev", "variance"] {
        let digits =
            computed(&adapter, table, &format!("{}(amount)", function.to_uppercase())).await;
        expected.insert(format!("amount_{function}"), digits);
    }

    let big = groups(
        adapter,
        table,
        json!({
            "groupBy": { "region": true },
            "aggregates": [
                { "amount_sum": {} }, { "amount_avg": {} }, { "amount_min": {} },
                { "amount_max": {} }, { "amount_stddev": {} }, { "amount_variance": {} },
                { "qty_sum": {} }, { "score_sum": {} }, { "count": {} }
            ],
            "where": { "region_eq": "big" }
        }),
    )
    .await;

    let group = &big[0];
    assert_eq!(group["amount_sum"], json!("12345678901234567.9000000000"), "{group}");
    for (alias, digits) in &expected {
        assert_eq!(group[alias], json!(digits), "{alias}: {group}");
    }
    assert_eq!(group["qty_sum"], json!(7), "a sum of a bigint measure stays a number: {group}");
    assert_eq!(group["score_sum"], json!(3.75), "a Float stored as numeric stays a number");
    assert_eq!(group["count"], json!(2), "a count stays a number: {group}");
}

#[tokio::test]
async fn ordering_by_a_decimal_aggregate_sorts_its_value_not_its_text() {
    let table = "tf_decimal_digits_order";
    let Some(adapter) = seeded(table).await else {
        return;
    };
    let rows = groups(
        adapter,
        table,
        json!({
            "groupBy": { "region": true },
            "aggregates": [{ "amount_sum": {} }],
            "having": { "amount_sum_lt": 100 },
            "orderBy": { "amount_sum": "DESC" }
        }),
    )
    .await;

    let sums: Vec<&Value> = rows.iter().map(|g| &g["amount_sum"]).collect();
    assert_eq!(sums, [&json!("10.0000000000"), &json!("9.0000000000")], "{rows:?}");
}

#[tokio::test]
async fn a_window_over_a_decimal_measure_keeps_every_digit() {
    let table = "tf_decimal_digits_window";
    let Some(adapter) = seeded(table).await else {
        return;
    };
    let window = |kind: &str, key: &str, alias: &str| {
        json!({
            "function": { "type": kind, key: "amount" },
            "alias": alias,
            "orderBy": [{ "field": "id", "direction": "ASC" }]
        })
    };
    let request = json!({
        "table": table,
        "select": [
            { "type": "filter", "name": "id", "alias": "id" },
            { "type": "measure", "name": "amount", "alias": "amount" },
            { "type": "measure", "name": "qty", "alias": "qty" },
            { "type": "measure", "name": "score", "alias": "score" }
        ],
        "windows": [
            window("running_sum", "measure", "running"),
            window("running_max", "measure", "highest"),
            window("lag", "field", "previous"),
            window("first_value", "field", "first"),
            json!({ "function": { "type": "running_sum", "measure": "score" }, "alias": "scored",
                    "orderBy": [{ "field": "id", "direction": "ASC" }] }),
            { "function": { "type": "row_number" }, "alias": "n",
              "orderBy": [{ "field": "id", "direction": "ASC" }] }
        ],
        "where": { "region_eq": "big" },
        "orderBy": [{ "field": "id", "direction": "ASC" }]
    });
    let executor = Executor::new(CompiledSchema::new(), Arc::new(adapter));
    let response = executor
        .execute_window_query(&request, "decimal_digits_sales_window", &fact_table(table))
        .await
        .unwrap();
    let rows = response["data"]["decimal_digits_sales_window"]
        .as_array()
        .unwrap_or_else(|| panic!("{response}"));

    let second = &rows[1];
    assert_eq!(second["amount"], json!("0.0100000000"), "{second}");
    assert_eq!(second["running"], json!("12345678901234567.9000000000"), "{second}");
    assert_eq!(second["highest"], json!("12345678901234567.8900000000"), "{second}");
    assert_eq!(second["previous"], json!("12345678901234567.8900000000"), "{second}");
    assert_eq!(second["first"], json!("12345678901234567.8900000000"), "{second}");
    assert_eq!(rows[0]["previous"], Value::Null, "{rows:?}");
    assert_eq!(second["qty"], json!(4), "a measure that is not a Decimal stays a number");
    assert_eq!(second["score"], json!(2.25), "a Float stored as numeric stays a number");
    assert_eq!(second["scored"], json!(3.75), "and so does its running sum");
    assert_eq!(second["n"], json!(2), "a rank stays a number");
}

/// A balance carried forward per account is reduced in a common table expression first;
/// the digits survive it.
#[tokio::test]
async fn a_carried_forward_decimal_balance_keeps_every_digit() {
    let Some(url) = fraiseql_test_support::try_database_url() else {
        return;
    };
    let table = "tf_decimal_digits_balance";
    let adapter = PostgresAdapter::new(&url).await.unwrap();
    for ddl in [
        format!("DROP TABLE IF EXISTS {table}"),
        format!("CREATE TABLE {table} (account_id bigint, day date, balance numeric, data jsonb)"),
        format!(
            "INSERT INTO {table} VALUES \
             (1, '2026-01-03', 12345678901234567.89, '{{}}'), \
             (2, '2026-01-04', 0.01, '{{}}'), \
             (1, '2026-02-10', 1.000000000000000001, '{{}}')"
        ),
    ] {
        adapter.execute_raw_query(&ddl).await.unwrap();
    }
    let filter = |name: &str, sql_type| FilterColumn {
        name: name.to_string(),
        sql_type,
        indexed: false,
        hierarchy: None,
    };
    let metadata = FactTableMetadata {
        measures: vec![MeasureColumn {
            additivity: Additivity::SemiAdditive {
                over:   "day".to_string(),
                using:  SemiAdditiveReduction::Last,
                entity: vec!["account_id".to_string()],
            },
            ..measure("balance", SqlType::Decimal)
        }],
        denormalized_filters: vec![
            filter("account_id", SqlType::BigInt),
            filter("day", SqlType::Date),
        ],
        ..fact_table(table)
    };
    let executor = Executor::new(CompiledSchema::new(), Arc::new(adapter));
    let response = executor
        .execute_aggregate_query(
            &json!({
                "table": table,
                "where": { "day_gte": "2026-01-01", "day_lt": "2026-03-01" },
                "groupBy": { "day_month": true },
                "aggregates": [{ "balance_sum": {} }],
                "orderBy": { "day_month": "ASC" }
            }),
            "decimal_digits_balances_aggregate",
            &metadata,
        )
        .await
        .unwrap();
    let sums: Vec<&Value> = response["data"]["decimal_digits_balances_aggregate"]
        .as_array()
        .unwrap_or_else(|| panic!("{response}"))
        .iter()
        .map(|g| &g["balance_sum"])
        .collect();
    assert_eq!(
        sums,
        [
            &json!("12345678901234567.90"),
            &json!("1.010000000000000001")
        ],
        "{response}"
    );
}
