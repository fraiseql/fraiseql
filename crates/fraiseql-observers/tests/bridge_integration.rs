//! PostgreSQL → NATS bridge integration tests
//!
//! These tests verify the bridge implementation against real PostgreSQL and NATS servers.
//!
//! ## Running Tests
//!
//! 1. Start PostgreSQL and NATS:
//!
//!    ```bash
//!    docker run -d --name postgres -p 5432:5432 -e \
//!      POSTGRES_PASSWORD=postgres postgres:16
//!    docker run -d --name nats -p 4222:4222 nats:latest -js
//!    ```
//!
//! 2. Create the test database and schema:
//!
//!    ```bash
//!    psql -h localhost -U postgres -c "CREATE DATABASE fraiseql_test"
//!    psql -h localhost -U postgres -d fraiseql_test -f migrations/03_add_nats_transport.sql
//!    ```
//!
//! 3. Run tests:
//!
//!    ```bash
//!    DATABASE_URL=postgres://postgres:postgres@localhost/fraiseql_test \
//!    cargo test --test bridge_integration --features nats -- --ignored
//!    ```

#![allow(unused_imports)]
#![cfg(feature = "nats")]

use std::{sync::Arc, time::Duration};

use uuid::Uuid;

#[cfg(test)]
mod bridge_tests {
    use fraiseql_observers::{
        event::{EntityEvent, EventKind},
        transport::{
            BridgeConfig, CheckpointStore, EventFilter, EventTransport, NatsConfig, NatsTransport,
            PostgresCheckpointStore,
        },
    };
    use futures::StreamExt;
    use serde_json::json;
    use sqlx::{PgPool, postgres::PgPoolOptions};

    use super::*;

    /// Get database URL from environment
    fn get_database_url() -> String {
        std::env::var("DATABASE_URL")
            .unwrap_or_else(|_| "postgres://postgres:postgres@localhost/fraiseql_test".to_string())
    }

    /// Create a test database pool
    async fn create_test_pool() -> PgPool {
        PgPoolOptions::new()
            .max_connections(5)
            .connect(&get_database_url())
            .await
            .expect("Failed to connect to test database")
    }

    /// Set up test schema (checkpoint table and change log columns)
    async fn setup_test_schema(pool: &PgPool) {
        // Create core schema if not exists
        sqlx::query("CREATE SCHEMA IF NOT EXISTS core")
            .execute(pool)
            .await
            .expect("Failed to create core schema");

        // Create checkpoint table
        sqlx::query(
            r"
            CREATE TABLE IF NOT EXISTS core.tb_transport_checkpoint (
                transport_name TEXT PRIMARY KEY,
                last_pk BIGINT NOT NULL,
                updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
            )
            ",
        )
        .execute(pool)
        .await
        .expect("Failed to create checkpoint table");

        // Create change log table for testing
        sqlx::query(
            r"
            CREATE TABLE IF NOT EXISTS core.tb_entity_change_log (
                pk_entity_change_log BIGSERIAL PRIMARY KEY,
                id UUID NOT NULL DEFAULT gen_random_uuid(),
                fk_customer_org BIGINT,
                fk_contact BIGINT,
                object_type TEXT NOT NULL,
                object_id UUID NOT NULL,
                modification_type TEXT NOT NULL,
                change_status TEXT,
                object_data JSONB,
                extra_metadata JSONB,
                created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
                nats_published_at TIMESTAMPTZ,
                nats_event_id UUID
            )
            ",
        )
        .execute(pool)
        .await
        .expect("Failed to create change log table");
    }

    /// Clean up test data
    async fn cleanup_test_data(pool: &PgPool, test_id: &str) {
        // Delete checkpoint for this test
        sqlx::query("DELETE FROM core.tb_transport_checkpoint WHERE transport_name LIKE $1")
            .bind(format!("%{test_id}%"))
            .execute(pool)
            .await
            .ok();

        // Delete change log entries for this test
        sqlx::query("DELETE FROM core.tb_entity_change_log WHERE object_type LIKE $1")
            .bind(format!("%{test_id}%"))
            .execute(pool)
            .await
            .ok();
    }

