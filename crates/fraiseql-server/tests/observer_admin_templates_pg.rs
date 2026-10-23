//! Observer admin API: a webhook `body_template` is checked against the observed entity's
//! fields at create and update, driven through the real routers against PostgreSQL.
//!
//! A placeholder the changed row cannot fill dead-letters every event at delivery. Where
//! the compiled schema declares the observer's entity type, the admin API refuses such a
//! template with 422 instead. An update is checked as the observer it leaves: a new entity
//! type against the stored actions, new actions against the stored entity type.
//!
//! The admin bearer gate is not mounted here: these tests inject the operator principal it
//! would have admitted.
//!
//! # Running
//!
//! ```bash
//! cargo test -p fraiseql-server --features observers --test observer_admin_templates_pg -- --test-threads=1
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
use chrono::{Duration, Utc};
use fraiseql_core::{
    db::postgres::PostgresAdapter,
    runtime::Executor,
    schema::{CompiledSchema, FieldDefinition, FieldType, TypeDefinition},
    security::AuthenticatedUser,
};
use fraiseql_server::{
    middleware::{AuthUser, TenantClaim},
    observers::{ObserverRepository, ObserverState, observer_routes},
};
use observer_test_helpers::setup_observer_schema;
use serde_json::{Value, json};
use sqlx::PgPool;
use tower::ServiceExt;

/// `Order { id, total, customer: Customer }` and `Customer { email }`.
fn schema() -> CompiledSchema {
    let mut schema = CompiledSchema::new();
    let mut order = TypeDefinition::new("Order", "v_order");
    order.fields = vec![
        FieldDefinition::new("id", FieldType::Int),
        FieldDefinition::new("total", FieldType::Float),
        FieldDefinition::new("customer", FieldType::Object("Customer".to_string())),
    ];
    let mut customer = TypeDefinition::new("Customer", "v_customer");
    customer.fields = vec![FieldDefinition::new("email", FieldType::String)];
    schema.types = vec![order, customer];
    schema.build_indexes();
    schema
}

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
    sqlx::query("TRUNCATE tb_observer_log, tb_observer RESTART IDENTITY CASCADE")
        .execute(&pool)
        .await
        .unwrap();

    let adapter = Arc::new(PostgresAdapter::new(svc.url()).await.unwrap());
    let executor = Arc::new(ArcSwap::from_pointee(Executor::read_only(schema(), adapter)));
    let operator = AuthUser(AuthenticatedUser {
        user_id:      fraiseql_core::types::UserId::new("admin-user"),
        scopes:       vec!["fraiseql:admin".to_owned()],
        expires_at:   Utc::now() + Duration::hours(1),
        email:        None,
        display_name: None,
        extra_claims: HashMap::new(),
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

    async fn create(&self, entity_type: &str, template: &str) -> (StatusCode, Value) {
        self.call(
            Method::POST,
            "/api/observers",
            json!({
                "name": format!("templated-{entity_type}"),
                "entity_type": entity_type,
                "event_type": "INSERT",
                "actions": [webhook(template)],
            }),
        )
        .await
    }

    async fn stored_template(&self, id: &str) -> String {
        let (actions,): (Value,) =
            sqlx::query_as("SELECT actions FROM tb_observer WHERE id = $1::uuid")
                .bind(id)
                .fetch_one(&self.pool)
                .await
                .unwrap();
        actions[0]["body_template"].as_str().unwrap().to_owned()
    }

    async fn stored_entity_type(&self, id: &str) -> String {
        let (entity_type,): (String,) =
            sqlx::query_as("SELECT entity_type FROM tb_observer WHERE id = $1::uuid")
                .bind(id)
                .fetch_one(&self.pool)
                .await
                .unwrap();
        entity_type
    }

    async fn count(&self) -> i64 {
        let (n,): (i64,) = sqlx::query_as("SELECT count(*) FROM tb_observer")
            .fetch_one(&self.pool)
            .await
            .unwrap();
        n
    }
}

fn webhook(template: &str) -> Value {
    json!({ "type": "webhook", "url": "https://hooks.example/x", "body_template": template })
}

const VALID: &str = r#"{"id": "{{ id }}", "to": "{{ customer.email }}"}"#;
/// Fills on an `Order`, which declares `total`; not on a `Customer`, which does not.
const ORDER_ONLY: &str = r#"{"total": "{{ total }}"}"#;
const UNFILLABLE: &str = r#"{"x": "{{ nope }}"}"#;

fn assert_refused_naming(status: StatusCode, body: &Value, path: &str) {
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{body}");
    let error = body["error"].as_str().unwrap_or_default();
    assert!(error.contains(&format!("`{{{{ {path} }}}}`")), "names `{path}`: {body}");
}

#[tokio::test]
async fn create_refuses_a_placeholder_the_entity_cannot_fill() {
    let rig = rig().await;
    let (status, body) = rig.create("Order", UNFILLABLE).await;
    assert_refused_naming(status, &body, "nope");
    assert_eq!(rig.count().await, 0, "nothing is written");
}

#[tokio::test]
async fn create_accepts_a_template_whose_paths_exist() {
    let rig = rig().await;
    let (status, body) = rig.create("Order", VALID).await;
    assert_eq!(status, StatusCode::CREATED, "{body}");
}

#[tokio::test]
async fn update_refuses_new_actions_against_the_stored_entity_type() {
    let rig = rig().await;
    let (_, created) = rig.create("Order", VALID).await;
    let id = created["id"].as_str().unwrap();
    let (status, body) = rig
        .call(
            Method::PATCH,
            &format!("/api/observers/{id}"),
            json!({ "actions": [webhook(UNFILLABLE)] }),
        )
        .await;
    assert_refused_naming(status, &body, "nope");
    assert_eq!(rig.stored_template(id).await, VALID, "the stored template is unchanged");
}

#[tokio::test]
async fn update_refuses_a_new_entity_type_the_stored_template_cannot_fill() {
    let rig = rig().await;
    let (_, created) = rig.create("Order", ORDER_ONLY).await;
    let id = created["id"].as_str().unwrap();
    let (status, body) = rig
        .call(
            Method::PATCH,
            &format!("/api/observers/{id}"),
            json!({ "entity_type": "Customer" }),
        )
        .await;
    assert_refused_naming(status, &body, "total");
    assert_eq!(rig.stored_entity_type(id).await, "Order", "the stored entity type is unchanged");
}

#[tokio::test]
async fn update_refuses_new_actions_and_entity_type_checked_together() {
    let rig = rig().await;
    let (_, created) = rig.create("Order", VALID).await;
    let id = created["id"].as_str().unwrap();
    let (status, body) = rig
        .call(
            Method::PATCH,
            &format!("/api/observers/{id}"),
            json!({ "entity_type": "Customer", "actions": [webhook(ORDER_ONLY)] }),
        )
        .await;
    assert_refused_naming(status, &body, "total");
}

#[tokio::test]
async fn update_accepts_a_template_whose_paths_exist() {
    let rig = rig().await;
    let (_, created) = rig.create("Order", VALID).await;
    let id = created["id"].as_str().unwrap();
    let (status, body) = rig
        .call(
            Method::PATCH,
            &format!("/api/observers/{id}"),
            json!({ "actions": [webhook(ORDER_ONLY)] }),
        )
        .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(rig.stored_template(id).await, ORDER_ONLY);
}
