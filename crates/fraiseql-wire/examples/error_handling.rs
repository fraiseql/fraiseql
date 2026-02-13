//! Error handling patterns
//!
//! Demonstrates how to handle errors gracefully in fraiseql-wire applications.

use fraiseql_wire::Result;

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt::init();

    println!("fraiseql-wire error handling example");
    println!();

    // Uncomment to run against real database
    /*
    use fraiseql_wire::FraiseClient;
    use futures::StreamExt;

    // Example 1: Handle connection errors
    println!("Example 1: Connection error handling");
    match FraiseClient::connect("postgres://invalid/database").await {
        Ok(client) => println!("Connected successfully"),
        Err(e) => {
            eprintln!("Connection failed: {}", e);
            eprintln!("Error category: {}", e.category());

            if e.is_retriable() {
                eprintln!("This error might succeed on retry");
            } else {
                eprintln!("This error is not retriable");
            }
        }
    }

    println!();

    // Example 2: Handle stream errors
    println!("Example 2: Stream error handling");
    let mut client = FraiseClient::connect("postgres://localhost/mydb").await?;

    let mut stream = client
        .query("user")
        .execute()
        .await?;

    let mut row_count = 0;
    let mut error_count = 0;

    while let Some(item) = stream.next().await {
        match item {
            Ok(json) => {
                // Process the JSON value
                println!("Row {}: {:?}", row_count, json);
                row_count += 1;
            }
            Err(e) => {
                error_count += 1;
                eprintln!("Error processing row: {}", e);
                eprintln!("Error category: {}", e.category());

                // Decide whether to continue or abort
                match e.category() {
                    "json_decode" => {
                        eprintln!("Skipping malformed JSON row");
                        continue;
                    }
                    "io" => {
                        eprintln!("Network error, aborting");
                        break;
                    }
                    _ => {
                        eprintln!("Unknown error, aborting");
                        break;
                    }
                }
            }
        }
    }

    println!("Processed {} rows, {} errors", row_count, error_count);

    client.close().await?;
    */

    println!("Error handling patterns in fraiseql-wire");
    println!();
    println!("1. Connection Errors:");
    println!("   - Check e.category() to understand error type");
    println!("   - Use e.is_retriable() to decide on retry logic");
    println!("   - Connection errors are typically non-retriable");
    println!();
    println!("2. Stream Errors:");
    println!("   - Handle each item individually");
    println!("   - json_decode errors: skip and continue");
    println!("   - io errors: may be retriable");
    println!("   - sql errors: typically abort");
    println!();
    println!("3. Error Categories:");
    println!("   - 'connection': connection issues");
    println!("   - 'authentication': auth failures");
    println!("   - 'protocol': protocol violations");
    println!("   - 'sql': SQL syntax or execution errors");
    println!("   - 'json_decode': invalid JSON in results");
    println!("   - 'io': network or I/O errors");
    println!("   - 'config': configuration issues");
    println!("   - 'invalid_schema': schema constraint violations");
    println!();
    println!("4. Retriable vs Non-Retriable:");
    println!("   - Retriable: Io, ConnectionClosed");
    println!("   - Non-retriable: all others");
    println!();
    println!("5. Using Error Information:");
    println!("   - error.to_string() for user-facing messages");
    println!("   - error.category() for programmatic decisions");
    println!("   - error.is_retriable() for retry logic");
    println!();
    println!("See CONTRIBUTING.md for instructions on running with a real database.");

    Ok(())
}
