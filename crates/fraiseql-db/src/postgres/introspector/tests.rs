use super::*;

#[test]
fn test_is_indexed_column_name_human_readable() {
    // Valid human-readable patterns
    assert!(PostgresIntrospector::is_indexed_column_name("items__product"));
    assert!(PostgresIntrospector::is_indexed_column_name("items__product__category"));
    assert!(PostgresIntrospector::is_indexed_column_name("items__product__category__code"));
    assert!(PostgresIntrospector::is_indexed_column_name("order_items__product_name"));

    // Invalid patterns
    assert!(!PostgresIntrospector::is_indexed_column_name("items"));
    assert!(!PostgresIntrospector::is_indexed_column_name("items_product")); // single underscore
    assert!(!PostgresIntrospector::is_indexed_column_name("__items")); // empty first segment
    assert!(!PostgresIntrospector::is_indexed_column_name("items__")); // empty last segment
}

#[test]
fn test_is_indexed_column_name_entity_id() {
    // Valid entity ID patterns
    assert!(PostgresIntrospector::is_indexed_column_name("f200100__code"));
    assert!(PostgresIntrospector::is_indexed_column_name("f1__name"));
    assert!(PostgresIntrospector::is_indexed_column_name("f123456789__field"));

    // Invalid entity ID patterns (that also aren't valid human-readable)
    assert!(!PostgresIntrospector::is_indexed_column_name("f__code")); // no digits after 'f', and 'f' alone is reserved

    // Note: fx123__code IS valid as a human-readable pattern (fx123 is a valid identifier)
    assert!(PostgresIntrospector::is_indexed_column_name("fx123__code")); // valid as human-readable
}

#[cfg(feature = "test-postgres")]
mod integration_tests {
    use deadpool_postgres::{Config, ManagerConfig, RecyclingMethod, Runtime};
    use tokio_postgres::NoTls;

    use super::*;
    use crate::postgres::PostgresAdapter;

    // Test DB URL from the `fraiseql_test_support` env-URL harness (`DATABASE_URL`), so this
    // suite runs against a Dagger-bound service (local == CI) instead of a hardcoded host.
    fn test_db_url() -> String {
        fraiseql_test_support::database_url()
    }

    // Helper to create test introspector
    async fn create_test_introspector() -> PostgresIntrospector {
        let _adapter = PostgresAdapter::new(&test_db_url())
            .await
            .expect("Failed to create test adapter");

        // Extract pool from adapter (we need a way to get the pool)
        // For now, create a new pool directly

        let mut cfg = Config::new();
        cfg.url = Some(test_db_url());
        cfg.manager = Some(ManagerConfig {
            recycling_method: RecyclingMethod::Fast,
        });
        cfg.pool = Some(deadpool_postgres::PoolConfig::new(10));

        let pool = cfg.create_pool(Some(Runtime::Tokio1), NoTls).expect("Failed to create pool");

        PostgresIntrospector::new(pool)
    }

    #[tokio::test]
    async fn test_get_columns_tf_sales() {
        let introspector = create_test_introspector().await;

        let columns = introspector.get_columns("tf_sales").await.expect("Failed to get columns");

        // Should have: id, revenue, quantity, cost, discount, data, customer_id, product_id,
        // occurred_at, created_at
        assert!(columns.len() >= 10, "Expected at least 10 columns, got {}", columns.len());

        // Check for key columns
        let column_names: Vec<String> = columns.iter().map(|(name, _, _)| name.clone()).collect();
        assert!(column_names.contains(&"revenue".to_string()));
        assert!(column_names.contains(&"quantity".to_string()));
        assert!(column_names.contains(&"data".to_string()));
        assert!(column_names.contains(&"customer_id".to_string()));
    }

    #[tokio::test]
    async fn test_get_indexed_columns_tf_sales() {
        let introspector = create_test_introspector().await;

        let indexed = introspector
            .get_indexed_columns("tf_sales")
            .await
            .expect("Failed to get indexed columns");

        // Should have indexes on: id (PK), customer_id, product_id, occurred_at, data (GIN)
        assert!(indexed.len() >= 4, "Expected at least 4 indexed columns, got {}", indexed.len());

        assert!(indexed.contains(&"customer_id".to_string()));
        assert!(indexed.contains(&"product_id".to_string()));
        assert!(indexed.contains(&"occurred_at".to_string()));
    }

    #[tokio::test]
    async fn test_database_type() {
        let introspector = create_test_introspector().await;
        assert_eq!(introspector.database_type(), DatabaseType::PostgreSQL);
    }
}
