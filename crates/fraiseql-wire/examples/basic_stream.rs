//! Basic streaming example

use fraiseql_wire::Result;

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt::init();

    println!("fraiseql-wire v{}", fraiseql_wire::VERSION);

    // Example usage (commented out - requires Postgres)
    /*
    use futures::StreamExt;

    let client = fraiseql_wire::FraiseClient::connect("postgres://localhost/mydb").await?;

    // Hybrid filtering: SQL reduces data over wire, Rust refines on client
    let mut stream = client
        .query("user")
        .where_sql("data->>'type' = 'customer'")  // Reduce data over wire
        .where_rust(|json| {
            // Application-level filtering
            json["lifetime_value"].as_f64().unwrap_or(0.0) > 10_000.0
        })
        .order_by("data->>'name' COLLATE \"C\" ASC")
        .chunk_size(256)
        .execute()
        .await?;

    let mut count = 0;
    while let Some(item) = stream.next().await {
        let json = item?;
        println!("{}", json);
        count += 1;
    }

    println!("Processed {} rows", count);
    */

    println!("See tests/integration.rs for working examples");

    Ok(())
}
