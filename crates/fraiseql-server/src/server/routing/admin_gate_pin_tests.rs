//! What the admin token gates answer today, through the routers the server mounts.
//!
//! Each admin router carries one of three middlewares: a bearer gate for one token, a
//! dual gate that accepts the write or the read-only token (the SQL console), and a
//! principal gate that also accepts tenant admin tokens (which needs a database, so it
//! is pinned in `tests/admin_gate_pg.rs`). These pin every status those gates answer,
//! which credential each route accepts, that the gate answers before any extractor
//! reads the request, and that every gate spends the one failure budget.
//!
//! Requests carry no `ConnectInfo`, so every caller here is the `unknown` peer.
#![allow(clippy::unwrap_used)] // Reason: test code, panics acceptable

use std::sync::Arc;

use axum::{Router, body::Body};
use fraiseql_core::schema::CompiledSchema;
use fraiseql_test_utils::failing_adapter::FailingAdapter;
use http::{Method, Request, StatusCode, header};
use tower::ServiceExt;

use crate::{server::Server, server_config::ServerConfig};

const ADMIN_TOKEN: &str = "an-admin-token-long-enough-to-be-accepted";
const READONLY_TOKEN: &str = "a-read-only-token-long-enough-to-be-accepted";
const METRICS_TOKEN: &str = "a-metrics-token-long-enough-to-be-accepted";

/// A budget small enough that a test spends it in a few requests.
const MAX_FAILURES: u32 = 2;

/// How a request presents a credential.
#[derive(Clone, Copy)]
enum Credential<'a> {
    None,
    /// An `Authorization` header that is not a bearer credential.
    Malformed,
    Bearer(&'a str),
}

/// One route, by method and a concrete path.
#[derive(Clone, Copy, Debug)]
struct Route {
    method: &'static str,
    path:   &'static str,
}

const fn route(method: &'static str, path: &'static str) -> Route {
    Route { method, path }
}

/// A route that takes the write token (`admin_token`) and nothing else.
const WRITE: Route = route("GET", "/admin/v1/schema");
/// A route that takes the read token: `admin_readonly_token` when it is set.
const READ: Route = route("GET", "/api/v1/admin/config");
/// A route that takes `metrics_token` and nothing else.
const METRICS: Route = route("GET", "/metrics");
/// The SQL console, which takes either admin token.
#[cfg(feature = "admin-sql")]
const SQL: Route = route("POST", "/api/v1/admin/sql");

fn config(readonly: bool) -> ServerConfig {
    ServerConfig {
        cors_enabled: false,
        admin_api_enabled: true,
        admin_token: Some(ADMIN_TOKEN.to_owned()),
        admin_readonly_token: readonly.then(|| READONLY_TOKEN.to_owned()),
        admin_auth_max_failures: MAX_FAILURES,
        metrics_enabled: true,
        metrics_token: Some(METRICS_TOKEN.to_owned()),
        #[cfg(feature = "admin-sql")]
        admin_sql: Some(crate::server_config::AdminSqlConfig {
            enabled: true,
            ..Default::default()
        }),
        ..ServerConfig::default()
    }
}

/// The server's router, split-token when `readonly`, single-token otherwise.
async fn app(readonly: bool) -> Router {
    let server = Box::pin(Server::new(
        config(readonly),
        CompiledSchema::new(),
        Arc::new(FailingAdapter::new()),
        None,
    ))
    .await
    .expect("Server::new should succeed for an empty schema");
    let state = server.build_app_state();
    server.build_router(&state)
}

async fn send_with(
    app: &Router,
    route: Route,
    credential: Credential<'_>,
    content_type: &str,
    body: &'static str,
) -> http::Response<axum::body::Body> {
    let mut request = Request::builder()
        .method(Method::from_bytes(route.method.as_bytes()).unwrap())
        .uri(route.path)
        .header(header::CONTENT_TYPE, content_type);
    match credential {
        Credential::None => {},
        Credential::Malformed => request = request.header(header::AUTHORIZATION, "Basic YTpi"),
        Credential::Bearer(token) => {
            request = request.header(header::AUTHORIZATION, format!("Bearer {token}"));
        },
    }
    app.clone().oneshot(request.body(Body::from(body)).unwrap()).await.unwrap()
}

async fn send(app: &Router, route: Route, credential: Credential<'_>) -> StatusCode {
    send_with(app, route, credential, "application/json", "{}").await.status()
}

/// The gate let the request through to its handler: whatever the handler answered, it is
/// none of the gate's refusals.
fn admitted(status: StatusCode) -> bool {
    ![
        StatusCode::UNAUTHORIZED,
        StatusCode::FORBIDDEN,
        StatusCode::TOO_MANY_REQUESTS,
    ]
    .contains(&status)
}

