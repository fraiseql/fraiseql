//! Basic query example
//!
//! This example demonstrates the simplest usage of fraiseql-wire:
//! connecting to a database and streaming JSON entities.
//!
//! Run with:
//! ```bash
//! cargo run --example basic_query
//! ```
//!
//! Requires a running Postgres instance with a test database.

use fraiseql_wire::client::FraiseClient;
use futures::stream::StreamExt;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("fraiseql-wire Basic Query Example\n");

    // Build connection string from environment or defaults
    let host = std::env::var("POSTGRES_HOST").unwrap_or_else(|_| "localhost".to_string());
    let user = std::env::var("POSTGRES_USER").unwrap_or_else(|_| "postgres".to_string());
    let password = std::env::var("POSTGRES_PASSWORD").unwrap_or_else(|_| "postgres".to_string());
    let db = std::env::var("POSTGRES_DB").unwrap_or_else(|_| "fraiseql_test".to_string());

    let conn_string = format!("postgres://{}:{}@{}/{}", user, password, host, db);

    println!("Connecting to: {}@{}/{}", user, host, db);

    // Connect to the database
    let client = FraiseClient::connect(&conn_string).await.map_err(|e| Box::new(e) as Box<dyn std::error::Error>)?;
    println!("✓ Connected to Postgres\n");

    // Execute a simple query to stream all projects
    println!("Querying: SELECT data FROM v_projects");
    let mut stream = client.query("projects").execute().await.map_err(|e| Box::new(e) as Box<dyn std::error::Error>)?;

    println!("✓ Query started, streaming results:\n");

    // Process results as they arrive
    let mut count = 0;
    while let Some(result) = stream.next().await {
        match result {
            Ok(value) => {
                count += 1;
                println!("Row {}: {}", count, value);
            }
            Err(e) => {
                eprintln!("Error deserializing row: {}", e);
                return Err(Box::new(e) as Box<dyn std::error::Error>);
            }
        }
    }

    println!("\n✓ Done! Received {} rows", count);

    Ok(())
}
