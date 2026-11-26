//! The GraphQL validation-error budget is the one `[security.rate_limiting]` configures.
//!
//! The budget is sized from the policy the server's rate limiter resolves, whichever file
//! carries it, and a zero in either handler budget refuses boot even with the auth
//! endpoint rules off.

#![allow(clippy::unwrap_used, clippy::panic)] // Reason: test code, panics acceptable

use std::sync::Arc;

use fraiseql_core::schema::{CompiledSchema, RateLimitingSecurityConfig, SecurityConfig};
use fraiseql_test_utils::failing_adapter::FailingAdapter;

use super::Server;
use crate::{
    middleware::ServerRateLimitingConfig, routes::graphql::AppState, server_config::ServerConfig,
};

const BUDGET: u32 = 3;

fn policy_with_budget() -> RateLimitingSecurityConfig {
    RateLimitingSecurityConfig {
        graphql_validation_error_max_requests: BUDGET,
        graphql_validation_error_window_secs: 3_600,
        ..RateLimitingSecurityConfig::DEFAULT
    }
}

fn schema_with(policy: Option<RateLimitingSecurityConfig>) -> CompiledSchema {
    let mut schema = CompiledSchema::new();
    schema.security = Some(SecurityConfig {
        rate_limiting: policy,
        ..SecurityConfig::default()
    });
    schema
}

fn config() -> ServerConfig {
    ServerConfig {
        cors_enabled: false,
        ..ServerConfig::default()
    }
}

async fn app_state(config: ServerConfig, schema: CompiledSchema) -> AppState {
    Box::pin(Server::new(config, schema, Arc::new(FailingAdapter::new()), None))
        .await
        .unwrap()
        .build_app_state()
}

/// How many validation errors one client is allowed before the budget refuses it.
fn allowed(state: &AppState) -> u32 {
    let mut n = 0;
    while state.graphql_rate_limiter.check("203.0.113.7").is_ok() {
        n += 1;
        assert!(n <= 1_000, "the budget never refused");
    }
    n
}

#[tokio::test]
async fn the_compiled_policy_sizes_the_validation_error_budget() {
    let state = app_state(config(), schema_with(Some(policy_with_budget()))).await;
    assert_eq!(allowed(&state), BUDGET);
}

#[tokio::test]
async fn the_server_files_rate_limiting_table_sizes_it_too() {
    let config = ServerConfig {
        rate_limiting: Some(ServerRateLimitingConfig {
            policy: policy_with_budget(),
            ..ServerRateLimitingConfig::default()
        }),
        ..config()
    };
    let state = app_state(config, schema_with(None)).await;
    assert_eq!(allowed(&state), BUDGET);
}

#[tokio::test]
async fn with_no_section_the_budget_is_the_policy_default() {
    let state = app_state(config(), schema_with(None)).await;
    assert_eq!(
        allowed(&state),
        RateLimitingSecurityConfig::DEFAULT.graphql_validation_error_max_requests
    );
}

#[tokio::test]
async fn a_zero_handler_budget_refuses_boot_with_the_endpoint_rules_off() {
    let zeroed: [(&str, fn(&mut RateLimitingSecurityConfig)); 4] = [
        ("auth_mfa_verify_max_requests", |p| p.auth_mfa_verify_max_requests = 0),
        ("auth_mfa_verify_window_secs", |p| p.auth_mfa_verify_window_secs = 0),
        ("graphql_validation_error_max_requests", |p| {
            p.graphql_validation_error_max_requests = 0;
        }),
        ("graphql_validation_error_window_secs", |p| {
            p.graphql_validation_error_window_secs = 0;
        }),
    ];
    for (key, zero) in zeroed {
        let mut policy = RateLimitingSecurityConfig {
            auth_rules_enabled: false,
            ..RateLimitingSecurityConfig::DEFAULT
        };
        zero(&mut policy);
        let refusal = Box::pin(Server::new(
            config(),
            schema_with(Some(policy)),
            Arc::new(FailingAdapter::new()),
            None,
        ))
        .await
        .err()
        .unwrap_or_else(|| panic!("{key} = 0 booted"))
        .to_string();
        assert!(refusal.contains(&format!("{key} = 0")), "{key}: {refusal}");
    }
}
