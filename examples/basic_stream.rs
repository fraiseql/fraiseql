//! Basic streaming example

use fraiseql_wire::Result;

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt::init();

    println!("fraiseql-wire v{}", fraiseql_wire::VERSION);

    // Example usage (commented out - requires Postgres)
    /*
    let mut client = FraiseClient::connect("postgres://localhost/mydb").await?;

    let mut stream = client
        .query("user")
        .where_sql("data->>'status' = 'active'")
        .order_by("data->>'name' ASC")
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

    client.close().await?;
    */

    println!("See tests/integration.rs for working examples");

    Ok(())
}
