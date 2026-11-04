//! Where the OIDC layer accepts the `__Host-access_token` cookie, on the real router.
//!
//! The cookie is accepted on the mounts whose routes refuse a body that is not `JSON` or
//! only read (see [`AccessCookie`](super::AccessCookie)), and refused on the others, where
//! the same token sent as `Authorization: Bearer` is still accepted. Each mount that
//! accepts it also refuses a request carrying no credential, so an accepted cookie is the
//! cookie authenticating, not the route being open.
#![allow(clippy::unwrap_used)] // Reason: test code, panics acceptable

use std::sync::Arc;

use axum::{Router, body::Body};
use fraiseql_core::schema::{CompiledSchema, RestConfig, SubscriptionDefinition};
use fraiseql_test_utils::failing_adapter::FailingAdapter;
use http::{Request, StatusCode, header};
use tower::ServiceExt;

use crate::{
    server::Server,
    server_config::{AsyncOperationsConfig, ServerConfig},
    testing::idp::{TestIdp, token},
    token_revocation::{InMemoryRevocationStore, TokenRevocationManager},
};

/// The full router of a server under OIDC with every mount these tests address: `/graphql`,
/// async operations, `REST`, the subscription socket, the playground, `/auth/me`, the
/// revocation routes, introspection, schema export and metadata, the design audit, and the
/// observer admin API.
async fn app(idp: &TestIdp) -> Router {
    let mut schema = CompiledSchema::new();
    schema.subscriptions.push(SubscriptionDefinition::new("orderChanged", "Order"));
    schema.rest_config = Some(RestConfig {
        enabled: true,
        require_auth: true,
        ..RestConfig::default()
    });
    let async_operations = AsyncOperationsConfig {
        operations: vec!["noSuchOperation".to_string()],
        ..AsyncOperationsConfig::default()
    };
    let config = ServerConfig {
        cors_enabled: false,
        subscriptions_enabled: true,
        subscription_require_auth: Some(true),
        introspection_require_auth: true,
        design_api_require_auth: true,
        ..ServerConfig::default()
    };
    let mut server = Box::pin(Server::new(config, schema, Arc::new(FailingAdapter::new()), None))
        .await
        .unwrap()
        .with_revocation_manager(Arc::new(TokenRevocationManager::new(
            Arc::new(InMemoryRevocationStore::new()),
            false,
            false,
            3600,
        )));
    // The pool is never connected: the auth layer answers before any handler runs, and
    // the handlers' own failures are not 401s.
    let pool = sqlx::PgPool::connect_lazy("postgres://test:test@localhost/test").unwrap();
    server.async_operations = Server::build_async_operations(
        &ServerConfig {
            async_operations: Some(async_operations),
            ..ServerConfig::default()
        },
        Some(&pool),
    )
    .unwrap();
    server.db_pool = Some(pool);
    // Boot refuses the playground outside development mode; the mount is what is tested.
    server.config.playground_enabled = true;
    server.config.playground_require_auth = Some(true);
    server.config.introspection_enabled = true;
    server.oidc_validator = Some(idp.validator(true));
    server.config.auth = Some(fraiseql_core::security::OidcConfig {
        me: Some(fraiseql_core::security::oidc::MeEndpointConfig {
            enabled:       true,
            expose_claims: Vec::new(),
        }),
        ..fraiseql_core::security::OidcConfig::default()
    });
    server.build_router(&server.build_app_state())
}

/// How a request presents the token.
#[derive(Clone, Copy, Debug)]
enum Credential {
    Cookie,
    Bearer,
    Nothing,
}

/// A request, before its credential.
#[derive(Clone, Copy)]
struct Call {
    what:         &'static str,
    method:       &'static str,
    uri:          &'static str,
    content_type: Option<&'static str>,
    body:         &'static str,
    websocket:    bool,
}

impl Call {
    const fn get(what: &'static str, uri: &'static str) -> Self {
        Self {
            what,
            method: "GET",
            uri,
            content_type: None,
            body: "",
            websocket: false,
        }
    }

    const fn post(
        what: &'static str,
        uri: &'static str,
        content_type: &'static str,
        body: &'static str,
    ) -> Self {
        Self {
            what,
            method: "POST",
            uri,
            content_type: Some(content_type),
            body,
            websocket: false,
        }
    }

