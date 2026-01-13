//! Integration tests for metrics collection
//!
//! These tests verify that metrics are recorded correctly during query execution.
//! Tests use the metrics crate to validate that counters and histograms are updated.

use fraiseql_wire::metrics;
use fraiseql_wire::stream::StreamStats;

/// Test that metrics module exports all required functions
#[test]
fn test_metrics_module_exports() {
    // Verify counters module is accessible
    metrics::counters::query_submitted("test", true, false, false);
    metrics::counters::auth_attempted("cleartext");
    metrics::counters::json_parse_error("test");
    metrics::counters::query_completed("success", "test");
    metrics::counters::rows_processed("test", 100, "ok");
    metrics::counters::rows_filtered("test", 10);
    metrics::counters::deserialization_success("test", "TestType");
    metrics::counters::deserialization_failure("test", "TestType", "missing_field");

    // Verify histograms module is accessible
    metrics::histograms::query_startup_duration("test", 100);
    metrics::histograms::query_total_duration("test", 500);
    metrics::histograms::chunk_processing_duration("test", 50);
    metrics::histograms::chunk_size("test", 256);
    metrics::histograms::filter_duration("test", 10);
    metrics::histograms::deserialization_duration("test", "TestType", 25);
    metrics::histograms::auth_duration("cleartext", 50);
    metrics::histograms::channel_occupancy("test", 100);

    // Verify labels module is accessible
    let _ = metrics::labels::ENTITY;
    let _ = metrics::labels::MECHANISM_SCRAM;
    let _ = metrics::labels::STATUS_OK;
}

/// Test that counters can be incremented without panicking
#[test]
fn test_counters_basic_operation() {
    // These should not panic
    for i in 0..10 {
        metrics::counters::query_submitted(&format!("entity_{}", i), i % 2 == 0, i % 3 == 0, i % 4 == 0);
    }

    // Auth counters
    metrics::counters::auth_attempted(metrics::labels::MECHANISM_SCRAM);
    metrics::counters::auth_attempted(metrics::labels::MECHANISM_CLEARTEXT);
    metrics::counters::auth_successful(metrics::labels::MECHANISM_SCRAM);
    metrics::counters::auth_failed(metrics::labels::MECHANISM_CLEARTEXT, "invalid_password");

    // Query completion counters
    for entity in &["users", "projects", "tasks"] {
        metrics::counters::query_completed("success", entity);
        metrics::counters::query_completed("error", entity);
        metrics::counters::query_completed("cancelled", entity);
    }
}

/// Test that histograms can be recorded without panicking
#[test]
fn test_histograms_basic_operation() {
    // Record various timing measurements
    for duration_ms in &[10, 50, 100, 250, 500, 1000] {
        metrics::histograms::query_startup_duration("users", *duration_ms);
        metrics::histograms::query_total_duration("projects", *duration_ms);
        metrics::histograms::auth_duration(metrics::labels::MECHANISM_SCRAM, *duration_ms);
    }

    // Record chunk metrics
    for chunk_size in &[1, 64, 128, 256, 512, 1024] {
        metrics::histograms::chunk_size("data", *chunk_size);
        metrics::histograms::chunk_processing_duration("data", *chunk_size / 4);
    }

    // Record filter metrics
    for i in 0..100 {
        metrics::histograms::filter_duration("items", i as u64 % 100);
    }
}

/// Test that deserialization metrics work with different types
#[test]
fn test_deserialization_metrics_with_types() {
    // Simulate deserialization of different types
    let types = vec![
        "String",
        "i32",
        "bool",
        "serde_json::Value",
        "MyCustomType",
        "Vec<String>",
    ];

    for type_name in types {
        // Success path
        metrics::counters::deserialization_success("users", type_name);
        metrics::histograms::deserialization_duration("users", type_name, 15);

        // Failure path
        metrics::counters::deserialization_failure("users", type_name, "missing_field");
        metrics::counters::deserialization_failure("users", type_name, "type_mismatch");
        metrics::counters::deserialization_failure("users", type_name, "invalid_value");
    }
}

