//! A rolled-up fact table that declares its grain, executed against PostgreSQL.
//!
//! `tf_gr_day` holds a row a day from 1 January to 30 June 2026 for accounts 1 and 2:
//! `amount` 1, `balance` the day of the year times the account. `tf_gr_month` and
//! `tf_gr_half` hold the same rows rolled up to one per month and one per half-month,
//! dated on the period's first day, each with the period's summed `amount` and its
//! closing `balance`. Both declare that grain on `day`. `tf_gr_bare` is `tf_gr_month`
//! without the declaration.
//!
//! A bound or a bucket the rolled-up rows can answer is answered as the day table answers
//! it; one that would split a row is refused, naming the bounds or buckets that would not.
//! The undeclared table answers the split range wrongly, as every rolled-up table did.
//!
//! Self-skips when no `DATABASE_URL` is set, so it is inert in the database-free `test` leg
//! and runs in the Dagger `integration: postgres` leg (`--test '*'`).
//!
//! **Execution engine:** `PostgreSQL` · **Infrastructure:** `DATABASE_URL` ·
//! **Parallelism:** creates and drops its own `tf_gr_*` fixtures → run `--test-threads=1`.
#![allow(clippy::unwrap_used, clippy::panic, clippy::print_stderr)] // Reason: test code — panics and skip diagnostics are acceptable

use std::{collections::HashMap, sync::Arc};

use fraiseql_core::{
    compiler::fact_table::{
        Additivity, DimensionColumn, FactGrain, FactRowScope, FactTableMetadata, FilterColumn,
        GrainBucket, MeasureColumn, SemiAdditiveReduction, SqlType,
    },
    db::{DatabaseAdapter, postgres::PostgresAdapter},
    runtime::Executor,
    schema::CompiledSchema,
};
use serde_json::{Value, json};

const DAY: &str = "tf_gr_day";
const MONTH: &str = "tf_gr_month";
const HALF: &str = "tf_gr_half";
const BARE: &str = "tf_gr_bare";

fn fact_table(table: &str, grain: Option<GrainBucket>) -> FactTableMetadata {
    let measure = |name: &str, additivity: Additivity| MeasureColumn {
        name: name.to_string(),
        sql_type: SqlType::BigInt,
        nullable: false,
        additivity,
        column: None,
    };
    let filter = |name: &str, sql_type: SqlType| FilterColumn {
        name: name.to_string(),
        sql_type,
        indexed: true,
        hierarchy: None,
    };
    FactTableMetadata {
        table_name:               table.to_string(),
        type_name:                None,
        row_scope:                FactRowScope::Server,
        inject_params:            indexmap::IndexMap::default(),
        measures:                 vec![
            measure("amount", Additivity::Additive),
            measure(
                "balance",
                Additivity::SemiAdditive {
                    over:   "day".to_string(),
                    using:  SemiAdditiveReduction::Last,
                    entity: vec!["account_id".to_string()],
                },
            ),
        ],
        dimensions:               DimensionColumn {
            name:  "data".to_string(),
            paths: vec![],
        },
        denormalized_filters:     vec![
            filter("account_id", SqlType::BigInt),
            filter("day", SqlType::Date),
        ],
        calendar_dimensions:      vec![],
        grain:                    grain.map(|bucket| FactGrain {
            column: "day".to_string(),
            bucket,
            edges_from: None,
        }),
        native_dimension_mapping: HashMap::new(),
    }
}

/// The fixture: `None` without a database.
async fn executor() -> Option<Executor> {
    let url = fraiseql_test_support::try_database_url()?;
    let adapter = PostgresAdapter::new(&url).await.unwrap();
    let rolled = |table: &str, start: &str| {
        format!(
            "CREATE TABLE {table} AS SELECT account_id, {start} AS day, sum(amount)::bigint AS \
             amount, (array_agg(balance ORDER BY day DESC))[1] AS balance, '{{}}'::jsonb AS data \
             FROM {DAY} GROUP BY 1, 2"
        )
    };
    for ddl in [
        format!("DROP TABLE IF EXISTS {DAY}, {MONTH}, {HALF}, {BARE}"),
        format!(
            "CREATE TABLE {DAY} AS SELECT a::bigint AS account_id, d::date AS day, 1::bigint AS \
             amount, EXTRACT(DOY FROM d)::bigint * a AS balance, '{{}}'::jsonb AS data FROM \
             generate_series(1, 2) a, generate_series('2026-01-01'::date, '2026-06-30'::date, \
             interval '1 day') d"
        ),
        rolled(MONTH, "date_trunc('month', day)::date"),
        rolled(
            HALF,
            "(date_trunc('month', day) + CASE WHEN EXTRACT(DAY FROM day) < 16 THEN interval '0 \
             days' ELSE interval '15 days' END)::date",
        ),
        format!("CREATE TABLE {BARE} AS SELECT * FROM {MONTH}"),
    ] {
        let _: Vec<HashMap<String, Value>> = adapter.execute_raw_query(&ddl).await.unwrap();
    }
    let mut schema = CompiledSchema::new();
    for (table, grain) in [
        (DAY, None),
        (MONTH, Some(GrainBucket::Month)),
        (HALF, Some(GrainBucket::HalfMonth)),
        (BARE, None),
    ] {
        schema.add_fact_table(table.to_string(), fact_table(table, grain));
    }
    Some(Executor::new(schema, Arc::new(adapter)))
}

