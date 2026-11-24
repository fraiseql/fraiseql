//! A rolled-up fact table linked to a finer one (`grain.edges_from`), executed against
//! PostgreSQL.
//!
//! `tf_lg_day` holds a row a day from 1 January to 30 June 2026 for accounts 1 and 2:
//! `amount` the day of the month times the account, `qty` 1, `balance` the day of the year
//! times the account, and a `category` dimension, `even` or `odd` by the day of the month.
//! The other tables hold the same rows rolled up per account and category, dated on their
//! period's first day:
//!
//! | table            | grain      | `edges_from`   |
//! |------------------|------------|----------------|
//! | `tf_lg_month`    | month      | `tf_lg_day`    |
//! | `tf_lg_mon`      | month      | —              |
//! | `tf_lg_quarter`  | quarter    | `tf_lg_mon`    |
//! | `tf_lg_half`     | half-month | —              |
//! | `tf_lg_month_h`  | month      | `tf_lg_half`   |
//!
//! A range that splits a coarse row is answered from whole periods of the coarse table and
//! the edges around them from the finer one, recombined: the finer table's answer. Only a
//! sum recombines; any other aggregate over such a range is refused, in the coarse table's
//! words.
//!
//! Self-skips when no `DATABASE_URL` is set, so it is inert in the database-free `test` leg
//! and runs in the Dagger `integration: postgres` leg (`--test '*'`).
//!
//! **Execution engine:** `PostgreSQL` · **Infrastructure:** `DATABASE_URL` ·
//! **Parallelism:** creates and drops its own `tf_lg_*` fixtures (and `nulls_order` its own
//! `tf_lgn_*`) → run `--test-threads=1`.
#![allow(clippy::unwrap_used, clippy::panic, clippy::print_stderr)] // Reason: test code — panics and skip diagnostics are acceptable

use std::{
    collections::HashMap,
    sync::{Arc, Mutex},
};

use async_trait::async_trait;
use fraiseql_core::{
    cache::{
        CacheConfig, CachedDatabaseAdapter, FactTableCacheConfig, FactTableVersionStrategy,
        QueryResultCache,
    },
    chrono::Utc,
    compiler::fact_table::{
        Additivity, DimensionColumn, DimensionPath, FactGrain, FactRowScope, FactTableMetadata,
        FilterColumn, GrainBucket, MeasureColumn, SemiAdditiveReduction, SqlType,
    },
    db::{
        postgres::PostgresAdapter,
        traits::DatabaseAdapter,
        types::{DatabaseType, JsonbValue, OrderByClause, PoolMetrics, ReadRouting},
        where_clause::WhereClause,
    },
    error::Result,
    runtime::Executor,
    schema::{CompiledSchema, InjectedParamSource, SqlProjectionHint},
    security::SecurityContext,
};
use serde_json::{Value, json};

#[path = "aggregate_linked_grain_pg/nulls_order.rs"]
mod nulls_order;

const DAY: &str = "tf_lg_day";
const MONTH: &str = "tf_lg_month";
const MON: &str = "tf_lg_mon";
const QUARTER: &str = "tf_lg_quarter";
const HALF: &str = "tf_lg_half";
const MONTH_H: &str = "tf_lg_month_h";

/// The unaligned range most pins ask: from inside January to inside May.
fn split_range() -> Value {
    json!({ "day_gte": "2026-01-10", "day_lte": "2026-05-20" })
}

fn fact_table(table: &str, bucket: GrainBucket, edges_from: Option<&str>) -> FactTableMetadata {
    let measure = |name: &str, sql_type: SqlType, additivity: Additivity| MeasureColumn {
        name: name.to_string(),
        sql_type,
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
            measure("amount", SqlType::BigInt, Additivity::Additive),
            measure("qty", SqlType::Int, Additivity::Additive),
            measure(
                "balance",
                SqlType::BigInt,
                Additivity::SemiAdditive {
                    over:   "day".to_string(),
                    using:  SemiAdditiveReduction::Last,
                    entity: vec!["account_id".to_string()],
                },
            ),
        ],
        dimensions:               DimensionColumn {
            name:  "data".to_string(),
            paths: vec![DimensionPath {
                name:      "category".to_string(),
                json_path: "data->>'category'".to_string(),
                data_type: "text".to_string(),
            }],
        },
        denormalized_filters:     vec![
            filter("account_id", SqlType::BigInt),
            filter("day", SqlType::Date),
        ],
        calendar_dimensions:      vec![],
        grain:                    Some(FactGrain {
            column: "day".to_string(),
            bucket,
            edges_from: edges_from.map(str::to_string),
        }),
        native_dimension_mapping: HashMap::new(),
    }
}

