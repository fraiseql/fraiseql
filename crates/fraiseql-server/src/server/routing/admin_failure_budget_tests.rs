//! The admin failure budget is one per client, across every admin router (#1760).
//!
//! Each admin router used to build its own failure limiter, so an IP locked out of one
//! still had the full budget on every other: about thirteen times the configured
//! `admin_auth_max_failures` against the same token. These build the routers the server
//! mounts and spread failures across two of them.
#![allow(clippy::unwrap_used)] // Reason: test code, panics acceptable

use std::sync::Arc;

use axum::body::Body;
use fraiseql_core::schema::CompiledSchema;
use fraiseql_test_utils::failing_adapter::FailingAdapter;
use http::{Request, StatusCode};
use tower::ServiceExt;

use crate::{server::Server, server_config::ServerConfig};

const ADMIN_TOKEN: &str = "an-admin-token-long-enough-to-be-accepted";
const MAX_FAILURES: u32 = 3;

async fn server() -> Server {
    let config = ServerConfig {
        cors_enabled: false,
        admin_api_enabled: true,
        admin_token: Some(ADMIN_TOKEN.to_owned()),
        admin_auth_max_failures: MAX_FAILURES,
        ..ServerConfig::default()
    };
    Box::pin(Server::new(
        config,
        CompiledSchema::new(),
        Arc::new(FailingAdapter::new()),
        None,
    ))
    .await
    .expect("Server::new should succeed for an empty schema")
}

async fn status(app: &axum::Router, method: &str, uri: &str, token: &str) -> StatusCode {
    app.clone()
        .oneshot(
            Request::builder()
                .method(method)
                .uri(uri)
                .header("authorization", format!("Bearer {token}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap()
        .status()
}

/// The write and read admin routers are mounted separately; a wrong token on either
/// spends the same budget. The last request is the only one that would not exhaust a
/// per-router budget on its own: one failure on the write router, the rest on the read
/// router, then the write router must already refuse.
#[tokio::test]
async fn failures_spread_across_two_admin_routers_exhaust_one_budget() {
    let server = server().await;
    let state = server.build_app_state();
    let app = server.build_router(&state);

    assert_eq!(
        status(&app, "POST", "/api/v1/admin/cache/clear", "wrong").await,
        StatusCode::FORBIDDEN,
        "a wrong admin token is refused"
    );
    for _ in 1..MAX_FAILURES {
        let refused = status(&app, "GET", "/api/v1/admin/config", "wrong").await;
        assert!(
            refused == StatusCode::FORBIDDEN || refused == StatusCode::TOO_MANY_REQUESTS,
            "the read router refused a wrong token with {refused}"
        );
    }

    // {MAX_FAILURES} failures are spent, only one of them here: the write router refuses
    // before it checks the token, even the right one.
    assert_eq!(
        status(&app, "POST", "/api/v1/admin/cache/clear", ADMIN_TOKEN).await,
        StatusCode::TOO_MANY_REQUESTS,
        "{MAX_FAILURES} failures across two admin routers must block the client on both"
    );
}

/// Every admin auth state is built on the shared limiter. Most admin routers mount only
/// with a database, OIDC or a feature's own config, so the test above drives two of them;
/// this pins the rest by position: outside the auth module's own constructors, a limiter
/// is built once (the server's), and no production code calls a constructor that builds
/// its own.
#[test]
fn no_admin_router_builds_a_failure_limiter_of_its_own() {
    const OWN_LIMITER: [&str; 4] = [
        "FailureLimiter::new(",
        "BearerAuthState::with_max_failures(",
        "AdminDualAuthState::new(",
        "AdminPrincipalState::new(",
    ];
    let src = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
    let mut found = Vec::new();
    let mut files = vec![src.clone()];
    while let Some(path) = files.pop() {
        if path.is_dir() {
            files.extend(std::fs::read_dir(&path).unwrap().map(|entry| entry.unwrap().path()));
            continue;
        }
        let name = path.file_name().unwrap().to_string_lossy();
        let relative = path.strip_prefix(&src).unwrap().to_string_lossy().replace('\\', "/");
        let is_test = name == "tests.rs"
            || name.ends_with("_tests.rs")
            || relative.split('/').any(|part| part == "tests");
        if !name.ends_with(".rs") || is_test || relative == "middleware/auth.rs" {
            continue;
        }
        for (index, line) in std::fs::read_to_string(&path).unwrap().lines().enumerate() {
            if line.trim_start().starts_with("//") {
                continue;
            }
            for call in OWN_LIMITER {
                if line.contains(call) {
                    found.push(format!("{relative}:{}: {}", index + 1, line.trim()));
                }
            }
        }
    }
    let server_limiters = found.iter().filter(|hit| hit.starts_with("server/builder.rs:")).count();
    assert_eq!(
        server_limiters, 1,
        "the server builds the one admin failure limiter: {found:#?}"
    );
    let elsewhere: Vec<_> =
        found.iter().filter(|hit| !hit.starts_with("server/builder.rs:")).collect();
    assert!(
        elsewhere.is_empty(),
        "an admin auth state builds its own failure limiter, which gives a client a budget \
         per router (#1760); take `admin_failure_limiter.clone()` instead:\n{elsewhere:#?}"
    );
}
