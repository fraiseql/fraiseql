//! Test fraiseql-wire directly without adapter layer
//!
//! Run with: cargo test -p fraiseql-core --features wire-backend,test-postgres --test
//! wire_direct_test

#[cfg(all(feature = "wire-backend", feature = "test-postgres"))]
mod wire_direct_tests {
    use fraiseql_wire::FraiseClient;
    use futures::StreamExt;

    #[tokio::test]
    async fn test_direct_v_users_query() {
        let conn_str = std::env::var("DATABASE_URL").unwrap_or_else(|_| {
            "postgresql://fraiseql_test:fraiseql_test_password@localhost:5433/test_fraiseql"
                .to_string()
        });

        println!("Connecting to: {}", conn_str);

        let client = FraiseClient::connect(&conn_str).await.unwrap();

        println!("Querying v_users directly...");

        // Query v_users directly
        let stream_result =
            client.query::<serde_json::Value>("v_users").chunk_size(1024).execute().await;

        match &stream_result {
            Ok(_) => println!("Query executed successfully"),
            Err(e) => println!("Query failed with error: {:?}", e),
        }

        let mut stream = stream_result.unwrap();

        let mut count = 0;
        while let Some(result) = stream.next().await {
            match result {
                Ok(_item) => {
                    count += 1;
                    if count >= 10 {
                        break;
                    }
                },
                Err(e) => {
                    println!("ERROR: {}", e);
                    panic!("Query failed: {}", e);
                },
            }
        }

        println!("SUCCESS: Got {} rows", count);
        assert_eq!(count, 10, "Should get 10 rows");
    }
}
