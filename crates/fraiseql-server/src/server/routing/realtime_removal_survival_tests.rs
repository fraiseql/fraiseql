//! Survival pins for the #605 `/realtime/v1` removal train (Phase 00).
//!
//! These are **black-box HTTP pins**: they assemble a `Server` and mount its base, admin, and
//! extension routes the way the binary does (`Server::new` → `build_app_state` →
//! `mount_base_and_admin_routes` + `mount_extensions`), then probe by path string only — they
//! import no
//! `realtime` symbol. That is deliberate: the removal train deletes the realtime code in
//! stages, and a pin that referenced any realtime type would have to change as the code
//! shrinks, defeating its purpose. Because these probe paths only, they stay green and
//! *unchanged* through every phase:
//!
//! - `/realtime/v1` → **404** today (dormant subsystem is never assembled in a default server) and
//!   still **404** after Phase 06 (the code no longer exists). The invariant — "a default
//!   production assembly does not expose the entity-stream endpoint" — is the core guarantee the
//!   whole train protects, and this pin also guards against a future permissive assembly
//!   re-introducing it.
//! - `/ws` → **not 404** (the live GraphQL-subscription path, which the train must preserve). The
//!   handler-level behaviour of that path (#596 row-visibility, #611 hot-reload) is covered by
//!   `tests/graphql_ws_row_visibility_pin_test.rs` and `routes/subscriptions/tests.rs`; this pin
//!   adds only the *structural* guarantee that the production router actually mounts it, so tearing
//!   out realtime does not collaterally unmount the path that supersedes it.
//!
//! This file is intended to OUTLIVE the train as a permanent regression guard, so it is an
//! expected survivor of the Phase 07 archaeology grep.
#![allow(clippy::unwrap_used)] // Reason: test code, panics acceptable

use std::sync::Arc;

use axum::{Router, body::Body};
use fraiseql_core::{cache::CachedDatabaseAdapter, schema::CompiledSchema};
use fraiseql_test_utils::failing_adapter::FailingAdapter;
use http::{Request, StatusCode};
use tower::ServiceExt;

use crate::{server::Server, server_config::ServerConfig};

/// Assemble a full production router (base + admin + extensions) from `config` with an empty
/// schema and a healthy mock adapter — no OIDC validator, nothing that would mount the dormant
/// realtime subsystem (which requires an explicit `.with_realtime(..)`).
async fn prod_router(config: ServerConfig) -> Router {
    let server: Server<CachedDatabaseAdapter<FailingAdapter>> =
        Server::new(config, CompiledSchema::new(), Arc::new(FailingAdapter::new()), None)
            .await
            .expect("Server::new should succeed for an empty schema + default config");
    let state = server.build_app_state();
    let app = server.mount_base_and_admin_routes(Router::new(), &state);
    server.mount_extensions(app, &state)
}

/// Pin #2: a default production assembly does NOT mount the `/realtime/v1` entity-stream
/// endpoint. 404 today because the dormant subsystem is never assembled; 404 after Phase 06
/// because the code is gone. Either way, the endpoint is unreachable by default.
#[tokio::test]
async fn default_prod_assembly_does_not_mount_realtime_v1() {
    let app = prod_router(ServerConfig::default()).await;

    let response = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/realtime/v1")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(
        response.status(),
        StatusCode::NOT_FOUND,
        "the dormant /realtime/v1 entity-stream endpoint must not be mounted in a default \
         production assembly (a mounted WS route without an upgrade header answers 400/426, \
         not 404) — this invariant must hold before, during, and after the #605 removal",
    );
}

/// Pin #1 (structural): the live `/ws` GraphQL-subscription path IS mounted by a production
/// assembly. Uses `subscription_require_auth = false` so the path mounts without an OIDC
/// validator (with the default `require_auth`, subscriptions fail closed absent a validator —
/// a separate, intentional posture). A non-upgrade GET to a mounted WS route answers 400/426,
/// never 404; 404 would mean the path that *supersedes* the removed entity stream was
/// collaterally unmounted. Handler-level policy behaviour (#596/#611) is pinned elsewhere.
#[tokio::test]
async fn prod_assembly_mounts_live_ws_subscription_path() {
    let config = ServerConfig {
        subscription_require_auth: Some(false),
        ..ServerConfig::default()
    };
    let app = prod_router(config).await;

    let response = app
        .oneshot(Request::builder().method("GET").uri("/ws").body(Body::empty()).unwrap())
        .await
        .unwrap();

    assert_ne!(
        response.status(),
        StatusCode::NOT_FOUND,
        "the live /ws GraphQL-subscription path must stay mounted in a default assembly \
         (subscriptions_enabled defaults true) throughout the #605 realtime removal",
    );
}

