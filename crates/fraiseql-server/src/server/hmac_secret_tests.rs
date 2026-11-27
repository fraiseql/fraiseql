//! An `hmac_secret_env` whose variable is unset or empty refuses boot, as the other `*_env`
//! keys do, rather than leaving the address-hash key and the idempotency signature off.

#![allow(clippy::unwrap_used, clippy::panic)] // Reason: test code, panics acceptable

use std::sync::Arc;

use fraiseql_core::schema::CompiledSchema;
use fraiseql_test_utils::failing_adapter::FailingAdapter;

use super::{Server, hmac_secret::resolve};
use crate::server_config::ServerConfig;

const VAR: &str = "FRAISEQL_TEST_2665_HMAC_SECRET";

fn env(value: Option<&'static str>) -> impl Fn(&str) -> Option<String> {
    move |name| (name == VAR).then_some(value).flatten().map(str::to_string)
}

#[test]
fn no_key_is_no_secret() {
    assert_eq!(resolve(None, env(Some("ignored"))).unwrap(), None);
}

#[test]
fn a_named_variable_is_the_secret() {
    assert_eq!(resolve(Some(VAR), env(Some("s3cret"))).unwrap().as_deref(), Some("s3cret"));
}

#[test]
fn an_unset_or_empty_variable_is_refused_naming_it() {
    for value in [None, Some(""), Some("  ")] {
        let refusal = resolve(Some(VAR), env(value)).unwrap_err().to_string();
        assert!(refusal.contains(VAR), "{value:?}: {refusal}");
        assert!(refusal.contains("hmac_secret_env"), "{value:?}: {refusal}");
    }
}

#[allow(clippy::future_not_send)] // Reason: `temp_env`'s lock guard is not `Send`.
async fn boot(value: Option<&'static str>) -> Option<String> {
    let config = ServerConfig {
        cors_enabled: false,
        hmac_secret_env: Some(VAR.to_string()),
        ..ServerConfig::default()
    };
    Box::pin(temp_env::async_with_vars([(VAR, value)], async {
        Box::pin(Server::new(
            config,
            CompiledSchema::new(),
            Arc::new(FailingAdapter::new()),
            None,
        ))
        .await
        .err()
        .map(|e| e.to_string())
    }))
    .await
}

#[tokio::test]
async fn the_server_refuses_to_boot_over_an_unset_secret() {
    let refusal = boot(None).await.expect("an unset hmac_secret_env variable booted");
    assert!(refusal.contains(VAR), "{refusal}");
}

#[tokio::test]
async fn the_server_refuses_to_boot_over_an_empty_secret() {
    let refusal = boot(Some("")).await.expect("an empty hmac_secret_env variable booted");
    assert!(refusal.contains(VAR), "{refusal}");
}

#[tokio::test]
async fn the_server_boots_over_a_set_secret() {
    assert_eq!(boot(Some("s3cret")).await, None);
}