/// The six tables, each changed by `edit`.
fn schema(edit: impl Fn(&mut FactTableMetadata)) -> CompiledSchema {
    let mut schema = CompiledSchema::new();
    for (table, bucket, edges_from) in [
        (DAY, GrainBucket::Day, None),
        (MONTH, GrainBucket::Month, Some(DAY)),
        (MON, GrainBucket::Month, None),
        (QUARTER, GrainBucket::Quarter, Some(MON)),
        (HALF, GrainBucket::HalfMonth, None),
        (MONTH_H, GrainBucket::Month, Some(HALF)),
    ] {
        let mut metadata = fact_table(table, bucket, edges_from);
        edit(&mut metadata);
        schema.add_fact_table(table.to_string(), metadata);
    }
    schema.build_indexes();
    schema
}

/// The fixture tables, created afresh: `None` without a database.
async fn seeded() -> Option<PostgresAdapter> {
    let url = fraiseql_test_support::try_database_url()?;
    let adapter = PostgresAdapter::new(&url).await.unwrap();
    let rolled = |table: &str, start: &str| {
        format!(
            "CREATE TABLE {table} AS SELECT account_id, {start} AS day, sum(amount)::bigint AS \
             amount, sum(qty)::int AS qty, (array_agg(balance ORDER BY day DESC))[1] AS \
             balance, data FROM {DAY} GROUP BY account_id, 2, data"
        )
    };
    let half = "(date_trunc('month', day) + CASE WHEN EXTRACT(DAY FROM day) < 16 THEN \
                interval '0 days' ELSE interval '15 days' END)::date";
    for ddl in [
        format!("DROP TABLE IF EXISTS {DAY}, {MONTH}, {MON}, {QUARTER}, {HALF}, {MONTH_H}"),
        format!(
            "CREATE TABLE {DAY} AS SELECT a::bigint AS account_id, d::date AS day, \
             EXTRACT(DAY FROM d)::bigint * a AS amount, 1::int AS qty, EXTRACT(DOY FROM \
             d)::bigint * a AS balance, jsonb_build_object('category', CASE WHEN EXTRACT(DAY \
             FROM d)::int % 2 = 0 THEN 'even' ELSE 'odd' END) AS data FROM generate_series(1, \
             2) a, generate_series('2026-01-01'::date, '2026-06-30'::date, interval '1 day') d"
        ),
        rolled(MONTH, "date_trunc('month', day)::date"),
        rolled(MON, "date_trunc('month', day)::date"),
        rolled(QUARTER, "date_trunc('quarter', day)::date"),
        rolled(HALF, half),
        rolled(MONTH_H, "date_trunc('month', day)::date"),
    ] {
        let _: Vec<HashMap<String, Value>> = adapter.execute_raw_query(&ddl).await.unwrap();
    }
    Some(adapter)
}

async fn executor() -> Option<Executor> {
    let adapter = seeded().await?;
    Some(Executor::new(schema(|_| {}), Arc::new(adapter)))
}

fn root(table: &str) -> String {
    format!("{}_aggregate", table.strip_prefix("tf_").unwrap())
}

/// `request` asked of `table`'s aggregate root, selecting its group keys and aggregates:
/// the rows, or the refusal's text.
async fn answer(
    executor: &Executor,
    table: &str,
    request: &Value,
    principal: Option<&SecurityContext>,
) -> std::result::Result<Value, String> {
    let keys = |argument: &str| -> Vec<String> {
        match &request[argument] {
            Value::Object(map) => map.keys().cloned().collect(),
            Value::Array(items) => items
                .iter()
                .filter_map(Value::as_object)
                .flat_map(|item| item.keys().cloned())
                .collect(),
            _ => Vec::new(),
        }
    };
    let fields = [keys("groupBy"), keys("aggregates")].concat().join(" ");
    let root = root(table);
    let document = format!("{{ {root} {{ {fields} }} }}");
    let response = match principal {
        Some(principal) => {
            executor.execute_with_security(&document, Some(request), principal).await
        },
        None => executor.execute(&document, Some(request)).await,
    }
    .map_err(|e| e.to_string())?;
    if let Some(errors) = response.get("errors") {
        return Err(errors.to_string());
    }
    Ok(response["data"][&root].clone())
}

