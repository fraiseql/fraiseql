//! The auth layer of each admin router no other suite reaches: the JWKS refresh, the
//! metrics endpoints, the identity-cache flush, the suppression admin API and the observer
//! changelog, runtime and dead-letter routers.
//!
//! Each router is mounted by the server's own constructor (`build_router`), and each
//! test makes two calls: one with no credential, refused with `401`, and one carrying the
//! router's credential, admitted (neither `401` nor `403`, and not `404`, so the route is
//! mounted). The observer handlers also refuse a call with no principal themselves, so on
//! those routers it is the admitted call that shows the layer is the one admitting.
#![allow(clippy::unwrap_used)] // Reason: test code, panics acceptable

use std::sync::Arc;

use axum::{Router, body::Body};
use fraiseql_core::schema::CompiledSchema;
use fraiseql_test_utils::failing_adapter::FailingAdapter;
use http::{Method, Request, StatusCode};
use tower::ServiceExt;

use crate::{server::Server, server_config::ServerConfig};

/// The admin bearer token these servers are configured with.
const ADMIN_TOKEN: &str = "mount-gate-admin-token-0123456789abcdef";

/// The metrics bearer token.
const METRICS_TOKEN: &str = "mount-gate-metrics-token-0123456789";

/// A server built from `config`, before its router is.
async fn server(config: ServerConfig) -> Server {
    Box::pin(Server::new(
        ServerConfig {
            cors_enabled: false,
            ..config
        },
        CompiledSchema::new(),
        Arc::new(FailingAdapter::new()),
        None,
    ))
    .await
    .unwrap()
}

/// The status `app` answers `method uri`, carrying `bearer` when given.
async fn status(app: &Router, method: Method, uri: &str, bearer: Option<&str>) -> StatusCode {
    let mut request = Request::builder()
        .method(method)
        .uri(uri)
        .header("content-type", "application/json");
    if let Some(bearer) = bearer {
        request = request.header("authorization", format!("Bearer {bearer}"));
    }
    app.clone()
        .oneshot(request.body(Body::from("{}")).unwrap())
        .await
        .unwrap()
        .status()
}

/// `method uri` refuses a call with no credential and admits one carrying `bearer`.
async fn assert_gated(app: &Router, method: Method, uri: &str, bearer: &str) {
    assert_eq!(
        status(app, method.clone(), uri, None).await,
        StatusCode::UNAUTHORIZED,
        "{method} {uri} with no credential"
    );
    let admitted = status(app, method.clone(), uri, Some(bearer)).await;
    assert!(
        ![
            StatusCode::UNAUTHORIZED,
            StatusCode::FORBIDDEN,
            StatusCode::NOT_FOUND
        ]
        .contains(&admitted),
        "{method} {uri} with its credential must reach the handler, got {admitted}"
    );
}

/// A pool that is never connected: the layer answers before any handler, and a handler's
/// own failure to reach it is none of the statuses the admitted call rules out.
#[cfg(any(
    all(feature = "observers", feature = "auth"),
    feature = "inbound-email"
))]
fn unconnected_pool() -> sqlx::PgPool {
    sqlx::postgres::PgPoolOptions::new()
        .acquire_timeout(std::time::Duration::from_secs(1))
        .connect_lazy("postgres://test:test@127.0.0.1:1/test")
        .unwrap()
}

#[cfg(feature = "auth")]
#[tokio::test]
async fn the_jwks_refresh_takes_the_admin_token() {
    let idp = crate::testing::idp::TestIdp::start().await;
    let mut server = server(ServerConfig {
        admin_api_enabled: true,
        admin_token: Some(ADMIN_TOKEN.to_string()),
        ..ServerConfig::default()
    })
    .await;
    server.oidc_validator = Some(idp.validator(false));
    let app = server.build_router(&server.build_app_state());

    assert_gated(&app, Method::POST, "/admin/v1/auth/refresh-jwks", ADMIN_TOKEN).await;
}

#[tokio::test]
async fn the_metrics_endpoints_take_the_metrics_token() {
    let server = server(ServerConfig {
        metrics_enabled: true,
        metrics_token: Some(METRICS_TOKEN.to_string()),
        ..ServerConfig::default()
    })
    .await;
    let app = server.build_router(&server.build_app_state());

    for uri in [
        server.config.metrics_path.clone(),
        server.config.metrics_json_path.clone(),
    ] {
        assert_gated(&app, Method::GET, &uri, METRICS_TOKEN).await;
    }
}

#[cfg(feature = "auth")]
#[tokio::test]
async fn the_identity_cache_flush_takes_the_admin_token() {
    let server = server(ServerConfig {
        admin_token: Some(ADMIN_TOKEN.to_string()),
        ..ServerConfig::default()
    })
    .await;
    let mut state = server.build_app_state();
    state.identity = crate::identity::RequestIdentity::default()
        .with_resolver(Arc::new(crate::identity::tests::resolver_returning(&[])));
    let app = server.build_router(&state);

    assert_gated(&app, Method::POST, "/api/identity/flush-all", ADMIN_TOKEN).await;
}

