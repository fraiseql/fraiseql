//! Test helpers for observer E2E integration testing.
//!
//! Provides utilities for:
//! - Database schema setup (observer tables)
//! - Mock webhook server management
//! - Observer configuration builders
//! - Change log entry insertion with Debezium envelopes
//! - Assertion helpers

#![allow(dead_code)] // Some helpers may not be used in all test files

use serde_json::json;
use sqlx::PgPool;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::Mutex;
use wiremock::{Mock, MockServer, ResponseTemplate};
use wiremock::matchers::{method, path};

/// Get database URL from environment or use default
pub fn get_database_url() -> String {
    std::env::var("DATABASE_URL")
        .unwrap_or_else(|_| "postgresql://postgres:postgres@localhost/fraiseql_test".to_string())
}

/// Create a PostgreSQL connection pool for tests
pub async fn create_test_pool() -> PgPool {
    sqlx::postgres::PgPoolOptions::new()
        .max_connections(5)
        .connect(&get_database_url())
        .await
        .expect("Failed to connect to test database")
}

/// Set up all observer-related tables for testing
pub async fn setup_observer_schema(pool: &PgPool) -> Result<(), sqlx::Error> {
    // 1. Create core schema
    sqlx::query("CREATE SCHEMA IF NOT EXISTS core")
        .execute(pool)
        .await?;

    // 2. Create tb_entity_change_log (with Debezium envelope)
    sqlx::query(
        r#"
        CREATE TABLE IF NOT EXISTS core.tb_entity_change_log (
            pk_entity_change_log BIGSERIAL PRIMARY KEY,
            id UUID NOT NULL DEFAULT gen_random_uuid(),
            fk_customer_org TEXT,
            fk_contact TEXT,
            object_type TEXT NOT NULL,
            object_id TEXT NOT NULL,
            modification_type TEXT NOT NULL,
            change_status TEXT,
            object_data JSONB NOT NULL,
            extra_metadata JSONB,
            created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
        )
        "#,
    )
    .execute(pool)
    .await?;

    // 3. Create observer management tables
    sqlx::query(
        r#"
        CREATE TABLE IF NOT EXISTS tb_observer (
            pk_observer BIGSERIAL PRIMARY KEY,
            id UUID NOT NULL DEFAULT gen_random_uuid() UNIQUE,
            name VARCHAR(255) NOT NULL,
            description TEXT,
            entity_type VARCHAR(255),
            event_type VARCHAR(50),
            condition_expression TEXT,
            actions JSONB NOT NULL DEFAULT '[]',
            enabled BOOLEAN NOT NULL DEFAULT true,
            priority INTEGER NOT NULL DEFAULT 100,
            retry_config JSONB NOT NULL DEFAULT '{"max_attempts": 3, "backoff": "exponential", "initial_delay_ms": 100}',
            timeout_ms INTEGER NOT NULL DEFAULT 30000,
            fk_customer_org BIGINT,
            created_by VARCHAR(255),
            updated_by VARCHAR(255),
            created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
            updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
            deleted_at TIMESTAMPTZ
        )
        "#,
    )
    .execute(pool)
    .await?;

    // 4. Create observer log table
    sqlx::query(
        r#"
        CREATE TABLE IF NOT EXISTS tb_observer_log (
            pk_observer_log BIGSERIAL PRIMARY KEY,
            id UUID NOT NULL DEFAULT gen_random_uuid() UNIQUE,
            fk_observer BIGINT NOT NULL REFERENCES tb_observer(pk_observer),
            fk_entity_change_log BIGINT,
            event_id UUID NOT NULL,
            entity_type VARCHAR(255) NOT NULL,
            entity_id VARCHAR(255) NOT NULL,
            event_type VARCHAR(50) NOT NULL,
            status VARCHAR(50) NOT NULL,
            action_index INTEGER,
            action_type VARCHAR(50),
            started_at TIMESTAMPTZ,
            completed_at TIMESTAMPTZ,
            duration_ms INTEGER,
            error_code VARCHAR(100),
            error_message TEXT,
            attempt_number INTEGER NOT NULL DEFAULT 1,
            max_attempts INTEGER NOT NULL DEFAULT 3,
            trace_id VARCHAR(64),
            created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
        )
        "#,
    )
    .execute(pool)
    .await?;

    // 5. Create checkpoint table
    sqlx::query(
        r#"
        CREATE TABLE IF NOT EXISTS observer_checkpoints (
            listener_id VARCHAR(255) PRIMARY KEY,
            last_processed_id BIGINT NOT NULL DEFAULT 0,
            last_processed_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
            batch_size INT NOT NULL DEFAULT 100,
            event_count INT NOT NULL DEFAULT 0,
            consecutive_errors INT NOT NULL DEFAULT 0,
            last_error TEXT,
            updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
        )
        "#,
    )
    .execute(pool)
    .await?;

    Ok(())
}

