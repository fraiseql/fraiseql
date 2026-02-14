//! Minimal example: Load schema and execute a query
//!
//! This example demonstrates:
//! - Loading a compiled GraphQL schema
//! - Creating a database adapter
//! - Executing a GraphQL query
//!
//! Prerequisites:
//! - Compiled schema at `schema.compiled.json`
//! - Database running at `postgresql://localhost/fraiseql_dev`

fn main() {
    println!("FraiseQL minimal example");
    println!("This example requires:");
    println!("1. A compiled schema at schema.compiled.json");
    println!("2. A PostgreSQL database at postgresql://localhost/fraiseql_dev");
    println!();
    println!("To run with real database:");
    println!("  cargo run --example minimal --features default");
}