/// `aggregates` over `range`, grouped by `group` and ordered by it.
fn request(range: &Value, group: &[&str], aggregates: &[&str]) -> Value {
    json!({
        "where": range,
        "groupBy": group.iter().map(|g| ((*g).to_string(), json!(true))).collect::<serde_json::Map<_, _>>(),
        "aggregates": aggregates.iter().map(|a| json!({ *a: true })).collect::<Vec<_>>(),
        "orderBy": group.iter().map(|g| json!({ "field": g, "direction": "ASC" })).collect::<Vec<_>>(),
    })
}

/// The coarse table answers `request` as its finer table does, and the answer holds rows.
async fn answers_as_fine(executor: &Executor, coarse: &str, fine: &str, request: &Value) {
    let expected = answer(executor, fine, request, None).await.unwrap();
    let rows = expected.as_array().unwrap_or_else(|| panic!("{expected}"));
    assert!(
        !rows.is_empty() && !rows[0].as_object().unwrap().values().any(Value::is_null),
        "{expected}"
    );
    let actual = answer(executor, coarse, request, None).await;
    assert_eq!(actual, Ok(expected), "{coarse} over {request}");
}

/// The issue's pin: an unaligned range over linked tables equals the fine table's answer,
/// with no time key and by month, and so does a range with one side, a range inside one
/// month (no whole month: the edges are the range), and every spelling of the bounds.
#[tokio::test]
async fn an_unaligned_range_is_answered_as_the_finer_table_answers_it() {
    let Some(executor) = executor().await else {
        eprintln!("SKIP: no PostgreSQL (set DATABASE_URL)");
        return;
    };
    let ranges = [
        split_range(),
        json!({ "day_gt": "2026-01-09", "day_lt": "2026-05-21" }),
        json!({ "day_gte": "2026-01-10" }),
        json!({ "day_lte": "2026-05-20" }),
        json!({ "day_gte": "2026-03-05", "day_lte": "2026-03-25" }),
        json!({ "day_gte": "2026-02-01", "day_lte": "2026-04-30" }),
        json!({ "day_gte": "2026-01-10", "day_lt": "2026-02-01", "account_id_eq": 2 }),
    ];
    for range in &ranges {
        for group in [
            &[][..],
            &["day_month"],
            &["category"],
            &["category", "day_month"],
        ] {
            let request = request(range, group, &["amount_sum", "qty_sum"]);
            answers_as_fine(&executor, MONTH, DAY, &request).await;
        }
    }
}

/// A quarter table whose edges are months answers a range on month boundaries that splits
/// quarters as the month table does.
#[tokio::test]
async fn a_quarter_table_reads_its_edges_from_months() {
    let Some(executor) = executor().await else {
        eprintln!("SKIP: no PostgreSQL (set DATABASE_URL)");
        return;
    };
    for range in [
        json!({ "day_gte": "2026-02-01", "day_lte": "2026-05-31" }),
        json!({ "day_gte": "2026-03-01" }),
    ] {
        for group in [&[][..], &["day_quarter"], &["category"]] {
            let request = request(&range, group, &["amount_sum"]);
            answers_as_fine(&executor, QUARTER, MON, &request).await;
        }
    }
}

