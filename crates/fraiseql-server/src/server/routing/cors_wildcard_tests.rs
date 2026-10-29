//! A `"*"` in `cors_origins` is refused at boot, not panicked on.
//!
//! The CORS layer is built from the origin list with tower-http's `AllowOrigin::list`,
//! which panics on a wildcard. These drive the real boot path, `Server::new` and then
//! `build_router`, so the refusal is pinned where an operator meets it, not just in
//! `ServerConfig::validate`.
#![allow(clippy::unwrap_used)] // Reason: test code, panics acceptable

use std::sync::Arc;

use fraiseql_core::schema::CompiledSchema;
use fraiseql_test_utils::failing_adapter::FailingAdapter;

use crate::{server::Server, server_config::ServerConfig};

/// Boot a server with CORS enabled for `origins`, building its full router.
///
/// Boxed at the delegation point: `Server::new`'s future trips `clippy::large_futures`
/// (pedantic, denied) at every call site otherwise.
async fn boot_with_origins(origins: &[&str]) -> Result<(), String> {
    let config = ServerConfig {
        cors_enabled: true,
        cors_origins: origins.iter().map(ToString::to_string).collect(),
        ..ServerConfig::default()
    };
    let server = Box::pin(Server::new(
        config,
        CompiledSchema::new(),
        Arc::new(FailingAdapter::new()),
        None,
    ))
    .await
    .map_err(|e| e.to_string())?;
    let _router = server.build_router(&server.build_app_state());
    Ok(())
}

#[tokio::test]
async fn a_wildcard_origin_is_refused_at_boot_naming_the_fix() {
    let err = boot_with_origins(&["*"]).await.unwrap_err();
    assert!(err.contains("cors_origins"), "{err}");
    assert!(err.contains("list each origin"), "{err}");
}

#[tokio::test]
async fn a_wildcard_among_real_origins_is_refused_at_boot() {
    let err = boot_with_origins(&["https://app.example.com", "*"]).await.unwrap_err();
    assert!(err.contains("cors_origins"), "{err}");
}

#[tokio::test]
async fn listed_origins_boot() {
    boot_with_origins(&["https://app.example.com"]).await.unwrap();
}
