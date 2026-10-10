#![allow(clippy::unwrap_used, clippy::panic, clippy::print_stderr)] // Reason: test code — panics and skip diagnostics are acceptable

//! #1459: an aggregate over a measure declared not additive over time is reduced per entity
//! and per bucket first, and a request that reduction cannot answer is refused.
//!
//! `tf_sa_account_day` records a balance only on the days it changed (change-only rows):
//!
//! | account | region | rows |
//! |---|---|---|
//! | 1 | north | 100 on Dec 20, 200 on Jan 10, 260 on Jan 25, 120 on Mar 5 |
//! | 2 | south | 50 on Jan 5, 30 on Feb 15 |
//! | 3 | north | 1000 on Nov 30, nothing after |
//!
//! Each `bal_*` column holds the same balance, declared with a different reduction, so one
//! fixture answers each. Summed across days, as every measure used to be, January is 510
//! (and account 3 is absent from the quarter it holds 1000 through).
//!
//! `tf_sa_boom` is the same rows with every measure read through a function that raises:
//! a request refused before its measures are read is answered by the refusal. The function
//! is `STABLE`, so the view is flattened into the query reading it and a measure the query
//! does not reference is never computed (a volatile one is computed for every row read).
//!
//! **Execution engine:** `PostgreSQL` · **Infrastructure:** `DATABASE_URL` ·
//! **Parallelism:** creates and drops its own `tf_sa_*` relations.

use std::{collections::HashMap, sync::Arc};

use fraiseql_core::{
    compiler::fact_table::{
        Additivity, DimensionColumn, DimensionPath, FactTableMetadata, FilterColumn, MeasureColumn,
        SemiAdditiveReduction, SqlType,
    },
    db::{DatabaseAdapter, postgres::PostgresAdapter},
    runtime::{Executor, RuntimeConfig},
    schema::CompiledSchema,
};
use serde_json::{Value, json};

const TABLE: &str = "tf_sa_account_day";
const BOOM: &str = "tf_sa_boom";
const MEASURES: [&str; 8] = [
    "closing_balance",
    "bal_first",
    "bal_avg",
    "bal_min",
    "bal_max",
    "bal_delta",
    "ratio",
    "deposits",
];

fn semi(using: SemiAdditiveReduction) -> Additivity {
    Additivity::SemiAdditive {
        over: "day".to_string(),
        using,
        entity: vec!["account_id".to_string()],
    }
}

fn measure(name: &str, additivity: Additivity) -> MeasureColumn {
    MeasureColumn {
        name: name.to_string(),
        sql_type: SqlType::BigInt,
        nullable: false,
        additivity,
    }
}

fn fact_table(table: &str) -> FactTableMetadata {
    FactTableMetadata {
        table_name:               table.to_string(),
        type_name:                None,
        measures:                 vec![
            measure("closing_balance", semi(SemiAdditiveReduction::Last)),
            measure("bal_first", semi(SemiAdditiveReduction::First)),
            measure("bal_avg", semi(SemiAdditiveReduction::Avg)),
            measure("bal_min", semi(SemiAdditiveReduction::Min)),
            measure("bal_max", semi(SemiAdditiveReduction::Max)),
            measure(
                "bal_delta",
                Additivity::Delta {
                    over:   "day".to_string(),
                    entity: vec!["account_id".to_string()],
                },
            ),
            measure("ratio", Additivity::NonAdditive),
            measure("deposits", Additivity::Additive),
        ],
        dimensions:               DimensionColumn {
            name:  "data".to_string(),
            paths: vec![DimensionPath {
                name:      "region".to_string(),
                json_path: "data->>'region'".to_string(),
                data_type: "string".to_string(),
            }],
        },
        denormalized_filters:     vec![
            FilterColumn {
                name:      "account_id".to_string(),
                sql_type:  SqlType::BigInt,
                indexed:   true,
                hierarchy: None,
            },
            FilterColumn {
                name:      "day".to_string(),
                sql_type:  SqlType::Date,
                indexed:   true,
                hierarchy: None,
            },
            // When the row was booked: a time column, but not the one reduced over.
            FilterColumn {
                name:      "booked_on".to_string(),
                sql_type:  SqlType::Date,
                indexed:   false,
                hierarchy: None,
            },
        ],
        calendar_dimensions:      vec![],
        native_measures:          HashMap::new(),
        native_dimension_mapping: HashMap::new(),
    }
}

