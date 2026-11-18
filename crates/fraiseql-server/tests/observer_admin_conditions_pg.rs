//! Observer admin API: a `condition_expression` is parsed at create and update, driven
//! through the real routers against PostgreSQL.
//!
//! The runtime parses a condition when an event reaches the observer, so one that does not
//! parse used to be stored and then fail on every event. The admin API now parses it with
//! the runtime's parser and refuses it with 400, writing nothing. The expected reason is
//! taken from that parser here, so the tests follow whatever it refuses.
//!
//! The admin bearer gate is not mounted here: these tests inject the operator principal it
//! would have admitted.
//!
//! # Running
//!
//! ```bash
//! cargo test -p fraiseql-server --features observers --test observer_admin_conditions_pg -- --test-threads=1
//! ```

#![cfg(feature = "observers")]
#![allow(clippy::unwrap_used)] // Reason: test code, panics are acceptable
#![allow(clippy::missing_panics_doc)] // Reason: test functions, panics are expected
#![allow(missing_docs)] // Reason: test code does not require documentation

mod observer_test_helpers;

use std::{collections::HashMap, sync::Arc};

use arc_swap::ArcSwap;
use axum::{
    Router,
    body::Body,
    http::{Method, Request, StatusCode},
};
use chrono::{DateTime, Duration, Utc};
use fraiseql_core::{
    db::postgres::PostgresAdapter, runtime::Executor, schema::CompiledSchema,
    security::AuthenticatedUser,
};
use fraiseql_observers::ConditionParser;
use fraiseql_server::{
    middleware::{AuthUser, TenantClaim},
    observers::{ObserverRepository, ObserverState, observer_routes},
};
use observer_test_helpers::setup_observer_schema;
use serde_json::{Value, json};
use sqlx::PgPool;
use tower::ServiceExt;

struct Rig {
    pool: PgPool,
    api:  Router,
    _svc: fraiseql_test_support::Service,
}

async fn rig() -> Rig {
    let svc = fraiseql_test_support::postgres()
        .await
        .expect("DATABASE_URL must be set (or enable fraiseql-test-support/local-testcontainers)");
    let pool = PgPool::connect(svc.url()).await.unwrap();
    setup_observer_schema(&pool).await.unwrap();
    sqlx::query("TRUNCATE core.tb_observer_log, core.tb_observer RESTART IDENTITY CASCADE")
        .execute(&pool)
        .await
        .unwrap();

    let adapter = Arc::new(PostgresAdapter::new(svc.url()).await.unwrap());
    let executor =
        Arc::new(ArcSwap::from_pointee(Executor::read_only(CompiledSchema::new(), adapter)));
    let operator = AuthUser(AuthenticatedUser {
        user_id:      fraiseql_core::types::UserId::new("admin-user"),
        scopes:       vec!["fraiseql:admin".to_owned()],
        expires_at:   Utc::now() + Duration::hours(1),
        email:        None,
        display_name: None,
        extra_claims: HashMap::new(),
        provenance:   fraiseql_core::security::Provenance::BearerToken,
    });
    let api = Router::new()
        .nest(
            "/api/observers",
            observer_routes(ObserverState {
                repository: ObserverRepository::new(pool.clone()),
                runtime: None,
                executor,
            }),
        )
        .layer(axum::Extension(operator))
        .layer(axum::Extension(TenantClaim(Arc::from("tenant_id"))));
    Rig {
        pool,
        api,
        _svc: svc,
    }
}

impl Rig {
    async fn call(&self, method: Method, uri: &str, body: Value) -> (StatusCode, Value) {
        let request = Request::builder()
            .method(method)
            .uri(uri)
            .header("content-type", "application/json")
            .body(Body::from(body.to_string()))
            .unwrap();
        let response = self.api.clone().oneshot(request).await.unwrap();
        let status = response.status();
        let bytes = axum::body::to_bytes(response.into_body(), 1 << 20).await.unwrap();
        (status, serde_json::from_slice(&bytes).unwrap_or(Value::Null))
    }

    /// `POST` an observer; `condition` is left out of the request when `None`.
    async fn create(&self, condition: Option<&str>) -> (StatusCode, Value) {
        let mut observer = json!({
            "name": "conditioned",
            "entity_type": "Order",
            "event_type": "INSERT",
            "actions": [{ "type": "webhook", "url": "https://hooks.example/x" }],
        });
        if let Some(condition) = condition {
            observer["condition_expression"] = json!(condition);
        }
        self.call(Method::POST, "/api/observers", observer).await
    }