/// Pin #3 (Phase 01): the `/admin/v1/realtime/*` studio monitor routes are gone. With the
/// studio admin API enabled (an admin token configured so the studio admin router mounts), an
/// unauthenticated GET to a *removed* route answers **404** (path absent), whereas a *present*
/// admin route answers **401** (mounted, but behind the bearer-auth layer). So 404 proves the
/// route is unmounted without needing a valid token. Guards against a future re-mount of the
/// removed monitor surface. Stays green for the rest of the train (the routes never return).
#[tokio::test]
async fn admin_assembly_does_not_expose_realtime_monitor_routes() {
    let config = ServerConfig {
        admin_api_enabled: true,
        admin_token: Some("realtime-removal-test-admin-token-0123456789".to_string()),
        ..ServerConfig::default()
    };
    let app = prod_router(config).await;

    // Positive control: a *surviving* studio admin route answers 401 (mounted, behind the
    // bearer-auth layer) to an unauthenticated GET. This proves the studio admin router is
    // actually mounted under this config — without it, the 404 assertions below would pass
    // vacuously if a future refactor stopped the router from mounting at all.
    let control = app
        .clone()
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/admin/v1/schema")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(
        control.status(),
        StatusCode::UNAUTHORIZED,
        "the studio admin router must be mounted (a surviving admin route answers 401 \
         unauthenticated) — otherwise the realtime-route 404 checks below are vacuous",
    );

    for path in [
        "/admin/v1/realtime/stats",
        "/admin/v1/realtime/broadcast",
        "/admin/v1/realtime/presence",
        "/admin/v1/realtime/cdc",
    ] {
        let response = app
            .clone()
            .oneshot(Request::builder().method("GET").uri(path).body(Body::empty()).unwrap())
            .await
            .unwrap();

        assert_eq!(
            response.status(),
            StatusCode::NOT_FOUND,
            "removed studio monitor route {path} must be unmounted (404), not merely \
             auth-gated (401) — a mounted admin route would answer 401 here",
        );
    }
}

/// Pin #4 (Phase 02): the `POST /realtime/v1/broadcast` channel-broadcast endpoint is gone.
/// It was config-gated (via the builder's now-removed `with_broadcast`), so no configuration
/// can reach a mount any more — the endpoint is structurally absent. A default assembly (which
/// does mount live routes like `/ws` and `/health`, so the router is non-empty) answers **404**
/// on both the endpoint's real method (POST) and a probe method. Broadcast was removed *without
/// replacement* — `/ws` does not supersede it — so this endpoint must never return.
#[tokio::test]
async fn assembly_does_not_expose_realtime_broadcast_endpoint() {
    let app = prod_router(ServerConfig::default()).await;

    // Positive control: a live route exists (the router is non-empty), so a 404 on the
    // broadcast path is meaningful absence, not an empty-router artifact.
    let control = app
        .clone()
        .oneshot(Request::builder().method("GET").uri("/health").body(Body::empty()).unwrap())
        .await
        .unwrap();
    assert_ne!(
        control.status(),
        StatusCode::NOT_FOUND,
        "sanity: a default assembly must mount /health, proving the router is non-empty",
    );

    for method in ["POST", "GET"] {
        let response = app
            .clone()
            .oneshot(
                Request::builder()
                    .method(method)
                    .uri("/realtime/v1/broadcast")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(
            response.status(),
            StatusCode::NOT_FOUND,
            "POST /realtime/v1/broadcast was removed (Cluster C); no config can mount it \
             (probed with {method}) — 404, never a mounted 401/405",
        );
    }
}
