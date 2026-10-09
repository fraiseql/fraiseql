//! #1314: a `nearest` search that comes back with fewer than `k` rows says so.
//!
//! pgvector's HNSW search keeps `hnsw.ef_search` candidates (40 by default) and, with no
//! iterative scan, returns at most those. A selective filter can leave fewer than `k` matches
//! among them, and a `k` above `ef_search` is cut to it with no filter at all. Either way the
//! search succeeds with what it found, and the rows cannot tell that from "only these
//! matched". Under `vector_on_short_result`:
//!
//! - `signal` (the default) serves the rows with an unverified `nearest_possibly_truncated` notice
//!   under `extensions.notices`;
//! - `verify` counts, in the same statement, how many rows match (up to `k`), and gives the notice
//!   (`verified: true`) only when more matched than came back;
//! - `refuse` refuses that verified truncation, and serves a genuinely short result.
//!
//! **The corpus truncates on purpose.** 5 000 uniform-random 16-dimension vectors under an
//! HNSW index, 1% of them `cat = 1`, searched with `hnsw.iterative_scan = off`: pgvector's
//! own default, what `vector_hnsw_iterative_scan = "off"` sets, and how a pgvector older than
//! 0.8 behaves. The filter `cat <> 0` is the realistic trap: the planner estimates an
//! inequality on a JSONB expression to keep 99.5% of rows, so it walks the index, and the 40
//! candidates it keeps hold almost none of the 50 matches. Each truncation test first asserts
//! the search came back short, so a planner or pgvector change that stops it truncating fails
//! here rather than passing vacuously. (FraiseQL's default, `strict_order`, scans on until
//! `k` rows survive, up to `hnsw.max_scan_tuples`: on this corpus it completes the search, and
//! the last test holds that it then says nothing.)
//!
//! **Execution engine:** `PostgreSQL` + pgvector · **Infrastructure:** `DATABASE_URL` ·
//! **Parallelism:** drops and recreates its own `p1314_near` schema → run `--test-threads=1`.
#![allow(clippy::unwrap_used, clippy::panic, clippy::print_stderr)] // Reason: test code

use std::sync::Arc;

use fraiseql_core::{
    cache::{CacheConfig, CachedDatabaseAdapter, QueryResultCache},
    db::{
        DatabaseAdapter as _,
        postgres::{
            HnswIterativeScan, PoolPrewarmConfig, PostgresAdapter, PostgresTlsConfig,
            VectorScanConfig,
        },
        traits::Writer,
    },
    error::FraiseQLError,
    runtime::{Executor, RuntimeConfig, notices::ShortResultPolicy},
    schema::{
        CompiledSchema, FieldDefinition, FieldType, QueryDefinition, TypeDefinition, VectorConfig,
    },
    security::SecurityContext,
};
use fraiseql_test_support::try_database_url;
use serde_json::{Value, json};

const SCHEMA: &str = "p1314_near";

/// The search vector: fixed, so every run asks the same question of the same corpus.
const PROBE: &str =
    "[0.1, 0.1, 0.1, 0.1, 0.1, 0.1, 0.1, 0.1, 0.1, 0.1, 0.1, 0.1, 0.1, 0.1, 0.1, 0.1]";

async fn seed(adapter: &PostgresAdapter) {
    for stmt in [
        "CREATE EXTENSION IF NOT EXISTS vector".to_string(),
        format!("DROP SCHEMA IF EXISTS {SCHEMA} CASCADE"),
        format!("CREATE SCHEMA {SCHEMA}"),
        format!(
            "CREATE TABLE {SCHEMA}.tb_doc (id bigint PRIMARY KEY, cat int NOT NULL, \
             embedding vector(16) NOT NULL)"
        ),
        // One statement, so the seed and the draws share a connection: the corpus is the
        // same on every run.
        format!(
            "DO $$ BEGIN PERFORM setseed(0.1314); \
             INSERT INTO {SCHEMA}.tb_doc SELECT k, CASE WHEN k % 100 = 0 THEN 1 ELSE 0 END, \
             (SELECT array_agg(random()::real - 0.5) FROM generate_series(1, 16) d \
             WHERE k > 0)::vector FROM generate_series(1, 5000) k; END $$"
        ),
        format!("CREATE INDEX ON {SCHEMA}.tb_doc USING hnsw (embedding vector_cosine_ops)"),
        format!("ANALYZE {SCHEMA}.tb_doc"),
        format!(
            "CREATE VIEW {SCHEMA}.v_doc AS SELECT id, jsonb_build_object('id', id, 'cat', cat) \
             AS data, embedding FROM {SCHEMA}.tb_doc"
        ),
    ] {
        let _: Vec<std::collections::HashMap<String, Value>> =
            adapter.execute_raw_query(&stmt).await.expect("fixture setup (needs pgvector)");
    }
}