    /// Insert test change log entries
    async fn insert_change_log_entries(pool: &PgPool, count: usize, object_type: &str) -> Vec<i64> {
        let mut pks = Vec::with_capacity(count);

        for i in 0..count {
            let pk: (i64,) = sqlx::query_as(
                r"
                INSERT INTO core.tb_entity_change_log
                    (object_type, object_id, modification_type, object_data)
                VALUES ($1, $2, 'INSERT', $3)
                RETURNING pk_entity_change_log
                ",
            )
            .bind(object_type)
            .bind(Uuid::new_v4())
            .bind(json!({"index": i}))
            .fetch_one(pool)
            .await
            .expect("Failed to insert change log entry");

            pks.push(pk.0);
        }

        pks
    }

    // =========================================================================
    // Checkpoint Store Tests
    // =========================================================================

    /// Test checkpoint store persistence (save and load)
    #[tokio::test]
    #[ignore = "requires PostgreSQL - run with: cargo test --test bridge_integration --features nats -- --ignored"]
    async fn test_checkpoint_store_persistence() {
        let pool = create_test_pool().await;
        setup_test_schema(&pool).await;

        let test_id = Uuid::new_v4().to_string();
        let transport_name = format!("test_checkpoint_{test_id}");

        let checkpoint_store = PostgresCheckpointStore::new(pool.clone());

        // Initially no checkpoint
        let cursor = checkpoint_store
            .get_checkpoint(&transport_name)
            .await
            .expect("get_checkpoint should succeed");
        assert_eq!(cursor, None, "New transport should have no checkpoint");

        // Save checkpoint
        checkpoint_store
            .save_checkpoint(&transport_name, 12345)
            .await
            .expect("save_checkpoint should succeed");

        // Retrieve checkpoint
        let cursor = checkpoint_store
            .get_checkpoint(&transport_name)
            .await
            .expect("get_checkpoint should succeed");
        assert_eq!(cursor, Some(12345), "Checkpoint should be 12345");

        // Update checkpoint (idempotent upsert)
        checkpoint_store
            .save_checkpoint(&transport_name, 23456)
            .await
            .expect("save_checkpoint should succeed");

        let cursor = checkpoint_store
            .get_checkpoint(&transport_name)
            .await
            .expect("get_checkpoint should succeed");
        assert_eq!(cursor, Some(23456), "Checkpoint should be updated to 23456");

        // Delete checkpoint
        checkpoint_store
            .delete_checkpoint(&transport_name)
            .await
            .expect("delete_checkpoint should succeed");

        let cursor = checkpoint_store
            .get_checkpoint(&transport_name)
            .await
            .expect("get_checkpoint should succeed");
        assert_eq!(cursor, None, "Checkpoint should be deleted");

        // Cleanup
        cleanup_test_data(&pool, &test_id).await;
    }

    /// Test checkpoint store handles concurrent updates
    #[tokio::test]
    #[ignore = "requires PostgreSQL - run with: cargo test --test bridge_integration --features nats -- --ignored"]
    async fn test_checkpoint_store_concurrent_updates() {
        let pool = create_test_pool().await;
        setup_test_schema(&pool).await;

        let test_id = Uuid::new_v4().to_string();
        let transport_name = format!("test_concurrent_{test_id}");

        let store1 = PostgresCheckpointStore::new(pool.clone());
        let store2 = PostgresCheckpointStore::new(pool.clone());

        // Concurrent saves - both should succeed (last writer wins)
        let (r1, r2) = tokio::join!(
            store1.save_checkpoint(&transport_name, 100),
            store2.save_checkpoint(&transport_name, 200)
        );

        assert!(r1.is_ok(), "First save should succeed");
        assert!(r2.is_ok(), "Second save should succeed");

        // One of them should have won
        let cursor = store1
            .get_checkpoint(&transport_name)
            .await
            .expect("get_checkpoint should succeed");
        assert!(cursor == Some(100) || cursor == Some(200), "Cursor should be 100 or 200");

        // Cleanup
        cleanup_test_data(&pool, &test_id).await;
    }

    // =========================================================================
    // Change Log Entry Tests
    // =========================================================================

