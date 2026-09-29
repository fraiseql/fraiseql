//! Arrow Flight `OptimizedView` serves exactly the views its operator declares in
//! `flight_views`, against a real PostgreSQL.
//!
//! The ticket reads a registered view whole — no row policy, field gate or authorizer
//! (#716) — so what the registry holds is what every Flight principal can read. The
//! library pre-fills its registry with four demo names for its own tests (`va_orders`,
//! `va_users`, `ta_orders`, `ta_users`); the server must serve none of them unless declared.
//!
//! `#[ignore]` — needs `DATABASE_URL`. Named explicitly by the Dagger `integration` leg
//! (the `observers` suite, which binds a Postgres), so it either runs or the leg fails. Run
//! with: `cargo test -p fraiseql-server --features arrow --test flight_views_pg --
//! --ignored --test-threads=1`.

// PostgreSQL build only: under `wire-backend` the adapter reads no arbitrary SQL, so no view
// can be typed and none is served (pinned database-free in the lib's `arrow::tests`). The
// leg that runs this suite must not pass `wire-backend`, or the binary holds no test.
#![cfg(all(feature = "arrow", not(feature = "wire-backend")))]
#![allow(clippy::unwrap_used, clippy::expect_used)] // Reason: test code, panics acceptable

use std::sync::Arc;

use fraiseql_core::db::{DatabaseAdapter as _, postgres::PostgresAdapter};
use fraiseql_server::arrow::{create_flight_service, register_flight_views};

async fn adapter() -> Arc<PostgresAdapter> {
    let url = fraiseql_test_support::database_url();
    Arc::new(PostgresAdapter::with_pool_size(&url, 1).await.unwrap())
}

/// The server's Flight service registers no view its operator did not declare.
#[tokio::test]
#[ignore = "needs DATABASE_URL; run by the Dagger integration leg"]
async fn the_server_serves_no_flight_view_its_operator_did_not_declare() {
    let service = create_flight_service(adapter().await, &[]);
    for view in ["va_orders", "va_users", "ta_orders", "ta_users"] {
        assert!(
            !service.schema_registry().contains(view),
            "`{view}` is served though no operator declared it"
        );
    }
}

/// A declared view that exists and has a row is served; one that does not exist is not;
/// nothing else is.
#[tokio::test]
#[ignore = "needs DATABASE_URL; run by the Dagger integration leg"]
async fn the_server_serves_exactly_the_flight_views_its_operator_declared() {
    let pg = adapter().await;
    pg.execute_raw_query(
        "CREATE OR REPLACE VIEW p_flight_declared AS SELECT 1::bigint AS id, 'a'::text AS label",
    )
    .await
    .unwrap();

    let service = create_flight_service(Arc::clone(&pg), &[]);
    let served = register_flight_views(
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
