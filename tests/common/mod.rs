//! Common test utilities and helpers
//!
//! This module provides shared infrastructure for integration and end-to-end tests.

use std::sync::Once;
use tracing_subscriber::{fmt, EnvFilter};

static INIT: Once = Once::new();

/// Initialize test logging
///
/// Call this at the beginning of tests that need logging.
/// Uses the RUST_LOG environment variable to control log levels.
///
/// # Example
///
/// ```
/// use tests::common::init_test_logging;
///
/// #[tokio::test]
/// async fn my_test() {
///     init_test_logging();
///     // test code...
/// }
/// ```
pub fn init_test_logging() {
    INIT.call_once(|| {
        let filter = EnvFilter::try_from_default_env()
            .unwrap_or_else(|_| EnvFilter::new("debug"));

        fmt()
            .with_env_filter(filter)
            .with_test_writer()
            .init();
    });
}

/// Get database URL from environment or default
pub fn get_database_url() -> String {
    std::env::var("DATABASE_URL")
        .unwrap_or_else(|_| "postgresql://postgres:postgres@localhost:5432/fraiseql_test".to_string())
}

/// Database test helper
pub mod db {
    use super::*;

    /// Create a test database connection
    pub async fn create_test_pool() -> deadpool_postgres::Pool {
        let config = deadpool_postgres::Config {
            host: Some("localhost".to_string()),
            port: Some(5432),
            dbname: Some("fraiseql_test".to_string()),
            user: Some("postgres".to_string()),
            password: Some("postgres".to_string()),
            ..Default::default()
        };

        config
            .create_pool(
                Some(deadpool_postgres::Runtime::Tokio1),
                tokio_postgres::NoTls,
            )
            .expect("Failed to create test pool")
    }

    /// Execute SQL and ignore errors (for cleanup)
    pub async fn execute_sql_ignore_errors(
        pool: &deadpool_postgres::Pool,
        sql: &str,
    ) {
        if let Ok(client) = pool.get().await {
            let _ = client.execute(sql, &[]).await;
        }
    }

    /// Clean up test database
    pub async fn cleanup_test_db(pool: &deadpool_postgres::Pool) {
        execute_sql_ignore_errors(pool, "DROP SCHEMA IF EXISTS public CASCADE").await;
        execute_sql_ignore_errors(pool, "CREATE SCHEMA public").await;
    }
}

/// Schema test helper
pub mod schema {
    /// Load test schema from file
    pub fn load_test_schema(name: &str) -> String {
        let path = format!("tests/fixtures/schemas/{}.json", name);
        std::fs::read_to_string(&path)
            .unwrap_or_else(|_| panic!("Failed to load test schema: {}", path))
    }
}

/// Assertion helpers
pub mod assert {
    use serde_json::Value;

    /// Assert JSON contains expected fields
    pub fn json_contains(actual: &Value, expected: &Value) {
        match (actual, expected) {
            (Value::Object(actual_map), Value::Object(expected_map)) => {
                for (key, expected_value) in expected_map {
                    let actual_value = actual_map
                        .get(key)
                        .unwrap_or_else(|| panic!("Missing key: {}", key));
                    json_contains(actual_value, expected_value);
                }
            }
            (Value::Array(actual_arr), Value::Array(expected_arr)) => {
                assert_eq!(
                    actual_arr.len(),
                    expected_arr.len(),
                    "Array length mismatch"
                );
                for (actual_item, expected_item) in actual_arr.iter().zip(expected_arr.iter()) {
                    json_contains(actual_item, expected_item);
                }
            }
            _ => assert_eq!(actual, expected, "Value mismatch"),
        }
    }

    /// Assert GraphQL response has no errors
    pub fn no_graphql_errors(response: &Value) {
        if let Some(errors) = response.get("errors") {
            panic!("GraphQL errors: {:#?}", errors);
        }
    }

    /// Assert GraphQL response has specific error
    pub fn has_graphql_error(response: &Value, message_contains: &str) {
        let errors = response
            .get("errors")
            .expect("Expected errors field")
            .as_array()
            .expect("Errors should be array");

        assert!(
            errors.iter().any(|e| {
                e.get("message")
                    .and_then(|m| m.as_str())
                    .map(|s| s.contains(message_contains))
                    .unwrap_or(false)
            }),
            "Expected error containing '{}', got: {:#?}",
            message_contains,
            errors
        );
    }
}

/// Mock data generators
pub mod fixtures {
    use chrono::{DateTime, Utc};
    use serde_json::{json, Value};
    use uuid::Uuid;

    /// Generate random UUID
    pub fn random_uuid() -> Uuid {
        Uuid::new_v4()
    }

    /// Generate random user fixture
    pub fn random_user() -> Value {
        json!({
            "id": random_uuid(),
            "email": format!("user_{}@example.com", random_uuid()),
            "name": format!("Test User {}", random_uuid()),
            "created_at": Utc::now().to_rfc3339(),
        })
    }

    /// Generate random project fixture
    pub fn random_project() -> Value {
        json!({
            "id": random_uuid(),
            "name": format!("Project {}", random_uuid()),
            "description": "Test project description",
            "created_at": Utc::now().to_rfc3339(),
        })
    }
}

/// Benchmark helpers
pub mod bench {
    use std::time::{Duration, Instant};

    /// Simple benchmark runner
    pub fn bench<F>(name: &str, iterations: usize, mut f: F) -> Duration
    where
        F: FnMut(),
    {
        // Warmup
        for _ in 0..10 {
            f();
        }

        // Benchmark
        let start = Instant::now();
        for _ in 0..iterations {
            f();
        }
        let elapsed = start.elapsed();

        let per_iteration = elapsed / iterations as u32;
        println!(
            "{}: {} iterations in {:?} ({:?}/iter)",
            name, iterations, elapsed, per_iteration
        );

        elapsed
    }

    /// Async benchmark runner
    pub async fn bench_async<F, Fut>(name: &str, iterations: usize, mut f: F) -> Duration
    where
        F: FnMut() -> Fut,
        Fut: std::future::Future<Output = ()>,
    {
        // Warmup
        for _ in 0..10 {
            f().await;
        }

        // Benchmark
        let start = Instant::now();
        for _ in 0..iterations {
            f().await;
        }
        let elapsed = start.elapsed();

        let per_iteration = elapsed / iterations as u32;
        println!(
            "{}: {} iterations in {:?} ({:?}/iter)",
            name, iterations, elapsed, per_iteration
        );

        elapsed
    }
}