/// Clean up test data for a specific test run
pub async fn cleanup_test_data(pool: &PgPool, test_id: &str) -> Result<(), sqlx::Error> {
    // Delete in dependency order
    sqlx::query("DELETE FROM tb_observer_log WHERE event_id::text LIKE $1")
        .bind(format!("%{test_id}%"))
        .execute(pool)
        .await
        .ok();

    sqlx::query("DELETE FROM tb_observer WHERE name LIKE $1")
        .bind(format!("%{test_id}%"))
        .execute(pool)
        .await
        .ok();

    sqlx::query("DELETE FROM core.tb_entity_change_log WHERE object_type LIKE $1")
        .bind(format!("%{test_id}%"))
        .execute(pool)
        .await
        .ok();

    sqlx::query("DELETE FROM observer_checkpoints WHERE listener_id LIKE $1")
        .bind(format!("%{test_id}%"))
        .execute(pool)
        .await
        .ok();

    Ok(())
}

/// Mock webhook server with request tracking
pub struct MockWebhookServer {
    pub server: MockServer,
    requests: Arc<Mutex<Vec<serde_json::Value>>>,
}

impl MockWebhookServer {
    /// Create a new mock webhook server
    pub async fn start() -> Self {
        let server = MockServer::start().await;
        let requests = Arc::new(Mutex::new(Vec::new()));

        Self { server, requests }
    }

    /// Configure success response (200 OK)
    pub async fn mock_success(&self) {
        let requests = Arc::clone(&self.requests);

        Mock::given(method("POST"))
            .and(path("/webhook"))
            .respond_with(move |req: &wiremock::Request| {
                // Capture request body
                let body: serde_json::Value = serde_json::from_slice(&req.body)
                    .unwrap_or_else(|_| serde_json::json!({}));

                // Use try_lock() instead of blocking_lock() to avoid panic in async context
                if let Ok(mut reqs) = requests.try_lock() {
                    reqs.push(body.clone());
                }

                ResponseTemplate::new(200)
                    .set_body_json(serde_json::json!({"status": "success"}))
            })
            .mount(&self.server)
            .await;
    }

    /// Configure failure response (500 Internal Server Error)
    pub async fn mock_failure(&self, status_code: u16) {
        Mock::given(method("POST"))
            .and(path("/webhook"))
            .respond_with(ResponseTemplate::new(status_code))
            .mount(&self.server)
            .await;
    }

    /// Configure transient failure (fails N times, then succeeds)
    pub async fn mock_transient_failure(&self, fail_count: usize) {
        let counter = Arc::new(Mutex::new(0));
        let requests = Arc::clone(&self.requests);

        Mock::given(method("POST"))
            .and(path("/webhook"))
            .respond_with(move |req: &wiremock::Request| {
                let mut count = counter.try_lock().expect("Counter lock failed");
                *count += 1;
                let current_count = *count;
                drop(count); // Release lock before other operations

                if current_count <= fail_count {
                    ResponseTemplate::new(500)
                } else {
                    let body: serde_json::Value = serde_json::from_slice(&req.body)
                        .unwrap_or_else(|_| serde_json::json!({}));

                    if let Ok(mut reqs) = requests.try_lock() {
                        reqs.push(body.clone());
                    }

                    ResponseTemplate::new(200)
                }
            })
            .mount(&self.server)
            .await;
    }

    /// Get webhook URL
    pub fn webhook_url(&self) -> String {
        format!("{}/webhook", self.server.uri())
    }

    /// Get received requests
    pub async fn received_requests(&self) -> Vec<serde_json::Value> {
        self.requests.lock().await.clone()
    }

    /// Get request count
    pub async fn request_count(&self) -> usize {
        self.requests.lock().await.len()
    }

