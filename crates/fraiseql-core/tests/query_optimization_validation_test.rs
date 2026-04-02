//! Query Optimization Validation Tests
//!
//! This test suite validates that the query optimization infrastructure
//! (SQL projection + result projection) is structurally correct and meets
//! design-target latency ceilings. Actual performance numbers are hardware-
//! specific — run `cargo bench --bench sql_projection_benchmark` for Criterion
//! measurements on your machine.
//!
//! **Design Targets (not guaranteed figures):**
//! - SQL projection latency: 2-8µs for PostgreSQL SQL generation
//! - Result projection: <50µs for 1K rows
//! - __typename addition: <100µs for 1K rows
//! - Complete pipeline: <10ms for 100K rows
//!
//! **Structural Properties (always true by construction):**
//! - SQL projection reduces payload proportional to fields omitted
//! - Field filtering is accurate with aliasing support
//! - Multi-database support (PostgreSQL, MySQL, SQLite)
//!
//! ## Running Tests
//!
//! ```bash
//! # All query optimization tests
//! cargo test --test query_optimization_validation_test -r
//!
//! # Specific test
//! cargo test --test query_optimization_validation_test test_sql_projection_latency -r -- --nocapture
//!
//! # With logging
//! RUST_LOG=debug cargo test --test query_optimization_validation_test -r -- --nocapture
//! ```

#![allow(clippy::unwrap_used)] // Reason: test code, panics are acceptable
#![allow(clippy::useless_let_if_seq)] // Reason: test uses let+if pattern for clarity
use std::time::Instant;

use fraiseql_core::{
    db::{
        projection_generator::{
            MySqlProjectionGenerator, PostgresProjectionGenerator, SqliteProjectionGenerator,
        },
        types::JsonbValue,
    },
    runtime::{FieldMapping, ResultProjector},
};
use serde_json::{Map, Value as JsonValue, json};

// ============================================================================
// Test Helpers - Sample data generation
// ============================================================================

/// Generate sample JSONB rows with specified field and row counts
fn generate_sample_rows(row_count: usize, field_count: usize) -> Vec<JsonbValue> {
    let mut rows = Vec::with_capacity(row_count);

    for row_id in 0..row_count {
        let mut obj = Map::new();

        // Always include these standard fields
        obj.insert("id".to_string(), json!(format!("id_{}", row_id)));
        obj.insert("name".to_string(), json!(format!("User {}", row_id)));
        obj.insert("email".to_string(), json!(format!("user{}@example.com", row_id)));

        // Add extra fields up to field_count
        for field_idx in 3..field_count {
            obj.insert(format!("field_{}", field_idx), json!(format!("value_{}", field_idx)));
        }

        // Add some realistic nested data
        obj.insert("status".to_string(), json!("active"));
        obj.insert("created_at".to_string(), json!("2024-01-14T00:00:00Z"));
        obj.insert("metadata".to_string(), json!({"score": 100}));

        rows.push(JsonbValue::new(JsonValue::Object(obj)));
    }

    rows
}

#[cfg(test)]
mod query_optimization_tests {
    use super::*;

    // ============================================================================
    // SECTION 1: SQL Projection Generation Latency (3 tests)
    // ============================================================================
    // Tests that SQL projection generation meets latency targets.
    // Why this matters: Sub-microsecond SQL generation keeps query planning fast.
    // Target: PostgreSQL <8µs for 20 fields, MySQL <10µs, SQLite <10µs.

    #[test]
    fn test_postgres_projection_sql_generation_small() {
        // Verify PostgreSQL projection SQL generation is fast for small field counts
        let generator = PostgresProjectionGenerator::new();
        let fields = vec!["id".to_string(), "name".to_string(), "email".to_string()];

        // Warmup: discard the first call to avoid cold-start allocation overhead
        // skewing the timing assertion in loaded CI environments.
        let _ = generator.generate_projection_sql(&fields);

        let start = Instant::now();
        let sql = generator.generate_projection_sql(&fields);
        let elapsed = start.elapsed();

        let sql = sql.unwrap_or_else(|e| panic!("Should generate valid SQL for 3 fields: {e}"));
        assert!(!sql.is_empty(), "Generated SQL should not be empty");
        assert!(
            elapsed.as_millis() < 50,
            "PostgreSQL projection for 3 fields should be <50ms (actual: {:?})",
            elapsed
        );
    }

