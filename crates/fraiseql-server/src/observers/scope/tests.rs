#![allow(clippy::unwrap_used)] // Reason: test code, panics are acceptable

use std::{collections::HashMap, sync::Arc};

use axum::{
    Router,
    body::Body,
    http::{Method, Request, StatusCode},
};
use chrono::{Duration, Utc};
use fraiseql_core::security::{AuthenticatedUser, SecurityContext};
use serde_json::{Value, json};
use sqlx::PgPool;
use tokio::sync::RwLock;
use tower::ServiceExt;

use super::{ObserverScope, ScopeRefusal};
use crate::{
    middleware::{AuthUser, TenantClaim},
    observers::{
        DlqState, RuntimeHealthState, observer_dlq_routes, observer_runtime_routes,
        runtime::{ObserverRuntime, ObserverRuntimeConfig},
    },
};

fn admin(tenant: Option<Value>) -> AuthenticatedUser {
    let mut extra_claims = HashMap::new();
    if let Some(tenant) = tenant {
        extra_claims.insert("tenant_id".to_owned(), tenant);
    }
    AuthenticatedUser {
        user_id: fraiseql_core::types::UserId::new("admin-user"),
        scopes: vec!["fraiseql:admin".to_owned()],
        expires_at: Utc::now() + Duration::hours(1),
        email: None,
        display_name: None,
        extra_claims,
    }
}

fn context(tenant: Option<Value>) -> SecurityContext {
    fraiseql_core::security::SecurityContext::from_token_claims(
        &admin(tenant),
        "req-1".to_owned(),
        Some("tenant_id"),
    )
}

mod scope_of {
    use super::*;

    #[test]
    fn a_token_naming_no_tenant_is_the_operator() {
        assert_eq!(ObserverScope::of(Some(&context(None))), Ok(ObserverScope::Operator));
    }

    #[test]
    fn a_numeric_tenant_is_that_organisation() {
        assert_eq!(
            ObserverScope::of(Some(&context(Some(json!(42))))),
            Ok(ObserverScope::Organisation(42))
        );
        assert_eq!(
            ObserverScope::of(Some(&context(Some(json!("42"))))),
            Ok(ObserverScope::Organisation(42))
        );
    }

    #[test]
    fn a_tenant_that_is_not_an_i64_is_refused() {
        for tenant in [
            json!("acme"),
            json!("550e8400-e29b-41d4-a716-446655440000"),
            json!(1.5),
        ] {
            assert_eq!(
                ObserverScope::of(Some(&context(Some(tenant.clone())))),
                Err(ScopeRefusal::TenantIsNotAnOrganisation),
                "{tenant}"
            );
        }
    }

    #[test]
    fn no_principal_is_refused() {
        assert_eq!(ObserverScope::of(None), Err(ScopeRefusal::Unauthenticated));
    }

    #[test]
    fn only_an_organisation_owns_new_observers() {
        assert_eq!(ObserverScope::Operator.organisation(), None);
        assert_eq!(ObserverScope::Organisation(7).organisation(), Some(7));
    }
}

/// The runtime controls and the dead-letter queue span every tenant, so they refuse a token
/// that names one. Each route is listed: the gate is a per-handler extractor, and a handler
/// that lost it would serve the tenant.
mod operator_surfaces {
    use super::*;

    const ROUTES: &[(&str, &str)] = &[
        ("GET", "/runtime/health"),
        ("POST", "/runtime/reload"),
        ("GET", "/delivery/health"),
        ("GET", "/dlq"),
        ("GET", "/dlq/stats"),
        ("POST", "/dlq/retry-all"),
        ("GET", "/dlq/00000000-0000-0000-0000-000000000001"),
        ("DELETE", "/dlq/00000000-0000-0000-0000-000000000001"),
        ("POST", "/dlq/00000000-0000-0000-0000-000000000001/retry"),
    ];

    fn routes(tenant: Option<Value>) -> Router {
        let pool = PgPool::connect_lazy("postgres://test:test@localhost/test").unwrap();
        let runtime = Arc::new(RwLock::new(ObserverRuntime::new(ObserverRuntimeConfig::new(pool))));
        observer_runtime_routes(RuntimeHealthState {
            runtime: runtime.clone(),
        })
        .merge(observer_dlq_routes(DlqState { runtime }))
        .layer(axum::Extension(AuthUser(admin(tenant))))
        .layer(axum::Extension(TenantClaim(Arc::from("tenant_id"))))
    }

    async fn status(router: Router, method: &str, uri: &str) -> StatusCode {
        let request = Request::builder()
            .method(Method::from_bytes(method.as_bytes()).unwrap())
            .uri(uri)
            .body(Body::empty())
            .unwrap();
        router.oneshot(request).await.unwrap().status()
    }

    #[tokio::test]
    async fn refuse_an_organisation() {
        for (method, uri) in ROUTES {
            assert_eq!(
                status(routes(Some(json!(1))), method, uri).await,
                StatusCode::FORBIDDEN,
                "{method} {uri}"
            );
        }
    }

    #[tokio::test]
    async fn refuse_a_tenant_that_is_not_an_organisation() {
        for (method, uri) in ROUTES {
            assert_eq!(
                status(routes(Some(json!("acme"))), method, uri).await,
                StatusCode::FORBIDDEN,
                "{method} {uri}"
            );
        }
    }

    #[tokio::test]
    async fn admit_the_operator() {
        for (method, uri) in ROUTES {
            let status = status(routes(None), method, uri).await;
            assert!(
                !matches!(status, StatusCode::FORBIDDEN | StatusCode::UNAUTHORIZED),
                "{method} {uri} answered {status}"
            );
        }
    }
}