/// Every gated route and the token it takes, on a split-token server.
fn gates() -> Vec<(Route, &'static str)> {
    // Reason: only the build with the SQL console adds to the list.
    #[cfg_attr(not(feature = "admin-sql"), allow(unused_mut))]
    let mut gates = vec![
        (WRITE, ADMIN_TOKEN),
        (READ, READONLY_TOKEN),
        (METRICS, METRICS_TOKEN),
    ];
    #[cfg(feature = "admin-sql")]
    gates.push((SQL, READONLY_TOKEN));
    gates
}

/// A request with no `Authorization` header, or one that is not a bearer credential, is
/// refused as unauthenticated with a bearer challenge, and spends no budget: more of
/// them than the budget, then the right token, is served.
#[tokio::test]
async fn a_missing_or_malformed_credential_is_unauthenticated_and_spends_no_budget() {
    for (route, token) in gates() {
        let app = app(true).await;
        for credential in [Credential::None, Credential::Malformed] {
            for _ in 0..=MAX_FAILURES {
                let response = send_with(&app, route, credential, "application/json", "{}").await;
                assert_eq!(response.status(), StatusCode::UNAUTHORIZED, "{route:?}");
                assert_eq!(
                    response.headers().get(header::WWW_AUTHENTICATE).unwrap(),
                    "Bearer",
                    "{route:?} answers a missing or malformed credential with a bearer challenge"
                );
            }
        }
        let served = send(&app, route, Credential::Bearer(token)).await;
        assert!(admitted(served), "{route:?} refused its own token with {served}");
    }
}

/// A wrong token is forbidden; the failure that spends the budget is refused as too many,
/// and so is the right token after it.
#[tokio::test]
async fn a_wrong_token_spends_the_budget_and_then_the_right_one_is_refused() {
    for (route, token) in gates() {
        let app = app(true).await;
        let statuses = [
            send(&app, route, Credential::Bearer("wrong")).await,
            send(&app, route, Credential::Bearer("wrong")).await,
            send(&app, route, Credential::Bearer(token)).await,
        ];
        assert_eq!(
            statuses,
            [
                StatusCode::FORBIDDEN,
                StatusCode::TOO_MANY_REQUESTS,
                StatusCode::TOO_MANY_REQUESTS
            ],
            "{route:?}: a failure, the failure that spends a budget of two, then the right token"
        );
    }
}

/// Which token each route takes. On a split-token server the read routes refuse the write
/// token, while the SQL console takes both; the metrics routes take only their own token,
/// and no admin route takes it.
#[tokio::test]
async fn each_route_takes_its_own_credentials_and_refuses_the_others() {
    // Reason: only the build with the SQL console adds to the list.
    #[cfg_attr(not(feature = "admin-sql"), allow(unused_mut))]
    let mut cases = vec![
        (true, WRITE, ADMIN_TOKEN, true),
        (true, WRITE, READONLY_TOKEN, false),
        (true, WRITE, METRICS_TOKEN, false),
        (true, READ, READONLY_TOKEN, true),
        (true, READ, ADMIN_TOKEN, false),
        (true, READ, METRICS_TOKEN, false),
        (true, METRICS, METRICS_TOKEN, true),
        (true, METRICS, ADMIN_TOKEN, false),
        (true, METRICS, READONLY_TOKEN, false),
        // Single-token: the read routes fall back to the write token.
        (false, READ, ADMIN_TOKEN, true),
        (false, READ, READONLY_TOKEN, false),
    ];
    #[cfg(feature = "admin-sql")]
    cases.extend([
        (true, SQL, ADMIN_TOKEN, true),
        (true, SQL, READONLY_TOKEN, true),
        (true, SQL, METRICS_TOKEN, false),
        (false, SQL, ADMIN_TOKEN, true),
    ]);
    for (readonly, route, token, served) in cases {
        let status = send(&app(readonly).await, route, Credential::Bearer(token)).await;
        if served {
            assert!(
                admitted(status),
                "{route:?} (split: {readonly}) refused a token it takes: {status}"
            );
        } else {
            assert_eq!(
                status,
                StatusCode::FORBIDDEN,
                "{route:?} (split: {readonly}) must refuse a token it does not take"
            );
        }
    }
}

/// A token a route refuses is a failure like any wrong token: the write token on a
/// split-token read route spends the budget.
#[tokio::test]
async fn a_credential_a_route_refuses_spends_the_budget() {
    let app = app(true).await;
    assert_eq!(send(&app, READ, Credential::Bearer(ADMIN_TOKEN)).await, StatusCode::FORBIDDEN);
    assert_eq!(
        send(&app, READ, Credential::Bearer(ADMIN_TOKEN)).await,
        StatusCode::TOO_MANY_REQUESTS
    );
}