/// `having`, `orderBy`, `limit` and `offset` apply to the recombined groups, once: a group
/// is kept or dropped on its whole sum, not on either table's part of it.
#[tokio::test]
async fn having_order_and_page_apply_to_the_recombined_groups() {
    let Some(executor) = executor().await else {
        eprintln!("SKIP: no PostgreSQL (set DATABASE_URL)");
        return;
    };
    let range = split_range();
    for extra in [
        json!({ "having": { "amount_sum_gt": 900 }, "orderBy": [{ "field": "amount_sum", "direction": "DESC" }] }),
        json!({ "orderBy": [{ "field": "amount_sum", "direction": "DESC" }], "limit": 2, "offset": 1 }),
        json!({ "having": { "amount_sum_lt": 1400 }, "orderBy": [{ "field": "day_month", "direction": "ASC" }] }),
    ] {
        let mut request = request(&range, &["day_month"], &["amount_sum"]);
        for (key, value) in extra.as_object().unwrap() {
            request[key] = value.clone();
        }
        answers_as_fine(&executor, MONTH, DAY, &request).await;
    }
}

/// The edges are read under the coarse table's gates: a table scoping its rows by a
/// claim answers, for that claim, what the finer table answers for it.
#[tokio::test]
async fn the_edges_are_scoped_as_the_coarse_rows_are() {
    let Some(adapter) = seeded().await else {
        eprintln!("SKIP: no PostgreSQL (set DATABASE_URL)");
        return;
    };
    let schema = schema(|table| {
        table
            .inject_params
            .insert("account_id".to_string(), InjectedParamSource::Jwt("account".to_string()));
    });
    let executor = Executor::new(schema, Arc::new(adapter));
    let unscoped_executor = self::executor().await.unwrap();
    let principal = principal(2);
    // Read from both tables, and (no whole month inside) from the finer one alone.
    for range in [
        split_range(),
        json!({ "day_gte": "2026-03-05", "day_lte": "2026-03-25" }),
    ] {
        let request = request(&range, &["day_month"], &["amount_sum"]);
        let fine = answer(&executor, DAY, &request, Some(&principal)).await.unwrap();
        let coarse = answer(&executor, MONTH, &request, Some(&principal)).await.unwrap();
        assert_eq!(coarse, fine, "{range}");
        let unscoped = answer(&unscoped_executor, MONTH, &request, None).await.unwrap();
        assert_ne!(
            coarse, unscoped,
            "the claim must narrow the answer for this pin to show anything"
        );
    }
}

fn principal(account: i64) -> SecurityContext {
    SecurityContext {
        user_id:          "user-1".into(),
        roles:            vec![],
        tenant_id:        None,
        scopes:           vec![],
        attributes:       HashMap::from([("account".to_string(), json!(account))]),
        request_id:       "req-1".to_string(),
        ip_address:       None,
        expires_at:       Utc::now() + fraiseql_core::chrono::Duration::hours(1),
        authenticated_at: Utc::now(),
        issuer:           None,
        audience:         None,
        email:            None,
        display_name:     None,
    }
}

/// Over a range whose middle is read from whole months, only a sum recombines: any other
/// aggregate is refused, naming the coarse table and never the finer one. A range on month
/// boundaries answers each of them from the months, as it did before the link.
#[tokio::test]
async fn an_aggregate_that_does_not_recombine_is_refused() {
    let Some(executor) = executor().await else {
        eprintln!("SKIP: no PostgreSQL (set DATABASE_URL)");
        return;
    };
    let prefix = "fact table `tf_lg_month` holds one row per month of `day`";
    for aggregate in [
        "amount_min",
        "amount_max",
        "amount_avg",
        "count",
        "amount_stddev",
        "amount_variance",
    ] {
        let unaligned = request(&split_range(), &[], &[aggregate]);
        let message = answer(&executor, MONTH, &unaligned, None).await.unwrap_err();
        assert!(message.contains(prefix), "{aggregate}: {message}");
        assert!(message.contains(&format!("`{aggregate}`")), "{aggregate}: {message}");
        assert!(message.contains("only a sum recombines"), "{aggregate}: {message}");
        assert!(!message.contains(DAY), "{aggregate}: {message}");
        let aligned = request(&json!({ "day_gte": "2026-02-01" }), &[], &[aggregate]);
        answer(&executor, MONTH, &aligned, None).await.unwrap();
    }
    let semi = request(&split_range(), &["day_month"], &["balance_sum"]);
    let message = answer(&executor, MONTH, &semi, None).await.unwrap_err();
    assert!(message.contains(prefix), "{message}");
    assert!(message.contains("`balance` is semi-additive"), "{message}");
    assert!(!message.contains(DAY), "{message}");
}

