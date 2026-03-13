//! Property-based tests for FraiseQL server.
//!
//! Uses proptest to verify invariants across all inputs: auth token parsing,
//! query complexity scoring, and rate-limit accounting.

mod property {
    mod property_auth_parsing;
    mod property_query_complexity;
    mod property_rate_limiting;
}