#[cfg(feature = "inbound-email")]
#[tokio::test]
async fn the_suppression_admin_api_takes_the_admin_token() {
    /// The variable naming the server's HMAC secret, read when the router is built.
    const HMAC_ENV: &str = "FRAISEQL_MOUNT_GATE_TEST_HMAC_SECRET";

    let mut server = server(ServerConfig {
        admin_token: Some(ADMIN_TOKEN.to_string()),
        hmac_secret_env: Some(HMAC_ENV.to_string()),
        ..ServerConfig::default()
    })
    .await;
    server.pools = Some(unconnected_pool()).into();
    let state = server.build_app_state();
    let app = temp_env::with_var(HMAC_ENV, Some("mount-gate-hmac-secret"), || {
        server.build_router(&state)
    });

    assert_gated(&app, Method::POST, "/api/email/suppression", ADMIN_TOKEN).await;
}

/// The observer routers, mounted under OIDC with a runtime, so the runtime and
/// dead-letter routers are mounted beside the management and changelog ones.
#[cfg(all(feature = "observers", feature = "auth"))]
mod observer_admin {
    use tokio::sync::RwLock;

    use super::*;
    use crate::{
        observers::runtime::{ObserverRuntime, ObserverRuntimeConfig},
        testing::idp::TestIdp,
    };

    async fn app(idp: &TestIdp) -> Router {
        let mut server = server(ServerConfig::default()).await;
        server.pools = Some(unconnected_pool()).into();
        server.observer_runtime = Some(Arc::new(RwLock::new(ObserverRuntime::new(
            ObserverRuntimeConfig::new(unconnected_pool()),
        ))));
        server.oidc_validator = Some(idp.validator(false));
        server.build_router(&server.build_app_state())
    }

    /// A platform operator's token: the admin scope, and no tenant.
    fn operator(idp: &TestIdp) -> String {
        idp.mint(&idp.claims("operator").set("scope", "fraiseql:admin"))
    }

    #[tokio::test]
    async fn the_changelog_takes_an_operators_token() {
        let idp = TestIdp::start().await;
        assert_gated(&app(&idp).await, Method::GET, "/api/observers/changelog", &operator(&idp))
            .await;
    }

    #[tokio::test]
    async fn the_runtime_routes_take_an_operators_token() {
        let idp = TestIdp::start().await;
        let app = app(&idp).await;
        assert_gated(&app, Method::GET, "/api/observers/runtime/health", &operator(&idp)).await;
    }

    #[tokio::test]
    async fn the_dead_letter_routes_take_an_operators_token() {
        let idp = TestIdp::start().await;
        assert_gated(&app(&idp).await, Method::GET, "/api/observers/dlq", &operator(&idp)).await;
    }
}

/// A router whose config asks for auth it has no way to check is not mounted at all:
/// its route answers `404` and the mount ledger records no mount of it. Each control
/// mounts the same router once the credential's source is configured, so the request names
/// a route the router serves.
mod unmounted_without_its_credential {
    use super::*;

    /// `true` when `server` recorded a mount of `transport`.
    fn recorded(server: &Server, transport: &str) -> bool {
        server.mounts.records().iter().any(|mount| mount.transport == transport)
    }

    /// The status of `method uri` on the router of `server`, and whether `transport` was
    /// recorded while building it.
    async fn answer(
        server: &Server,
        transport: &str,
        method: Method,
        uri: &str,
    ) -> (StatusCode, bool) {
        let app = server.build_router(&server.build_app_state());
        (status(&app, method, uri, None).await, recorded(server, transport))
    }

    /// The server of `config`, under a required OIDC validator when `oidc` is true.
    ///
    /// Boot refuses the playground and introspection outside development mode, and the
    /// metrics and admin switches without their tokens; the mount is what is tested, so
    /// these four are set after boot.
    async fn under(config: ServerConfig, oidc: bool) -> Server {
        let enabled = (
            config.introspection_enabled,
            config.playground_enabled,
            config.metrics_enabled,
            config.admin_api_enabled,
        );
        let mut server = server(ServerConfig {
            introspection_enabled: false,
            playground_enabled: false,
            metrics_enabled: config.metrics_enabled && config.metrics_token.is_some(),
            admin_api_enabled: config.admin_api_enabled && config.admin_token.is_some(),
            ..config
        })
        .await;
        (
            server.config.introspection_enabled,
            server.config.playground_enabled,
            server.config.metrics_enabled,
            server.config.admin_api_enabled,
        ) = enabled;
        if oidc {
            #[cfg(feature = "auth")]
            {
                let idp = crate::testing::idp::TestIdp::start().await;
                server.oidc_validator = Some(idp.validator(true));
            }
        }
        server
    }

    /// `method uri` of `transport` is absent without OIDC, and mounted with it.
    async fn assert_unmounted_without_oidc(
        config: ServerConfig,
        transport: &str,
        method: Method,
        uri: &str,
    ) {
        let (status, mounted) =
            answer(&under(config.clone(), false).await, transport, method.clone(), uri).await;
        assert_eq!(status, StatusCode::NOT_FOUND, "{method} {uri} without OIDC");
        assert!(!mounted, "{transport} recorded a mount without OIDC");
        #[cfg(feature = "auth")]
        {
            let (status, mounted) =
                answer(&under(config, true).await, transport, method.clone(), uri).await;
            assert_eq!(status, StatusCode::UNAUTHORIZED, "{method} {uri} under OIDC");
            assert!(mounted, "{transport} is mounted under OIDC");
        }
    }

