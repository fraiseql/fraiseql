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

/// #953 — the operator's `flight_upload_tables` really reaches the Flight service.
///
/// Without these, `with_upload_tables` would be a setter with no caller and the
/// allow-list would be unconfigurable from a config file — the shape that made
/// `with_bulk_export_tables` library-only, and the class of defect P26 found
/// repeatedly ("shipped" meaning the library, with nothing mounting it).
#[cfg(feature = "arrow")]
mod upload_allow_list_config {
    #![allow(clippy::unwrap_used)] // Reason: test code, panics are acceptable

    use fraiseql_arrow::FraiseQLFlightService;

    use super::super::apply_upload_allow_list;
    use crate::server_config::ServerConfig;

    /// The default configuration leaves Upload **disabled**, not merely empty.
    ///
    /// `None` and `Some(∅)` both refuse every table, but only `None` reports
    /// "Upload is disabled", which is what tells an operator they configured
    /// nothing — so the distinction is load-bearing and pinned here.
    #[test]
    fn an_absent_allow_list_leaves_upload_disabled() {
        let config = ServerConfig::default();
        assert!(
            config.flight_upload_tables.is_empty(),
            "Upload must be off by default — it is a raw client-directed INSERT"
        );

        let service = apply_upload_allow_list(FraiseQLFlightService::new(), &[]);
        assert!(
            service.upload_allowed_tables().is_none(),
            "an empty config must leave the service's allow-list None (disabled), not Some(empty)"
        );
    }

    /// A TOML file naming tables produces a service that admits exactly those.
    #[test]
    fn a_configured_allow_list_reaches_the_service() {
        let toml = r#"
            schema_path = "schema.compiled.json"
            flight_upload_tables = ["ta_measurements", "ta_events"]
        "#;
        let config: ServerConfig = toml::from_str(toml).expect("config must parse");
        assert_eq!(config.flight_upload_tables, ["ta_measurements", "ta_events"]);

        let service =
            apply_upload_allow_list(FraiseQLFlightService::new(), &config.flight_upload_tables);
        let allowed = service.upload_allowed_tables().expect("the allow-list must be set");

        assert!(allowed.contains("ta_measurements"));
        assert!(allowed.contains("ta_events"));
        assert_eq!(allowed.len(), 2, "the service must admit exactly the configured tables");
        assert!(
            !allowed.contains("core.tb_entity_change_log"),
            "nothing must be admitted that the operator did not name"
        );
    }
}