    #[test]
    fn test_postgres_projection_sql_generation_large() {
        // Verify PostgreSQL projection SQL generation stays fast for large field counts
        let generator = PostgresProjectionGenerator::new();
        let fields: Vec<String> = (0..20).map(|i| format!("field_{}", i)).collect();

        let start = Instant::now();
        let sql = generator.generate_projection_sql(&fields);
        let elapsed = start.elapsed();

        let sql = sql.unwrap_or_else(|e| panic!("Should generate valid SQL for 20 fields: {e}"));
        assert!(!sql.is_empty(), "Generated SQL should not be empty");
        assert!(
            elapsed.as_millis() < 50,
            "PostgreSQL projection for 20 fields should be <50ms (actual: {:?})",
            elapsed
        );
    }

    #[test]
    fn test_multi_database_projection_generation() {
        // Verify all databases support projection SQL generation with reasonable latency
        let postgres_gen = PostgresProjectionGenerator::new();
        let mysql_gen = MySqlProjectionGenerator::new();
        let sqlite_gen = SqliteProjectionGenerator::new();

        let fields = vec!["id".to_string(), "name".to_string()];

        let start_pg = Instant::now();
        let pg_sql = postgres_gen.generate_projection_sql(&fields);
        let elapsed_pg = start_pg.elapsed();

        let start_mysql = Instant::now();
        let mysql_sql = mysql_gen.generate_projection_sql(&fields);
        let elapsed_mysql = start_mysql.elapsed();

        let start_sqlite = Instant::now();
        let sqlite_sql = sqlite_gen.generate_projection_sql(&fields);
        let elapsed_sqlite = start_sqlite.elapsed();

        let _pg =
            pg_sql.unwrap_or_else(|e| panic!("PostgreSQL should generate projection SQL: {e}"));
        let _mysql =
            mysql_sql.unwrap_or_else(|e| panic!("MySQL should generate projection SQL: {e}"));
        let _sqlite =
            sqlite_sql.unwrap_or_else(|e| panic!("SQLite should generate projection SQL: {e}"));

        // All should be reasonably fast (50ms allows for debug builds and CI load)
        assert!(
            elapsed_pg.as_millis() < 50,
            "PostgreSQL generation should be fast (actual: {elapsed_pg:?})"
        );
        assert!(
            elapsed_mysql.as_millis() < 50,
            "MySQL generation should be fast (actual: {elapsed_mysql:?})"
        );
        assert!(
            elapsed_sqlite.as_millis() < 50,
            "SQLite generation should be fast (actual: {elapsed_sqlite:?})"
        );
    }

    // ============================================================================
    // SECTION 2: Result Projection Performance (3 tests)
    // ============================================================================
    // Tests result projection (client-side field filtering) performance.
    // Why this matters: Fast field filtering on result sets maintains throughput.
    // Target: <50µs for 1K rows with 10 fields.

    #[test]
    fn test_result_projection_small_dataset() {
        // Verify result projection is fast on small datasets
        let projector = ResultProjector::with_mappings(vec![
            FieldMapping::simple("id"),
            FieldMapping::simple("name"),
            FieldMapping::simple("email"),
        ]);

        let rows = generate_sample_rows(100, 10);

        let start = Instant::now();
        let result = projector.project_results(&rows, true);
        let elapsed = start.elapsed();

        let projected =
            result.unwrap_or_else(|e| panic!("Should project 100 rows successfully: {e}"));
        if let JsonValue::Array(arr) = &projected {
            assert_eq!(arr.len(), 100, "Should project all 100 rows");
        } else {
            panic!("Expected JSON array from list projection, got: {projected}");
        }

        assert!(
            elapsed.as_millis() < 50,
            "Projecting 100 rows should be <50ms (actual: {:?})",
            elapsed
        );
    }

