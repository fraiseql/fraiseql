//! Example: Stream pause/resume
//!
//! This example demonstrates how to use pause() and resume() on a JsonStream
//! for explicit manual control over query execution.
//!
//! Note: This example requires a Postgres database with the fraiseql schema.
//! To use, set the DATABASE_URL environment variable and run:
//!
//! ```sh
//! export DATABASE_URL="postgres://user:password@localhost/database"
//! cargo run --example pause_resume
//! ```
//!
//! Pause/resume allows you to:
//! - Pause stream reading to control resource usage
//! - Pause to do processing between chunks
//! - Resume to continue reading from where it left off
//! - Keep the connection alive without consuming data

fn main() {
    println!(
        "{}",
        r#"
=== Pause/Resume API Example ===

The FraiseClient provides pause() and resume() methods on JsonStream:

Usage:
    // Create a stream
    let mut stream = client.query::<T>("entity").execute().await?;

    // Consume some rows
    let mut count = 0;
    while let Some(Ok(row)) = stream.next().await {
        println!("Row: {:?}", row);
        count += 1;
        if count >= 10 { break; }
    }

    // Pause the background task (stops reading from Postgres)
    stream.pause().await?;
    println!("Stream paused!");

    // Do some processing without the background task reading more data
    // Memory usage stays bounded at current buffered rows

    // Resume when ready
    stream.resume().await?;
    println!("Stream resumed!");

    // Continue consuming rows
    while let Some(Ok(row)) = stream.next().await {
        println!("Row: {:?}", row);
    }

Key semantics:
- pause() and resume() are idempotent (safe to call multiple times)
- pause() before resume() is a no-op
- Cannot pause/resume a completed or failed stream
- Stream memory stays bounded during pause (no background reading)
- Connection stays open (no reconnect needed)
- Metrics: fraiseql_stream_paused_total, fraiseql_stream_resumed_total
"#
    );
}
