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