    #[test]
    fn test_result_projection_medium_dataset() {
        // Verify result projection stays fast on 1K rows (documented target)
        let projector = ResultProjector::with_mappings(vec![
            FieldMapping::simple("id"),
            FieldMapping::simple("name"),
            FieldMapping::simple("email"),
            FieldMapping::simple("status"),
            FieldMapping::simple("created_at"),
        ]);

        let rows = generate_sample_rows(1000, 8);

        let start = Instant::now();
        let result = projector.project_results(&rows, true);
        let elapsed = start.elapsed();

        let projected =
            result.unwrap_or_else(|e| panic!("Should project 1000 rows successfully: {e}"));
        if let JsonValue::Array(arr) = &projected {
            assert_eq!(arr.len(), 1000, "Should project all 1000 rows");
        } else {
            panic!("Expected JSON array from list projection, got: {projected}");
        }

        assert!(
            elapsed.as_millis() < 100,
            "Projecting 1000 rows should be <100ms (actual: {:?})",
            elapsed
        );
    }

    #[test]
    fn test_result_projection_with_aliasing() {
        // Verify aliasing works correctly during projection
        let projector = ResultProjector::with_mappings(vec![
            FieldMapping::aliased("user_id", "id"),
            FieldMapping::aliased("user_name", "name"),
            FieldMapping::simple("email"),
        ]);

        let mut row = Map::new();
        row.insert("user_id".to_string(), json!("123"));
        row.insert("user_name".to_string(), json!("Alice"));
        row.insert("email".to_string(), json!("alice@example.com"));
        let jsonb = JsonbValue::new(JsonValue::Object(row));

        let result = projector
            .project_results(&[jsonb], false)
            .unwrap_or_else(|e| panic!("Should project with aliasing: {e}"));

        if let JsonValue::Object(ref result_obj) = result {
            assert!(result_obj.get("id").is_some(), "Aliased user_id → id");
            assert!(result_obj.get("name").is_some(), "Aliased user_name → name");
            assert!(result_obj.get("email").is_some(), "Simple field email");
            // Original names should not be in result
            assert!(result_obj.get("user_id").is_none(), "Original user_id should not appear");
        } else {
            panic!("Expected JSON object from single-row projection, got: {result}");
        }
    }

    // ============================================================================
    // SECTION 3: __typename Addition (2 tests)
    // ============================================================================
    // Tests __typename field addition performance.
    // Why this matters: __typename enables type introspection in GraphQL responses.
    // Target: <100µs for 1K rows.

    #[test]
    fn test_typename_addition_latency() {
        // Verify __typename addition is fast for large result sets
        let projector = ResultProjector::with_mappings(vec![
            FieldMapping::simple("id"),
            FieldMapping::simple("name"),
        ])
        .with_typename("User");

        let rows = generate_sample_rows(1000, 5);

        let start = Instant::now();
        let result = projector.project_results(&rows, true);
        let elapsed = start.elapsed();

        let projected =
            result.unwrap_or_else(|e| panic!("Should project 1000 rows with __typename: {e}"));

        // Check length
        if let JsonValue::Array(ref arr) = projected {
            assert_eq!(arr.len(), 1000, "Should project all 1000 rows");

            // Verify __typename is added correctly
            if let Some(JsonValue::Object(first)) = arr.first() {
                assert_eq!(
                    first.get("__typename").and_then(|v| v.as_str()),
                    Some("User"),
                    "Should have __typename = User"
                );
            }
        }

        assert!(
            elapsed.as_millis() < 150,
            "__typename addition for 1000 rows should be <150ms (actual: {:?})",
            elapsed
        );
    }

