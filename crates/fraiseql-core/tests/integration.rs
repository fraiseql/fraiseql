//! Integration tests for FraiseQL core requiring live infrastructure.
//!
//! These tests exercise multi-database query execution, aggregations,
//! relay pagination, and cross-database operations against real database
//! instances provisioned by testcontainers.
//!
//! Note: Individual test files (multi_database_integration.rs, aggregation_integration.rs,
//! etc.) are now top-level test targets and can be run independently with:
//!   cargo test -p fraiseql-core --test <test_name> -- --test-threads=1
