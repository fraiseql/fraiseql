//! Queries past the shape of `{ things { id } }`, against the e-commerce example.
//!
//! Five patterns, each executed and printed:
//!
//! 1. **Nested selection** — `order → customer` and `order → items → product`, three levels deep in
//!    one statement. FraiseQL does not resolve a nested field with a second query; the view builds
//!    the object and the engine projects the selection set out of it.
//! 2. **Variables** — the same document, bound differently, without string interpolation anywhere.
//! 3. **Filtering** — `where`, including through a nested object.
//! 4. **Ordering and pagination** — `orderBy`, `limit`, `offset`.
//! 5. **Two root fields in one operation** — one round trip, two results.
//!
//! `where`, `orderBy`, `limit` and `offset` are auto-params: the compiler adds them
//! to every list query, so nothing in `examples/ecommerce/schema.py` declares them.
//!
//! Run it:
//!
//! ```text
//! ./run.sh
//! ```

use std::{path::PathBuf, sync::Arc};

use anyhow::{Context, Result};
use fraiseql_core::{db::postgres::PostgresAdapter, runtime::Executor, schema::CompiledSchema};
use serde_json::json;

/// The e-commerce schema from `examples/ecommerce`, compiled.
const SCHEMA: &str = "../ecommerce/schema.compiled.json";

/// Ada Lovelace, from `examples/ecommerce/sql/setup.sql`. The seed data uses fixed
/// UUIDs, so this id is stable across a rebuild.
const ADA: &str = "c3000000-0000-4000-8000-000000000001";

#[tokio::main]
async fn main() -> Result<()> {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(SCHEMA);
    let json_text = std::fs::read_to_string(&path).with_context(|| missing_schema(&path))?;
    let schema = CompiledSchema::from_json(&json_text, false)
        .with_context(|| format!("{} is not a compiled schema", path.display()))?;

    let url = std::env::var("DATABASE_URL").context(
        "DATABASE_URL is not set. Point it at the database examples/ecommerce/sql/setup.sql \
         was loaded into.",
    )?;
    let adapter =
        Arc::new(PostgresAdapter::new(&url).await.context("failed to connect to PostgreSQL")?);
    let executor = Executor::new(schema, adapter);

    let cases: [(&str, &str, Option<serde_json::Value>); 5] = [
        (
            "1. nested selection — three levels, one statement",
            r"
            query DeepOrder($limit: Int) {
                orders(limit: $limit) {
                    orderNumber
                    total
                    customer { fullName country }
                    items {
                        quantity
                        unitPrice
                        product { sku name }
                    }
                }
            }",
            Some(json!({"limit": 2})),
        ),
        (
            "2. variables — one document, bound at call time",
            r"
            query OneProduct($id: ID!) {
                product(id: $id) {
                    sku
                    name
                    price
                    category { name }
                }
            }",
            Some(json!({"id": "b2000000-0000-4000-8000-000000000007"})),
        ),
        (
            "3. filtering — on a derived field, and through a nested object",
            r"
            query Filtered($customerId: ID!) {
                unavailable: products(where: {inStock: {eq: false}}) {
                    sku
                    stock
                }
                theirOrders: orders(where: {customer: {id: {eq: $customerId}}}) {
                    orderNumber
                    status
                }
            }",
            Some(json!({"customerId": ADA})),
        ),
        (
            "4. ordering and pagination",
            r"
            query Page($limit: Int, $offset: Int) {
                orders(orderBy: {placedAt: DESC}, limit: $limit, offset: $offset) {
                    orderNumber
                    placedAt
                }
            }",
            Some(json!({"limit": 3, "offset": 2})),
        ),
        (
            "5. two root fields, one round trip",
            r"
            query Overview {
                categories { name productCount }
                customers(orderBy: {lifetimeValue: DESC}, limit: 3) {
                    fullName
                    lifetimeValue
                }
            }",
            None,
        ),
    ];

    let mut failures = 0;
    for (label, query, variables) in cases {
        println!("── {label}");
        match executor.execute(query, variables.as_ref()).await {
            Ok(response) => {
                // `Ok` is not success: a GraphQL response carries resolution errors
                // in-band. Reporting the `data` without looking at `errors` is how a
                // partial failure gets read as a good result.
                if let Some(errors) = response.get("errors").and_then(serde_json::Value::as_array) {
                    if !errors.is_empty() {
                        failures += 1;
                        println!("   in-band errors: {}", serde_json::to_string(errors)?);
                        println!();
                        continue;
                    }
                }
                println!("{}", indent(&serde_json::to_string_pretty(&response["data"])?));
            },
            Err(err) => {
                failures += 1;
                println!("   failed: {err}");
            },
        }
        println!();
    }

    if failures > 0 {
        anyhow::bail!("{failures} of 5 queries did not resolve");
    }
    Ok(())
}

fn indent(text: &str) -> String {
    text.lines().map(|line| format!("   {line}")).collect::<Vec<_>>().join("\n")
}

fn missing_schema(path: &std::path::Path) -> String {
    format!(
        "cannot read {}.\n\nThe compiled schema is a build artifact (it is gitignored). Make it:\n\
         \n    cargo run -p fraiseql-cli -- compile examples/ecommerce/schema.json \\\n\
         \x20        -o examples/ecommerce/schema.compiled.json\n\n\
         or run ./run.sh from this directory, which does that first.",
        path.display()
    )
}
