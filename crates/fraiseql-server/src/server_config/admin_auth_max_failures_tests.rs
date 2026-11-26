//! `admin_auth_max_failures = 0` is refused when the configuration is loaded. The limiter
//! blocks an address once its failures reach the budget, so a budget of 0 refuses every
//! address after its first failure, a correct token included: the strictest setting, not
//! the "off" the key's documentation once promised.

#![allow(clippy::unwrap_used, clippy::panic)] // Reason: test code, panics are acceptable

use std::sync::Arc;

use fraiseql_core::schema::CompiledSchema;
use fraiseql_test_utils::failing_adapter::FailingAdapter;

use super::ServerConfig;

fn loaded(toml: &str) -> ServerConfig {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("fraiseql-server.toml");
    std::fs::write(&path, toml).unwrap();
    ServerConfig::load_file(&path).unwrap()
}

#[test]
fn a_budget_of_zero_is_refused_naming_the_key() {
    let config = loaded("cors_enabled = false\nadmin_auth_max_failures = 0\n");
    let error = config.validate().expect_err("a budget of 0 is refused");
    assert!(error.contains("admin_auth_max_failures"), "{error}");
    assert!(error.contains("positive"), "{error}");
}

#[test]
fn a_budget_of_one_is_accepted() {
    loaded("cors_enabled = false\nadmin_auth_max_failures = 1\n")
        .validate()
        .unwrap();
}

#[tokio::test]
async fn a_server_is_not_built_over_a_budget_of_zero() {
    let config = ServerConfig {
        admin_auth_max_failures: 0,
        cors_enabled: false,
        ..ServerConfig::default()
    };
    let built = Box::pin(crate::Server::new(
        config,
        CompiledSchema::new(),
        Arc::new(FailingAdapter::new()),
        None,
    ))
    .await;
    let Err(error) = built else {
        panic!("a server was built over admin_auth_max_failures = 0");
    };
    assert!(error.to_string().contains("admin_auth_max_failures"), "{error}");
}
