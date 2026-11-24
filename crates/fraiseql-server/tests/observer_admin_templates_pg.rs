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

/// `Order { id, total, reviewStatus, customer: Customer }` and `Customer { email }`.
fn schema() -> CompiledSchema {
    let mut schema = CompiledSchema::new();
    let mut order = TypeDefinition::new("Order", "v_order");
    order.fields = vec![
        FieldDefinition::new("id", FieldType::Int),
        FieldDefinition::new("total", FieldType::Float),
        FieldDefinition::new("reviewStatus", FieldType::String),
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
    rig_over(schema()).await
}

/// `schema()` with `Order` captured from `tb_order`, a table of plain columns: a captured
/// change is stored as the table's row, which holds no embedded `customer`.
fn row_captured() -> CompiledSchema {
    let mut schema = schema();
    schema.subscribable.push(fraiseql_core::schema::SubscribableEntity {
        entity_type: "Order".to_string(),
        tables:      vec!["tb_order".to_string()],
        pre_image:   false,
    });
    schema
}

async fn rig_over(schema: CompiledSchema) -> Rig {
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
    let executor = Arc::new(ArcSwap::from_pointee(Executor::read_only(schema, adapter)));
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
            sqlx::query_as("SELECT actions FROM core.tb_observer WHERE id = $1::uuid")
                .bind(id)
                .fetch_one(&self.pool)
                .await
                .unwrap();
        actions[0]["body_template"].as_str().unwrap().to_owned()
    }

    async fn stored_entity_type(&self, id: &str) -> String {
        let (entity_type,): (String,) =
            sqlx::query_as("SELECT entity_type FROM core.tb_observer WHERE id = $1::uuid")
                .bind(id)
                .fetch_one(&self.pool)
                .await
                .unwrap();
        entity_type
    }

    async fn count(&self) -> i64 {
        let (n,): (i64,) = sqlx::query_as("SELECT count(*) FROM core.tb_observer")
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

/// A row holds `reviewStatus` under `review_status`, and that is the name a template reads.
#[tokio::test]
async fn create_checks_a_multi_word_field_by_the_key_the_row_holds() {
    let rig = rig().await;
    let (status, body) = rig.create("Order", r#"{"s": "{{ review_status }}"}"#).await;
    assert_eq!(status, StatusCode::CREATED, "{body}");

    let (status, body) = rig.create("Order", r#"{"s": "{{ reviewStatus }}"}"#).await;
    assert_refused_naming(status, &body, "reviewStatus");
    assert_eq!(rig.count().await, 1, "only the first is written");
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

/// What the server logged while a test ran, as text.
#[derive(Clone, Default)]
struct Logged(Arc<std::sync::Mutex<Vec<u8>>>);

impl std::io::Write for Logged {
    fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
        self.0.lock().unwrap().extend_from_slice(bytes);
        Ok(bytes.len())
    }

    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

impl<'a> tracing_subscriber::fmt::MakeWriter<'a> for Logged {
    type Writer = Self;

    fn make_writer(&'a self) -> Self {
        self.clone()
    }
}

impl Logged {
    /// Capture this thread's log from here until the guard is dropped.
    fn start() -> (Self, tracing::subscriber::DefaultGuard) {
        let logged = Self::default();
        let subscriber = tracing_subscriber::fmt()
            .with_writer(logged.clone())
            .with_ansi(false)
            .with_max_level(tracing::Level::WARN)
            .finish();
        (logged, fraiseql_test_support::tracing_capture::set_default(subscriber))
    }

    fn text(&self) -> String {
        String::from_utf8_lossy(&self.0.lock().unwrap()).into_owned()
    }

    /// How many warnings of a template a captured row cannot fill were logged for `path`.
    fn warnings_of(&self, path: &str) -> usize {
        let placeholder = format!("`{{{{ {path} }}}}`");
        self.text()
            .lines()
            .filter(|line| line.contains("WARN") && line.contains(&placeholder))
            .filter(|line| line.contains("`tb_order`"))
            .count()
    }
}

/// #2031: a table of plain columns is captured as its row. A template reading the embedded
/// customer is stored, since a mutation's event fills it, and the server logs what a
/// captured event will not. The response is the observer, as for any create.
#[tokio::test]
async fn create_stores_and_logs_a_template_a_captured_row_cannot_fill() {
    let rig = rig_over(row_captured()).await;
    let (logged, _guard) = Logged::start();

    let (status, body) = rig.create("Order", VALID).await;

    assert_eq!(status, StatusCode::CREATED, "{body}");
    assert_eq!(logged.warnings_of("customer.email"), 1, "{}", logged.text());
    assert_eq!(logged.warnings_of("id"), 0, "a column carries a scalar: {}", logged.text());
}

#[tokio::test]
async fn create_logs_nothing_where_no_table_is_captured_as_its_row() {
    let rig = rig().await;
    let (logged, _guard) = Logged::start();

    let (status, body) = rig.create("Order", VALID).await;

    assert_eq!(status, StatusCode::CREATED, "{body}");
    assert_eq!(logged.warnings_of("customer.email"), 0, "{}", logged.text());
}

/// A column the captured row holds and the document does not is refused as before.
#[tokio::test]
async fn create_still_refuses_a_column_only_the_captured_row_holds() {
    let rig = rig_over(row_captured()).await;
    let (status, body) = rig.create("Order", r#"{"c": "{{ fk_customer }}"}"#).await;
    assert_refused_naming(status, &body, "fk_customer");
}

/// An update is read as the observer it leaves, as it is checked.
#[tokio::test]
async fn update_logs_a_template_a_captured_row_cannot_fill() {
    let rig = rig_over(row_captured()).await;
    let (_, created) = rig.create("Order", ORDER_ONLY).await;
    let id = created["id"].as_str().unwrap();
    let (logged, _guard) = Logged::start();

    let (status, body) = rig
        .call(
            Method::PATCH,
            &format!("/api/observers/{id}"),
            json!({ "actions": [webhook(VALID)] }),
        )
        .await;

    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(logged.warnings_of("customer.email"), 1, "{}", logged.text());
}

/// A refused update logs no such warning: nothing was stored.
#[tokio::test]
async fn a_refused_update_logs_no_captured_row_warning() {
    let rig = rig_over(row_captured()).await;
    let (_, created) = rig.create("Order", ORDER_ONLY).await;
    let id = created["id"].as_str().unwrap();
    let (logged, _guard) = Logged::start();

    let template = r#"{"to": "{{ customer.email }}", "x": "{{ nope }}"}"#;
    let (status, body) = rig
        .call(
            Method::PATCH,
            &format!("/api/observers/{id}"),
            json!({ "actions": [webhook(template)] }),
        )
        .await;

    assert_refused_naming(status, &body, "nope");
    assert_eq!(logged.warnings_of("customer.email"), 0, "{}", logged.text());
}

// #2122: a condition reads top-level keys of the event. Each key it names is checked
// against the observed type, as a template's placeholders are.

/// A template with no placeholder, so only the condition is checked.
const PLAIN: &str = r#"{"ok": true}"#;

impl Rig {
    async fn create_when(&self, entity_type: &str, condition: &str) -> (StatusCode, Value) {
        self.call(
            Method::POST,
            "/api/observers",
            json!({
                "name": format!("conditioned-{entity_type}"),
                "entity_type": entity_type,
                "event_type": "INSERT",
                "condition_expression": condition,
                "actions": [webhook(PLAIN)],
            }),
        )
        .await
    }

    async fn stored_condition(&self, id: &str) -> Option<String> {
        let (condition,): (Option<String>,) =
            sqlx::query_as("SELECT condition_expression FROM core.tb_observer WHERE id = $1::uuid")
                .bind(id)
                .fetch_one(&self.pool)
                .await
                .unwrap();
        condition
    }
}

impl Logged {
    /// How many warnings of a condition key a captured row bears on were logged for `key`.
    fn condition_warnings_of(&self, key: &str) -> usize {
        let says = format!("`condition` reads `{key}`");
        self.text()
            .lines()
            .filter(|line| line.contains("WARN") && line.contains(&says))
            .filter(|line| line.contains("`tb_order`"))
            .count()
    }
}

fn assert_condition_refused_naming(status: StatusCode, body: &Value, key: &str) {
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{body}");
    let error = body["error"].as_str().unwrap_or_default();
    assert!(error.contains(&format!("`condition` reads `{key}`")), "names `{key}`: {body}");
}

#[tokio::test]
async fn create_refuses_a_condition_key_no_field_holds() {
    let rig = rig().await;
    let (status, body) = rig.create_when("Order", "totl > 100").await;
    assert_condition_refused_naming(status, &body, "totl");
    assert_eq!(rig.count().await, 0, "nothing is stored");
}

#[tokio::test]
async fn create_refuses_a_published_name_in_a_condition_naming_the_stored_key() {
    let rig = rig().await;
    let (status, body) = rig.create_when("Order", "reviewStatus == 'approved'").await;
    assert_condition_refused_naming(status, &body, "reviewStatus");
    assert!(body["error"].as_str().unwrap().contains("`review_status`"), "{body}");
}

/// The schema holds no table's columns. Where the type's table is captured as its row, a
/// key no field holds may be a column: stored, and logged. Where the document is the
/// only image, refused.
#[tokio::test]
async fn the_same_unknown_condition_key_is_refused_or_stored_and_logged_by_the_types_tables() {
    let rig = rig().await;
    let (status, body) = rig.create_when("Order", "fk_customer == 7").await;
    assert_condition_refused_naming(status, &body, "fk_customer");

    let rig = rig_over(row_captured()).await;
    let (logged, _guard) = Logged::start();
    let (status, body) = rig.create_when("Order", "fk_customer == 7").await;
    assert_eq!(status, StatusCode::CREATED, "{body}");
    assert_eq!(logged.condition_warnings_of("fk_customer"), 1, "{}", logged.text());
}

#[tokio::test]
async fn create_refuses_a_dotted_key_in_a_condition_call_whatever_the_tables() {
    for rig in [rig().await, rig_over(row_captured()).await] {
        let (status, body) = rig.create_when("Order", "has_field('customer.email')").await;
        assert_condition_refused_naming(status, &body, "customer.email");
    }
}

#[tokio::test]
async fn create_stores_and_logs_a_condition_on_an_embedded_field_a_captured_row_lacks() {
    let rig = rig_over(row_captured()).await;
    let (logged, _guard) = Logged::start();

    let (status, body) = rig.create_when("Order", "has_field('customer') && total > 1").await;

    assert_eq!(status, StatusCode::CREATED, "{body}");
    assert_eq!(logged.condition_warnings_of("customer"), 1, "{}", logged.text());
    assert_eq!(logged.condition_warnings_of("total"), 0, "a column carries a scalar");
}

#[tokio::test]
async fn update_refuses_a_new_condition_against_the_stored_entity_type() {
    let rig = rig().await;
    let (_, created) = rig.create_when("Order", "total > 1").await;
    let id = created["id"].as_str().unwrap();

    let (status, body) = rig
        .call(
            Method::PATCH,
            &format!("/api/observers/{id}"),
            json!({ "condition_expression": "totl > 1" }),
        )
        .await;

    assert_condition_refused_naming(status, &body, "totl");
    assert_eq!(rig.stored_condition(id).await.as_deref(), Some("total > 1"));
}

#[tokio::test]
async fn update_refuses_a_new_entity_type_the_stored_condition_cannot_read() {
    let rig = rig().await;
    let (_, created) = rig.create_when("Order", "total > 1").await;
    let id = created["id"].as_str().unwrap();

    let (status, body) = rig
        .call(
            Method::PATCH,
            &format!("/api/observers/{id}"),
            json!({ "entity_type": "Customer" }),
        )
        .await;

    assert_condition_refused_naming(status, &body, "total");
    assert_eq!(rig.stored_entity_type(id).await, "Order");
}

/// An update that sends only actions leaves the stored condition as it was written, and
/// does not answer for it: a stored condition was checked on its own write.
#[tokio::test]
async fn an_update_of_the_actions_alone_does_not_recheck_the_stored_condition() {
    let rig = rig().await;
    let (_, created) = rig.create_when("Order", "total > 1").await;
    let id = created["id"].as_str().unwrap();
    sqlx::query(
        "UPDATE core.tb_observer SET condition_expression = 'totl > 1' WHERE id = $1::uuid",
    )
    .bind(id)
    .execute(&rig.pool)
    .await
    .unwrap();

    let (status, body) = rig
        .call(
            Method::PATCH,
            &format!("/api/observers/{id}"),
            json!({ "actions": [webhook(VALID)] }),
        )
        .await;

    assert_eq!(status, StatusCode::OK, "{body}");
}
