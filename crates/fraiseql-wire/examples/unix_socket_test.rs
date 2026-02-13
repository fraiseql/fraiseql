//! Test Unix socket connection
//!
//! This example demonstrates connecting to PostgreSQL via Unix socket.

use fraiseql_wire::FraiseClient;

#[tokio::main]
async fn main() {
    println!("Testing Unix socket connection with fraiseql-wire...\n");

    // First, show what connection string would be parsed
    let conn_str = "postgresql:///fraiseql_bench";
    println!("Connection string: {}", conn_str);
    println!("Expected behavior: Connect via Unix socket to fraiseql_bench database\n");

    match FraiseClient::connect(conn_str).await {
        Ok(_client) => {
            println!("✓ Successfully connected via Unix socket!");
            println!("\nConnection details:");
            println!("  - Transport: Unix socket");
            println!("  - Database: fraiseql_bench");
            println!("  - Port: 5432 (default)");
            println!("  - Socket path: /run/postgresql/.s.PGSQL.5432 (or /var/run/postgresql/.s.PGSQL.5432)");
        }
        Err(e) => {
            eprintln!("✗ Connection failed: {}", e);
            eprintln!("\nTroubleshooting:");
            eprintln!("  1. Check if PostgreSQL is running");
            eprintln!("  2. Verify socket location with: sudo ls -la /run/postgresql/");
            eprintln!("  3. Check database exists: psql -d fraiseql_bench");
            std::process::exit(1);
        }
    }
}
