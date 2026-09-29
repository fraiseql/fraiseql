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

/// The Flight `OptimizedView` path reads a registered view with no row scoping (#716), so
/// what the registry holds is what any Flight principal can read.
#[cfg(feature = "arrow")]
mod view_registry {
    #![allow(clippy::unwrap_used)] // Reason: test code, panics are acceptable

    use std::sync::Arc;

    use super::super::create_flight_service;

    /// The Flight service the server builds, over the adapter its feature set selects.
    async fn server_flight_service(url: &str) -> fraiseql_arrow::FraiseQLFlightService {
        #[cfg(not(feature = "wire-backend"))]
        let adapter = Arc::new(
            fraiseql_core::db::postgres::PostgresAdapter::with_pool_size(url, 1)
                .await
                .unwrap(),
        );
        #[cfg(feature = "wire-backend")]
        let adapter = Arc::new(fraiseql_core::db::FraiseWireAdapter::new(url));
        create_flight_service(adapter, &[])
    }

    /// **Reproduction.** The server's Flight service registered four demo names
    /// (`va_orders`, `va_users`, `ta_orders`, `ta_users`) whatever the operator declared: a
    /// database that has such a view served it, unscoped, to every Flight principal.
    #[tokio::test]
    async fn the_server_serves_no_flight_view_its_operator_did_not_declare() {
        let Some(url) = fraiseql_test_support::try_database_url() else {
            return;
        };
        let service = server_flight_service(&url).await;
        for view in ["va_orders", "va_users", "ta_orders", "ta_users"] {
            assert!(
                !service.schema_registry().contains(view),
                "`{view}` is served though no operator declared it"
            );
        }
    }

    /// A declared view that exists and has a row is served; one that does not exist is not;
    /// nothing else is. PostgreSQL backend only: the wire adapter reads no arbitrary SQL, so
    /// under `wire-backend` no view can be typed and none is served.
    #[cfg(not(feature = "wire-backend"))]
    #[tokio::test]
    async fn the_server_serves_exactly_the_flight_views_its_operator_declared() {
        use fraiseql_core::db::DatabaseAdapter as _;

        let Some(url) = fraiseql_test_support::try_database_url() else {
            return;
        };
        let pg = fraiseql_core::db::postgres::PostgresAdapter::with_pool_size(&url, 1)
            .await
            .unwrap();
        pg.execute_raw_query(
            "CREATE OR REPLACE VIEW p_flight_declared AS SELECT 1::bigint AS id, 'a'::text AS label",
        )
        .await
        .unwrap();

        let service = server_flight_service(&url).await;
        let served = super::super::register_flight_views(
            &service,
            &[
                "p_flight_declared".to_string(),
                "p_flight_absent".to_string(),
            ],
        )
        .await;

        pg.execute_raw_query("DROP VIEW p_flight_declared").await.unwrap();
        assert_eq!(served, ["p_flight_declared"]);
        let registry = service.schema_registry();
        assert!(registry.contains("p_flight_declared"));
        assert!(!registry.contains("p_flight_absent"));
        assert!(!registry.contains("va_users"));
        assert_eq!(registry.len(), 1, "exactly the declared, readable view");
    }

    /// A view name reaches SQL: only a plain identifier is accepted, at config validation.
    #[test]
    fn a_flight_view_must_be_named_by_a_plain_identifier() {
        let config = |views: &[&str]| crate::server_config::ServerConfig {
            flight_views: views.iter().map(ToString::to_string).collect(),
            cors_enabled: false,
            ..crate::server_config::ServerConfig::default()
        };
        assert!(config(&["va_public_metrics", "_v2"]).validate().is_ok());
        for bad in ["public.v", "v\"; DROP", "", "1v", "v-x"] {
            let err = config(&[bad]).validate().expect_err(bad);
            assert!(err.contains("flight_views"), "{err}");
        }
    }
}