async fn executor(max_semi_additive_cells: u64) -> Option<Executor> {
    let url = fraiseql_test_support::try_database_url()?;
    let adapter = PostgresAdapter::new(&url).await.unwrap();
    let columns = MEASURES.map(|m| format!("{m} bigint")).join(", ");
    let values = |balance: i64| [balance; 8].map(|b| b.to_string()).join(", ");
    let rows = [
        (1, "2025-12-20", 100, "north"),
        (1, "2026-01-10", 200, "north"),
        (1, "2026-01-25", 260, "north"),
        (1, "2026-03-05", 120, "north"),
        (2, "2026-01-05", 50, "south"),
        (2, "2026-02-15", 30, "south"),
        (3, "2025-11-30", 1000, "north"),
    ]
    .map(|(account, day, balance, region)| {
        format!("({account}, '{day}', {}, '{{\"region\": \"{region}\"}}')", values(balance))
    })
    .join(", ");
    let raising = MEASURES.map(|m| format!("tf_sa_boom_read({m}) AS {m}")).join(", ");
    for ddl in [
        format!("DROP VIEW IF EXISTS {BOOM}"),
        format!("DROP TABLE IF EXISTS {TABLE}"),
        "DROP FUNCTION IF EXISTS tf_sa_boom_read(bigint)".to_string(),
        format!("CREATE TABLE {TABLE} (account_id bigint, day date, {columns}, data jsonb, booked_on date)"),
        format!(
            "INSERT INTO {TABLE} (account_id, day, {}, data) VALUES {rows}",
            MEASURES.join(", ")
        ),
        format!("UPDATE {TABLE} SET booked_on = day"),
        format!("CREATE INDEX ON {TABLE} (account_id, day)"),
        "CREATE FUNCTION tf_sa_boom_read(bigint) RETURNS bigint LANGUAGE plpgsql STABLE AS $$ BEGIN \
         RAISE EXCEPTION 'a measure of tf_sa_boom was read'; END $$"
            .to_string(),
        format!("CREATE VIEW {BOOM} AS SELECT account_id, day, {raising}, data, booked_on FROM {TABLE}"),
    ] {
        adapter.execute_raw_query(&ddl).await.unwrap();
    }
    let mut schema = CompiledSchema::new();
    schema.add_fact_table(TABLE.to_string(), fact_table(TABLE));
    schema.add_fact_table(BOOM.to_string(), fact_table(BOOM));
    let config = RuntimeConfig {
        max_semi_additive_cells,
        ..RuntimeConfig::default()
    };
    Some(Executor::with_config(schema, Arc::new(adapter), config))
}

fn root(table: &str) -> String {
    format!("{}_aggregate", table.strip_prefix("tf_").unwrap())
}

/// The first quarter of 2026, by month.
fn quarter(table: &str, aggregates: &[&str]) -> Value {
    json!({
        "table": table,
        "where": { "day_gte": "2026-01-01", "day_lt": "2026-04-01" },
        "groupBy": { "day_month": true },
        "aggregates": aggregates.iter().map(|a| json!({ *a: {} })).collect::<Vec<_>>(),
        "orderBy": { "day_month": "ASC" },
    })
}

async fn aggregate(executor: &Executor, request: &Value) -> Result<Vec<Value>, String> {
    let table = request["table"].as_str().unwrap();
    let document = format!("{{ {} {{ day_month }} }}", root(table));
    let response = executor.execute(&document, Some(request)).await.map_err(|e| e.to_string())?;
    Ok(response["data"][root(table)]
        .as_array()
        .unwrap_or_else(|| panic!("{response}"))
        .clone())
}