    #[test]
    fn test_typename_in_projection() {
        // Verify __typename is correctly added to projected data
        let projector = ResultProjector::with_mappings(vec![
            FieldMapping::simple("id"),
            FieldMapping::simple("name"),
        ])
        .with_typename("Post");

        let mut row = Map::new();
        row.insert("id".to_string(), json!("post1"));
        row.insert("name".to_string(), json!("Alice"));

        let jsonb = JsonbValue::new(JsonValue::Object(row));
        let result = projector
            .project_results(&[jsonb], false)
            .unwrap_or_else(|e| panic!("Should project single row with __typename: {e}"));
        if let JsonValue::Object(ref result_obj) = result {
            assert_eq!(
                result_obj.get("__typename").and_then(|v| v.as_str()),
                Some("Post"),
                "Should have __typename = Post"
            );
        } else {
            panic!("Expected JSON object from single-row projection, got: {result}");
        }
    }

    // ============================================================================
    // SECTION 4: Payload Reduction (2 tests)
    // ============================================================================
    // Tests payload size reduction from SQL and result projection.
    // Why this matters: Smaller payloads = less network bandwidth + faster JSON parsing.
    // Structural check: projection reduces payload proportional to fields omitted.

    #[test]
    fn test_payload_size_without_projection() {
        // Measure baseline payload without projection
        let rows = generate_sample_rows(100, 15);

        let mut total_size = 0;
        for row in &rows {
            total_size += serde_json::to_string(&row.as_value()).map_or(0, |s| s.len());
        }

        assert!(
            total_size > 0,
            "Baseline payload should have size (measured: {} bytes)",
            total_size
        );
    }

    #[test]
    fn test_payload_size_with_projection() {
        // Measure payload with projection (only requested fields)
        let projector = ResultProjector::with_mappings(vec![
            FieldMapping::simple("id"),
            FieldMapping::simple("name"),
            FieldMapping::simple("email"),
        ]);

        let rows = generate_sample_rows(100, 15);

        let projected = projector
            .project_results(&rows, true)
            .unwrap_or_else(|e| panic!("Should project 100 rows with 3 fields: {e}"));

        let total_size = serde_json::to_string(&projected).map_or(0, |s| s.len());

        assert!(
            total_size > 0,
            "Projected payload should have size (measured: {} bytes)",
            total_size
        );

        // With projection to 3 fields vs 15+, we should see measurable reduction
        assert!(
            total_size < 10000,
            "Projected payload for 100 rows should be reasonable (<10KB)"
        );
    }

    // ============================================================================
    // SECTION 5: Field Filtering Accuracy (3 tests)
    // ============================================================================
    // Tests that field filtering is accurate and handles edge cases.
    // Why this matters: Incorrect field filtering breaks queries.
    // Target: 100% accuracy, all fields included/excluded correctly.

    #[test]
    fn test_field_filtering_includes_only_requested() {
        // Verify only requested fields appear in projection
        let projector = ResultProjector::with_mappings(vec![
            FieldMapping::simple("id"),
            FieldMapping::simple("name"),
        ]);

        let mut row = Map::new();
        row.insert("id".to_string(), json!("123"));
        row.insert("name".to_string(), json!("Alice"));
        row.insert("secret".to_string(), json!("REDACTED"));
        row.insert("internal_id".to_string(), json!("internal123"));

        let jsonb = JsonbValue::new(JsonValue::Object(row));
        let result = projector
            .project_results(&[jsonb], false)
            .unwrap_or_else(|e| panic!("Should project and filter fields: {e}"));
        if let JsonValue::Object(ref result_obj) = result {
            assert!(result_obj.get("id").is_some(), "Should include requested field id");
            assert!(result_obj.get("name").is_some(), "Should include requested field name");
            assert!(result_obj.get("secret").is_none(), "Should exclude unrequested field secret");
            assert!(
                result_obj.get("internal_id").is_none(),
                "Should exclude unrequested field internal_id"
            );
        } else {
            panic!("Expected JSON object from single-row projection, got: {result}");
        }
    }

