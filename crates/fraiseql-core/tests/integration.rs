//! Integration tests for FraiseQL core requiring live infrastructure.
//!
//! These tests exercise multi-database query execution, aggregations,
//! relay pagination, and cross-database operations against real database
//! instances provisioned by testcontainers.

mod integration {
    mod aggregation_integration;
    mod cross_database_test;
    mod fact_table_integration;
    mod multi_database_integration;
    mod relay_integration;
}
