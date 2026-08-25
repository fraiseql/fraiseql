//! The smallest thing that is genuinely FraiseQL: load a compiled schema, put an
//! executor on top of a PostgreSQL connection, run one GraphQL query, print the
//! response.
//!
//! There is no HTTP layer here and no code generation. `schema.compiled.json`
//! already contains the SQL for every query the schema declares; the executor
//! matches the incoming document to one of those templates, binds its variables
//! and projects the JSONB result. That is the whole runtime.
//!
//! Run it:
//!
//! ```text
//! ./run.sh
//! ```
//!
//! or, once the database and the compiled schema exist:
//!
//! ```text
//! DATABASE_URL=postgresql://localhost/fraiseql_example cargo run
//! ```

use std::{path::PathBuf, sync::Arc};

use anyhow::{Context, Result};
use fraiseql_core::{db::postgres::PostgresAdapter, runtime::Executor, schema::CompiledSchema};

/// The blog schema from `examples/basic`, compiled.
const SCHEMA: &str = "../basic/schema.compiled.json";

const QUERY: &str = r"
    query ListUsers {
        users(limit: 3) {
            id
            name
            email
        }
    }
";

#[tokio::main]
async fn main() -> Result<()> {
    // 1. Load the compiled schema.
    //
    // `strict_integrity = false` tolerates a schema compiled by a different build
    // of the toolchain. A deployment that pins its compiler should pass `true`,
    // which makes a stale schema a startup failure instead of a runtime surprise.
    let path = schema_path();
    let json = std::fs::read_to_string(&path).with_context(|| missing_schema(&path))?;
    let schema = CompiledSchema::from_json(&json, false)
        .with_context(|| format!("{} is not a compiled schema", path.display()))?;
    println!("Loaded {}", path.display());

    // 2. Connect. The adapter owns the connection pool.
    let url = std::env::var("DATABASE_URL").context(
        "DATABASE_URL is not set. Point it at the database sql/setup.sql was loaded into, \
         e.g. postgresql://localhost/fraiseql_example",
    )?;
    let adapter =
        Arc::new(PostgresAdapter::new(&url).await.context("failed to connect to PostgreSQL")?);

    // 3. Execute. `None` = no variables.
    let executor = Executor::new(schema, adapter);
    let response = executor.execute(QUERY, None).await.context("query execution failed")?;

    println!("{}", serde_json::to_string_pretty(&response)?);

    // A GraphQL response carries resolution errors in-band, so an `Ok` here is not
    // yet a success. Anything that reads the response — a health check, a CI step —
    // has to look at `errors` as well as at the return value.
    let has_errors = response
        .get("errors")
        .and_then(serde_json::Value::as_array)
        .is_some_and(|errors| !errors.is_empty());
    if has_errors {
        anyhow::bail!("the response carried GraphQL errors");
    }
    Ok(())
}

/// `../basic/schema.compiled.json`, resolved against this crate rather than the
/// working directory, so `cargo run` works from anywhere in the workspace.
fn schema_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(SCHEMA)
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