fn schema() -> CompiledSchema {
    let mut schema = CompiledSchema::new();
    let mut doc = TypeDefinition::new("Doc", format!("{SCHEMA}.v_doc"));
    doc.fields = vec![
        FieldDefinition::new("id", FieldType::Int),
        FieldDefinition::new("cat", FieldType::Int),
        FieldDefinition::new("embedding", FieldType::Vector)
            .with_vector_config(VectorConfig::new(16)),
    ];
    schema.types.push(doc);
    let mut docs = QueryDefinition::new("docs", "Doc")
        .returning_list()
        .with_sql_source(format!("{SCHEMA}.v_doc"));
    docs.auto_params.has_where = true;
    docs.auto_params.has_limit = true;
    schema.queries.push(docs);
    schema.build_indexes();
    schema
}

/// A seeded corpus, read with `hnsw.iterative_scan = off`.
async fn adapter() -> Option<PostgresAdapter> {
    let adapter = scanning(HnswIterativeScan::Off).await?;
    seed(&adapter).await;
    Some(adapter)
}

/// A pool whose every connection runs `hnsw.iterative_scan = hnsw`.
async fn scanning(hnsw: HnswIterativeScan) -> Option<PostgresAdapter> {
    let url = try_database_url()?;
    let config = PoolPrewarmConfig {
        min_size:            0,
        max_size:            2,
        timeout_secs:        Some(10),
        search_path:         None,
        tls:                 PostgresTlsConfig::default(),
        read_replicas:       None,
        max_streaming_reads: None,
        vector_scan:         VectorScanConfig {
            hnsw,
            ..VectorScanConfig::default()
        },
    };
    Some(PostgresAdapter::with_pool_config(&url, config).await.expect("connect"))
}

fn executor<A: Writer + 'static>(adapter: A, policy: ShortResultPolicy) -> Executor {
    let config = RuntimeConfig {
        nearest_short_result: policy,
        ..RuntimeConfig::default()
    };
    Executor::with_config(schema(), Arc::new(adapter), config)
}

/// `docs` nearest `PROBE`, `k` rows, under `filter` (a `where` literal) when given.
fn search(k: u32, filter: Option<&str>) -> String {
    let filter = filter.map(|f| format!(", where: {f}")).unwrap_or_default();
    format!("{{ docs(nearest: {{vector: {PROBE}, k: {k}}}{filter}) {{ id }} }}")
}

/// Matches 50 of 5 000 rows, but reads to the planner as matching nearly all of them.
const SELECTIVE: &str = "{cat: {neq: 0}}";

/// Matches exactly one row, so any search under it is genuinely short.
const ONE_ROW: &str = "{id: {eq: 100}}";

fn returned(response: &Value) -> usize {
    response["data"]["docs"]
        .as_array()
        .unwrap_or_else(|| panic!("{response}"))
        .len()
}

fn notices(response: &Value) -> &Value {
    &response["extensions"]["notices"]
}

/// The one notice a short `docs` search carries.
fn short_notice(k: u32, returned: usize, verified: bool) -> Value {
    json!([{
        "path": ["docs"],
        "kind": "nearest_possibly_truncated",
        "detail": { "requested": k, "returned": returned, "verified": verified },
    }])
}

/// The truncation the suite is about: fewer than `k` rows, while far more than `k` match.
fn assert_truncated(response: &Value, k: u32) -> usize {
    let n = returned(response);
    assert!(
        n < k as usize,
        "the corpus must truncate this search (it returned {n} of {k}); without that every \
         assertion below is vacuous: {response}"
    );
    n
}

