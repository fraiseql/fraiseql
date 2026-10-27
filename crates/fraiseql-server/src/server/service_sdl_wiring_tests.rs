//! The service SDL's auth switch reaches the serving state as configured.
//!
//! `service_sdl_require_auth` is resolved once, when the state is built: unset, it takes
//! `introspection_require_auth`, as its five `*_require_auth` siblings do. A request test
//! sets the state directly, so only a test of the built state sees this resolution.

#![allow(clippy::unwrap_used)] // Reason: test code

use std::sync::Arc;

use fraiseql_test_utils::{failing_adapter::FailingAdapter, schema_builder::TestSchemaBuilder};

use crate::{Server, server_config::ServerConfig};

async fn resolved(explicit: Option<bool>, introspection_require_auth: bool) -> bool {
    let config = ServerConfig {
        cors_enabled: false,
        introspection_require_auth,
        service_sdl_require_auth: explicit,
        ..ServerConfig::default()
    };
    let schema = TestSchemaBuilder::new().with_simple_query("users", "User", true).build();
    let server = Box::pin(Server::new(config, schema, Arc::new(FailingAdapter::new()), None))
        .await
        .unwrap();
    server.build_app_state().service_sdl_require_auth
}

#[tokio::test]
async fn an_unset_switch_takes_introspection_require_auth() {
    assert!(resolved(None, true).await);
    assert!(!resolved(None, false).await);
}

#[tokio::test]
async fn a_set_switch_overrides_introspection_require_auth() {
    assert!(resolved(Some(true), false).await);
    assert!(!resolved(Some(false), true).await);
}

#[test]
fn the_default_config_requires_auth() {
    let config = ServerConfig::default();
    assert_eq!(config.service_sdl_require_auth, None);
    assert!(config.introspection_require_auth);
}
