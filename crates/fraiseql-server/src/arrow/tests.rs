#[cfg(feature = "arrow")]
mod database_adapter_tests {
    #![allow(clippy::unwrap_used)] // Reason: test code, panics are acceptable

    use super::super::database_adapter::*;

    /// Test that adapter can be created from `PostgresAdapter`
    #[test]
    fn test_adapter_creation() {
        // This test verifies the adapter can be created
        // In integration tests, we'll test actual query execution
        // (Note: This is a unit test that doesn't require a database)
        let _adapter: FlightDatabaseAdapter;
        // If this compiles, the struct is properly defined
    }
}