    async fn create_with(&self, condition: &str) -> String {
        let (status, body) = self.create(Some(condition)).await;
        assert_eq!(status, StatusCode::CREATED, "{body}");
        body["id"].as_str().unwrap().to_owned()
    }

    async fn update(&self, id: &str, condition: &str) -> (StatusCode, Value) {
        self.call(
            Method::PATCH,
            &format!("/api/observers/{id}"),
            json!({ "condition_expression": condition }),
        )
        .await
    }

    /// The stored condition and when the row was last written.
    async fn stored(&self, id: &str) -> (Option<String>, DateTime<Utc>) {
        sqlx::query_as(
            "SELECT condition_expression, updated_at FROM core.tb_observer WHERE id = $1::uuid",
        )
        .bind(id)
        .fetch_one(&self.pool)
        .await
        .unwrap()
    }

    async fn count(&self) -> i64 {
        let (n,): (i64,) = sqlx::query_as("SELECT count(*) FROM core.tb_observer")
            .fetch_one(&self.pool)
            .await
            .unwrap();
        n
    }
}

const VALID: &str = "status == 'pending' || total > 100";
const OTHER_VALID: &str = "has_field('status')";
const BROKEN: &str = "((((";
const EMPTY: [&str; 2] = ["", "   "];

/// A 400 whose error names the field and carries the runtime parser's own reason.
fn assert_refused(condition: &str, status: StatusCode, body: &Value) {
    let reason = ConditionParser::new()
        .parse(condition)
        .expect_err("precondition: the runtime's parser refuses this condition");
    assert_eq!(status, StatusCode::BAD_REQUEST, "{condition:?}: {body}");
    assert_eq!(
        body["error"].as_str(),
        Some(format!("condition_expression does not parse: {reason}").as_str()),
        "{condition:?}"
    );
}

#[tokio::test]
async fn create_refuses_a_condition_that_does_not_parse() {
    let rig = rig().await;
    let (status, body) = rig.create(Some(BROKEN)).await;
    assert_refused(BROKEN, status, &body);
    assert_eq!(rig.count().await, 0, "nothing is written");
}

#[tokio::test]
async fn create_refuses_an_empty_condition() {
    let rig = rig().await;
    for condition in EMPTY {
        let (status, body) = rig.create(Some(condition)).await;
        assert_refused(condition, status, &body);
    }
    assert_eq!(rig.count().await, 0, "nothing is written");
}

#[tokio::test]
async fn create_stores_a_condition_that_parses() {
    let rig = rig().await;
    let id = rig.create_with(VALID).await;
    assert_eq!(rig.stored(&id).await.0.as_deref(), Some(VALID));
}

#[tokio::test]
async fn create_without_a_condition_stores_none() {
    let rig = rig().await;
    let (status, body) = rig.create(None).await;
    assert_eq!(status, StatusCode::CREATED, "{body}");
    let id = body["id"].as_str().unwrap();
    assert_eq!(rig.stored(id).await.0, None);
}

#[tokio::test]
async fn update_refuses_a_condition_that_does_not_parse() {
    let rig = rig().await;
    let id = rig.create_with(VALID).await;
    let before = rig.stored(&id).await;

    let (status, body) = rig.update(&id, BROKEN).await;
    assert_refused(BROKEN, status, &body);
    assert_eq!(rig.stored(&id).await, before, "the stored observer is unchanged");
}

#[tokio::test]
async fn update_refuses_an_empty_condition() {
    let rig = rig().await;
    let id = rig.create_with(VALID).await;
    let before = rig.stored(&id).await;

    for condition in EMPTY {
        let (status, body) = rig.update(&id, condition).await;
        assert_refused(condition, status, &body);
    }
    assert_eq!(rig.stored(&id).await, before, "the stored observer is unchanged");
}

#[tokio::test]
async fn update_stores_a_condition_that_parses() {
    let rig = rig().await;
    let id = rig.create_with(VALID).await;
    let (status, body) = rig.update(&id, OTHER_VALID).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(rig.stored(&id).await.0.as_deref(), Some(OTHER_VALID));
}

/// The condition is parsed before the stored observer is looked up.
#[tokio::test]
async fn update_refuses_the_condition_whatever_the_id_names() {
    let rig = rig().await;
    let (status, body) = rig.update("00000000-0000-0000-0000-000000000000", BROKEN).await;
    assert_refused(BROKEN, status, &body);
}