/// What the coarse table refuses for itself it still refuses, in its own words, whatever
/// the finer table would accept: a bucket finer than a month, an equality on the grain
/// column, a name it does not declare, and a bound that splits even the edges' rows.
#[tokio::test]
async fn what_the_coarse_table_refuses_is_refused_in_its_words() {
    let Some(executor) = executor().await else {
        eprintln!("SKIP: no PostgreSQL (set DATABASE_URL)");
        return;
    };
    let prefix = "fact table `tf_lg_month` holds one row per month of `day`";
    let day_bucket = request(&split_range(), &["day_day"], &["amount_sum"]);
    let message = answer(&executor, MONTH, &day_bucket, None).await.unwrap_err();
    assert!(message.contains(prefix) && message.contains("`day_day`"), "{message}");

    let mut equality = split_range();
    equality["day_eq"] = json!("2026-02-01");
    let message = answer(&executor, MONTH, &request(&equality, &[], &["amount_sum"]), None)
        .await
        .unwrap_err();
    assert!(message.contains("`day_eq` reads part of a month"), "{message}");

    // The error a linked table gives for a request it cannot parse is the one it gave
    // before the link: the finer table's parse never answers.
    let unknown = request(&split_range(), &[], &["amont_sum"]);
    let linked = answer(&executor, MONTH, &unknown, None).await.unwrap_err();
    let unlinked = answer(&executor, MON, &unknown, None).await.unwrap_err();
    assert_eq!(linked.replace("LgMonth", "LgMon"), unlinked);
    assert!(!linked.contains(DAY), "{linked}");

    // Half-month edges cannot answer from 10 January: refused as the month table refuses it.
    let message = answer(&executor, MONTH_H, &request(&split_range(), &[], &["amount_sum"]), None)
        .await
        .unwrap_err();
    assert!(
        message.contains("fact table `tf_lg_month_h` holds one row per month of `day`")
            && message.contains("`day_gte: 2026-01-10` does not start a month")
            && !message.contains(HALF),
        "{message}"
    );
}

/// A range inside one month holds no whole month: the finer table reads all of it, so
/// every aggregate answers as the finer table answers it.
#[tokio::test]
async fn a_range_inside_one_month_is_the_finer_tables_to_answer() {
    let Some(executor) = executor().await else {
        eprintln!("SKIP: no PostgreSQL (set DATABASE_URL)");
        return;
    };
    for inside in [
        json!({ "day_gte": "2026-03-05", "day_lte": "2026-03-25" }),
        // To a month's last day: the next month starts where the range ends.
        json!({ "day_gte": "2026-01-10", "day_lte": "2026-01-31" }),
    ] {
        for aggregate in ["count", "amount_max", "amount_avg"] {
            let request = request(&inside, &["category"], &[aggregate]);
            answers_as_fine(&executor, MONTH, DAY, &request).await;
        }
    }
}

/// A pair a loaded schema would refuse is never read as one: a hand-built schema whose
/// finer table declares a filter the coarse one does not refuses the split range as an
/// unlinked table does.
#[tokio::test]
async fn an_unanswerable_pair_is_not_read_as_one() {
    let Some(adapter) = seeded().await else {
        eprintln!("SKIP: no PostgreSQL (set DATABASE_URL)");
        return;
    };
    let schema = schema(|table| {
        if table.table_name == DAY {
            table.denormalized_filters.push(FilterColumn {
                name:      "region".to_string(),
                sql_type:  SqlType::Text,
                indexed:   false,
                hierarchy: None,
            });
        }
    });
    let executor = Executor::new(schema, Arc::new(adapter));
    let message = answer(&executor, MONTH, &request(&split_range(), &[], &["amount_sum"]), None)
        .await
        .unwrap_err();
    assert!(message.contains("`day_gte: 2026-01-10` does not start a month"), "{message}");
}