    /// `app`'s answer to this request carrying `credential`.
    async fn status(self, app: &Router, credential: Credential) -> StatusCode {
        let mut request = Request::builder().method(self.method).uri(self.uri);
        if let Some(content_type) = self.content_type {
            request = request.header(header::CONTENT_TYPE, content_type);
        }
        if self.websocket {
            request = request
                .header(header::CONNECTION, "Upgrade")
                .header(header::UPGRADE, "websocket")
                .header(header::SEC_WEBSOCKET_VERSION, "13")
                .header(header::SEC_WEBSOCKET_KEY, "dGhlIHNhbXBsZSBub25jZQ==")
                .header(header::SEC_WEBSOCKET_PROTOCOL, "graphql-transport-ws");
        }
        let token = token("alice", &format!("jti-{}", uuid::Uuid::new_v4()));
        request = match credential {
            Credential::Cookie => {
                request.header(header::COOKIE, format!("__Host-access_token={token}"))
            },
            Credential::Bearer => request.header(header::AUTHORIZATION, format!("Bearer {token}")),
            Credential::Nothing => request,
        };
        let request = request.body(Body::from(self.body)).unwrap();
        app.clone().oneshot(request).await.unwrap().status()
    }
}

const JSON: &str = "application/json";

/// The mounts that accept the cookie.
const ACCEPTED: [Call; 11] = [
    Call::get("GraphQL GET", "/graphql?query=%7B__typename%7D"),
    Call::post("GraphQL POST", "/graphql", JSON, r#"{"query":"{__typename}"}"#),
    Call::get("async operations", "/operations/v1/00000000-0000-0000-0000-000000000000"),
    Call::get("the playground", "/playground"),
    Call::get("/auth/me", "/auth/me"),
    Call::get("introspection", "/introspection"),
    Call::get("schema export", "/api/v1/schema.json"),
    Call::get("schema metadata", "/api/v1/schema/metadata"),
    Call::post("the design audit", "/api/v1/design/audit", JSON, "{}"),
    // Last: revoke-all revokes every token of alice's issued up to now, so a call after it
    // would be refused for that.
    Call::post("/auth/revoke", "/auth/revoke", JSON, "{}"),
    Call::post("/auth/revoke-all", "/auth/revoke-all", JSON, r#"{"sub":"alice"}"#),
];

#[tokio::test]
async fn the_cookie_is_a_credential_where_every_route_refuses_a_form_or_only_reads() {
    let idp = TestIdp::start().await;
    let app = app(&idp).await;
    for call in ACCEPTED {
        assert_eq!(
            call.status(&app, Credential::Nothing).await,
            StatusCode::UNAUTHORIZED,
            "{} must require a credential",
            call.what
        );
        let status = call.status(&app, Credential::Cookie).await;
        assert_ne!(status, StatusCode::UNAUTHORIZED, "{} with the cookie", call.what);
        assert_ne!(status, StatusCode::NOT_FOUND, "{} must be mounted", call.what);
    }
}

#[tokio::test]
async fn rest_the_socket_and_observer_admin_take_a_bearer_token_not_the_cookie() {
    let idp = TestIdp::start().await;
    let app = app(&idp).await;
    let ws = Call {
        websocket: true,
        ..Call::get("the subscription socket", "/ws")
    };
    let refused = [
        Call::get("REST", "/rest/v1/openapi.json"),
        ws,
        #[cfg(feature = "observers")]
        Call::post(
            "the observer admin API",
            "/api/observers/00000000-0000-0000-0000-000000000000/enable",
            JSON,
            "",
        ),
    ];
    for call in refused {
        assert_eq!(
            call.status(&app, Credential::Cookie).await,
            StatusCode::UNAUTHORIZED,
            "{} with the cookie",
            call.what
        );
        let status = call.status(&app, Credential::Bearer).await;
        assert_ne!(status, StatusCode::UNAUTHORIZED, "{} with a bearer token", call.what);
        assert_ne!(status, StatusCode::NOT_FOUND, "{} must be mounted", call.what);
    }
}

/// What makes the cookie safe on the `POST` routes that accept it: a form's body is
/// refused before it acts.
#[tokio::test]
async fn a_form_shaped_post_carrying_the_cookie_is_refused() {
    let idp = TestIdp::start().await;
    let app = app(&idp).await;
    for (what, uri) in [
        ("GraphQL", "/graphql"),
        ("/auth/revoke", "/auth/revoke"),
        ("/auth/revoke-all", "/auth/revoke-all"),
    ] {
        let call = Call::post(what, uri, "text/plain", r#"{"sub":"alice"}"#);
        assert_eq!(
            call.status(&app, Credential::Cookie).await,
            StatusCode::UNSUPPORTED_MEDIA_TYPE,
            "{what}"
        );
    }
}
