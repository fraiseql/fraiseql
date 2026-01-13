//! Ordering example demonstrating server-side sorting
//!
//! This example shows how to use ORDER BY to sort results
//! on the server (more efficient than client-side sorting).
//!
//! Run with:
//! ```bash
//! cargo run --example ordering
//! ```

use fraiseql_wire::client::FraiseClient;
use futures::stream::StreamExt;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("fraiseql-wire Ordering Example\n");

    let conn_string = format!(
        "postgres://{}:{}@{}/{}",
        std::env::var("POSTGRES_USER").unwrap_or_else(|_| "postgres".to_string()),
        std::env::var("POSTGRES_PASSWORD").unwrap_or_else(|_| "postgres".to_string()),
        std::env::var("POSTGRES_HOST").unwrap_or_else(|_| "localhost".to_string()),
        std::env::var("POSTGRES_DB").unwrap_or_else(|_| "fraiseql_test".to_string()),
    );

    // Example 1: Order by project name ascending
    println!("=== Example 1: Order by Name (ASC) ===");
    println!("Query: SELECT data FROM v_projects ORDER BY data->>'name' ASC\n");

    let client1 = FraiseClient::connect(&conn_string)
        .await
        .map_err(|e| Box::new(e) as Box<dyn std::error::Error>)?;
    println!("✓ Connected to Postgres\n");

    let mut stream = client1
        .query::<serde_json::Value>("projects")
        .order_by("data->>'name' ASC")
        .execute()
        .await
        .map_err(|e| Box::new(e) as Box<dyn std::error::Error>)?;

    let mut count = 0;
    let mut prev_name: Option<String> = None;

    while let Some(result) = stream.next().await {
        if let Ok(value) = result {
            if let Some(name) = value.get("name").and_then(|v| v.as_str()) {
                count += 1;
                println!("  {}. {}", count, name);

                // Verify ordering
                if let Some(ref prev) = prev_name {
                    if prev.as_str() > name {
                        eprintln!("  ⚠ Warning: Out of order! {} > {}", prev, name);
                    }
                }
                prev_name = Some(name.to_string());
            }
        }
    }

    println!("\n✓ Retrieved {} projects in sorted order\n", count);

    // Example 2: Order by name descending with collation
    println!("=== Example 2: Order by Name (DESC) with Collation ===");
    println!(
        "Query: SELECT data FROM v_projects ORDER BY data->>'name' COLLATE \"C\" DESC\n"
    );

    let client2 = FraiseClient::connect(&conn_string)
        .await
        .map_err(|e| Box::new(e) as Box<dyn std::error::Error>)?;

    let mut stream = client2
        .query::<serde_json::Value>("projects")
        .order_by("data->>'name' COLLATE \"C\" DESC")
        .execute()
        .await
        .map_err(|e| Box::new(e) as Box<dyn std::error::Error>)?;

    let mut count = 0;
    println!("Projects in reverse alphabetical order:");

    while let Some(result) = stream.next().await {
        if let Ok(value) = result {
            if let Some(name) = value.get("name").and_then(|v| v.as_str()) {
                count += 1;
                println!("  {}. {}", count, name);
            }
        }
    }

    println!("\n✓ Retrieved {} projects in reverse order\n", count);

    println!("=== Key Points ===");
    println!("• ORDER BY is executed entirely on the server");
    println!("• Results are streamed in sorted order");
    println!("• No client-side buffering or reordering");
    println!("• COLLATE specifies sort order (e.g., \"C\" for C locale)");
    println!("• More efficient than sorting in application code");

    Ok(())
}
