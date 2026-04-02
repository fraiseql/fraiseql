#![allow(clippy::unwrap_used)] // Reason: test code, panics are acceptable
#![allow(clippy::cast_possible_truncation)] // Reason: test uses usize→u32 for small test counts
#![allow(clippy::doc_markdown)] // Reason: test doc comments use non-standard code formatting
#![allow(clippy::format_push_string)] // Reason: test query builders use push_str(&format!()) for readability
#![allow(clippy::match_same_arms)] // Reason: test match arms are intentionally explicit
#![allow(missing_docs)] // Reason: test binary does not require crate-level documentation
#[cfg(all(feature = "wire-backend", feature = "test-postgres"))]
#[tokio::test]
async fn test_wire_connection() {
    let conn_str = std::env::var("DATABASE_URL")
        .unwrap_or_else(|_| "postgresql:///fraiseql_bench".to_string());

    println!("Testing fraiseql-wire connection with: {}", conn_str);

    match fraiseql_wire::FraiseClient::connect(&conn_str).await {
        Ok(client) => {
            println!("✅ Connection successful!");
            drop(client);
        },
        Err(e) => {
            eprintln!("❌ Connection failed: {}", e);
            panic!("Connection failed: {}", e);
        },
    }
}
