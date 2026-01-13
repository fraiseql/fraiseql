//! Example: Connecting to Postgres with TLS encryption
//!
//! This example demonstrates how to use TLS to securely connect to a remote Postgres server.
//!
//! # Usage
//!
//! ```bash
//! # Production: connect to a remote server with TLS (system root certificates)
//! FRAISEQL_CONNECTION_STRING="postgres://user:password@secure.db.example.com/mydb" \
//! cargo run --example tls
//!
//! # Development: connect with self-signed certificate
//! FRAISEQL_CONNECTION_STRING="postgres://user:password@localhost/mydb" \
//! cargo run --example tls
//! ```
//!
//! # Notes
//!
//! - System root certificates are used by default (via `rustls-native-certs`)
//! - For custom CA certificates, use `TlsConfig::builder().ca_cert_path(...)`
//! - Hostname verification is enabled by default (recommended for production)
//! - For development with self-signed certificates, use `danger_accept_invalid_certs(true)`

use fraiseql_wire::{FraiseClient, connection::TlsConfig};
use futures::stream::StreamExt;
use std::env;

#[tokio::main]
async fn main() -> fraiseql_wire::Result<()> {
    // Get connection string from environment or use example
    let connection_string = env::var("FRAISEQL_CONNECTION_STRING")
        .unwrap_or_else(|_| "postgres://localhost/fraiseql".to_string());

    println!("Connecting to: {}", connection_string);
    println!();

    // Create TLS configuration
    let tls = TlsConfig::builder()
        .verify_hostname(true)  // Verify server certificate matches hostname (production)
        .build()?;

    println!("✓ TLS configuration created");
    println!("  - System root certificates: enabled");
    println!("  - Hostname verification: enabled");
    println!();

    // Connect with TLS
    println!("Attempting TLS connection...");
    match FraiseClient::connect_tls(&connection_string, tls).await {
        Ok(client) => {
            println!("✓ Connected successfully!");
            println!();
            println!("Example: Querying users with TLS");
            println!();

            // Execute a simple query
            let mut stream = client
                .query("user")
                .chunk_size(256)
                .execute()
                .await?;

            println!("Results:");
            let mut count = 0;
            while let Some(result) = stream.next().await {
                let json = result?;
                println!("  {}", json);
                count += 1;

                // Limit output to first 5 rows for demo
                if count >= 5 {
                    println!("  ... (more results available)");
                    break;
                }
            }

            if count == 0 {
                println!("  (no results)");
            }

            println!();
            println!("✓ Query completed successfully over TLS");
        }
        Err(e) => {
            eprintln!("✗ Connection failed: {}", e);
            eprintln!();
            eprintln!("Troubleshooting tips:");
            eprintln!("  1. Ensure Postgres is running and accessible");
            eprintln!("  2. Check that TLS is enabled on the server");
            eprintln!("  3. For self-signed certificates, use:");
            eprintln!("     .danger_accept_invalid_certs(true)");
            eprintln!("     .danger_accept_invalid_hostnames(true)");
            eprintln!("  4. For custom CA certificates, use:");
            eprintln!("     .ca_cert_path(\"/path/to/ca.pem\")");
            return Err(e);
        }
    }

    Ok(())
}