    /// Test change log entry insertion and retrieval
    #[tokio::test]
    #[ignore = "requires PostgreSQL - run with: cargo test --test bridge_integration --features nats -- --ignored"]
    async fn test_change_log_entry_operations() {
        let pool = create_test_pool().await;
        setup_test_schema(&pool).await;

        let test_id = Uuid::new_v4().to_string();
        let object_type = format!("TestEntity_{test_id}");

        // Insert entries
        let pks = insert_change_log_entries(&pool, 5, &object_type).await;
        assert_eq!(pks.len(), 5, "Should insert 5 entries");

        // Verify PKs are monotonically increasing
        for i in 1..pks.len() {
            assert!(pks[i] > pks[i - 1], "PKs should be monotonically increasing");
        }

        // Query entries by cursor
        let entries: Vec<(i64, String)> = sqlx::query_as(
            r"
            SELECT pk_entity_change_log, object_type
            FROM core.tb_entity_change_log
            WHERE pk_entity_change_log > $1
            ORDER BY pk_entity_change_log ASC
            LIMIT 10
            ",
        )
        .bind(pks[1]) // Start after second entry
        .fetch_all(&pool)
        .await
        .expect("Query should succeed");

        assert_eq!(entries.len(), 3, "Should find 3 entries after cursor");
        assert_eq!(entries[0].0, pks[2], "First result should be third entry");

        // Cleanup
        cleanup_test_data(&pool, &test_id).await;
    }

    /// Test conditional `mark_published` prevents races
    #[tokio::test]
    #[ignore = "requires PostgreSQL - run with: cargo test --test bridge_integration --features nats -- --ignored"]
    async fn test_mark_published_conditional_update() {
        let pool = create_test_pool().await;
        setup_test_schema(&pool).await;

        let test_id = Uuid::new_v4().to_string();
        let object_type = format!("RaceTest_{test_id}");

        // Insert one entry
        let pks = insert_change_log_entries(&pool, 1, &object_type).await;
        let pk = pks[0];

        // First mark_published should succeed
        let event_id_1 = Uuid::new_v4();
        let result = sqlx::query(
            r"
            UPDATE core.tb_entity_change_log
            SET nats_published_at = NOW(),
                nats_event_id = $2
            WHERE pk_entity_change_log = $1
              AND nats_published_at IS NULL
            ",
        )
        .bind(pk)
        .bind(event_id_1)
        .execute(&pool)
        .await
        .expect("Query should succeed");

        assert_eq!(result.rows_affected(), 1, "First mark_published should affect 1 row");

        // Second mark_published should NOT update (already published)
        let event_id_2 = Uuid::new_v4();
        let result = sqlx::query(
            r"
            UPDATE core.tb_entity_change_log
            SET nats_published_at = NOW(),
                nats_event_id = $2
            WHERE pk_entity_change_log = $1
              AND nats_published_at IS NULL
            ",
        )
        .bind(pk)
        .bind(event_id_2)
        .execute(&pool)
        .await
        .expect("Query should succeed");

        assert_eq!(
            result.rows_affected(),
            0,
            "Second mark_published should affect 0 rows (already published)"
        );

        // Verify the first event_id was preserved
        let (stored_event_id,): (Uuid,) = sqlx::query_as(
            "SELECT nats_event_id FROM core.tb_entity_change_log WHERE pk_entity_change_log = $1",
        )
        .bind(pk)
        .fetch_one(&pool)
        .await
        .expect("Query should succeed");

        assert_eq!(stored_event_id, event_id_1, "First event_id should be preserved");

        // Cleanup
        cleanup_test_data(&pool, &test_id).await;
    }

    /// Test cursor-based fetching skips already-published entries
    #[tokio::test]
    #[ignore = "requires PostgreSQL - run with: cargo test --test bridge_integration --features nats -- --ignored"]
    async fn test_cursor_based_fetching() {
        let pool = create_test_pool().await;
        setup_test_schema(&pool).await;

        let test_id = Uuid::new_v4().to_string();
        let object_type = format!("CursorTest_{test_id}");

        // Insert 10 entries
        let pks = insert_change_log_entries(&pool, 10, &object_type).await;

        // Mark entries 3, 5, 7 as already published (simulating "holes")
        for &pk in &[pks[2], pks[4], pks[6]] {
            sqlx::query(
                r"
                UPDATE core.tb_entity_change_log
                SET nats_published_at = NOW(),
                    nats_event_id = $2
                WHERE pk_entity_change_log = $1
                ",
            )
            .bind(pk)
            .bind(Uuid::new_v4())
            .execute(&pool)
            .await
            .expect("Update should succeed");
        }

        // Fetch all entries from cursor 0 (bridge pattern: fetch by cursor, not by published
        // status)
        let entries: Vec<(i64, Option<chrono::DateTime<chrono::Utc>>)> = sqlx::query_as(
            r"
            SELECT pk_entity_change_log, nats_published_at
            FROM core.tb_entity_change_log
            WHERE pk_entity_change_log > $1
              AND object_type = $2
            ORDER BY pk_entity_change_log ASC
            ",
        )
        .bind(0i64)
        .bind(&object_type)
        .fetch_all(&pool)
        .await
        .expect("Query should succeed");

        assert_eq!(entries.len(), 10, "Should fetch all 10 entries");

        // Count published vs unpublished
        let published_count = entries.iter().filter(|(_, p)| p.is_some()).count();
        let unpublished_count = entries.iter().filter(|(_, p)| p.is_none()).count();

        assert_eq!(published_count, 3, "3 entries should be published");
        assert_eq!(unpublished_count, 7, "7 entries should be unpublished");

        // Cleanup
        cleanup_test_data(&pool, &test_id).await;
    }

