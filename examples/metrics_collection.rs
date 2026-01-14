//! Example: Metrics Collection with fraiseql-wire
//!
//! This example demonstrates how to collect and use metrics from fraiseql-wire queries.
//! Metrics are automatically recorded during query execution without any explicit setup.
//!
//! Run with: cargo run --example metrics_collection
//!
//! Note: This is a demonstration of the metrics API. In production, metrics would be
//! collected by a metrics exporter (Prometheus, OpenTelemetry, etc.) and made available
//! to your monitoring system.

use fraiseql_wire::metrics;

fn main() {
    println!("=== fraiseql-wire Metrics Collection Example ===\n");
    println!("This example demonstrates the metrics that are automatically collected\n");

    // Demonstration 1: Query Submission Metrics
    demo_query_submission();

    // Demonstration 2: Authentication Metrics
    demo_authentication();

    // Demonstration 3: Query Execution Metrics
    demo_query_execution();

    // Demonstration 4: Error Tracking
    demo_error_tracking();

    // Demonstration 5: Metrics Analysis
    demo_metrics_analysis();
}

/// Demonstrate query submission metrics
fn demo_query_submission() {
    println!("1. QUERY SUBMISSION METRICS");
    println!("   These are recorded when QueryBuilder::execute() is called\n");

    // Simulate different query patterns
    println!("   Recording queries with different predicates:");
    metrics::counters::query_submitted("users", true, false, false);
    println!("   ✓ Simple WHERE query: fraiseql_queries_total{{entity=\"users\", has_where_sql=\"true\", ...}}");

    metrics::counters::query_submitted("projects", true, true, true);
    println!("   ✓ Complex query: fraiseql_queries_total{{entity=\"projects\", has_where_sql=\"true\", has_where_rust=\"true\", has_order_by=\"true\"}}");

    metrics::counters::query_submitted("tasks", false, false, false);
    println!("   ✓ Simple full scan: fraiseql_queries_total{{entity=\"tasks\", ...}}\n");
}

/// Demonstrate authentication metrics
fn demo_authentication() {
    println!("2. AUTHENTICATION METRICS");
    println!("   These are recorded during connection authentication\n");

    println!("   SCRAM-SHA-256 Authentication:");
    metrics::counters::auth_attempted(metrics::labels::MECHANISM_SCRAM);
    println!("   → fraiseql_authentications_total{{mechanism=\"scram\"}}");

    metrics::histograms::auth_duration(metrics::labels::MECHANISM_SCRAM, 125);
    println!("   → fraiseql_auth_duration_ms{{mechanism=\"scram\"}} = 125ms");

    metrics::counters::auth_successful(metrics::labels::MECHANISM_SCRAM);
    println!("   ✓ fraiseql_authentications_successful_total{{mechanism=\"scram\"}}\n");

    println!("   Cleartext Authentication:");
    metrics::counters::auth_attempted(metrics::labels::MECHANISM_CLEARTEXT);
    println!("   → fraiseql_authentications_total{{mechanism=\"cleartext\"}}");

    metrics::histograms::auth_duration(metrics::labels::MECHANISM_CLEARTEXT, 8);
    println!("   → fraiseql_auth_duration_ms{{mechanism=\"cleartext\"}} = 8ms");

    metrics::counters::auth_successful(metrics::labels::MECHANISM_CLEARTEXT);
    println!("   ✓ fraiseql_authentications_successful_total{{mechanism=\"cleartext\"}}\n");

    println!("   Failed Authentication:");
    metrics::counters::auth_failed(metrics::labels::MECHANISM_SCRAM, "invalid_password");
    println!("   ✗ fraiseql_authentications_failed_total{{mechanism=\"scram\", reason=\"invalid_password\"}}\n");
}

/// Demonstrate query execution metrics
fn demo_query_execution() {
    println!("3. QUERY EXECUTION METRICS");
    println!("   These are recorded during streaming query processing\n");

    let entity = "users";

    println!("   Query Startup (time to first row):");
    metrics::histograms::query_startup_duration(entity, 45);
    println!(
        "   → fraiseql_query_startup_duration_ms{{entity=\"{}\"}} = 45ms",
        entity
    );

    println!("\n   Row Processing (5 chunks of 256 rows each):");
    for chunk_num in 0..5 {
        let chunk_duration = 18 + (chunk_num as u64 % 5);
        metrics::histograms::chunk_size(entity, 256);
        metrics::histograms::chunk_processing_duration(entity, chunk_duration);
        println!(
            "   → Chunk {}: {} rows processed in {}ms",
            chunk_num + 1,
            256,
            chunk_duration
        );
    }

    println!("\n   Filtering (10% of rows filtered by Rust predicates):");
    for row_num in 0..1280 {
        metrics::histograms::filter_duration(entity, 2);
        if row_num % 10 == 0 {
            metrics::counters::rows_filtered(entity, 1);
        }
    }
    println!(
        "   → fraiseql_rows_filtered_total{{entity=\"{}\"}} = 128 rows",
        entity
    );

    println!("\n   Deserialization (converting JSON to User struct):");
    metrics::counters::deserialization_success(entity, "User");
    metrics::histograms::deserialization_duration(entity, "User", 12);
    println!(
        "   → {} rows deserialized to User struct in ~12ms",
        1280 - 128
    );

    println!("\n   Query Completion:");
    metrics::counters::rows_processed(entity, 1152, "ok");
    metrics::histograms::query_total_duration(entity, 180);
    metrics::counters::query_completed("success", entity);
    println!(
        "   ✓ fraiseql_query_completed_total{{entity=\"{}\", status=\"success\"}}",
        entity
    );
    println!(
        "   ✓ fraiseql_query_total_duration_ms{{entity=\"{}\"}} = 180ms\n",
        entity
    );
}

