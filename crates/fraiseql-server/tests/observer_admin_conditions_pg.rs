//! Observer admin API: a `condition_expression` is parsed at create and update, driven
//! through the real routers against PostgreSQL.
//!
//! The runtime parses a condition when an event reaches the observer, so one that does not
//! parse used to be stored and then fail on every event. The admin API now parses it with
//! the runtime's parser and refuses it with 400, writing nothing. The expected reason is
//! taken from that parser here, so the tests follow whatever it refuses.
//!
//! An update clears a condition with `"condition_expression": null`; leaving the field out
//! leaves the condition as it is. The same holds for `description`. `retry_config` and
//! `timeout_ms` cannot be empty, so `null` resets each to what a create without it stores.
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
use chrono::{DateTime, Utc};
use fraiseql_core::{
    db::postgres::PostgresAdapter,
    runtime::Executor,
    schema::{CompiledSchema, ObserverDefinition},
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
        expires_at:   fraiseql_core::fraiseql_time::Timestamp::now()
            + fraiseql_core::fraiseql_time::jiff::SignedDuration::from_hours(1),
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
        self.patch(id, json!({ "condition_expression": condition })).await
    }

    async fn patch(&self, id: &str, body: Value) -> (StatusCode, Value) {
        self.call(Method::PATCH, &format!("/api/observers/{id}"), body).await
    }

    /// Store an observer the compiled schema declares, with `condition`, as the server does
    /// at boot, and return its id.
    async fn declared_with(&self, condition: &str) -> String {
        let observer = ObserverDefinition::new("order_created", "Order", "INSERT")
            .with_condition(condition)
            .with_action(json!({ "type": "webhook", "url": "https://hooks.example/x" }));
        ObserverRepository::new(self.pool.clone())
            .reconcile_declared("schema", &[observer])
            .await
            .unwrap();
        sqlx::query_scalar("SELECT id::text FROM core.tb_observer WHERE name = 'order_created'")
            .fetch_one(&self.pool)
            .await
            .unwrap()
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

    /// `POST` an observer named `name` with `settings` added to it, and return its id.
    async fn create_set(&self, name: &str, settings: Value) -> String {
        let mut observer = json!({
            "name": name,
            "actions": [{ "type": "webhook", "url": "https://hooks.example/x" }],
        });
        for (key, value) in settings.as_object().unwrap() {
            observer[key] = value.clone();
        }
        let (status, body) = self.call(Method::POST, "/api/observers", observer).await;
        assert_eq!(status, StatusCode::CREATED, "{body}");
        body["id"].as_str().unwrap().to_owned()
    }

    /// The stored description, retry config and timeout, and when the row was last written.
    async fn settings(&self, id: &str) -> (Option<String>, Value, i32, DateTime<Utc>) {
        sqlx::query_as(
            "SELECT description, retry_config, timeout_ms, updated_at FROM core.tb_observer \
             WHERE id = $1::uuid",
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

#[tokio::test]
async fn update_with_a_null_condition_clears_it() {
    let rig = rig().await;
    let id = rig.create_with(VALID).await;

    let (status, body) = rig.patch(&id, json!({ "condition_expression": null })).await;

    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["condition_expression"], Value::Null, "{body}");
    assert_eq!(rig.stored(&id).await.0, None, "the stored condition is cleared");
}

#[tokio::test]
async fn update_without_the_condition_leaves_it() {
    let rig = rig().await;
    let id = rig.create_with(VALID).await;

    let (status, body) = rig.patch(&id, json!({ "priority": 7 })).await;

    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["priority"], json!(7), "the update was written: {body}");
    assert_eq!(rig.stored(&id).await.0.as_deref(), Some(VALID));
}

/// Clearing a condition changes the observer, so it is refused on one the schema declares,
/// as any change but `enabled` is, and the declared condition stays.
#[tokio::test]
async fn update_with_a_null_condition_is_refused_on_a_declared_observer() {
    let rig = rig().await;
    let id = rig.declared_with(VALID).await;
    let before = rig.stored(&id).await;

    for body in [
        json!({ "condition_expression": null }),
        json!({ "condition_expression": null, "enabled": false }),
    ] {
        let (status, answer) = rig.patch(&id, body.clone()).await;
        assert_eq!(status, StatusCode::CONFLICT, "{body}: {answer}");
    }
    assert_eq!(rig.stored(&id).await, before, "the declared observer is unchanged");
}

/// Values none of which a create without them stores.
fn configured() -> Value {
    json!({
        "description": "pages the on-call",
        "retry_config": {
            "max_attempts": 9,
            "backoff_strategy": "linear",
            "initial_delay_ms": 5,
            "max_delay_ms": 50,
        },
        "timeout_ms": 1234,
    })
}

#[tokio::test]
async fn update_with_a_null_description_clears_it() {
    let rig = rig().await;
    let id = rig.create_set("configured", configured()).await;

    let (status, body) = rig.patch(&id, json!({ "description": null })).await;

    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["description"], Value::Null, "{body}");
    assert_eq!(rig.settings(&id).await.0, None, "the stored description is cleared");
}

