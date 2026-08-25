//! What failure looks like, and where it surfaces.
//!
//! A FraiseQL query can fail in three different places, and code that only handles
//! the first silently accepts the other two:
//!
//! * as `Err(FraiseQLError)` from [`Executor::execute`] — the document never became an execution,
//!   so there is no response at all;
//! * **in-band**, as a `data`/`errors` GraphQL response that `execute` returns as `Ok`. Treating
//!   that as success is how a failed query reports as a good one; and
//! * **not at all** — the reason to read the last case below. A `limit` the engine cannot read is
//!   dropped rather than rejected, so the query succeeds and returns the wrong number of rows.
//!   Checking the status is not checking the result.
//!
//! This example runs seven deliberately broken queries against the `examples/basic`
//! schema and prints, for each, which of the three happened and how any error
//! classifies. It ends by failing a connection on purpose.
//!
//! Every case here is executed, not described: the output is what the engine in
//! this tree actually does, including where that is not what it should do.
//!
//! Run it:
//!
//! ```text
//! ./run.sh
//! ```

use std::{path::PathBuf, sync::Arc};

use anyhow::{Context, Result};
use fraiseql_core::{
    FraiseQLError, db::postgres::PostgresAdapter, runtime::Executor, schema::CompiledSchema,
};
use serde_json::json;

/// The blog schema from `examples/basic`, compiled.
const SCHEMA: &str = "../basic/schema.compiled.json";

/// Each case is a label, a document, and its variables.
type Case = (&'static str, &'static str, Option<serde_json::Value>);

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

    let cases: [Case; 7] = [
        ("a document that does not parse", "{ users { id ", None),
        ("a field the type does not have", "{ users { id nope } }", None),
        ("a root field the schema does not have", "{ nosuchthing { id } }", None),
        (
            "a variable the document never declares",
            "query { user(id: $missing) { name } }",
            None,
        ),
        (
            "a well-formed query for a row that is not there",
            "query($id: ID!) { user(id: $id) { name } }",
            Some(json!({"id": "00000000-0000-4000-8000-000000000000"})),
        ),
        // Not a validation error, as of this writing: the mismatch is not caught
        // before execution, so it arrives as a database error and the message
        // names a PostgreSQL type. Tracked as #1197.
        (
            "a variable whose type does not match the argument",
            "query($id: Int!) { user(id: $id) { name } }",
            Some(json!({"id": 1})),
        ),
        // The one to be afraid of. This produces no error anywhere — the `limit`
        // is silently not applied and every row comes back. Read the row count,
        // not just the status. Also #1197.
        (
            "an argument whose value is the wrong type",
            "{ users(limit: \"1\") { name } }",
            None,
        ),
    ];

    for (label, query, variables) in cases {
        println!("── {label}");
        match executor.execute(query, variables.as_ref()).await {
            Err(err) => {
                println!("   Err from execute: {err}");
                println!("   classified as:    {}", classify(&err));
            },
            Ok(response) => {
                let errors = response
                    .get("errors")
                    .and_then(serde_json::Value::as_array)
                    .filter(|errors| !errors.is_empty());
                match errors {
                    // The trap: `execute` returned Ok and the query still failed.
                    Some(errors) => {
                        println!("   Ok, with {} in-band GraphQL error(s):", errors.len());
                        for error in errors {
                            println!("     {}", serde_json::to_string(error)?);
                        }
                    },
                    // No error of either kind. Print the row count as well as the
                    // body: a silently-dropped argument shows up here and nowhere
                    // else.
                    None => {
                        println!("   Ok, no errors: {}", serde_json::to_string(&response)?);
                        if let Some(rows) = root_row_count(&response) {
                            println!("   rows returned: {rows}");
                        }
                    },
                }
            },
        }
        println!();
    }

    // Connecting is its own failure surface, before any query exists. A bad host,
    // a wrong password and a database that is not there all arrive here.
    println!("── a database that is not listening");
    match PostgresAdapter::new("postgresql://127.0.0.1:1/definitely-not-here").await {
        Ok(_) => println!("   connected, unexpectedly"),
        Err(err) => {
            println!("   Err from connect: {err}");
            println!("   classified as:    {}", classify(&err));
        },
    }

    println!(
        "\nRead the last two cases again. Neither is what it should be, and one of them \
         raised nothing at all:\n\
         \x20 - a variable typed `Int!` where the argument is `ID!` was not rejected before \
         execution;\n\
         \x20 - `limit: \"1\"` returned every row, with exit 0 and an empty `errors`.\n\
         Both are tracked as #1197. Until it is fixed, a client that checks only the status \
         cannot tell\nthese from a correct response — check the shape of what you got back."
    );
    Ok(())
}

/// How many rows the single root field returned, when it returned a list.
fn root_row_count(response: &serde_json::Value) -> Option<usize> {
    let data = response.get("data")?.as_object()?;
    let (_, value) = data.iter().next()?;
    value.as_array().map(Vec::len)
}

/// Sort an error into the handling it needs.
///
/// The distinction that matters operationally is not the variant name but who has
/// to act: the caller (fix the query), the operator (fix the deployment), or
/// nobody (retry).
fn classify(err: &FraiseQLError) -> &'static str {
    match err {
        FraiseQLError::Parse { .. }
        | FraiseQLError::Validation { .. }
        | FraiseQLError::UnknownField { .. }
        | FraiseQLError::UnknownType { .. } => "the client's query is wrong — 400, do not retry",
        FraiseQLError::Authentication { .. } => "no valid identity — 401",
        FraiseQLError::Authorization { .. } => "identity is not allowed — 403",
        FraiseQLError::NotFound { .. } => "no such row — 404",
        FraiseQLError::RateLimited { .. } => "back off and retry after the window",
        FraiseQLError::CostExceeded { .. } => "the query is too expensive — narrow it",
        FraiseQLError::Timeout { .. } | FraiseQLError::Cancelled { .. } => {
            "the query did not finish — retry or optimise it"
        },
        FraiseQLError::Database { .. } | FraiseQLError::ConnectionPool { .. } => {
            "the deployment is wrong or the database is unwell — 500, page someone"
        },
        _ => "something else — log it with the full error, not just its message",
    }
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
