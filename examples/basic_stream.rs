//! Basic streaming example (placeholder for later phases)

use fraiseql_wire::Result;

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt::init();

    println!("fraiseql-wire v{}", fraiseql_wire::VERSION);
    println!("Example will be implemented in later phases");

    Ok(())
}
