#![allow(clippy::unwrap_used, clippy::panic, clippy::print_stderr)] // Reason: test code — panics and skip diagnostics are acceptable

//! A fact table's 64-bit results are answered exactly through its aggregate root.
//!
//! PostgreSQL computes `SUM` of an `int4` in `int8`, keeps an `int8` under `MIN` and `MAX`,
//! and computes `SUM` of an `int8` in `numeric`. Published as `Int`, the first two were a
//! field error past 2^31; published as `Float`, the last went through an `f64` and lost
//! every digit past the fifteenth. An `int8` result is a `BigInt` and `SUM` of one a
//! `Decimal`, each served as the string of its digits, and an `int8` group key is a
//! `BigInt` a filter compares exactly. A count, and `MIN`/`MAX` of an `int4`, stay `Int`.
//!
//! **Execution engine:** `PostgreSQL` · **Infrastructure:** `DATABASE_URL` ·
//! **Parallelism:** creates and drops its own `tf_big_int_sale` table.

use std::{collections::HashMap, sync::Arc};

use fraiseql_core::{
    compiler::fact_table::{
        Additivity, DimensionColumn, FactRowScope, FactTableMetadata, FilterColumn, MeasureColumn,
        SqlType,
    },
    db::{DatabaseAdapter, postgres::PostgresAdapter},
    runtime::Executor,
    schema::CompiledSchema,
};
use serde_json::{Value, json};

const TABLE: &str = "tf_big_int_sale";

/// 2^53 + 1, the first integer an `f64` cannot hold, and 2^53, the `f64` it rounds to.
const ODD: &str = "9007199254740993";
const EVEN: &str = "9007199254740992";

fn measure(name: &str, sql_type: SqlType) -> MeasureColumn {
    MeasureColumn {
        name: name.to_string(),
        sql_type,
        nullable: false,
        additivity: Additivity::Additive,
    }
}

fn filter(name: &str, sql_type: SqlType) -> FilterColumn {
    FilterColumn {
        name: name.to_string(),
        sql_type,
        indexed: false,
        hierarchy: None,
    }
}

fn fact_table() -> FactTableMetadata {
    FactTableMetadata {
        table_name:               TABLE.to_string(),
        type_name:                None,
        row_scope:                FactRowScope::Server,
        inject_params:            indexmap::IndexMap::default(),
        measures:                 vec![
            measure("qty", SqlType::Int),
            measure("units", SqlType::BigInt),
        ],
        dimensions:               DimensionColumn {
            name:  "data".to_string(),
            paths: vec![],
        },
        denormalized_filters:     vec![
            filter("account", SqlType::BigInt),
            filter("shelf", SqlType::Int),
        ],
        calendar_dimensions:      vec![],
        native_measures:          HashMap::new(),
        native_dimension_mapping: HashMap::new(),
    }
}

/// Account `ODD`: two rows of `qty` 2^31 - 1, whose sum no `Int` holds, and two of `units`
/// `i64::MAX`, whose sum no `int8` holds. Account `EVEN`: one small row.
static PROVISIONED: tokio::sync::OnceCell<()> = tokio::sync::OnceCell::const_new();

async fn executor() -> Option<Executor> {
    let url = fraiseql_test_support::try_database_url()?;
    let adapter = PostgresAdapter::new(&url).await.unwrap();
    PROVISIONED.get_or_init(|| provision(&adapter)).await;
    let mut schema = CompiledSchema::new();
    schema.fact_tables.insert(TABLE.to_string(), fact_table());
    // The roots and their row types are published, as in a loaded schema: rows are completed.
    schema.build_indexes();
    Some(Executor::new(schema, Arc::new(adapter)))
}

async fn provision(adapter: &PostgresAdapter) {
    for ddl in [
        format!("DROP TABLE IF EXISTS {TABLE}"),
        format!(
            "CREATE TABLE {TABLE} (id int, account bigint, shelf int, qty int, units bigint, \
             data jsonb)"
        ),
        format!(
            "INSERT INTO {TABLE} VALUES \
             (1, {ODD}, 1, 2147483647, 9223372036854775807, '{{}}'), \
             (2, {ODD}, 2, 2147483647, 9223372036854775807, '{{}}'), \
             (3, {EVEN}, 3, 1, -5, '{{}}')"
        ),
    ] {
        adapter.execute_raw_query(&ddl).await.unwrap();
    }
}

/// The rows of the `big_int_sale_aggregate` root of a response with no errors, by account.
fn groups(response: &Value) -> Vec<Value> {
    assert_eq!(response.get("errors"), None, "{response}");
    let mut rows = response["data"]["big_int_sale_aggregate"]
        .as_array()
        .unwrap_or_else(|| panic!("{response}"))
        .clone();
    rows.sort_by_key(|row| row["account"].as_str().map(str::to_string));
    rows
}

#[tokio::test]
async fn a_64_bit_aggregate_is_served_as_its_exact_digits() {
    let Some(executor) = executor().await else {
        eprintln!("skipping: DATABASE_URL not set");
        return;
    };
    let response = executor
        .execute(
            r"{ big_int_sale_aggregate(groupBy: {account: true},
                 aggregates: [{qty_sum: true}, {qty_max: true}, {units_sum: true},
                              {units_min: true}, {units_max: true}, {count: true}])
                 { account qty_sum qty_max units_sum units_min units_max count } }",
            None,
        )
        .await
        .unwrap();
    assert_eq!(
        groups(&response),
        [
            json!({"account": EVEN, "qty_sum": "1", "qty_max": 1, "units_sum": "-5",
                   "units_min": "-5", "units_max": "-5", "count": 1}),
            json!({"account": ODD, "qty_sum": "4294967294", "qty_max": 2_147_483_647,
                   "units_sum": "18446744073709551614", "units_min": "9223372036854775807",
                   "units_max": "9223372036854775807", "count": 2}),
        ]
    );
}

/// An `int8` key compares as the integer it is: 2^53 + 1 matches its own rows and not
/// 2^53's, which an `f64` reads as the same number. Its digits are a string, an integer
/// written inline, or a variable; a `having` on a `BigInt` sum takes the same.
#[tokio::test]
async fn a_big_int_key_and_sum_filter_by_the_exact_integer() {
    let Some(executor) = executor().await else {
        eprintln!("skipping: DATABASE_URL not set");
        return;
    };
    let accounts = |response: &Value| -> Vec<String> {
        groups(response)
            .iter()
            .map(|g| g["account"].as_str().unwrap().to_string())
            .collect()
    };
    for document in [
        format!(
            r#"{{ big_int_sale_aggregate(groupBy: {{account: true}}, where: {{account_eq: "{ODD}"}}) {{ account }} }}"#
        ),
        format!(
            r"{{ big_int_sale_aggregate(groupBy: {{account: true}}, where: {{account_eq: {ODD}}}) {{ account }} }}"
        ),
        r#"{ big_int_sale_aggregate(groupBy: {account: true}, aggregates: [{qty_sum: true}],
             having: {qty_sum_gt: "4294967293"}) { account } }"#
            .to_string(),
    ] {
        let response = executor.execute(&document, None).await.unwrap();
        assert_eq!(accounts(&response), [ODD], "{document}: {response}");
    }
    let response = executor
        .execute(
            "{ big_int_sale_aggregate { account } }",
            Some(&json!({"groupBy": {"account": true}, "where": {"account_eq": ODD}})),
        )
        .await
        .unwrap();
    assert_eq!(accounts(&response), [ODD], "{response}");
}