    /// Mock delayed response (responds after specified duration)
    pub async fn mock_delayed_response(&self, delay: Duration) {
        let requests = Arc::clone(&self.requests);

        Mock::given(method("POST"))
            .and(path("/webhook"))
            .respond_with(move |req: &wiremock::Request| {
                let body: serde_json::Value = serde_json::from_slice(&req.body)
                    .unwrap_or_else(|_| serde_json::json!({}));

                if let Ok(mut reqs) = requests.try_lock() {
                    reqs.push(body.clone());
                }

                ResponseTemplate::new(200)
                    .set_delay(delay)
                    .set_body_json(serde_json::json!({"status": "success"}))
            })
            .mount(&self.server)
            .await;
    }

    /// Reset mock server state (clear requests)
    pub async fn reset(&self) {
        let mut reqs = self.requests.lock().await;
        reqs.clear();
    }
}

/// Insert an observer configuration into the database
pub async fn create_test_observer(
    pool: &PgPool,
    name: &str,
    entity_type: Option<&str>,
    event_type: Option<&str>,
    condition: Option<&str>,
    webhook_url: &str,
) -> Result<i64, sqlx::Error> {
    let actions = json!([
        {
            "type": "webhook",
            "url": webhook_url,
            "method": "POST",
            "headers": {
                "Content-Type": "application/json"
            }
        }
    ]);

    let retry_config = json!({
        "max_attempts": 3,
        "backoff": "exponential",
        "initial_delay_ms": 100,
        "max_delay_ms": 5000
    });

    let row: (i64,) = sqlx::query_as(
        r#"
        INSERT INTO tb_observer (
            name, entity_type, event_type, condition_expression,
            actions, retry_config, enabled
        )
        VALUES ($1, $2, $3, $4, $5, $6, true)
        RETURNING pk_observer
        "#,
    )
    .bind(name)
    .bind(entity_type)
    .bind(event_type)
    .bind(condition)
    .bind(actions)
    .bind(retry_config)
    .fetch_one(pool)
    .await?;

    Ok(row.0)
}

/// Insert a change log entry with Debezium envelope
pub async fn insert_change_log_entry(
    pool: &PgPool,
    event_type: &str,  // INSERT, UPDATE, DELETE
    entity_type: &str,
    entity_id: &str,
    data: serde_json::Value,
    before_data: Option<serde_json::Value>,
) -> Result<i64, sqlx::Error> {
    // Convert event type to Debezium operation code
    let op_code = match event_type.to_uppercase().as_str() {
        "INSERT" | "C" => "c",
        "UPDATE" | "U" => "u",
        "DELETE" | "D" => "d",
        _ => "c", // Default to create
    };

    // Build Debezium envelope
    let object_data = json!({
        "op": op_code,
        "before": before_data,
        "after": data,
        "source": {
            "db": "fraiseql_test",
            "table": entity_type
        },
        "ts_ms": chrono::Utc::now().timestamp_millis()
    });

    let row: (i64,) = sqlx::query_as(
        r#"
        INSERT INTO core.tb_entity_change_log (
            object_type, object_id, modification_type, object_data
        )
        VALUES ($1, $2, $3, $4)
        RETURNING pk_entity_change_log
        "#,
    )
    .bind(entity_type)
    .bind(entity_id)
    .bind(event_type)
    .bind(object_data)
    .fetch_one(pool)
    .await?;

    Ok(row.0)
}

/// Wait for webhook server to receive N requests
pub async fn wait_for_webhook(
    server: &MockWebhookServer,
    expected_count: usize,
    timeout: Duration,
) {
    let start = tokio::time::Instant::now();

    loop {
        if start.elapsed() > timeout {
            panic!(
                "Timeout waiting for {} webhook calls. Got: {}",
                expected_count,
                server.request_count().await
            );
        }

        if server.request_count().await >= expected_count {
            break;
        }

        tokio::time::sleep(Duration::from_millis(100)).await;
    }
}

