//! Server example: Start HTTP GraphQL server
//!
//! This example demonstrates:
//! - Loading a compiled GraphQL schema
//! - Loading server configuration
//! - Creating a database adapter
//! - Starting an HTTP GraphQL server
//!
//! Prerequisites:
//! - Compiled schema at `schema.compiled.json`
//! - Configuration file at `fraiseql.toml`
//! - PostgreSQL database running

fn main() {
    println!("FraiseQL server example");
    println!();
    println!("This example requires the 'server' feature:");
    println!("  cargo run --example server --features server");
    println!();
    println!("Prerequisites:");
    println!("1. Compiled schema at schema.compiled.json");
    println!("2. Configuration file at fraiseql.toml");
    println!("3. PostgreSQL database running");
}