/// A window over a linked table refuses the bound an aggregate answers from the edges.
#[tokio::test]
async fn a_window_over_a_linked_table_still_refuses_a_split_range() {
    let Some(executor) = executor().await else {
        eprintln!("SKIP: no PostgreSQL (set DATABASE_URL)");
        return;
    };
    let window = json!({
        "select": [{ "type": "measure", "name": "amount", "alias": "amount" }],
        "windows": [{ "function": { "type": "row_number" }, "alias": "n",
                      "orderBy": [{ "field": "amount", "direction": "ASC" }] }],
        "where": { "day_gte": "2026-01-10" },
    });
    let message = executor
        .execute_window_query(
            &window,
            "lg_month_window",
            &fact_table(MONTH, GrainBucket::Month, Some(DAY)),
        )
        .await
        .expect_err("a split row must be refused")
        .to_string();
    assert!(message.contains("`day_gte: 2026-01-10` does not start a month"), "{message}");
}

/// The adapter the executor reads through, recording each aggregate's SQL and parameters.
struct Recording {
    inner: PostgresAdapter,
    reads: Mutex<Vec<(String, Vec<Value>)>>,
}

impl Recording {
    fn last(&self) -> (String, Vec<Value>) {
        self.reads.lock().unwrap().last().cloned().expect("an aggregate was read")
    }
}

// Reason: DatabaseAdapter is defined with #[async_trait]; an implementation must match its
// transformed signatures.
#[async_trait]
impl DatabaseAdapter for Recording {
    async fn execute_with_projection(
        &self,
        view: &str,
        projection: Option<&SqlProjectionHint>,
        where_clause: Option<&WhereClause>,
        limit: Option<u32>,
        offset: Option<u32>,
        order_by: Option<&[OrderByClause]>,
    ) -> Result<Vec<JsonbValue>> {
        DatabaseAdapter::execute_with_projection(
            &self.inner,
            view,
            projection,
            where_clause,
            limit,
            offset,
            order_by,
        )
        .await
    }

    async fn execute_where_query(
        &self,
        view: &str,
        where_clause: Option<&WhereClause>,
        limit: Option<u32>,
        offset: Option<u32>,
        order_by: Option<&[OrderByClause]>,
    ) -> Result<Vec<JsonbValue>> {
        self.inner
            .execute_where_query(view, where_clause, limit, offset, order_by)
            .await
    }

    async fn health_check(&self) -> Result<()> {
        self.inner.health_check().await
    }

    fn database_type(&self) -> DatabaseType {
        DatabaseType::PostgreSQL
    }

    fn pool_metrics(&self) -> PoolMetrics {
        self.inner.pool_metrics()
    }

    async fn execute_raw_query(&self, sql: &str) -> Result<Vec<HashMap<String, Value>>> {
        self.inner.execute_raw_query(sql).await
    }

    async fn execute_parameterized_aggregate(
        &self,
        sql: &str,
        params: &[Value],
    ) -> Result<Vec<HashMap<String, Value>>> {
        self.reads.lock().unwrap().push((sql.to_string(), params.to_vec()));
        self.inner.execute_parameterized_aggregate(sql, params).await
    }

    async fn execute_parameterized_aggregate_with_session(
        &self,
        sql: &str,
        params: &[Value],
        session_vars: &[(&str, &str)],
        routing: ReadRouting,
    ) -> Result<Vec<HashMap<String, Value>>> {
        self.reads.lock().unwrap().push((sql.to_string(), params.to_vec()));
        self.inner
            .execute_parameterized_aggregate_with_session(sql, params, session_vars, routing)
            .await
    }
}

/// Each scan of `plan` (an `EXPLAIN (ANALYZE, FORMAT JSON)` plan) as (relation, actual rows).
fn scans(plan: &Value) -> Vec<(String, u64)> {
    let mut found = Vec::new();
    if let Some(relation) = plan.get("Relation Name").and_then(Value::as_str) {
        // PostgreSQL 18 reports a node's rows as a per-loop average, a JSON number.
        let rows = plan["Actual Rows"].as_f64().unwrap() * plan["Actual Loops"].as_f64().unwrap();
        found.push((relation.to_string(), format!("{rows:.0}").parse().unwrap()));
    }
    for child in plan.get("Plans").and_then(Value::as_array).into_iter().flatten() {
        found.extend(scans(child));
    }
    found
}

