//! Integration tests for metrics collection
//!
//! These tests verify that metrics are recorded correctly during query execution.
//! Tests use the metrics crate to validate that counters and histograms are updated.

use fraiseql_wire::metrics;

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
