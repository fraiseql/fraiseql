//! A rolled-up table answering a split range orders its recombined groups by the placement the
//! `orderBy` direction names, as its finer table does: a group whose key is NULL sorts first or
//! last on both, where PostgreSQL puts it.
//!
//! **Parallelism:** creates and drops its own `tf_lgn_day` and `tf_lgn_month`.

use std::{collections::HashMap, sync::Arc};

use fraiseql_core::{
    compiler::fact_table::GrainBucket,
    db::{postgres::PostgresAdapter, traits::DatabaseAdapter},
    runtime::Executor,
    schema::CompiledSchema,
};
use serde_json::{Value, json};

use super::{answer, fact_table};

const DAY: &str = "tf_lgn_day";
const MONTH: &str = "tf_lgn_month";

/// The day table (every third day has no category, so its rows group under a NULL key) and
/// the month table rolled up from it, linked for its edges.
async fn seeded() -> Option<(PostgresAdapter, Executor)> {
    let url = fraiseql_test_support::try_database_url()?;
    let adapter = PostgresAdapter::new(&url).await.unwrap();
    for ddl in [
        format!("DROP TABLE IF EXISTS {DAY}, {MONTH}"),
        format!(
            "CREATE TABLE {DAY} AS SELECT 1::bigint AS account_id, d::date AS day, \
             EXTRACT(DAY FROM d)::bigint AS amount, 1::int AS qty, 0::bigint AS balance, CASE \
             WHEN EXTRACT(DOY FROM d)::int % 3 = 0 THEN '{{}}'::jsonb WHEN EXTRACT(DAY FROM \
             d)::int % 2 = 0 THEN '{{\"category\": \"even\"}}'::jsonb ELSE \
             '{{\"category\": \"odd\"}}'::jsonb END AS data FROM \
             generate_series('2026-01-01'::date, '2026-03-31'::date, interval '1 day') d"
        ),
        format!(
            "CREATE TABLE {MONTH} AS SELECT account_id, date_trunc('month', day)::date AS day, \
             sum(amount)::bigint AS amount, sum(qty)::int AS qty, max(balance) AS balance, data \
             FROM {DAY} GROUP BY account_id, 2, data"
        ),
    ] {
        let _: Vec<HashMap<String, Value>> = adapter.execute_raw_query(&ddl).await.unwrap();
    }
    let mut schema = CompiledSchema::new();
    schema.add_fact_table(DAY.to_string(), fact_table(DAY, GrainBucket::Day, None));
    schema.add_fact_table(MONTH.to_string(), fact_table(MONTH, GrainBucket::Month, Some(DAY)));
    schema.build_indexes();
    let executor = Executor::new(schema, Arc::new(PostgresAdapter::new(&url).await.unwrap()));
    Some((adapter, executor))
}

fn categories(rows: &Value) -> Vec<Option<String>> {
    rows.as_array()
        .unwrap_or_else(|| panic!("{rows}"))
        .iter()
        .map(|row| row["category"].as_str().map(str::to_string))
        .collect()
}

#[tokio::test]
async fn a_split_range_orders_its_null_group_where_the_direction_asks() {
    let Some((adapter, executor)) = seeded().await else {
        eprintln!("SKIP: no PostgreSQL (set DATABASE_URL)");
        return;
    };
    for (direction, sql_order) in [
        ("DESC_NULLS_LAST", "DESC NULLS LAST"),
        ("ASC_NULLS_FIRST", "ASC NULLS FIRST"),
    ] {
        let request = json!({
            "where": { "day_gte": "2026-01-10", "day_lte": "2026-03-20" },
            "groupBy": { "category": true },
            "aggregates": [{ "amount_sum": true }],
            "orderBy": [{ "field": "category", "direction": direction }],
        });
        let fine = answer(&executor, DAY, &request, None).await.unwrap();
        let expected: Vec<Option<String>> = adapter
            .execute_raw_query(&format!(
                "SELECT DISTINCT data->>'category' AS category FROM {DAY} ORDER BY 1 {sql_order}"
            ))
            .await
            .unwrap()
            .iter()
            .map(|row| row["category"].as_str().map(str::to_string))
            .collect();
        assert_eq!(categories(&fine), expected, "the fine table under {direction}");
        let coarse = answer(&executor, MONTH, &request, None).await.unwrap();
        assert_eq!(coarse, fine, "the split read under {direction}");
    }
}