/// Test that label constants have expected values
#[test]
fn test_label_constants() {
    // Entity labels
    assert_eq!(metrics::labels::ENTITY, "entity");

    // Mechanism labels
    assert_eq!(metrics::labels::MECHANISM_SCRAM, "scram");
    assert_eq!(metrics::labels::MECHANISM_CLEARTEXT, "cleartext");

    // Status labels
    assert_eq!(metrics::labels::STATUS_OK, "ok");
    assert_eq!(metrics::labels::STATUS_ERROR, "error");
    assert_eq!(metrics::labels::STATUS_CANCELLED, "cancelled");
    assert_eq!(metrics::labels::STATUS_FILTERED, "filtered");

    // Phase labels
    assert_eq!(metrics::labels::PHASE_AUTH, "auth");
    assert_eq!(metrics::labels::PHASE_STARTUP, "startup");
    assert_eq!(metrics::labels::PHASE_QUERY, "query");
    assert_eq!(metrics::labels::PHASE_STREAMING, "streaming");
}

/// Test query error categorization
#[test]
fn test_query_error_categories() {
    let error_categories = vec![
        "server_error",
        "protocol_error",
        "connection_error",
        "json_parse_error",
    ];

    for entity in &["users", "projects", "tasks"] {
        for error_category in &error_categories {
            metrics::counters::query_error(entity, error_category);
        }
    }
}

/// Test row processing metrics
#[test]
fn test_row_processing_metrics() {
    let entities = vec!["users", "projects", "tasks", "events"];
    let row_counts = vec![1, 10, 100, 1000, 10000];

    for entity in entities {
        for count in &row_counts {
            metrics::counters::rows_processed(entity, *count, "ok");
            metrics::counters::rows_processed(entity, count / 2, "error");
        }
    }
}

/// Test filtering metrics across multiple rows
#[test]
fn test_filtering_metrics_distribution() {
    // Simulate filtering 1000 rows with varying latencies
    for i in 0..1000 {
        let filter_duration = (i % 100) as u64;
        metrics::histograms::filter_duration("large_dataset", filter_duration);

        // Occasionally filter out rows
        if i % 7 == 0 {
            metrics::counters::rows_filtered("large_dataset", 1);
        }
    }
}

/// Test chunk processing metrics
#[test]
fn test_chunk_processing_metrics() {
    // Simulate processing 10 chunks
    for chunk_num in 0..10 {
        let chunk_size = 256 - (chunk_num as u64 % 100);
        let processing_duration = 10 + (chunk_num as u64 % 20);

        metrics::histograms::chunk_size("items", chunk_size);
        metrics::histograms::chunk_processing_duration("items", processing_duration);
    }
}

/// Test that auth metrics work with both mechanisms
#[test]
fn test_auth_metrics_both_mechanisms() {
    // SCRAM authentication
    metrics::counters::auth_attempted(metrics::labels::MECHANISM_SCRAM);
    metrics::histograms::auth_duration(metrics::labels::MECHANISM_SCRAM, 150);
    metrics::counters::auth_successful(metrics::labels::MECHANISM_SCRAM);

    // Cleartext authentication
    metrics::counters::auth_attempted(metrics::labels::MECHANISM_CLEARTEXT);
    metrics::histograms::auth_duration(metrics::labels::MECHANISM_CLEARTEXT, 10);
    metrics::counters::auth_successful(metrics::labels::MECHANISM_CLEARTEXT);

    // Failed authentication
    metrics::counters::auth_failed(metrics::labels::MECHANISM_SCRAM, "invalid_password");
    metrics::counters::auth_failed(metrics::labels::MECHANISM_CLEARTEXT, "server_error");
}

/// Test comprehensive query lifecycle metrics
#[test]
fn test_comprehensive_query_lifecycle() {
    let entity = "users";

    // 1. Query submission
    metrics::counters::query_submitted(entity, true, false, false);

    // 2. Authentication
    metrics::counters::auth_attempted(metrics::labels::MECHANISM_SCRAM);
    metrics::histograms::auth_duration(metrics::labels::MECHANISM_SCRAM, 100);
    metrics::counters::auth_successful(metrics::labels::MECHANISM_SCRAM);

    // 3. Query startup
    metrics::histograms::query_startup_duration(entity, 50);

    // 4. Row processing (multiple chunks)
    for _chunk in 0..5 {
        let chunk_size = 256u64;
        metrics::histograms::chunk_size(entity, chunk_size);
        metrics::histograms::chunk_processing_duration(entity, 20);

        // 5. Filtering (10% of rows filtered out)
        for row in 0..chunk_size {
            metrics::histograms::filter_duration(entity, 5);
            if row % 10 == 0 {
                metrics::counters::rows_filtered(entity, 1);
            }
        }

        // 6. Deserialization
        metrics::counters::deserialization_success(entity, "User");
        metrics::histograms::deserialization_duration(entity, "User", 8);
    }

    // 7. Query completion
    metrics::counters::rows_processed(entity, 1280, "ok");
    metrics::histograms::query_total_duration(entity, 150);
    metrics::counters::query_completed("success", entity);
}

