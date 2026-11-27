//! A rate limit whose bucket cap is zero refuses boot, from either file and from the
//! environment, whether or not the global limit is on.
//!
//! With a cap of zero every new client key evicts another key's bucket before taking its
//! own, so no bucket lasts. The auth endpoint rules keep their buckets under the same
//! cap with the global limit off, which is why both arms refuse.

#![allow(clippy::unwrap_used, clippy::panic)] // Reason: test code, panics acceptable

use std::sync::Arc;

use fraiseql_core::schema::{CompiledSchema, RateLimitingSecurityConfig, SecurityConfig};
use fraiseql_test_utils::failing_adapter::FailingAdapter;

use super::Server;
use crate::{
    middleware::{RateLimitOverrides, ServerRateLimitingConfig},
    server_config::ServerConfig,
};

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

/// The global limit on, or off with the auth endpoint rules still on.
fn policies() -> [(&'static str, RateLimitingSecurityConfig); 2] {
    [
        (
            "global limit on",
            RateLimitingSecurityConfig {
                enabled: true,
                ..RateLimitingSecurityConfig::DEFAULT
            },
        ),
        (
            "global limit off, auth rules on",
            RateLimitingSecurityConfig {
                enabled: false,
                auth_rules_enabled: true,
                ..RateLimitingSecurityConfig::DEFAULT
            },
        ),
    ]
}

async fn boot_refusal(config: ServerConfig, schema: CompiledSchema) -> Option<String> {
    Box::pin(Server::new(config, schema, Arc::new(FailingAdapter::new()), None))
        .await
        .err()
        .map(|e| e.to_string())
}

fn assert_names_the_cap(refusal: Option<String>, case: &str) {
    let refusal = refusal.unwrap_or_else(|| panic!("{case}: max_buckets = 0 booted"));
    assert!(refusal.contains("max_buckets = 0"), "{case}: {refusal}");
}

#[tokio::test]
async fn a_zero_cap_in_the_compiled_section_refuses_boot() {
    for (case, policy) in policies() {
        let policy = RateLimitingSecurityConfig {
            max_buckets: 0,
            ..policy
        };
        assert_names_the_cap(boot_refusal(config(), schema_with(Some(policy))).await, case);
    }
}

#[tokio::test]
async fn a_zero_cap_in_the_server_files_table_refuses_boot() {
    for (case, policy) in policies() {
        let config = ServerConfig {
            rate_limiting: Some(ServerRateLimitingConfig {
                policy: RateLimitingSecurityConfig {
                    max_buckets: 0,
                    ..policy
                },
                ..ServerRateLimitingConfig::default()
            }),
            ..config()
        };
        assert_names_the_cap(boot_refusal(config, schema_with(None)).await, case);
    }
}

#[tokio::test]
async fn a_zero_cap_from_the_environment_override_refuses_boot() {
    for (case, policy) in policies() {
        let config = ServerConfig {
            rate_limit_overrides: RateLimitOverrides {
                max_buckets: Some(0),
                ..RateLimitOverrides::default()
            },
            ..config()
        };
        assert_names_the_cap(boot_refusal(config, schema_with(Some(policy))).await, case);
    }
}

#[tokio::test]
async fn a_positive_cap_boots() {
    for (case, policy) in policies() {
        let policy = RateLimitingSecurityConfig {
            max_buckets: 1,
            ..policy
        };
        let refusal = boot_refusal(config(), schema_with(Some(policy))).await;
        assert!(refusal.is_none(), "{case}: {refusal:?}");
    }
}