/// A number however the driver renders it (a `numeric` sum may arrive as a string).
fn number(value: &Value) -> f64 {
    value
        .as_f64()
        .or_else(|| value.as_str().and_then(|s| s.parse().ok()))
        .unwrap_or_else(|| panic!("not a number: {value}"))
}

fn column(groups: &[Value], key: &str) -> Vec<f64> {
    groups.iter().map(|g| number(&g[key])).collect()
}

async fn monthly(executor: &Executor, aggregate_name: &str) -> Vec<f64> {
    let groups = aggregate(executor, &quarter(TABLE, &[aggregate_name])).await.unwrap();
    column(&groups, aggregate_name)
}

/// Each account's last balance known by the month's end, summed across accounts: account 3
/// carries its November 1000 through the quarter, account 1 its January 260 through
/// February.
#[tokio::test]
async fn a_last_balance_is_carried_forward_and_summed_across_entities() {
    let Some(executor) = executor(1_000).await else {
        eprintln!("skipping #1459: DATABASE_URL not set");
        return;
    };
    assert_eq!(monthly(&executor, "closing_balance_sum").await, vec![1310.0, 1290.0, 1150.0]);
}

/// Without bounds the range is the data's: from account 3's November row.
#[tokio::test]
async fn an_unbounded_range_starts_at_the_first_row() {
    let Some(executor) = executor(1_000).await else {
        return;
    };
    let mut request = quarter(TABLE, &["closing_balance_sum"]);
    request.as_object_mut().unwrap().remove("where");
    let groups = aggregate(&executor, &request).await.unwrap();
    assert_eq!(
        column(&groups, "closing_balance_sum"),
        vec![1000.0, 1100.0, 1310.0, 1290.0, 1150.0],
        "November to March"
    );
}

/// Each spelling of a bound bounds the range it says: `gt` the day after, `lte` the day
/// itself, `eq` one day (whose month holds what was known on that day, not at its end).
#[tokio::test]
async fn each_bound_spelling_bounds_its_own_range() {
    let Some(executor) = executor(1_000).await else {
        return;
    };
    for (bounds, expected) in [
        // Through January 20, before account 1's January 25 row.
        (json!({ "day_gt": "2025-12-31", "day_lte": "2026-01-20" }), vec![1250.0]),
        // Through February 14, before account 2's February row.
        (json!({ "day_gte": "2026-01-01", "day_lt": "2026-02-15" }), vec![1310.0, 1310.0]),
        // January as it stood on the 15th: account 1's 200, not its later 260.
        (json!({ "day_eq": "2026-01-15" }), vec![1250.0]),
    ] {
        let mut request = quarter(TABLE, &["closing_balance_sum"]);
        request["where"] = bounds.clone();
        let groups = aggregate(&executor, &request).await.unwrap();
        assert_eq!(column(&groups, "closing_balance_sum"), expected, "{bounds}");
    }
}

/// Each grain steps one bucket at a time over the data's range (Nov 30 to Mar 5), every
/// bucket present, the last holding the balances carried to its end.
#[tokio::test]
async fn each_grain_steps_one_bucket_at_a_time() {
    let Some(executor) = executor(1_000).await else {
        return;
    };
    for (grain, buckets, last) in [
        ("day", 96, 1150.0),
        // The Mondays from Nov 24 to Mar 2.
        ("week", 15, 1150.0),
        ("month", 5, 1150.0),
        ("quarter", 2, 1150.0),
        ("year", 2, 1150.0),
    ] {
        let key = format!("day_{grain}");
        let request = json!({
            "table": TABLE,
            "groupBy": { &key: true },
            "aggregates": [{ "closing_balance_sum": {} }],
            "orderBy": { &key: "ASC" },
        });
        let groups = aggregate(&executor, &request).await.unwrap();
        let sums = column(&groups, "closing_balance_sum");
        assert_eq!(sums.len(), buckets, "{grain}");
        assert_eq!(sums.last().copied(), Some(last), "{grain}");
    }
    // Q4 2025: account 3's 1000 and account 1's December 100.
    let request = json!({
        "table": TABLE,
        "groupBy": { "day_quarter": true },
        "aggregates": [{ "closing_balance_sum": {} }],
        "orderBy": { "day_quarter": "ASC" },
    });
    let groups = aggregate(&executor, &request).await.unwrap();
    assert_eq!(column(&groups, "closing_balance_sum"), vec![1100.0, 1150.0]);
}