    #[test]
    fn test_field_filtering_handles_missing_fields() {
        // Verify projection handles gracefully when source fields are missing
        let projector = ResultProjector::with_mappings(vec![
            FieldMapping::simple("id"),
            FieldMapping::simple("missing_field"),
            FieldMapping::simple("name"),
        ]);

        let mut row = Map::new();
        row.insert("id".to_string(), json!("123"));
        row.insert("name".to_string(), json!("Alice"));
        // missing_field not in row

        let jsonb = JsonbValue::new(JsonValue::Object(row));
        // Should handle gracefully (either skip missing or return null)
        let result = projector.project_results(&[jsonb], false).unwrap_or_else(|e| {
            panic!("Should handle missing fields gracefully without error: {e}")
        });
        assert!(
            matches!(result, JsonValue::Object(_)),
            "Expected JSON object from single-row projection, got: {result}"
        );
    }

    #[test]
    fn test_field_filtering_with_multiple_aliases() {
        // Verify aliasing works correctly with multiple renamed fields
        let projector = ResultProjector::with_mappings(vec![
            FieldMapping::aliased("user_id", "id"),
            FieldMapping::aliased("full_name", "name"),
            FieldMapping::aliased("email_address", "email"),
        ]);

        let mut row = Map::new();
        row.insert("user_id".to_string(), json!("u1"));
        row.insert("full_name".to_string(), json!("Bob"));
        row.insert("email_address".to_string(), json!("bob@example.com"));

        let jsonb = JsonbValue::new(JsonValue::Object(row));
        let result = projector
            .project_results(&[jsonb], false)
            .unwrap_or_else(|e| panic!("Should project with multiple aliases: {e}"));
        if let JsonValue::Object(ref result_obj) = result {
            assert_eq!(result_obj.get("id").and_then(|v| v.as_str()), Some("u1"));
            assert_eq!(result_obj.get("name").and_then(|v| v.as_str()), Some("Bob"));
            assert_eq!(result_obj.get("email").and_then(|v| v.as_str()), Some("bob@example.com"));
            // Original names should not appear
            assert!(
                result_obj.get("user_id").is_none(),
                "Original key user_id should not appear after aliasing"
            );
            assert!(
                result_obj.get("full_name").is_none(),
                "Original key full_name should not appear after aliasing"
            );
            assert!(
                result_obj.get("email_address").is_none(),
                "Original key email_address should not appear after aliasing"
            );
        } else {
            panic!("Expected JSON object from single-row projection, got: {result}");
        }
    }

    // ============================================================================
    // SECTION 6: PostgreSQL-Specific Features (2 tests)
    // ============================================================================
    // Tests PostgreSQL projection SQL generation features.
    // Why this matters: PostgreSQL is primary database with most features.
    // Target: Correct jsonb_build_object SQL generation.

    #[test]
    fn test_postgres_projection_sql_structure() {
        // Verify PostgreSQL projection generates correct jsonb_build_object syntax
        let generator = PostgresProjectionGenerator::new();
        let fields = vec!["id".to_string(), "name".to_string()];

        let sql = generator.generate_projection_sql(&fields).expect("Should generate SQL");

        assert!(sql.contains("jsonb_build_object("), "Should use jsonb_build_object");
        assert!(sql.contains("'id'"), "Should include id field");
        assert!(sql.contains("'name'"), "Should include name field");
        assert!(sql.contains("\"data\""), "Should reference data column");
    }

    #[test]
    fn test_postgres_select_clause_generation() {
        // Verify PostgreSQL generates complete SELECT clause
        let generator = PostgresProjectionGenerator::new();
        let fields = vec!["id".to_string(), "email".to_string()];

        let select = generator
            .generate_select_clause("users", &fields)
            .expect("Should generate SELECT");

        assert!(
            select.starts_with("SELECT jsonb_build_object("),
            "Should start with SELECT jsonb_build_object"
        );
        assert!(select.contains("FROM"), "Should include FROM clause");
        assert!(
            select.contains("\"users\"") || select.contains("`users`") || select.contains("users"),
            "Should include table name"
        );
    }
}