/// The stored `field`: `retry_config` or `timeout_ms`.
async fn stored_setting(rig: &Rig, id: &str, field: &str) -> Value {
    let (_, retry, timeout, _) = rig.settings(id).await;
    if field == "timeout_ms" {
        json!(timeout)
    } else {
        retry
    }
}

/// A `null` `field` stores what a create without the field stores.
async fn assert_null_resets(field: &str) {
    let rig = rig().await;
    let unset = rig.create_set("unset", json!({})).await;
    let expected = stored_setting(&rig, &unset, field).await;
    let id = rig.create_set("configured", configured()).await;
    assert_ne!(
        stored_setting(&rig, &id, field).await,
        expected,
        "precondition: {field} is not the default"
    );

    let (status, body) = rig.patch(&id, json!({ field: null })).await;

    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body[field], expected, "{body}");
    assert_eq!(stored_setting(&rig, &id, field).await, expected, "the stored {field} is reset");
}

#[tokio::test]
async fn update_with_a_null_retry_config_resets_it() {
    assert_null_resets("retry_config").await;
}

#[tokio::test]
async fn update_with_a_null_timeout_resets_it() {
    assert_null_resets("timeout_ms").await;
}

#[tokio::test]
async fn update_without_a_setting_leaves_it() {
    let rig = rig().await;
    let id = rig.create_set("configured", configured()).await;
    let before = rig.settings(&id).await;

    let (status, body) = rig.patch(&id, json!({ "priority": 7 })).await;

    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["priority"], json!(7), "the update was written: {body}");
    let after = rig.settings(&id).await;
    assert_eq!((after.0, after.1, after.2), (before.0, before.1, before.2));
}

/// Clearing or resetting `field` changes the observer, so it is refused on one the schema
/// declares, alone and beside `enabled`, and the declared observer stays as it is.
async fn assert_null_refused_on_declared(field: &str) {
    let rig = rig().await;
    let id = rig.declared_with(VALID).await;
    let before = rig.settings(&id).await;

    for body in [
        json!({ field: null }),
        json!({ field: null, "enabled": false }),
    ] {
        let (status, answer) = rig.patch(&id, body.clone()).await;
        assert_eq!(status, StatusCode::CONFLICT, "{body}: {answer}");
    }
    assert_eq!(rig.settings(&id).await, before, "the declared observer is unchanged");
}

#[tokio::test]
async fn update_with_a_null_description_is_refused_on_a_declared_observer() {
    assert_null_refused_on_declared("description").await;
}

#[tokio::test]
async fn update_with_a_null_retry_config_is_refused_on_a_declared_observer() {
    assert_null_refused_on_declared("retry_config").await;
}

#[tokio::test]
async fn update_with_a_null_timeout_is_refused_on_a_declared_observer() {
    assert_null_refused_on_declared("timeout_ms").await;
}