/// On a timestamp column the sub-day grains step, and an exclusive upper bound stops one
/// microsecond short. One meter reading, recorded when it changed.
#[tokio::test]
async fn a_timestamp_column_steps_by_second_minute_and_hour() {
    let Some(executor) = executor(1_000).await else {
        return;
    };
    let url = fraiseql_test_support::try_database_url().unwrap();
    let adapter = PostgresAdapter::new(&url).await.unwrap();
    for ddl in [
        "DROP TABLE IF EXISTS tf_sa_meter".to_string(),
        "CREATE TABLE tf_sa_meter (meter_id bigint, at timestamptz, reading bigint, data jsonb)"
            .to_string(),
        "INSERT INTO tf_sa_meter VALUES (1, '2026-01-01 10:00:00+00', 5, '{}'), \
         (1, '2026-01-01 10:00:01.5+00', 7, '{}'), (1, '2026-01-01 10:02:00+00', 9, '{}'), \
         (1, '2026-01-01 11:30:00+00', 11, '{}')"
            .to_string(),
    ] {
        adapter.execute_raw_query(&ddl).await.unwrap();
    }
    let mut metadata = fact_table("tf_sa_meter");
    metadata.measures = vec![measure(
        "reading",
        Additivity::SemiAdditive {
            over:   "at".to_string(),
            using:  SemiAdditiveReduction::Last,
            entity: vec!["meter_id".to_string()],
        },
    )];
    metadata.denormalized_filters = vec![
        FilterColumn {
            name:      "meter_id".to_string(),
            sql_type:  SqlType::BigInt,
            indexed:   true,
            hierarchy: None,
        },
        FilterColumn {
            name:      "at".to_string(),
            sql_type:  SqlType::Timestamp,
            indexed:   true,
            hierarchy: None,
        },
    ];
    let mut schema = CompiledSchema::new();
    schema.add_fact_table("tf_sa_meter".to_string(), metadata);
    let meter = Executor::with_config(schema, Arc::new(adapter), RuntimeConfig::default());
    drop(executor);
    for (grain, from, to, expected) in [
        ("second", "2026-01-01T10:00:00Z", "2026-01-01T10:00:03Z", vec![5.0, 7.0, 7.0]),
        ("minute", "2026-01-01T10:00:00Z", "2026-01-01T10:03:00Z", vec![7.0, 7.0, 9.0]),
        ("hour", "2026-01-01T10:00:00Z", "2026-01-01T12:00:00Z", vec![9.0, 11.0]),
    ] {
        let key = format!("at_{grain}");
        let request = json!({
            "table": "tf_sa_meter",
            "where": { "at_gte": from, "at_lt": to },
            "groupBy": { &key: true },
            "aggregates": [{ "reading_sum": {} }],
            "orderBy": { &key: "ASC" },
        });
        let response = meter
            .execute(&format!("{{ sa_meter_aggregate {{ {key} }} }}"), Some(&request))
            .await
            .unwrap();
        let groups = response["data"]["sa_meter_aggregate"].as_array().unwrap().clone();
        assert_eq!(column(&groups, "reading_sum"), expected, "{grain}");
    }
}