    // =========================================================================
    // Full Bridge Tests (require both PostgreSQL and NATS)
    // =========================================================================

    /// Test bridge starts from checkpoint (not table max)
    #[tokio::test]
    #[ignore = "requires PostgreSQL and NATS - run with: cargo test --test bridge_integration --features nats -- --ignored"]
    async fn test_bridge_starts_from_checkpoint() {
        use fraiseql_observers::transport::PostgresNatsBridge;

        let pool = create_test_pool().await;
        setup_test_schema(&pool).await;

        let test_id = Uuid::new_v4().to_string();
        let object_type = format!("BridgeStartTest_{test_id}");
        let transport_name = format!("bridge_start_{test_id}");

        // Insert 100 entries
        let pks = insert_change_log_entries(&pool, 100, &object_type).await;

        // Set checkpoint to 50 (simulating previous run that processed first 50)
        let checkpoint_store = Arc::new(PostgresCheckpointStore::new(pool.clone()));
        checkpoint_store
            .save_checkpoint(&transport_name, pks[49])
            .await
            .expect("save_checkpoint should succeed");

        // Create NATS transport
        let nats_config = NatsConfig {
            stream_name: format!("test_bridge_start_{test_id}"),
            consumer_name: format!("test_consumer_{test_id}"),
            subject_prefix: format!("test.bridge.{test_id}"),
            ..Default::default()
        };

        let nats_transport = match NatsTransport::new(nats_config).await {
            Ok(t) => Arc::new(t),
            Err(e) => {
                eprintln!("Skipping test - NATS not available: {e}");
                cleanup_test_data(&pool, &test_id).await;
                return;
            },
        };

        // Create bridge config
        let bridge_config = BridgeConfig {
            transport_name:     transport_name.clone(),
            batch_size:         10,
            poll_interval_secs: 1,
            notify_channel:     format!("test_notify_{test_id}"),
        };

        // Create bridge
        let bridge = PostgresNatsBridge::new(
            pool.clone(),
            nats_transport.clone(),
            checkpoint_store.clone(),
            bridge_config,
        );

        // Subscribe to NATS to count received events
        let filter = EventFilter::default();
        let mut stream = nats_transport.subscribe(filter).await.expect("Subscribe should succeed");

        // Run bridge with shutdown signal
        let (shutdown_tx, shutdown_rx) = tokio::sync::broadcast::channel(1);
        let bridge_handle =
            tokio::spawn(async move { bridge.run_with_shutdown(shutdown_rx).await });

        // Collect events with timeout
        let mut received_count = 0;
        let timeout = Duration::from_secs(10);
        let start = std::time::Instant::now();

        while received_count < 50 && start.elapsed() < timeout {
            if let Ok(Some(result)) =
                tokio::time::timeout(Duration::from_millis(500), stream.next()).await
            {
                if result.is_ok() {
                    received_count += 1;
                }
            }
        }

        // Shutdown bridge
        let _ = shutdown_tx.send(());
        let _ = tokio::time::timeout(Duration::from_secs(2), bridge_handle).await;

        // Should receive ~50 events (entries 51-100, not 1-100)
        assert!(
            (45..=55).contains(&received_count),
            "Should receive approximately 50 events (got {received_count})"
        );

        // Verify checkpoint advanced
        let final_cursor = checkpoint_store
            .get_checkpoint(&transport_name)
            .await
            .expect("get_checkpoint should succeed");
        assert!(
            final_cursor.unwrap_or(0) > pks[49],
            "Checkpoint should advance past initial position"
        );

        // Cleanup
        cleanup_test_data(&pool, &test_id).await;
    }

