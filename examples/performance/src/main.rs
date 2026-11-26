//! Measuring a query instead of guessing about it.
//!
//! One timing is noise. This runs the same query many times and reports the distribution,
//! then does the same for a wider selection over the same rows, so the cost of the payload is
//! separated from the cost of the round trip.
//!
//! Numbers printed here are from your machine and your database. They are useful as
//! a ratio between the two selections, not as an absolute.
//!
//! Run it:
//!
//! ```text
//! ./run.sh
//! ```

#![allow(clippy::print_stdout)] // Reason: an example shows its results by printing them

use std::{path::PathBuf, sync::Arc, time::Instant};

use anyhow::{Context, Result};
use fraiseql_core::{db::postgres::PostgresAdapter, runtime::Executor, schema::CompiledSchema};

/// The blog schema from `examples/basic`, compiled.
const SCHEMA: &str = "../basic/schema.compiled.json";

/// Enough repetitions that a single scheduling hiccup does not move the median.
const RUNS: usize = 50;

const NARROW: &str = "{ posts(limit: 10) { id } }";
const WIDE: &str = "{ posts(limit: 10) { id title content authorName authorEmail createdAt } }";

#[tokio::main]
async fn main() -> Result<()> {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(SCHEMA);
    let json_text = std::fs::read_to_string(&path).with_context(|| missing_schema(&path))?;
    let schema = CompiledSchema::from_json(&json_text, false)
        .with_context(|| format!("{} is not a compiled schema", path.display()))?;

    let url = std::env::var("DATABASE_URL").context(
        "DATABASE_URL is not set. Point it at the database examples/basic/sql/setup.sql \
         was loaded into.",
    )?;
    let adapter =
        Arc::new(PostgresAdapter::new(&url).await.context("failed to connect to PostgreSQL")?);
    let executor = Executor::new(schema, adapter);

    let narrow = measure(&executor, NARROW, RUNS).await?;
    let wide = measure(&executor, WIDE, RUNS).await?;

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

    Ok(())
}

/// Run `query` `runs` times and return every duration in microseconds.
async fn measure(executor: &Executor, query: &str, runs: usize) -> Result<Vec<u64>> {
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
