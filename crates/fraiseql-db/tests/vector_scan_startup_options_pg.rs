#![cfg(feature = "postgres")]
#![allow(clippy::unwrap_used, clippy::panic, clippy::print_stderr)] // Reason: test code, panics are acceptable

//! #1116: the pgvector scan settings reach the server, on every connection a pool
//! opens.
//!
//! `compose_startup_options`'s unit tests assert the *string*. That string is only
//! worth anything if PostgreSQL accepts it and pgvector then honours it, and both
//! halves have a way of being wrong that a string assertion cannot see:
//!
//! * A two-part GUC name is accepted as a **placeholder** before the extension that owns it loads,
//!   and `SHOW` on a placeholder returns whatever text was set — including a nonsense value. So a
//!   test that connects and reads the setting back proves nothing unless pgvector's library has
//!   actually loaded in that session. Every query here casts to `vector` in the same statement,
//!   which forces the load before the settings are read — see `observed_settings`.
//! * An invalid value is a `WARNING`, not an error, after which the setting falls back to `off`.
//!   Nothing fails; the search just quietly goes back to under-returning.
//!
//! So these tests assert that **different configurations produce different
//! observed values**. A constant — a placeholder echo, a hardcoded default, a
//! setting that never left the struct — cannot pass all three.
//!
//! **Execution engine:** `PostgreSQL` + pgvector · **Infrastructure:**
//! `DATABASE_URL` (the rigs run `pgvector/pgvector:pg16`).

use fraiseql_db::{
    DatabaseAdapter as _,
    postgres::{
        HnswIterativeScan, IvfflatIterativeScan, PoolPrewarmConfig, PostgresAdapter,
        PostgresTlsConfig, VectorScanConfig,
    },
};

/// Create the pgvector extension, as `graphql_vector_e2e_pg`'s seed does.
///
/// ⚠ The rig image **ships** pgvector but the database does not have the extension
/// **created** — those are different things, and the difference is invisible on a
/// developer machine where some earlier suite already ran `CREATE EXTENSION`. This
/// suite passed locally for exactly that reason and then failed in
/// `integration (postgres)` with `type "vector" does not exist` (SQLSTATE 42704).
/// A test may not inherit state another suite happened to leave behind.
///
/// Fails loudly rather than self-skipping, matching the convention in the vector
/// e2e suite: a silently skipped suite reads as passing.
async fn ensure_pgvector(url: &str) {
    let adapter = PostgresAdapter::new(url).await.expect("connect to the test database");
    let _: Vec<std::collections::HashMap<String, serde_json::Value>> = adapter
        .execute_raw_query("CREATE EXTENSION IF NOT EXISTS vector")
        .await
        .expect("CREATE EXTENSION vector (the rig image must ship pgvector)");
}

/// The two GUCs as the server sees them, read through a pool built with `scan`.
///
/// The `::vector` cast and the two `current_setting` calls are one statement on
/// purpose, and the ordering inside it is not left to chance: the cast is a
/// constant expression, so the **planner** folds it and calls pgvector's
/// `vector_in` / `vector_cosine_distance` before execution begins, while
/// `current_setting` is `stable` and therefore is not folded. The library is
/// loaded — and the GUCs are real rather than placeholders — by the time the
/// settings are read. `max_size: 1` pins it to one connection besides.
async fn observed_settings(url: &str, scan: VectorScanConfig) -> (String, String) {
    let adapter = PostgresAdapter::with_pool_config(
        url,
        PoolPrewarmConfig {
            min_size:            0,
            max_size:            1,
            timeout_secs:        Some(10),
            search_path:         None,
            tls:                 PostgresTlsConfig::default(),
            read_replicas:       None,
            max_streaming_reads: None,
            vector_scan:         scan,
        },
    )
    .await
    .expect("pool with vector scan settings");

    let rows: Vec<std::collections::HashMap<String, serde_json::Value>> = adapter
        .execute_raw_query(
            "SELECT ('[1,0,0]'::vector <=> '[0,1,0]'::vector)::text AS forces_pgvector_to_load, \
             current_setting('hnsw.iterative_scan') AS hnsw, \
             current_setting('ivfflat.iterative_scan') AS ivfflat",
        )
        .await
        .expect("the probe query (needs pgvector)");

    let row = rows.first().expect("one row");
    let get = |key: &str| {
        row.get(key)
            .and_then(serde_json::Value::as_str)
            .unwrap_or_else(|| panic!("column {key} missing from {row:?}"))
            .to_string()
    };
    (get("hnsw"), get("ivfflat"))
}

fn url() -> Option<String> {
    fraiseql_test_support::try_database_url()
}

#[tokio::test]
async fn the_default_settings_reach_the_server() {
    let Some(url) = url() else {
        eprintln!("SKIP default_settings: DATABASE_URL not set");
        return;
    };
    ensure_pgvector(&url).await;
    let (hnsw, ivfflat) = observed_settings(&url, VectorScanConfig::default()).await;
    assert_eq!(hnsw, "strict_order", "hnsw.iterative_scan, as the server resolved it");
    assert_eq!(ivfflat, "relaxed_order", "ivfflat.iterative_scan, as the server resolved it");
}

/// A second, different configuration produces different values.
///
/// This is what makes the test above falsifiable: a placeholder echo would agree
/// with whatever string was sent, but a *hardcoded* value, a setting dropped on
/// the way to the pool, or a value pgvector rejected and replaced with `off`
/// would make these two tests report the same thing.
#[tokio::test]
async fn a_different_configuration_is_observably_different() {
    let Some(url) = url() else {
        eprintln!("SKIP different_configuration: DATABASE_URL not set");
        return;
    };
    ensure_pgvector(&url).await;
    let (hnsw, ivfflat) = observed_settings(
        &url,
        VectorScanConfig {
            hnsw:      HnswIterativeScan::RelaxedOrder,
            ivfflat:   IvfflatIterativeScan::Off,
            ef_search: None,
        },
    )
    .await;
    assert_eq!(hnsw, "relaxed_order", "the operator's choice, not the default");
    // `Off` writes no fragment, so pgvector's own default stands.
    assert_eq!(ivfflat, "off", "an unset ivfflat setting leaves pgvector's default");
}

/// Turning both off leaves the server on its own defaults.
///
/// ⚠ On a rig with no `ALTER DATABASE … SET hnsw.iterative_scan`, "we wrote
/// nothing" and "we wrote `off`" are indistinguishable from here — both read back
/// as `off`. The distinction is what `turning_the_settings_off_writes_nothing_
/// rather_than_forcing_off` asserts, in the unit tests, where the composed
/// options string is visible. What this test adds is that the off path still
/// produces a **working pool**: a startup packet with no `-c` fragments at all is
/// its own case, and it used to be the only one that existed.
#[tokio::test]
async fn turning_both_off_leaves_pgvectors_own_defaults() {
    let Some(url) = url() else {
        eprintln!("SKIP both_off: DATABASE_URL not set");
        return;
    };
    ensure_pgvector(&url).await;
    let (hnsw, ivfflat) = observed_settings(
        &url,
        VectorScanConfig {
            hnsw:      HnswIterativeScan::Off,
            ivfflat:   IvfflatIterativeScan::Off,
            ef_search: None,
        },
    )
    .await;
    assert_eq!(hnsw, "off", "pgvector's own default for hnsw");
    assert_eq!(ivfflat, "off", "pgvector's own default for ivfflat");
}