    /// Test bridge handles crash recovery correctly
    #[tokio::test]
    #[ignore = "requires PostgreSQL and NATS - run with: cargo test --test bridge_integration --features nats -- --ignored"]
    async fn test_bridge_crash_recovery() {
        use fraiseql_observers::transport::PostgresNatsBridge;

        let pool = create_test_pool().await;
        setup_test_schema(&pool).await;

        let test_id = Uuid::new_v4().to_string();
        let object_type = format!("CrashRecoveryTest_{test_id}");
        let transport_name = format!("crash_recovery_{test_id}");

        // Insert 20 entries
        let _pks = insert_change_log_entries(&pool, 20, &object_type).await;

        let checkpoint_store = Arc::new(PostgresCheckpointStore::new(pool.clone()));

        // Create NATS transport
        let nats_config = NatsConfig {
            stream_name: format!("test_crash_{test_id}"),
            consumer_name: format!("test_crash_consumer_{test_id}"),
            subject_prefix: format!("test.crash.{test_id}"),
            ..Default::default()
        };

        let nats_transport = match NatsTransport::new(nats_config).await {
            Ok(t) => Arc::new(t),
            Err(e) => {
                eprintln!("Skipping test - NATS not available: {e}");
                cleanup_test_data(&pool, &test_id).await;
                return;
            },
        };

        let bridge_config = BridgeConfig {
            transport_name:     transport_name.clone(),
            batch_size:         5,
            poll_interval_secs: 1,
            notify_channel:     format!("test_crash_notify_{test_id}"),
        };

        // First run: process some entries then "crash"
        {
            let bridge = PostgresNatsBridge::new(
                pool.clone(),
                nats_transport.clone(),
                checkpoint_store.clone(),
                bridge_config.clone(),
            );

            let (shutdown_tx, shutdown_rx) = tokio::sync::broadcast::channel(1);
            let bridge_handle =
                tokio::spawn(async move { bridge.run_with_shutdown(shutdown_rx).await });

            // Let it process for a bit
            tokio::time::sleep(Duration::from_secs(2)).await;

            // "Crash" by sending shutdown
            let _ = shutdown_tx.send(());
            let _ = tokio::time::timeout(Duration::from_secs(2), bridge_handle).await;
        }

        // Check checkpoint was saved
        let checkpoint_after_crash = checkpoint_store
            .get_checkpoint(&transport_name)
            .await
            .expect("get_checkpoint should succeed");
        assert!(checkpoint_after_crash.is_some(), "Checkpoint should be saved before crash");

        let crash_checkpoint = checkpoint_after_crash.unwrap();
        assert!(crash_checkpoint > 0, "Checkpoint should be > 0");

        // Second run: should resume from checkpoint
        {
            let bridge = PostgresNatsBridge::new(
                pool.clone(),
                nats_transport.clone(),
                checkpoint_store.clone(),
                bridge_config,
            );

            let (shutdown_tx, shutdown_rx) = tokio::sync::broadcast::channel(1);
            let bridge_handle =
                tokio::spawn(async move { bridge.run_with_shutdown(shutdown_rx).await });

            // Let it process remaining entries
            tokio::time::sleep(Duration::from_secs(2)).await;

            // Shutdown
            let _ = shutdown_tx.send(());
            let _ = tokio::time::timeout(Duration::from_secs(2), bridge_handle).await;
        }

        // Verify final checkpoint
        let final_checkpoint = checkpoint_store
            .get_checkpoint(&transport_name)
            .await
            .expect("get_checkpoint should succeed");

        assert!(
            final_checkpoint.unwrap_or(0) >= crash_checkpoint,
            "Final checkpoint should be >= crash checkpoint"
        );

        // Verify all entries are marked as published
        let unpublished_count: (i64,) = sqlx::query_as(
            r"
            SELECT COUNT(*)
            FROM core.tb_entity_change_log
            WHERE object_type = $1
              AND nats_published_at IS NULL
            ",
        )
        .bind(&object_type)
        .fetch_one(&pool)
        .await
        .expect("Query should succeed");

        assert_eq!(
            unpublished_count.0, 0,
            "All entries should be marked as published after recovery"
        );

        // Cleanup
        cleanup_test_data(&pool, &test_id).await;
    }
}