fn root(table: &str) -> String {
    format!("{}_aggregate", table.strip_prefix("tf_").unwrap())
}

/// `measure` summed per `group` key over `range`, in order, or the refusal.
async fn by(
    executor: &Executor,
    table: &str,
    group: &str,
    measure: &str,
    range: Value,
) -> Result<Vec<(String, String)>, String> {
    let request = json!({
        "where": range,
        "groupBy": { group: true },
        "aggregates": [{ measure: true }],
        "orderBy": [{ "field": group, "direction": "ASC" }],
    });
    let root = root(table);
    let response = executor
        .execute(&format!("{{ {root} {{ {group} {measure} }} }}"), Some(&request))
        .await
        .map_err(|e| e.to_string())?;
    let text = |v: &Value| v.as_str().map_or_else(|| v.to_string(), str::to_string);
    Ok(response["data"][&root]
        .as_array()
        .unwrap_or_else(|| panic!("{response}"))
        .iter()
        .map(|row| (text(&row[group]), text(&row[measure])))
        .collect())
}

fn refused(result: Result<Vec<(String, String)>, String>, names: &[&str]) {
    let message = result.expect_err("a split row must be refused");
    for name in names {
        assert!(message.contains(name), "{message:?} does not name {name:?}");
    }
}

/// Each spelling of a bound that splits a month is refused, naming the month boundaries
/// on either side in the same spelling.
#[tokio::test]
async fn a_bound_inside_a_month_is_refused_naming_the_months_boundaries() {
    let Some(executor) = executor().await else {
        return;
    };
    for (key, day, nearest) in [
        ("day_gte", "2026-01-10", ["`day_gte: 2026-01-01`", "`day_gte: 2026-02-01`"]),
        ("day_lt", "2026-03-20", ["`day_lt: 2026-03-01`", "`day_lt: 2026-04-01`"]),
        ("day_gt", "2026-01-10", ["`day_gt: 2025-12-31`", "`day_gt: 2026-01-31`"]),
        ("day_lte", "2026-03-20", ["`day_lte: 2026-02-28`", "`day_lte: 2026-03-31`"]),
        // The day a month starts on ends none: `_lte` 1 March keeps 1 March's row only.
        ("day_lte", "2026-03-01", ["`day_lte: 2026-02-28`", "`day_lte: 2026-03-31`"]),
        ("day_gt", "2026-03-01", ["`day_gt: 2026-02-28`", "`day_gt: 2026-03-31`"]),
    ] {
        let result = by(&executor, MONTH, "day_month", "amount_sum", json!({ key: day })).await;
        refused(
            result,
            &[
                "`tf_gr_month` holds one row per month of `day`",
                nearest[0],
                nearest[1],
            ],
        );
    }
}

/// An operator other than a range bound reads part of a month.
#[tokio::test]
async fn an_equality_on_the_grain_column_is_refused() {
    let Some(executor) = executor().await else {
        return;
    };
    for (key, value) in [
        ("day_eq", json!("2026-01-01")),
        ("day_neq", json!("2026-01-01")),
        ("day_in", json!(["2026-01-01"])),
    ] {
        let result = by(&executor, MONTH, "day_month", "amount_sum", json!({ key: value })).await;
        refused(result, &[&format!("`{key}` reads part of a month"), "`_gte`"]);
    }
}

