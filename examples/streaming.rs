//! Streaming large result sets with bounded memory
//!
//! This example demonstrates fraiseql-wire's key advantage:
//! streaming results with memory bounded by chunk_size, not result_size.
//!
//! Run with:
//! ```bash
//! cargo run --example streaming
//! ```

use fraiseql_wire::client::FraiseClient;
use futures::stream::StreamExt;
use std::time::Instant;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("fraiseql-wire Streaming Example\n");

    let conn_string = format!(
        "postgres://{}:{}@{}/{}",
        std::env::var("POSTGRES_USER").unwrap_or_else(|_| "postgres".to_string()),
        std::env::var("POSTGRES_PASSWORD").unwrap_or_else(|_| "postgres".to_string()),
        std::env::var("POSTGRES_HOST").unwrap_or_else(|_| "localhost".to_string()),
        std::env::var("POSTGRES_DB").unwrap_or_else(|_| "fraiseql_test".to_string()),
    );

    println!("=== Example 1: Streaming with Default Chunk Size ===\n");

    // Stream results with default chunk size (256)
    let start = Instant::now();
    let client = FraiseClient::connect(&conn_string)
        .await
        .map_err(|e| Box::new(e) as Box<dyn std::error::Error>)?;
    println!("✓ Connected to Postgres\n");

    let mut stream = client
        .query::<serde_json::Value>("projects")
        .execute()
        .await
        .map_err(|e| Box::new(e) as Box<dyn std::error::Error>)?;

    println!("Starting streaming with default chunk_size...\n");

    let mut count = 0;
    let mut last_print = Instant::now();

    while let Some(result) = stream.next().await {
        match result {
            Ok(value) => {
                count += 1;

                // Print progress every 1 second
                if last_print.elapsed().as_secs() >= 1 {
                    let elapsed = start.elapsed().as_secs_f64();
                    let throughput = count as f64 / elapsed;
                    println!(
                        "Progress: {} rows received ({:.0} rows/sec)",
                        count, throughput
                    );
                    last_print = Instant::now();
                }

                // Process the JSON value
                if let Some(name) = value.get("name").and_then(|v| v.as_str()) {
                    if count <= 3 || count % 100 == 0 {
                        println!("  Row {}: {}", count, name);
                    }
                }
            }
            Err(e) => {
                eprintln!("Error processing row {}: {}", count, e);
                return Err(Box::new(e) as Box<dyn std::error::Error>);
            }
        }
    }

    let elapsed = start.elapsed();
    let throughput = count as f64 / elapsed.as_secs_f64();

    println!("\n=== Results ===");
    println!("Total rows: {}", count);
    println!("Time elapsed: {:?}", elapsed);
    println!("Throughput: {:.0} rows/sec", throughput);
    println!("\n=== Memory Characteristics ===");
    println!("• Memory usage bounded by chunk_size (256 rows)");
    println!("• Not affected by total result size");
    println!("• Works efficiently with 1M+ row queries");
    println!("• Results processed as they arrive");

    // Example 2: Tuning chunk_size for your workload
    println!("\n=== Example 2: Custom Chunk Size ===\n");
    println!("Smaller chunk (64): More frequent network round-trips, less memory");
    println!("Larger chunk (512): Fewer round-trips, slightly more memory");
    println!("Default (256): Good balance for most use cases\n");

    let start = Instant::now();
    let client2 = FraiseClient::connect(&conn_string)
        .await
        .map_err(|e| Box::new(e) as Box<dyn std::error::Error>)?;

    let mut stream = client2
        .query::<serde_json::Value>("projects")
        .chunk_size(512) // Larger chunk
        .execute()
        .await
        .map_err(|e| Box::new(e) as Box<dyn std::error::Error>)?;

    let mut count512 = 0;
    while let Some(result) = stream.next().await {
        if result.is_ok() {
            count512 += 1;
        }
    }

    let elapsed512 = start.elapsed();
    println!("With chunk_size=512: {} rows in {:?}", count512, elapsed512);

    Ok(())
}