/// Demonstrate error tracking metrics
fn demo_error_tracking() {
    println!("4. ERROR TRACKING METRICS\n");

    println!("   JSON Parse Error:");
    metrics::counters::json_parse_error("events");
    println!("   → fraiseql_json_parse_errors_total{{entity=\"events\"}}");
    metrics::counters::query_error("events", "json_parse_error");
    metrics::counters::query_completed("error", "events");
    println!("   ✗ Query failed with JSON parse error\n");

    println!("   Deserialization Error:");
    metrics::counters::deserialization_failure("projects", "Project", "missing_field");
    println!("   → fraiseql_rows_deserialization_failed_total{{entity=\"projects\", type_name=\"Project\", reason=\"missing_field\"}}");
    metrics::counters::query_error("projects", "deserialization_error");
    println!("   ✗ Row deserialization failed\n");

    println!("   Protocol Error:");
    metrics::counters::query_error("tasks", "protocol_error");
    metrics::counters::query_completed("error", "tasks");
    println!("   ✗ Protocol error during query execution\n");

    println!("   Cancellation:");
    metrics::counters::query_completed("cancelled", "logs");
    println!("   ✗ Query was cancelled before completion\n");
}

/// Demonstrate metrics analysis patterns
fn demo_metrics_analysis() {
    println!("5. METRICS ANALYSIS PATTERNS\n");

    println!("   Pattern 1: Query Success Rate");
    println!("   Formula: fraiseql_query_completed_total{{status=\"success\"}} / fraiseql_query_completed_total");
    println!("   Use Case: Monitor query reliability\n");

    println!("   Pattern 2: Error Rate by Category");
    println!("   Formula: fraiseql_query_error_total / fraiseql_query_total_duration_ms");
    println!("   Use Case: Identify error sources (JSON, protocol, connection)\n");

    println!("   Pattern 3: Query Performance P99");
    println!("   Formula: histogram_quantile(0.99, fraiseql_query_total_duration_ms)");
    println!("   Use Case: SLO monitoring\n");

    println!("   Pattern 4: Filter Effectiveness");
    println!("   Formula: fraiseql_rows_filtered_total / (rows_processed + rows_filtered)");
    println!("   Use Case: Optimize Rust predicates\n");

    println!("   Pattern 5: Deserialization by Type");
    println!("   Formula: fraiseql_deserialization_duration_ms by type_name");
    println!("   Use Case: Identify slow types\n");

    println!("   Pattern 6: Authentication Success Rate");
    println!(
        "   Formula: fraiseql_authentications_successful_total / fraiseql_authentications_total"
    );
    println!("   Use Case: Monitor auth health\n");
}

/// Helper function to display metric concepts
#[allow(dead_code)]
fn explain_metrics() {
    println!("=== Understanding fraiseql-wire Metrics ===\n");

    println!("Counters (monotonic increasing):");
    println!("  - fraiseql_queries_total: Query requests");
    println!("  - fraiseql_query_completed_total: Completed queries");
    println!("  - fraiseql_query_error_total: Failed queries");
    println!("  - fraiseql_rows_deserialized_total: Successfully typed rows");
    println!("  - fraiseql_rows_filtered_total: Rows removed by predicates\n");

    println!("Histograms (distribution of values):");
    println!("  - fraiseql_query_startup_duration_ms: Time to first row");
    println!("  - fraiseql_query_total_duration_ms: Total query execution time");
    println!("  - fraiseql_chunk_processing_duration_ms: Per-chunk processing latency");
    println!("  - fraiseql_filter_duration_ms: Rust filter execution time");
    println!("  - fraiseql_deserialization_duration_ms: Type conversion latency");
    println!("  - fraiseql_auth_duration_ms: Authentication latency\n");

    println!("Labels (dimensions for filtering):");
    println!("  - entity: Table/view being queried");
    println!("  - mechanism: Auth mechanism (cleartext, scram)");
    println!("  - status: Completion status (success, error, cancelled)");
    println!("  - type_name: Deserialization target type");
    println!("  - error_category: Error classification\n");
}