/// A range on month boundaries, in each spelling, is answered as the day table answers it:
/// the summed amount, and the closing balance carried by the semi-additive `last`.
#[tokio::test]
async fn a_range_on_month_boundaries_is_answered_as_the_day_table_answers_it() {
    let Some(executor) = executor().await else {
        return;
    };
    for range in [
        json!({ "day_gte": "2026-02-01", "day_lte": "2026-04-30" }),
        json!({ "day_gt": "2026-01-31", "day_lt": "2026-05-01" }),
    ] {
        for measure in ["amount_sum", "balance_sum"] {
            let day = by(&executor, DAY, "day_month", measure, range.clone()).await.unwrap();
            let month = by(&executor, MONTH, "day_month", measure, range.clone()).await.unwrap();
            assert_eq!(month, day, "{measure} over {range}");
            assert_eq!(month.len(), 3, "{measure} over {range}");
        }
    }
    let amounts = by(&executor, MONTH, "day_month", "amount_sum", json!({})).await.unwrap();
    assert_eq!(
        amounts.iter().map(|(_, a)| a.as_str()).collect::<Vec<_>>(),
        ["62", "56", "62", "60", "62", "60"]
    );
}

/// A month's row falls wholly inside a quarter, a semester and a year, and straddles weeks
/// and half-months: the first are answered as the day table answers them, the others
/// refused, naming the buckets that are answered.
#[tokio::test]
async fn a_bucket_finer_than_a_month_or_straddled_by_one_is_refused() {
    let Some(executor) = executor().await else {
        return;
    };
    for group in ["day_month", "day_quarter", "day_semester", "day_year"] {
        let day = by(&executor, DAY, group, "amount_sum", json!({})).await.unwrap();
        assert_eq!(
            by(&executor, MONTH, group, "amount_sum", json!({})).await.unwrap(),
            day,
            "{group}"
        );
    }
    for group in ["day_week", "day_half_month", "day_day"] {
        refused(
            by(&executor, MONTH, group, "amount_sum", json!({})).await,
            &[
                &format!("`{group}` groups by a period finer than a row"),
                "`day_month`, `day_quarter`, `day_semester`, `day_year`",
            ],
        );
    }
}

/// A half-month table answers a bound on the 16th and a grouping by half-month, and
/// refuses a bound on the 10th and a week.
#[tokio::test]
async fn a_half_month_grain_answers_the_16th_and_refuses_the_10th() {
    let Some(executor) = executor().await else {
        return;
    };
    let range = json!({ "day_gte": "2026-01-16", "day_lte": "2026-03-15" });
    for group in ["day_half_month", "day_month"] {
        let day = by(&executor, DAY, group, "amount_sum", range.clone()).await.unwrap();
        assert_eq!(
            by(&executor, HALF, group, "amount_sum", range.clone()).await.unwrap(),
            day,
            "{group}"
        );
    }
    refused(
        by(&executor, HALF, "day_month", "amount_sum", json!({ "day_gte": "2026-01-10" })).await,
        &[
            "`tf_gr_half` holds one row per half-month of `day`",
            "`day_gte: 2026-01-01`",
            "`day_gte: 2026-01-16`",
        ],
    );
    refused(by(&executor, HALF, "day_week", "amount_sum", json!({})).await, &["`day_week`"]);
}

/// A window over the rolled-up rows refuses the bound an aggregate refuses.
#[tokio::test]
async fn a_window_refuses_a_bound_inside_a_month() {
    let Some(executor) = executor().await else {
        return;
    };
    let window = |range: Value| {
        json!({
            "select": [{ "type": "measure", "name": "amount", "alias": "amount" }],
            "windows": [{ "function": { "type": "row_number" }, "alias": "n",
                          "orderBy": [{ "field": "amount", "direction": "ASC" }] }],
            "where": range,
        })
    };
    let metadata = fact_table(MONTH, Some(GrainBucket::Month));
    let message = executor
        .execute_window_query(
            &window(json!({ "day_gte": "2026-01-10" })),
            "gr_month_window",
            &metadata,
        )
        .await
        .expect_err("a split row must be refused")
        .to_string();
    assert!(message.contains("`day_gte: 2026-01-10` does not start a month"), "{message}");
    executor
        .execute_window_query(
            &window(json!({ "day_gte": "2026-02-01" })),
            "gr_month_window",
            &metadata,
        )
        .await
        .unwrap();
}

/// Without the declaration, the rolled-up table answers a split range as it always did:
/// January's row is dropped and March's counted whole, 118 where the days sum to 140.
#[tokio::test]
async fn an_undeclared_rolled_up_table_is_read_as_before() {
    let Some(executor) = executor().await else {
        return;
    };
    let range = json!({ "day_gte": "2026-01-10", "day_lte": "2026-03-20" });
    let total = |rows: Vec<(String, String)>| {
        rows.iter().map(|(_, a)| a.parse::<i64>().unwrap()).sum::<i64>()
    };
    assert_eq!(
        total(by(&executor, DAY, "day_month", "amount_sum", range.clone()).await.unwrap()),
        140
    );
    assert_eq!(total(by(&executor, BARE, "day_month", "amount_sum", range).await.unwrap()), 118);
}
