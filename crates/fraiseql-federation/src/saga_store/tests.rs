use super::*;

    #[tokio::test]
    async fn test_postgres_connection() {
        if std::env::var("DATABASE_URL").is_err() {
            eprintln!("Skipping: DATABASE_URL not set");
            return;
        }
        // Use SAGA_STORE_TEST_URL (postgres-specific) so this test is unaffected
        // when the suite is invoked with DATABASE_URL pointing to MySQL/SQL Server.
        let url = std::env::var("SAGA_STORE_TEST_URL").unwrap_or_else(|_| {
            "postgresql://fraiseql_test:fraiseql_test_password@localhost:5433/test_fraiseql"
                .to_string()
        });
        let store = PostgresSagaStore::new(&url).await.expect("Failed to create store");
        store.health_check().await.expect("Health check failed");
    }
