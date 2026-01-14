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
    println!("=== Pause/Resume API Example ===\n\
The FraiseClient provides pause() and resume() methods on JsonStream:\n\
\n\
Usage:\n\
    // Create a stream\n\
    let mut stream = client.query::<T>(\"entity\").execute().await?;\n\
\n\
    // Consume some rows\n\
    let mut count = 0;\n\
    while let Some(Ok(row)) = stream.next().await {{\n\
        println!(\"Row: {{:?}}\", row);\n\
        count += 1;\n\
        if count >= 10 {{ break; }}\n\
    }}\n\
\n\
    // Pause the background task (stops reading from Postgres)\n\
    stream.pause().await?;\n\
    println!(\"Stream paused!\");\n\
\n\
    // Do some processing without the background task reading more data\n\
    // Memory usage stays bounded at current buffered rows\n\
\n\
    // Resume when ready\n\
    stream.resume().await?;\n\
    println!(\"Stream resumed!\");\n\
\n\
    // Continue consuming rows\n\
    while let Some(Ok(row)) = stream.next().await {{\n\
        println!(\"Row: {{:?}}\", row);\n\
    }}\n\
\n\
Key semantics:\n\
- pause() and resume() are idempotent (safe to call multiple times)\n\
- pause() before resume() is a no-op\n\
- Cannot pause/resume a completed or failed stream\n\
- Stream memory stays bounded during pause (no background reading)\n\
- Connection stays open (no reconnect needed)\n\
- Metrics: fraiseql_stream_paused_total, fraiseql_stream_resumed_total");
}