/// A bucket key reads exactly as the plain aggregate's key for the same month.
#[tokio::test]
async fn a_carried_bucket_key_is_the_plain_aggregates_key() {
    let Some(executor) = executor(1_000).await else {
        return;
    };
    let keys =
        |groups: Vec<Value>| groups.iter().map(|g| g["day_month"].clone()).collect::<Vec<_>>();
    let reduced =
        keys(aggregate(&executor, &quarter(TABLE, &["closing_balance_sum"])).await.unwrap());
    let plain = keys(aggregate(&executor, &quarter(TABLE, &["deposits_sum"])).await.unwrap());
    assert_eq!(reduced, plain);
}

/// Each declared reduction, per account and month, then summed across accounts.
#[tokio::test]
async fn each_reduction_answers_its_own_measure() {
    let Some(executor) = executor(1_000).await else {
        return;
    };
    for (aggregate_name, expected) in [
        // The month's first row, else the balance carried in (account 1 in February: 260;
        // account 3 throughout: 1000).
        ("bal_first_sum", [1250.0, 1290.0, 1150.0]),
        // Over the month's own rows; an account with none contributes nothing.
        ("bal_avg_sum", [280.0, 30.0, 120.0]),
        ("bal_min_sum", [250.0, 30.0, 120.0]),
        ("bal_max_sum", [310.0, 30.0, 120.0]),
        // Last − first of the month's own rows: account 1 in January, 260 − 200.
        ("bal_delta_sum", [60.0, 0.0, 0.0]),
    ] {
        assert_eq!(monthly(&executor, aggregate_name).await, expected, "{aggregate_name}");
    }
}

/// The requested function runs across entities: the largest carried balance.
#[tokio::test]
async fn the_requested_function_runs_across_entities() {
    let Some(executor) = executor(1_000).await else {
        return;
    };
    assert_eq!(monthly(&executor, "closing_balance_max").await, vec![1000.0, 1000.0, 1000.0]);
}

/// Another grouping is read from the row each cell carries.
#[tokio::test]
async fn another_grouping_is_read_from_the_carried_row() {
    let Some(executor) = executor(1_000).await else {
        return;
    };
    let mut request = quarter(TABLE, &["closing_balance_sum"]);
    request["groupBy"]["region"] = json!(true);
    request["orderBy"] = json!({ "day_month": "ASC", "region": "ASC" });
    let groups = aggregate(&executor, &request).await.unwrap();
    let cells: Vec<(String, f64)> = groups
        .iter()
        .map(|g| (g["region"].as_str().unwrap().to_string(), number(&g["closing_balance_sum"])))
        .collect();
    let expected: Vec<(String, f64)> = [
        ("north", 1260.0),
        ("south", 50.0),
        ("north", 1260.0),
        ("south", 30.0),
        ("north", 1120.0),
        ("south", 30.0),
    ]
    .map(|(r, v)| (r.to_string(), v))
    .to_vec();
    assert_eq!(cells, expected);
}

fn assert_refused(result: Result<Vec<Value>, String>, names: &str, why: &str) {
    let err = result.expect_err(why);
    assert!(!err.contains("was read"), "{why}: refused before a measure is read: {err}");
    assert!(err.contains(&format!("`{names}`")), "{why}: names `{names}`: {err}");
}

