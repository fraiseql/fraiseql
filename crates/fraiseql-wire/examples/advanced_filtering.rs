//! Advanced filtering example
//!
//! Demonstrates hybrid filtering: SQL predicates reduce data over the wire,
//! Rust predicates provide application-level filtering.

use fraiseql_wire::Result;

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt::init();

    println!("fraiseql-wire advanced filtering example");
    println!();

    // Uncomment to run against real database
    /*
    use fraiseql_wire::FraiseClient;
    use futures::StreamExt;

    let client = FraiseClient::connect("postgres://localhost/mydb").await?;

    println!("Querying projects with hybrid filtering...");
    println!();

    // Hybrid filtering: SQL + Rust
    let mut stream = client
        .query("project")
        // SQL predicates: reduce data over the wire
        .where_sql("data->>'status' = 'active'")
        .where_sql("(data->>'priority')::int >= 5")
        // Rust predicates: application-level logic
        .where_rust(|json| {
            // Complex business logic that can't easily be expressed in SQL
            let estimated_cost = json["estimated_cost"].as_f64().unwrap_or(0.0);
            let team_size = json["team_size"].as_i64().unwrap_or(0);

            estimated_cost > 10_000.0 && team_size > 2
        })
        // Server-side ordering
        .order_by("data->>'name' COLLATE \"C\" ASC")
        .chunk_size(100)
        .execute()
        .await?;

    println!("Results:");
    println!("{:<40} {:<12} {:<15}", "Name", "Team Size", "Cost");
    println!("{}", "-".repeat(70));

    let mut count = 0;
    while let Some(item) = stream.next().await {
        let project = item?;
        let name = project["name"]
            .as_str()
            .unwrap_or("unknown");
        let team_size = project["team_size"]
            .as_i64()
            .unwrap_or(0);
        let cost = project["estimated_cost"]
            .as_f64()
            .unwrap_or(0.0);

        println!("{:<40} {:<12} ${:<14.2}", name, team_size, cost);
        count += 1;
    }

    println!("{}", "-".repeat(70));
    println!("Total: {} projects", count);
    println!();

    println!("Filtering Summary:");
    println!("  - SQL filters (server-side):");
    println!("    * status = 'active'");
    println!("    * priority >= 5");
    println!("  - Rust filters (client-side):");
    println!("    * estimated_cost > $10,000");
    println!("    * team_size > 2");
    */

    println!("Advanced filtering example");
    println!();
    println!("This example demonstrates hybrid filtering in fraiseql-wire:");
    println!();
    println!("1. SQL predicates (where_sql):");
    println!("   - Executed on the server");
    println!("   - Reduce data over the wire");
    println!("   - Use for heavy filtering");
    println!();
    println!("2. Rust predicates (where_rust):");
    println!("   - Executed on the client");
    println!("   - Applied to streamed JSON values");
    println!("   - Use for complex business logic");
    println!();
    println!("3. ORDER BY (server-side):");
    println!("   - Executed on the server");
    println!("   - Results streamed in order");
    println!("   - No client-side reordering needed");
    println!();
    println!("See CONTRIBUTING.md for instructions on running with a real database.");

    Ok(())
}
