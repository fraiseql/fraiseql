//! Arrow Flight is bound when its capability starts, before the start is recorded. A port
//! another socket holds leaves the server serving without Flight and records Flight as not
//! started, where it used to be recorded running while its background task logged that it
//! never served.

use std::sync::Arc;

use super::server_configuring_every_in_process_capability;

#[tokio::test]
async fn a_flight_port_another_socket_holds_is_recorded_as_not_started() {
    let Some(svc) = fraiseql_test_support::postgres().await else {
        eprintln!(
            "SKIP a_flight_port_another_socket_holds_is_recorded_as_not_started: no postgres"
        );
        return;
    };
    let held = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let modules = tempfile::tempdir().unwrap();
    let pool = sqlx::PgPool::connect(svc.url()).await.unwrap();
    crate::observers::install_runtime_schema_for_tests(&pool).await;
    let server = server_configuring_every_in_process_capability(
        pool,
        modules.path(),
        held.local_addr().unwrap(),
    )
    .await;
    let starts = Arc::clone(&server.capability_starts);
    let bound = Arc::clone(&server.flight_bound_addr);
    server
        .serve_with_shutdown(std::future::ready(()))
        .await
        .expect("the server serves on without Flight");
    assert_eq!(starts.started("arrow_flight"), Some(false), "Flight is reported not started");
    assert_eq!(bound.get(), None, "Flight bound nothing");
}