/// Each request the reduction cannot answer is refused, naming the measure, before any
/// measure is read.
#[tokio::test]
async fn a_request_the_reduction_cannot_answer_is_refused() {
    let Some(executor) = executor(1_000).await else {
        return;
    };
    let no_time_grouping = {
        let mut r = quarter(BOOM, &["closing_balance_sum"]);
        r["groupBy"] = json!({ "region": true });
        r["orderBy"] = json!({ "region": "ASC" });
        r
    };
    let two_time_groupings = {
        let mut r = quarter(BOOM, &["closing_balance_sum"]);
        r["groupBy"]["day_week"] = json!(true);
        r
    };
    let another_time_column = {
        let mut r = quarter(BOOM, &["closing_balance_sum"]);
        r["groupBy"] = json!({ "booked_on_month": true });
        r["orderBy"] = json!({ "booked_on_month": "ASC" });
        r
    };
    let a_time_inequality = {
        let mut r = quarter(BOOM, &["closing_balance_sum"]);
        r["where"]["day_neq"] = json!("2026-02-01");
        r
    };
    for (request, names, why) in [
        (quarter(BOOM, &["ratio_sum"]), "ratio", "a non-additive measure"),
        (
            quarter(BOOM, &["ratio_max"]),
            "ratio",
            "any function over a non-additive measure",
        ),
        (no_time_grouping, "closing_balance", "no time grouping"),
        (two_time_groupings, "closing_balance", "two time groupings"),
        (
            another_time_column,
            "closing_balance",
            "a bucket of a column it is not reduced over",
        ),
        (
            quarter(BOOM, &["closing_balance_sum", "count"]),
            "closing_balance",
            "with a count",
        ),
        (
            quarter(BOOM, &["closing_balance_sum", "deposits_sum"]),
            "closing_balance",
            "with an additive measure",
        ),
        (
            quarter(BOOM, &["closing_balance_sum", "bal_avg_sum"]),
            "closing_balance",
            "with a measure reduced differently",
        ),
        (a_time_inequality, "closing_balance", "a time filter that is not a bound"),
    ] {
        assert_refused(aggregate(&executor, &request).await, names, why);
    }
    for list in ["array_agg", "json_agg", "jsonb_agg", "string_agg"] {
        let request = quarter(BOOM, &[&format!("closing_balance_{list}")]);
        assert_refused(aggregate(&executor, &request).await, "closing_balance", list);
    }
}

/// A carried-forward request over more cells than the bound is refused before a measure is
/// read; at the bound it runs. The quarter crosses three months with three accounts.
#[tokio::test]
async fn a_request_over_more_cells_than_the_bound_is_refused_before_any_measure_is_read() {
    let Some(at_bound) = executor(9).await else {
        return;
    };
    assert_eq!(monthly(&at_bound, "closing_balance_sum").await, vec![1310.0, 1290.0, 1150.0]);

    let below = executor(8).await.unwrap();
    let result = aggregate(&below, &quarter(BOOM, &["closing_balance_sum"])).await;
    let err = result.as_ref().expect_err("nine cells over a bound of eight").clone();
    assert_refused(result, "closing_balance", "over the bound");
    assert!(err.contains("3 buckets by 3 entities"), "{err}");
}

/// A window aggregate over a measure not additive over time is refused, naming it; a value
/// function over it, which combines nothing, runs.
#[tokio::test]
async fn a_window_aggregate_over_a_semi_additive_measure_is_refused() {
    let Some(executor) = executor(1_000).await else {
        return;
    };
    let window = |table: &str, function: Value| {
        json!({
            "table": table,
            "select": [{ "type": "filter", "name": "day", "alias": "day" }],
            "windows": [{ "function": function, "alias": "w",
                          "orderBy": [{ "field": "day", "direction": "ASC" }] }],
            "orderBy": [{ "field": "day", "direction": "ASC" }],
        })
    };
    let run = |table: &'static str, function: Value| {
        let request = window(table, function);
        let executor = &executor;
        async move {
            let root = format!("{}_window", table.strip_prefix("tf_").unwrap());
            executor
                .execute(&format!("{{ {root} {{ day w }} }}"), Some(&request))
                .await
                .map_err(|e| e.to_string())
        }
    };
    for function in [
        "running_sum",
        "running_avg",
        "running_min",
        "running_max",
        "running_stddev",
        "running_variance",
    ] {
        for measure in ["closing_balance", "bal_delta", "ratio"] {
            let err = run(BOOM, json!({ "type": function, "measure": measure }))
                .await
                .expect_err("a window aggregate over it");
            assert!(
                !err.contains("was read") && err.contains(&format!("`{measure}`")),
                "{function}({measure}): {err}"
            );
        }
    }
    run(TABLE, json!({ "type": "lag", "field": "closing_balance", "offset": 1 }))
        .await
        .expect("a value function combines nothing");
    run(TABLE, json!({ "type": "running_sum", "measure": "deposits" }))
        .await
        .expect("an additive running sum");
}