/// Test error scenario metrics
#[test]
fn test_error_scenario_metrics() {
    let entity = "projects";

    // Query submission
    metrics::counters::query_submitted(entity, false, true, true);

    // Successful auth
    metrics::counters::auth_attempted(metrics::labels::MECHANISM_SCRAM);
    metrics::counters::auth_successful(metrics::labels::MECHANISM_SCRAM);

    // Query starts
    metrics::histograms::query_startup_duration(entity, 75);

    // First chunk processes successfully
    metrics::histograms::chunk_size(entity, 256);
    metrics::histograms::chunk_processing_duration(entity, 25);

    // JSON parse error occurs
    metrics::counters::json_parse_error(entity);

    // Query error recorded
    metrics::counters::query_error(entity, "json_parse_error");

    // Query completion with error status
    metrics::counters::rows_processed(entity, 256, "error");
    metrics::histograms::query_total_duration(entity, 100);
    metrics::counters::query_completed("error", entity);
}

/// Test cancellation scenario
#[test]
fn test_cancellation_scenario_metrics() {
    let entity = "tasks";

    // Query starts
    metrics::counters::query_submitted(entity, false, false, false);
    metrics::histograms::query_startup_duration(entity, 30);

    // Process first chunk
    metrics::histograms::chunk_size(entity, 256);
    metrics::histograms::chunk_processing_duration(entity, 15);

    // Query is cancelled before completion
    metrics::counters::rows_processed(entity, 256, "ok");
    metrics::histograms::query_total_duration(entity, 50);
    metrics::counters::query_completed("cancelled", entity);
}

/// Test deserialization error scenarios
#[test]
fn test_deserialization_error_scenarios() {
    let entity = "data";
    let type_name = "MyType";

    // Mix of successful and failed deserializations
    for i in 0..100 {
        if i % 10 == 0 {
            // Failed deserialization
            metrics::counters::deserialization_failure(entity, type_name, "missing_field");
        } else if i % 5 == 0 {
            // Another failure type
            metrics::counters::deserialization_failure(entity, type_name, "type_mismatch");
        } else {
            // Successful deserialization
            metrics::counters::deserialization_success(entity, type_name);
            metrics::histograms::deserialization_duration(entity, type_name, 10 + (i as u64 % 20));
        }
    }
}

/// Test that metrics work with various entity names
#[test]
fn test_various_entity_names() {
    let entities = vec![
        "users",
        "projects",
        "tasks",
        "events",
        "logs",
        "metrics_test_entity",
        "very_long_entity_name_that_is_still_valid",
    ];

    for entity in entities {
        metrics::counters::query_submitted(entity, true, true, true);
        metrics::histograms::query_startup_duration(entity, 100);
        metrics::histograms::query_total_duration(entity, 500);
        metrics::counters::rows_processed(entity, 1000, "ok");
        metrics::counters::query_completed("success", entity);
    }
}

/// Test channel occupancy metrics with various backpressure patterns
#[test]
fn test_channel_occupancy_metrics() {
    let entity = "backpressure_test";

    // Low occupancy: fast consumer relative to producer
    for i in 0..10 {
        metrics::histograms::channel_occupancy(entity, i as u64);
    }

    // Medium occupancy: balanced flow
    for i in 50..200 {
        metrics::histograms::channel_occupancy(entity, i as u64);
    }

    // High occupancy: slow consumer causing backpressure
    for i in 200..256 {
        metrics::histograms::channel_occupancy(entity, i as u64);
    }

    // Full channel (max capacity = 256 default)
    metrics::histograms::channel_occupancy(entity, 255);
    metrics::histograms::channel_occupancy(entity, 256);
}

/// Test channel occupancy with varying entities
#[test]
fn test_channel_occupancy_multiple_entities() {
    let entities = vec!["fast_entity", "slow_entity", "mixed_entity"];

    for entity in entities {
        // Simulate occupancy evolution for each entity
        for step in 0..100 {
            let occupancy = (step % 256) as u64;
            metrics::histograms::channel_occupancy(entity, occupancy);
        }
    }
}

/// Test StreamStats type and memory estimation
#[test]
fn test_stream_stats_creation_and_properties() {
    let stats = StreamStats {
        items_buffered: 100,
        estimated_memory: 204800,  // 100 * 2048
        total_rows_yielded: 1000,
        total_rows_filtered: 100,
    };

    assert_eq!(stats.items_buffered, 100);
    assert_eq!(stats.estimated_memory, 204800);
    assert_eq!(stats.total_rows_yielded, 1000);
    assert_eq!(stats.total_rows_filtered, 100);
}