fn alice() -> SecurityContext {
    SecurityContext {
        user_id:          "alice".into(),
        roles:            vec![],
        tenant_id:        None,
        scopes:           vec![],
        attributes:       std::collections::HashMap::new(),
        request_id:       "req-1314".to_string(),
        ip_address:       None,
        authenticated_at: chrono::Utc::now(),
        expires_at:       chrono::Utc::now() + chrono::Duration::hours(1),
        issuer:           None,
        audience:         None,
        email:            None,
        display_name:     None,
    }
}

#[tokio::test]
async fn signal_serves_a_truncated_search_with_an_unverified_notice() {
    let Some(adapter) = adapter().await else {
        eprintln!("skipping #1314: DATABASE_URL not set");
        return;
    };
    let executor = executor(adapter, ShortResultPolicy::Signal);
    let response = executor.execute(&search(10, Some(SELECTIVE)), None).await.unwrap();
    let n = assert_truncated(&response, 10);
    assert_eq!(notices(&response), &short_notice(10, n, false), "{response}");

    // The authenticated entry is a separate runner arm.
    let response = executor
        .execute_with_security(&search(10, Some(SELECTIVE)), None, &alice())
        .await
        .unwrap();
    let n = assert_truncated(&response, 10);
    assert_eq!(notices(&response), &short_notice(10, n, false), "{response}");
}

/// A `k` above `ef_search` is cut to it with no filter at all.
#[tokio::test]
async fn an_unfiltered_search_past_ef_search_is_signalled_too() {
    let Some(adapter) = adapter().await else {
        eprintln!("skipping #1314: DATABASE_URL not set");
        return;
    };
    let executor = executor(adapter, ShortResultPolicy::Verify);
    let response = executor.execute(&search(100, None), None).await.unwrap();
    let n = assert_truncated(&response, 100);
    assert_eq!(notices(&response), &short_notice(100, n, true), "{response}");
}

#[tokio::test]
async fn a_full_page_carries_no_notice() {
    let Some(adapter) = adapter().await else {
        eprintln!("skipping #1314: DATABASE_URL not set");
        return;
    };
    let executor = executor(adapter, ShortResultPolicy::Refuse);
    let response = executor.execute(&search(10, None), None).await.unwrap();
    assert_eq!(returned(&response), 10, "{response}");
    assert!(response.get("extensions").is_none(), "{response}");
}

#[tokio::test]
async fn verify_confirms_a_truncation_by_counting_the_matches() {
    let Some(adapter) = adapter().await else {
        eprintln!("skipping #1314: DATABASE_URL not set");
        return;
    };
    let executor = executor(adapter, ShortResultPolicy::Verify);
    let response = executor.execute(&search(10, Some(SELECTIVE)), None).await.unwrap();
    let n = assert_truncated(&response, 10);
    assert_eq!(notices(&response), &short_notice(10, n, true), "{response}");

    let response = executor
        .execute_with_security(&search(10, Some(SELECTIVE)), None, &alice())
        .await
        .unwrap();
    let n = assert_truncated(&response, 10);
    assert_eq!(notices(&response), &short_notice(10, n, true), "{response}");
}

/// Fewer than `k` rows match at all: a short result that is the whole answer.
#[tokio::test]
async fn a_genuinely_short_result_is_signalled_unverified_and_otherwise_left_alone() {
    let Some(adapter) = adapter().await else {
        eprintln!("skipping #1314: DATABASE_URL not set");
        return;
    };
    let signal = executor(adapter, ShortResultPolicy::Signal);
    let response = signal.execute(&search(10, Some(ONE_ROW)), None).await.unwrap();
    assert_eq!(returned(&response), 1, "{response}");
    assert_eq!(notices(&response), &short_notice(10, 1, false), "{response}");

    for policy in [ShortResultPolicy::Verify, ShortResultPolicy::Refuse] {
        let executor = executor(scanning(HnswIterativeScan::Off).await.unwrap(), policy);
        let response = executor.execute(&search(10, Some(ONE_ROW)), None).await.unwrap();
        assert_eq!(returned(&response), 1, "{policy:?}: {response}");
        assert!(response.get("extensions").is_none(), "{policy:?}: {response}");
    }
}