/// The rows read from each table for `request` over `MONTH`, by replaying the SQL the
/// executor ran under `EXPLAIN ANALYZE`.
async fn rows_read(
    executor: &Executor,
    recording: &Recording,
    request: &Value,
) -> Vec<(String, u64)> {
    answer(executor, MONTH, request, None).await.unwrap();
    let (sql, params) = recording.last();
    let explained = recording
        .inner
        .execute_parameterized_aggregate(&format!("EXPLAIN (ANALYZE, FORMAT JSON) {sql}"), &params)
        .await
        .unwrap();
    let plan: Value = match &explained[0]["QUERY PLAN"] {
        Value::String(text) => serde_json::from_str(text).unwrap(),
        other => other.clone(),
    };
    let mut scans = scans(&plan[0]["Plan"]);
    scans.sort();
    scans
}

/// The middle of the range is read from the month table, and only the edges from the day
/// table: from 10 January to 20 May, February to April are three month rows per account
/// and category (12 rows), and the edges 22 January days and 20 May days per account (84
/// rows). A range on month boundaries reads the month table alone.
#[tokio::test]
async fn the_middle_is_read_from_the_coarse_table() {
    let Some(adapter) = seeded().await else {
        eprintln!("SKIP: no PostgreSQL (set DATABASE_URL)");
        return;
    };
    let recording = Arc::new(Recording {
        inner: adapter,
        reads: Mutex::new(Vec::new()),
    });
    let executor = Executor::read_only(schema(|_| {}), Arc::clone(&recording));
    let split = request(&split_range(), &["category"], &["amount_sum"]);
    assert_eq!(
        rows_read(&executor, &recording, &split).await,
        [(DAY.to_string(), 84), (MONTH.to_string(), 12)]
    );
    // Ending on a month's last day, the range reads that month from the month table.
    let to_april = request(
        &json!({ "day_gte": "2026-01-10", "day_lte": "2026-04-30" }),
        &["category"],
        &["amount_sum"],
    );
    assert_eq!(
        rows_read(&executor, &recording, &to_april).await,
        [(DAY.to_string(), 44), (MONTH.to_string(), 12)]
    );
    let aligned = request(
        &json!({ "day_gte": "2026-02-01", "day_lt": "2026-05-01" }),
        &["category"],
        &["amount_sum"],
    );
    assert_eq!(rows_read(&executor, &recording, &aligned).await, [(MONTH.to_string(), 12)]);
}

/// A cached read of a linked table changes when an edge row of the finer table does: no
/// cache answers it from what it read before.
#[tokio::test]
async fn a_change_to_an_edge_row_reaches_the_next_answer() {
    let Some(adapter) = seeded().await else {
        eprintln!("SKIP: no PostgreSQL (set DATABASE_URL)");
        return;
    };
    let mut strategies = FactTableCacheConfig::default();
    for table in [DAY, MONTH] {
        strategies.set_strategy(table, FactTableVersionStrategy::time_based(300));
    }
    let cached = CachedDatabaseAdapter::with_fact_table_config(
        adapter,
        QueryResultCache::new(CacheConfig::enabled()),
        "linked-v1".to_string(),
        strategies,
    );
    let executor = Executor::new(schema(|_| {}), Arc::new(cached));
    let request = request(&split_range(), &[], &["amount_sum"]);
    let document = format!("{{ {} {{ amount_sum }} }}", root(MONTH));
    assert_eq!(executor.response_cache_ttl(&document, None), None);

    let before = answer(&executor, MONTH, &request, None).await.unwrap();
    let edit = format!(
        "UPDATE {DAY} SET amount = amount + 1000 WHERE day = '2026-01-15' AND account_id = 1"
    );
    let writer = PostgresAdapter::new(&fraiseql_test_support::try_database_url().unwrap())
        .await
        .unwrap();
    let _: Vec<HashMap<String, Value>> = writer.execute_raw_query(&edit).await.unwrap();
    let after = answer(&executor, MONTH, &request, None).await.unwrap();
    let total = |v: &Value| {
        v[0]["amount_sum"]
            .as_i64()
            .unwrap_or_else(|| v[0]["amount_sum"].as_str().unwrap().parse().unwrap())
    };
    assert_eq!(total(&after), total(&before) + 1000, "{before} then {after}");
}
