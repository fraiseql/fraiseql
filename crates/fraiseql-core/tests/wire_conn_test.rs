#[cfg(feature = "wire-backend")]
#[tokio::test]
async fn test_wire_connection() {
    let conn_str = std::env::var("DATABASE_URL")
        .unwrap_or_else(|_| "postgresql:///fraiseql_bench".to_string());

    println!("Testing fraiseql-wire connection with: {}", conn_str);

    match fraiseql_wire::FraiseClient::connect(&conn_str).await {
        Ok(client) => {
            println!("✅ Connection successful!");
            drop(client);
        }
        Err(e) => {
            eprintln!("❌ Connection failed: {}", e);
            panic!("Connection failed: {}", e);
        }
    }
}