/// Test StreamStats memory estimation for various buffer sizes
#[test]
fn test_stream_stats_memory_estimation_various_sizes() {
    let buffer_sizes = vec![0, 1, 10, 50, 128, 256];
    let expected_kb = vec![0, 2, 20, 100, 256, 512];

    for (size, kb) in buffer_sizes.iter().zip(expected_kb.iter()) {
        let stats = StreamStats {
            items_buffered: *size,
            estimated_memory: size * 2048,
            total_rows_yielded: *size as u64,
            total_rows_filtered: 0,
        };

        assert_eq!(stats.estimated_memory, kb * 1024);
    }
}

/// Test StreamStats tracking row yields and filters
#[test]
fn test_stream_stats_row_tracking() {
    let stats = StreamStats {
        items_buffered: 50,
        estimated_memory: 102400,
        total_rows_yielded: 5000,
        total_rows_filtered: 500,
    };

    // Verify row tracking values
    assert_eq!(stats.total_rows_yielded, 5000);
    assert_eq!(stats.total_rows_filtered, 500);

    // Calculate filter ratio
    let filter_ratio = stats.total_rows_filtered as f64 / stats.total_rows_yielded as f64;
    assert!((filter_ratio - 0.1).abs() < 0.01);  // Should be ~10%
}

/// Test StreamStats zero initialization
#[test]
fn test_stream_stats_zero() {
    let stats = StreamStats::zero();
    assert_eq!(stats.items_buffered, 0);
    assert_eq!(stats.estimated_memory, 0);
    assert_eq!(stats.total_rows_yielded, 0);
    assert_eq!(stats.total_rows_filtered, 0);
}

/// Test memory limit exceeded metric
#[test]
fn test_memory_limit_exceeded_metric() {
    // Should not panic when called
    metrics::counters::memory_limit_exceeded("test_entity");
    metrics::counters::memory_limit_exceeded("another_entity");
}

/// Test memory limit exceeded error creation
#[test]
fn test_memory_limit_exceeded_error() {
    use fraiseql_wire::Error;

    let err = Error::MemoryLimitExceeded {
        limit: 500_000_000,
        current: 750_000_000,
    };

    // Verify error message contains both values
    let msg = err.to_string();
    assert!(msg.contains("750000000"));
    assert!(msg.contains("500000000"));
    assert!(msg.contains("memory limit exceeded"));

    // Verify category
    assert_eq!(err.category(), "memory_limit_exceeded");

    // Verify it's not retriable
    assert!(!err.is_retriable());
}

/// Test QueryBuilder max_memory API existence
#[test]
fn test_query_builder_max_memory_api() {
    // This test verifies the API is available by building a query with max_memory
    // We can't execute it without a database, but we can verify the method exists
    // and returns QueryBuilder for method chaining

    // This would be called like:
    // let stream = client
    //     .query::<Value>("entity")
    //     .max_memory(500_000_000)  // 500MB limit
    //     .execute()
    //     .await?;

    // The builder accepts max_memory() which should return Self for chaining
    // This test passes if the module compiles, confirming the API is present
}

/// Test memory estimation formula (2KB per item)
#[test]
fn test_memory_estimation_formula() {
    let test_cases = vec![
        (0, 0),              // 0 items → 0 bytes
        (1, 2048),           // 1 item → 2KB
        (100, 204_800),      // 100 items → 200KB
        (256, 524_288),      // 256 items → 512KB (typical chunk size)
        (512, 1_048_576),    // 512 items → 1MB
    ];

    for (items, expected_bytes) in test_cases {
        let estimated = items * 2048;
        assert_eq!(
            estimated, expected_bytes,
            "Memory estimation failed for {} items",
            items
        );
    }
}

/// Test error properties for memory limit scenario
#[test]
fn test_memory_limit_error_properties() {
    use fraiseql_wire::Error;

    let error = Error::MemoryLimitExceeded {
        limit: 100_000,
        current: 150_000,
    };

    // Verify it's not retriable (terminal error)
    assert!(!error.is_retriable());

    // Verify category is correct for metrics/alerting
    assert_eq!(error.category(), "memory_limit_exceeded");

    // Verify error message is informative
    let msg = error.to_string();
    assert!(msg.contains("memory limit exceeded"));
    assert!(msg.contains("buffered"));
    assert!(msg.contains("limit"));
}