/// The gate answers before any extractor reads the request: with no credential, a body of
/// the wrong media type, a malformed body or a malformed path parameter is still 401, not
/// the 400, 415 or 422 the handler's extractors would answer.
#[tokio::test]
async fn the_gate_answers_before_the_request_is_read() {
    let app = app(true).await;
    // Reason: only the build with the SQL console adds to the list.
    #[cfg_attr(not(feature = "admin-sql"), allow(unused_mut))]
    let mut cases = vec![
        (route("POST", "/api/v1/admin/cache/clear"), "text/plain", "not json"),
        (route("POST", "/api/v1/admin/explain"), "application/json", "{"),
        (route("POST", "/admin/v1/users/not-an-id/revoke"), "application/json", "{"),
    ];
    #[cfg(feature = "admin-sql")]
    cases.push((SQL, "text/plain", "not json"));
    for (route, content_type, body) in cases {
        let status = send_with(&app, route, Credential::None, content_type, body).await.status();
        assert_eq!(status, StatusCode::UNAUTHORIZED, "{route:?} read the request before the gate");
    }
}

/// Every token-gated route a database-free server mounts refuses an anonymous request.
/// A route that drops its gate, or is no longer mounted, fails here by name.
#[tokio::test]
async fn every_admin_route_refuses_an_anonymous_request() {
    // Reason: only a build with the SQL console or federation adds to the list.
    #[cfg_attr(
        not(any(feature = "admin-sql", feature = "federation")),
        allow(unused_mut)
    )]
    let mut routes = vec![
        // Studio admin API: the write token.
        route("GET", "/admin/v1/schema"),
        route("GET", "/admin/v1/health/detailed"),
        route("POST", "/admin/v1/users/some-user/revoke"),
        route("GET", "/admin/v1/metrics/summary"),
        // The admin write router.
        route("POST", "/api/v1/admin/reload-schema"),
        route("POST", "/api/v1/admin/cache/clear"),
        route("POST", "/api/v1/admin/query-stats/reset"),
        route("PUT", "/api/v1/admin/tenants/acme"),
        route("DELETE", "/api/v1/admin/tenants/acme"),
        route("POST", "/api/v1/admin/tenants/acme/suspend"),
        route("POST", "/api/v1/admin/tenants/acme/resume"),
        route("PUT", "/api/v1/admin/domains/example.com"),
        route("DELETE", "/api/v1/admin/domains/example.com"),
        // The admin read router.
        route("GET", "/api/v1/admin/cache/stats"),
        route("GET", "/api/v1/admin/config"),
        route("POST", "/api/v1/admin/explain"),
        route("GET", "/api/v1/admin/tenants"),
        route("GET", "/api/v1/admin/tenants/acme"),
        route("GET", "/api/v1/admin/tenants/acme/health"),
        route("GET", "/api/v1/admin/tenants/acme/events"),
        route("GET", "/api/v1/admin/domains"),
        route("POST", "/api/v1/query/explain"),
        route("GET", "/api/v1/admin/grafana-dashboard"),
        route("GET", "/api/v1/admin/usage"),
        route("GET", "/api/v1/admin/query-stats"),
        route("GET", "/api/v1/admin/query-stats/1"),
        route("GET", "/api/v1/query/stats"),
        // Metrics.
        route("GET", "/metrics"),
        route("GET", "/metrics/json"),
    ];
    #[cfg(feature = "federation")]
    routes.extend([
        route("GET", "/api/v1/federation/subgraphs"),
        route("GET", "/api/v1/federation/graph"),
        route("GET", "/api/v1/federation/plan"),
    ]);
    #[cfg(feature = "admin-sql")]
    routes.push(SQL);
    for readonly in [false, true] {
        let app = app(readonly).await;
        for route in &routes {
            assert_eq!(
                send(&app, *route, Credential::None).await,
                StatusCode::UNAUTHORIZED,
                "{route:?} (split: {readonly}) served an anonymous request"
            );
        }
    }
}

/// Failures on routers behind different middlewares spend one budget: a wrong token on
/// the SQL console (the dual gate) or the write router, then one on the metrics routes,
/// is the failure that spends a budget of two.
#[tokio::test]
async fn failures_on_different_kinds_of_gate_spend_one_budget() {
    let app = app(true).await;
    #[cfg(feature = "admin-sql")]
    let first = SQL;
    #[cfg(not(feature = "admin-sql"))]
    let first = WRITE;
    assert_eq!(send(&app, first, Credential::Bearer("wrong")).await, StatusCode::FORBIDDEN);
    assert_eq!(
        send(&app, METRICS, Credential::Bearer("wrong")).await,
        StatusCode::TOO_MANY_REQUESTS,
        "a failure on {first:?} and one on the metrics routes must spend one budget of two"
    );
    assert_eq!(
        send(&app, READ, Credential::Bearer(READONLY_TOKEN)).await,
        StatusCode::TOO_MANY_REQUESTS,
        "the spent budget refuses the read router too"
    );
}
