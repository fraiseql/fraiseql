//! Filtering example with SQL and Rust predicates
//!
//! This example demonstrates filtering data using:
//! - SQL WHERE clauses (server-side, more efficient)
//! - Rust predicates (client-side, more flexible)
//!
//! Run with:
//! ```bash
//! cargo run --example filtering
//! ```

use fraiseql_wire::client::FraiseClient;
use futures::stream::StreamExt;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("fraiseql-wire Filtering Example\n");

    let conn_string = format!(
        "postgres://{}:{}@{}/{}",
        std::env::var("POSTGRES_USER").unwrap_or_else(|_| "postgres".to_string()),
        std::env::var("POSTGRES_PASSWORD").unwrap_or_else(|_| "postgres".to_string()),
        std::env::var("POSTGRES_HOST").unwrap_or_else(|_| "localhost".to_string()),
        std::env::var("POSTGRES_DB").unwrap_or_else(|_| "fraiseql_test".to_string()),
    );

    // Example 1: SQL WHERE clause (server-side filtering)
    println!("=== Example 1: SQL WHERE Clause ===");
    println!("Query: SELECT data FROM v_projects WHERE data->>'status' = 'active'\n");

    let client1 = FraiseClient::connect(&conn_string)
        .await
        .map_err(|e| Box::new(e) as Box<dyn std::error::Error>)?;
    println!("✓ Connected to Postgres\n");

    let mut stream = client1
        .query::<serde_json::Value>("projects")
        .where_sql("data->>'status' = 'active'")
        .execute()
        .await
        .map_err(|e| Box::new(e) as Box<dyn std::error::Error>)?;

    let mut active_count = 0;
    while let Some(result) = stream.next().await {
        if let Ok(value) = result {
            active_count += 1;
            if let Some(name) = value.get("name").and_then(|v| v.as_str()) {
                println!("  Active project: {}", name);
            }
        }
    }

    println!("\n✓ Found {} active projects\n", active_count);

    // Example 2: Rust predicate (client-side filtering)
    println!("=== Example 2: Rust Predicate ===");
    println!("Query: SELECT data FROM v_projects WHERE (Rust): priority == 'high'\n");

    let client2 = FraiseClient::connect(&conn_string)
        .await
        .map_err(|e| Box::new(e) as Box<dyn std::error::Error>)?;

    let mut stream = client2
        .query::<serde_json::Value>("projects")
        .where_rust(|json| {
            json.get("priority")
                .and_then(|p| p.as_str())
                .map(|p| p == "high")
                .unwrap_or(false)
        })
        .execute()
        .await
        .map_err(|e| Box::new(e) as Box<dyn std::error::Error>)?;

    let mut high_priority = 0;
    while let Some(result) = stream.next().await {
        if let Ok(value) = result {
            high_priority += 1;
            if let Some(name) = value.get("name").and_then(|v| v.as_str()) {
                println!("  High priority project: {}", name);
            }
        }
    }

    println!("\n✓ Found {} high priority projects\n", high_priority);

    // Example 3: Combined predicates (SQL + Rust)
    println!("=== Example 3: Combined Predicates ===");
    println!(
        "Query: SELECT data FROM v_projects WHERE status='active' AND (Rust): priority == 'high'\n"
    );

    let client3 = FraiseClient::connect(&conn_string)
        .await
        .map_err(|e| Box::new(e) as Box<dyn std::error::Error>)?;

    let mut stream = client3
        .query::<serde_json::Value>("projects")
        .where_sql("data->>'status' = 'active'")
        .where_rust(|json| {
            json.get("priority")
                .and_then(|p| p.as_str())
                .map(|p| p == "high")
                .unwrap_or(false)
        })
        .execute()
        .await
        .map_err(|e| Box::new(e) as Box<dyn std::error::Error>)?;

    let mut combined = 0;
    while let Some(result) = stream.next().await {
        if let Ok(value) = result {
            combined += 1;
            if let Some(name) = value.get("name").and_then(|v| v.as_str()) {
                println!("  Active & high priority: {}", name);
            }
        }
    }

    println!("\n✓ Found {} matching projects\n", combined);

    println!("=== Summary ===");
    println!("• SQL predicates: Reduce data at source (network efficient)");
    println!("• Rust predicates: Filter on client (flexible)");
    println!("• Combined: Best of both (filter server, refine client)");

    Ok(())
}