    #[tokio::test]
    async fn the_playground_requiring_auth_is_not_mounted_without_oidc() {
        let config = ServerConfig {
            playground_enabled: true,
            playground_require_auth: Some(true),
            ..ServerConfig::default()
        };
        let path = config.playground_path.clone();
        Box::pin(assert_unmounted_without_oidc(config, "playground", Method::GET, &path)).await;
    }

    #[tokio::test]
    async fn introspection_requiring_auth_is_not_mounted_without_oidc() {
        let config = ServerConfig {
            introspection_enabled: true,
            introspection_require_auth: true,
            ..ServerConfig::default()
        };
        let path = config.introspection_path.clone();
        Box::pin(assert_unmounted_without_oidc(config, "introspection", Method::GET, &path)).await;
    }

    #[tokio::test]
    async fn the_schema_export_requiring_auth_is_not_mounted_without_oidc() {
        let config = ServerConfig {
            introspection_enabled: true,
            introspection_require_auth: true,
            schema_export_require_auth: Some(true),
            ..ServerConfig::default()
        };
        Box::pin(assert_unmounted_without_oidc(
            config,
            "schema-export",
            Method::GET,
            "/api/v1/schema.json",
        ))
        .await;
    }

    #[tokio::test]
    async fn the_schema_metadata_requiring_auth_is_not_mounted_without_oidc() {
        let config = ServerConfig {
            introspection_enabled: true,
            introspection_require_auth: true,
            metadata_require_auth: Some(true),
            ..ServerConfig::default()
        };
        let uri = "/api/v1/schema/metadata";
        Box::pin(assert_unmounted_without_oidc(config, "schema-metadata", Method::GET, uri)).await;
    }

    #[tokio::test]
    async fn the_design_audit_requiring_auth_is_not_mounted_without_oidc() {
        let config = ServerConfig {
            design_api_require_auth: true,
            ..ServerConfig::default()
        };
        let uri = "/api/v1/design/audit";
        Box::pin(assert_unmounted_without_oidc(config, "design-audit", Method::POST, uri)).await;
    }

    #[tokio::test]
    async fn the_metrics_endpoints_are_not_mounted_without_a_token() {
        let config = ServerConfig {
            metrics_enabled: true,
            ..ServerConfig::default()
        };
        let uri = config.metrics_path.clone();
        let (status, mounted) =
            answer(&under(config.clone(), false).await, "metrics", Method::GET, &uri).await;
        assert_eq!(status, StatusCode::NOT_FOUND, "GET {uri} without a metrics token");
        assert!(!mounted, "metrics recorded a mount without a token");

        let with_token = under(
            ServerConfig {
                metrics_token: Some(METRICS_TOKEN.to_string()),
                ..config
            },
            false,
        )
        .await;
        assert_eq!(
            answer(&with_token, "metrics", Method::GET, &uri).await,
            (StatusCode::UNAUTHORIZED, true)
        );
    }

    #[tokio::test]
    async fn the_admin_api_is_not_mounted_without_a_token() {
        let config = ServerConfig {
            admin_api_enabled: true,
            ..ServerConfig::default()
        };
        let uri = "/api/v1/admin/config";
        let (status, mounted) =
            answer(&under(config.clone(), false).await, "admin", Method::GET, uri).await;
        assert_eq!(status, StatusCode::NOT_FOUND, "GET {uri} without an admin token");
        assert!(!mounted, "admin recorded a mount without a token");

        let with_token = under(
            ServerConfig {
                admin_token: Some(ADMIN_TOKEN.to_string()),
                ..config
            },
            false,
        )
        .await;
        assert_eq!(
            answer(&with_token, "admin", Method::GET, uri).await,
            (StatusCode::UNAUTHORIZED, true)
        );
    }
}

/// `playground_require_auth` requires a credential whatever the validator's own
/// `required` flag says, as introspection, schema export and metadata do.
#[cfg(feature = "auth")]
#[tokio::test]
async fn the_playground_requiring_auth_refuses_a_call_with_no_credential_under_optional_oidc() {
    let idp = crate::testing::idp::TestIdp::start().await;
    let mut server = server(ServerConfig {
        playground_require_auth: Some(true),
        ..ServerConfig::default()
    })
    .await;
    // Boot refuses the playground outside development mode; the mount is what is tested.
    server.config.playground_enabled = true;
    server.oidc_validator = Some(idp.validator(false));
    let app = server.build_router(&server.build_app_state());
    let path = server.config.playground_path.clone();

    assert_eq!(status(&app, Method::GET, &path, None).await, StatusCode::UNAUTHORIZED);
    let token = idp.token("alice", "j-alice");
    assert_eq!(status(&app, Method::GET, &path, Some(&token)).await, StatusCode::OK);
}
