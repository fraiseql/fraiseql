//! The in-handler limiter is the one the caller sized, and with none the routes never
//! refuse on their own.
//!
//! The limiter runs before anything else in each handler, so a lazy pool serves: the
//! password state's helper is called directly, and the verification routes refuse a
//! request with no session before any store is touched.
#![allow(clippy::unwrap_used, clippy::panic)] // Reason: test code, panics are acceptable

use axum::{body::Body, http::Request};
use tower::ServiceExt as _;

use super::*;
use crate::{AuthRateLimitConfig, InMemoryAccountStore, PostgresSessionStore};

const WINDOW_SECS: u64 = 600;

fn limiter_of_one() -> Option<Arc<KeyedRateLimiter>> {
    Some(Arc::new(KeyedRateLimiter::new(AuthRateLimitConfig {
        enabled:      true,
        max_requests: 1,
        window_secs:  WINDOW_SECS,
    })))
}

fn authenticator(pool: sqlx::PgPool) -> Arc<LocalPasswordAuthenticator> {
    Arc::new(LocalPasswordAuthenticator::new(pool, Arc::new(InMemoryAccountStore::new())))
}

fn lazy_pool() -> sqlx::PgPool {
    sqlx::PgPool::connect_lazy("postgres://unused/unused").expect("lazy pool needs no server")
}

fn password_state(rate_limiter: Option<Arc<KeyedRateLimiter>>) -> LocalPasswordRouteState {
    let pool = lazy_pool();
    LocalPasswordRouteState {
        authenticator: authenticator(pool.clone()),
        session_store: Arc::new(PostgresSessionStore::new(pool)),
        rate_limiter,
        session_cookies: None,
    }
}

fn peer() -> SocketAddr {
    SocketAddr::from(([198, 51, 100, 9], 4000))
}

#[tokio::test]
async fn a_password_state_given_a_limiter_of_one_refuses_the_second_request() {
    let state = password_state(limiter_of_one());
    assert!(rate_limit(&state, peer(), "password_login").is_none(), "the first request");
    let refusal = rate_limit(&state, peer(), "password_login").expect("the second is refused");
    assert_eq!(refusal.status(), StatusCode::TOO_MANY_REQUESTS);
    assert_eq!(refusal.headers()[axum::http::header::RETRY_AFTER], WINDOW_SECS.to_string());
}

#[tokio::test]
async fn a_password_state_given_no_limiter_never_refuses() {
    let state = password_state(None);
    for n in 1..=50 {
        assert!(rate_limit(&state, peer(), "password_login").is_none(), "request {n}");
    }
}

async fn verify_start_statuses(rate_limiter: Option<Arc<KeyedRateLimiter>>) -> Vec<StatusCode> {
    let router = email_verification_routes(Arc::new(EmailVerificationRouteState {
        authenticator: authenticator(lazy_pool()),
        rate_limiter,
    }));
    let mut statuses = Vec::new();
    for _ in 0..3 {
        let request = Request::builder()
            .method("POST")
            .uri("/auth/v1/email/verify/start")
            .header("content-type", "application/json")
            .extension(ConnectInfo(peer()))
            .body(Body::from("{}"))
            .unwrap();
        statuses.push(router.clone().oneshot(request).await.unwrap().status());
    }
    statuses
}

#[tokio::test]
async fn a_verification_state_given_a_limiter_of_one_refuses_the_second_request() {
    assert_eq!(
        verify_start_statuses(limiter_of_one()).await,
        [
            StatusCode::UNAUTHORIZED,
            StatusCode::TOO_MANY_REQUESTS,
            StatusCode::TOO_MANY_REQUESTS
        ]
    );
}

#[tokio::test]
async fn a_verification_state_given_no_limiter_never_refuses() {
    assert_eq!(verify_start_statuses(None).await, [StatusCode::UNAUTHORIZED; 3]);
}
