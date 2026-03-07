//! Property-based tests for FraiseQL core.
//!
//! Uses proptest to verify invariants that hold across all inputs: parser safety,
//! schema consistency, SQL generation correctness, and error handling behaviour.

mod property {
    mod property_cache_invalidation;
    mod property_error_handling;
    mod property_error_sanitization;
    mod property_graphql;
    mod property_schema;
    mod property_sql_generation;
    mod property_tests;
}
