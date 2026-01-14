//! Test querying views with fraiseql-wire

#[cfg(feature = "wire-backend")]
mod wire_view_tests {
    use fraiseql_core::db::{DatabaseAdapter, FraiseWireAdapter};

    #[tokio::test]
    async fn test_query_v_users_view() {
        let conn_str = std::env::var("DATABASE_URL")
            .unwrap_or_else(|_| "postgresql:///fraiseql_bench".to_string());

        println!("Connecting to: {}", conn_str);

        let adapter = FraiseWireAdapter::new(&conn_str).with_chunk_size(1024);

        println!("Querying v_users with limit 10...");

        let results = adapter
            .execute_where_query("v_users", None, Some(10), None)
            .await;

        match &results {
            Ok(rows) => {
                println!("SUCCESS: Got {} rows", rows.len());
                if let Some(first) = rows.first() {
                    println!("First row: {:?}", first);
                }
            }
            Err(e) => {
                println!("ERROR: {}", e);
            }
        }

        assert!(results.is_ok(), "Query should succeed");
        assert_eq!(results.unwrap().len(), 10, "Should return 10 rows");
    }
}