#[tokio::test]
async fn refuse_refuses_a_verified_truncation() {
    let Some(adapter) = adapter().await else {
        eprintln!("skipping #1314: DATABASE_URL not set");
        return;
    };
    let executor = executor(adapter, ShortResultPolicy::Refuse);
    for refusal in [
        executor.execute(&search(10, Some(SELECTIVE)), None).await,
        executor
            .execute_with_security(&search(10, Some(SELECTIVE)), None, &alice())
            .await,
    ] {
        let Err(FraiseQLError::Unsupported { message }) = refusal else {
            panic!("a verified truncation must be refused under `refuse`: {refusal:?}");
        };
        assert!(message.contains("vector_on_short_result"), "names the setting: {message}");
        assert!(message.contains("vector_hnsw_ef_search"), "names the knob: {message}");
    }
}

/// The engine's direct read (`execute_query_direct`) settles a search the same way.
#[tokio::test]
async fn the_direct_read_settles_a_short_search_too() {
    let Some(adapter) = adapter().await else {
        eprintln!("skipping #1314: DATABASE_URL not set");
        return;
    };
    let executor = executor(adapter, ShortResultPolicy::Verify);
    let query_def = executor.schema().queries[0].clone();
    let mut arguments = std::collections::HashMap::new();
    arguments.insert(
        "nearest".to_string(),
        json!({ "vector": serde_json::from_str::<Value>(PROBE).unwrap(), "k": 10 }),
    );
    arguments.insert("where".to_string(), json!({ "cat": { "neq": 0 } }));
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
    let (rows, notices) = fraiseql_core::runtime::notices::collect_notices(
        executor.execute_query_direct(&direct, None, None, None),
    )
    .await;
    let response = rows.unwrap();
    let n = returned(&response);
    assert!(n < 10, "the corpus must truncate this search: {n} rows");
    assert_eq!(serde_json::to_value(&notices).unwrap(), short_notice(10, n, true));
}

/// A cached page answers with its notice: the count is cached with the page and settled on
/// every hit.
#[tokio::test]
async fn a_cache_hit_keeps_the_notice() {
    let Some(adapter) = adapter().await else {
        eprintln!("skipping #1314: DATABASE_URL not set");
        return;
    };
    let cached = CachedDatabaseAdapter::new(
        adapter,
        QueryResultCache::new(CacheConfig::enabled()),
        "1.0.0".to_string(),
    );
    let executor = executor(cached, ShortResultPolicy::Verify);
    let first = executor.execute(&search(10, Some(SELECTIVE)), None).await.unwrap();
    let n = assert_truncated(&first, 10);
    // Emptied behind the cache's back: only a hit can answer what the miss did.
    let behind = PostgresAdapter::new(&try_database_url().unwrap()).await.unwrap();
    let _: Vec<std::collections::HashMap<String, Value>> =
        behind.execute_raw_query(&format!("DELETE FROM {SCHEMA}.tb_doc")).await.unwrap();
    let hit = executor.execute(&search(10, Some(SELECTIVE)), None).await.unwrap();
    assert_eq!(hit, first, "a hit answers exactly what the miss did");
    assert_eq!(notices(&hit), &short_notice(10, n, true), "{hit}");
}

/// Under FraiseQL's default scan (`strict_order`) the same search scans on until `k` rows
/// survive: it completes, and a complete search says nothing.
#[tokio::test]
async fn a_search_the_iterative_scan_completes_carries_no_notice() {
    let Some(seeded) = adapter().await else {
        eprintln!("skipping #1314: DATABASE_URL not set");
        return;
    };
    drop(seeded);
    let default = scanning(HnswIterativeScan::default()).await.unwrap();
    let executor = executor(default, ShortResultPolicy::Signal);
    let response = executor.execute(&search(10, Some(SELECTIVE)), None).await.unwrap();
    assert_eq!(returned(&response), 10, "{response}");
    assert!(response.get("extensions").is_none(), "{response}");
}