/// Assert observer log entry
pub async fn assert_observer_log(
    pool: &PgPool,
    entity_id: &str,
    expected_status: &str,
    expected_attempts: Option<i32>,
) {
    let row: Option<(String, i32, Option<i32>)> = sqlx::query_as(
        r#"
        SELECT status, attempt_number, duration_ms
        FROM tb_observer_log
        WHERE entity_id = $1
        ORDER BY created_at DESC
        LIMIT 1
        "#,
    )
    .bind(entity_id)
    .fetch_optional(pool)
    .await
    .unwrap();

    assert!(
        row.is_some(),
        "No observer log entry found for entity {}",
        entity_id
    );
    let (status, attempts, duration) = row.unwrap();
    assert_eq!(
        status, expected_status,
        "Expected status {}, got {}",
        expected_status, status
    );

    if let Some(expected) = expected_attempts {
        assert_eq!(
            attempts, expected,
            "Expected {} attempts, got {}",
            expected, attempts
        );
    }

    assert!(
        duration.is_some() && duration.unwrap() > 0,
        "Duration should be positive"
    );
}

/// Assert webhook payload structure
pub fn assert_webhook_payload(
    payload: &serde_json::Value,
    expected_entity_id: &str,
    expected_field_value: Option<(&str, &str)>,
) {
    assert!(
        payload["after"]["id"].as_str().is_some(),
        "Webhook payload missing after.id"
    );
    assert_eq!(
        payload["after"]["id"].as_str().unwrap(),
        expected_entity_id
    );

    if let Some((field, value)) = expected_field_value {
        assert_eq!(
            payload["after"][field].as_str().unwrap(),
            value,
            "Field {} mismatch",
            field
        );
    }
}

/// Get count of observer logs with specific status
pub async fn get_observer_log_count(
    pool: &PgPool,
    status: &str,
) -> Result<i64, sqlx::Error> {
    let count: (i64,) = sqlx::query_as(
        "SELECT COUNT(*) FROM tb_observer_log WHERE status = $1",
    )
    .bind(status)
    .fetch_one(pool)
    .await?;

    Ok(count.0)
}

/// Get all observer logs for an entity
pub async fn get_observer_logs_for_entity(
    pool: &PgPool,
    entity_id: &str,
) -> Result<Vec<(String, i32, Option<i32>)>, sqlx::Error> {
    sqlx::query_as(
        r#"
        SELECT status, attempt_number, duration_ms
        FROM tb_observer_log
        WHERE entity_id = $1
        ORDER BY attempt_number ASC
        "#,
    )
    .bind(entity_id)
    .fetch_all(pool)
    .await
}

/// Check if checkpoint exists for a listener
pub async fn check_checkpoint_exists(
    pool: &PgPool,
    listener_id: &str,
) -> Result<bool, sqlx::Error> {
    let row: Option<(i32,)> = sqlx::query_as(
        "SELECT COUNT(*) FROM observer_checkpoints WHERE listener_id = $1",
    )
    .bind(listener_id)
    .fetch_optional(pool)
    .await?;

    Ok(row.map(|(count,)| count > 0).unwrap_or(false))
}

/// Get checkpoint value (last processed ID) for a listener
pub async fn get_checkpoint_value(
    pool: &PgPool,
    listener_id: &str,
) -> Result<i64, sqlx::Error> {
    let row: Option<(i64,)> = sqlx::query_as(
        "SELECT last_processed_id FROM observer_checkpoints WHERE listener_id = $1",
    )
    .bind(listener_id)
    .fetch_optional(pool)
    .await?;

    Ok(row.map(|(id,)| id).unwrap_or(0))
}

/// Wait for multiple runtime events to be recorded
pub async fn wait_for_runtime_events(
    pool: &PgPool,
    expected_status: &str,
    expected_count: i64,
    timeout: Duration,
) {
    let start = tokio::time::Instant::now();

    loop {
        if start.elapsed() > timeout {
            panic!(
                "Timeout waiting for {} events with status {}. Got: {:?}",
                expected_count,
                expected_status,
                get_observer_log_count(pool, expected_status)
                    .await
                    .unwrap_or(0)
            );
        }

        if let Ok(count) = get_observer_log_count(pool, expected_status).await {
            if count >= expected_count {
                break;
            }
        }

        tokio::time::sleep(Duration::from_millis(100)).await;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_get_database_url() {
        let url = get_database_url();
        assert!(!url.is_empty());
    }

    #[tokio::test]
    async fn test_mock_webhook_server_creation() {
        let server = MockWebhookServer::start().await;
        let url = server.webhook_url();
        assert!(url.contains("http://"));
    }

    #[tokio::test]
    async fn test_webhook_url_format() {
        let server = MockWebhookServer::start().await;
        let url = server.webhook_url();
        assert!(url.ends_with("/webhook"));
    }
}
