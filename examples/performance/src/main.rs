//! Measuring a query instead of guessing about it.
//!
//! Three things, in order of how often they matter:
//!
//! 1. **Wall clock, repeated.** One timing is noise. This runs the same query many times and
//!    reports the distribution, then does the same for a wider selection over the same rows so the
//!    cost of the payload is separated from the cost of the round trip.
//! 2. **[`QueryTraceBuilder`]** — per-phase spans, so "the query is slow" becomes "the projection
//!    is slow" or "the connection is slow".
//! 3. **[`SqlQueryLogBuilder`]** — one structured record per statement, with a slow threshold,
//!    which is what you actually ship to a log aggregator.
//!
//! Numbers printed here are from your machine and your database. They are useful as
//! a ratio between the two selections, not as an absolute.
//!
//! Run it:
//!
//! ```text
//! ./run.sh
//! ```

use std::{path::PathBuf, sync::Arc, time::Instant};

use anyhow::{Context, Result};
use fraiseql_core::{
    db::{DatabaseAdapter, postgres::PostgresAdapter},
    runtime::{Executor, QueryTraceBuilder, SqlQueryLogBuilder},
    schema::CompiledSchema,
};

/// The blog schema from `examples/basic`, compiled.
const SCHEMA: &str = "../basic/schema.compiled.json";

/// Enough repetitions that a single scheduling hiccup does not move the median.
const RUNS: usize = 50;

const NARROW: &str = "{ posts(limit: 10) { id } }";
const WIDE: &str = "{ posts(limit: 10) { id title content authorName authorEmail createdAt } }";

#[tokio::main]
async fn main() -> Result<()> {
    // Phase-trace the startup path too: on a cold process, loading and connecting
    // routinely cost more than the first query does.
    let mut trace = QueryTraceBuilder::new("example-performance", NARROW);

    let started = Instant::now();
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(SCHEMA);
    let json_text = std::fs::read_to_string(&path).with_context(|| missing_schema(&path))?;
    let schema = CompiledSchema::from_json(&json_text, false)
        .with_context(|| format!("{} is not a compiled schema", path.display()))?;
    trace.record_phase_success("load_schema", elapsed_us(started));

    let started = Instant::now();
    let url = std::env::var("DATABASE_URL").context(
        "DATABASE_URL is not set. Point it at the database examples/basic/sql/setup.sql \
         was loaded into.",
    )?;
    let adapter =
        Arc::new(PostgresAdapter::new(&url).await.context("failed to connect to PostgreSQL")?);
    let executor = Executor::new(schema, Arc::clone(&adapter));
    trace.record_phase_success("connect", elapsed_us(started));

    // ── 1. Wall clock, repeated ────────────────────────────────────────────
    let started = Instant::now();
    let narrow = measure(&executor, NARROW, RUNS).await?;
    let wide = measure(&executor, WIDE, RUNS).await?;
    // Recorded so the phases below add up to the total. A trace whose spans account
    // for a fraction of its own duration tells you nothing about where the time went.
    trace.record_phase_success("benchmark", elapsed_us(started));

    println!("{RUNS} runs each, microseconds\n");
    println!("{:<10} {:>9} {:>9} {:>9} {:>9}", "selection", "first", "min", "median", "max");
    report("1 field", &narrow);
    report("6 fields", &wide);
    println!(
        "\nThe first run of each is above its own minimum — it pays for parsing the \
         document — and the\nvery first also warms the connection pool, which is why it \
         is the outlier. The median is what\na served request costs; the max is what a \
         scheduling hiccup costs."
    );

    // ── 2. Phase trace ─────────────────────────────────────────────────────
    let started = Instant::now();
    let response = executor.execute(NARROW, None).await.context("query execution failed")?;
    trace.record_phase_success("execute", elapsed_us(started));

    let rows = response
        .get("data")
        .and_then(|data| data.get("posts"))
        .and_then(serde_json::Value::as_array)
        .map(Vec::len);
    let finished = trace.finish(true, None, rows)?;

    println!("\n── phase trace");
    println!("{}", finished.to_log_string());
    if let Some(slowest) = finished.slowest_phase() {
        println!(
            "slowest phase: {} at {}us of {}us total",
            slowest.phase, slowest.duration_us, finished.total_duration_us
        );
    }

    // ── 3. Structured SQL log ──────────────────────────────────────────────
    //
    // A real deployment builds one of these per statement and emits it to the log
    // aggregator. `with_slow_threshold` is what makes "slow" a property of the
    // record rather than a judgement made later by whoever reads the dashboard.
    //
    // Both statements below are really executed and really timed — the builder
    // starts its clock when it is constructed. A log record assembled around a
    // duration that was never measured is worse than no record.
    println!("\n── structured SQL log (slow threshold 10ms)");
    for (label, sql) in [
        ("a fast statement", "SELECT data FROM v_post LIMIT 10"),
        ("a statement that sleeps 20ms", "SELECT pg_sleep(0.02)"),
    ] {
        let builder =
            SqlQueryLogBuilder::new("example-performance", sql, 0).with_slow_threshold(10_000);
        let rows = adapter.execute_raw_query(sql).await.context("raw statement failed")?;
        let log = builder.finish_success(Some(rows.len()));
        println!("{label}: {}", log.to_log_string());
        println!("  is_slow={} duration={:.2}ms", log.is_slow(), log.duration_ms());
    }

    Ok(())
}

/// Run `query` `runs` times and return every duration in microseconds.
async fn measure<A: DatabaseAdapter>(
    executor: &Executor<A>,
    query: &str,
    runs: usize,
) -> Result<Vec<u64>> {
    let mut durations = Vec::with_capacity(runs);
    for _ in 0..runs {
        let started = Instant::now();
        executor.execute(query, None).await.context("query execution failed")?;
        durations.push(elapsed_us(started));
    }
    Ok(durations)
}

fn report(label: &str, durations: &[u64]) {
    let mut sorted = durations.to_vec();
    sorted.sort_unstable();
    println!(
        "{:<10} {:>9} {:>9} {:>9} {:>9}",
        label,
        durations.first().copied().unwrap_or_default(),
        sorted.first().copied().unwrap_or_default(),
        sorted.get(sorted.len() / 2).copied().unwrap_or_default(),
        sorted.last().copied().unwrap_or_default(),
    );
}

fn elapsed_us(started: Instant) -> u64 {
    u64::try_from(started.elapsed().as_micros()).unwrap_or(u64::MAX)
}

fn missing_schema(path: &std::path::Path) -> String {
    format!(
        "cannot read {}.\n\nThe compiled schema is a build artifact (it is gitignored). Make it:\n\
         \n    cargo run -p fraiseql-cli -- compile examples/basic/schema.json \\\n\
         \x20        -o examples/basic/schema.compiled.json\n\n\
         or run ./run.sh from this directory, which does that first.",
        path.display()
    )
}
