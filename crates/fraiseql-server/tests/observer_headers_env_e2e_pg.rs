//! A webhook header value can come from the environment (`headers_env`, header name to
//! environment variable name), resolved when the webhook is delivered.
//!
//! Driven end to end: a schema is compiled by the real compiler, its observers are stored
//! in `core.tb_observer` as the server stores them at boot (or created through the observer
//! admin API), and a real `ObserverRuntime` delivers to a wiremock receiver. The value
//! reaches the receiver and appears in neither the compiled schema nor `core.tb_observer`.
//! A variable that is not set dead-letters the event, and nothing is sent.
//!
//! # Running
//!
//! The observer leg's private-webhook allowance (`FRAISEQL_ALLOW_PRIVATE_WEBHOOKS`)
//! is required, as the receiver is on loopback:
//!
//! ```bash
//! FRAISEQL_ENV=development FRAISEQL_ALLOW_PRIVATE_WEBHOOKS=true \
//! FRAISEQL_OBSERVERS_ALLOW_INSECURE=true \
//! cargo test -p fraiseql-server --features observers --test observer_headers_env_e2e_pg -- --test-threads=1
//! ```

#![allow(clippy::unwrap_used)] // Reason: test code, panics are acceptable
#![allow(clippy::missing_panics_doc)] // Reason: test functions, panics are expected
#![allow(missing_docs)] // Reason: test code does not require documentation
#![allow(clippy::print_stdout, clippy::print_stderr)] // Reason: the shared helpers print progress

mod observer_test_helpers;

use std::{collections::HashMap, sync::Arc, time::Duration};

use arc_swap::ArcSwap;
use axum::{
    Router,
    body::Body,
    http::{Method, Request, StatusCode},
};
use fraiseql_core::{db::postgres::PostgresAdapter, runtime::Executor, schema::CompiledSchema};
use fraiseql_server::{
    middleware::{AuthUser, TenantClaim},
    observers::{
        ObserverRepository, ObserverState, observer_routes,
        runtime::{ObserverRuntime, ObserverRuntimeConfig},
    },
};
use observer_test_helpers::{
    MockWebhookServer, create_test_pool, get_observer_log_count_for_entity,
    insert_change_log_entry, setup_observer_schema, wait_for_observer_log_status, wait_for_webhook,
};
use serde_json::{Value, json};
use sqlx::PgPool;
use tower::ServiceExt;
use uuid::Uuid;

/// The value only the environment holds. Each test asserts it is stored nowhere.
const TOKEN: &str = "Bearer env-only-9d1c4f7a2b";

async fn start_runtime(pool: &PgPool) -> ObserverRuntime {
    let config = ObserverRuntimeConfig::new(pool.clone()).with_poll_interval(50);
    let mut runtime = ObserverRuntime::new(config);
    runtime.start().await.expect("the observer runtime starts");
    runtime
}

/// Compile an authoring schema that declares one observer on `entity`, with `action`,
/// through the real compiler.
fn compile_declaring(entity: &str, action: &Value) -> CompiledSchema {
    let schema = json!({
        "types": [{"name": entity, "sql_source": "v_order", "fields": [
            {"name": "id", "type": "ID", "nullable": false},
            {"name": "total", "type": "Int", "nullable": false}
        ]}],
        "queries": [{"name": "orders", "return_type": entity, "returns_list": true,
                     "sql_source": "v_order"}],
        "observers_config": {"enabled": true},
        "observers": [{
            "name": format!("hook-{entity}"),
            "entity": entity,
            "event": "INSERT",
            "actions": [action],
            "retry": {"max_attempts": 1, "backoff_strategy": "fixed",
                      "initial_delay_ms": 10, "max_delay_ms": 10}
        }]
    });
    let intermediate: fraiseql_cli::schema::intermediate::IntermediateSchema =
        serde_json::from_value(schema).unwrap();
    fraiseql_cli::schema::SchemaConverter::convert(intermediate)
        .expect("a webhook taking a header value from the environment compiles")
}

/// The `actions` JSONB of every stored observer named `name`, as text.
async fn stored_actions(pool: &PgPool, name: &str) -> String {
    sqlx::query_scalar::<_, String>(
        "SELECT actions::text FROM core.tb_observer WHERE name = $1 AND deleted_at IS NULL",
    )
    .bind(name)
    .fetch_one(pool)
    .await
    .expect("the observer is stored")
}

/// Insert one change for `entity`, and return its entity id.
async fn change(pool: &PgPool, entity: &str) -> String {
    let id = Uuid::new_v4().to_string();
    insert_change_log_entry(pool, "INSERT", entity, &id, json!({"id": id, "total": 42}), None)
        .await
        .expect("the change is logged");
    id
}

/// The `Authorization` header of every request the receiver got.
async fn authorizations(receiver: &MockWebhookServer) -> Vec<Option<String>> {
    receiver
        .server
        .received_requests()
        .await
        .unwrap_or_default()
        .iter()
        .map(|request| {
            request
                .headers
                .get("authorization")
                .and_then(|value| value.to_str().ok())
                .map(str::to_owned)
        })
        .collect()
}

/// A unique environment variable name for one test.
fn variable(test_id: &str) -> String {
    format!("FRAISEQL_TEST_HOOK_TOKEN_{}", test_id.to_ascii_uppercase())
}

#[tokio::test]
async fn a_declared_webhook_sends_a_header_value_only_the_environment_holds() {
    let test_id = Uuid::new_v4().simple().to_string();
    let pool = create_test_pool().await;
    setup_observer_schema(&pool).await.unwrap();
    let receiver = MockWebhookServer::start().await;
    receiver.mock_success().await;
    let entity = format!("Order{test_id}");
    let var = variable(&test_id);

    let compiled = compile_declaring(
        &entity,
        &json!({"type": "webhook", "url": receiver.webhook_url(),
                "headers_env": {"Authorization": var}}),
    );
    let compiled_json = serde_json::to_string(&compiled).unwrap();
    ObserverRepository::new(pool.clone())
        .reconcile_declared(&format!("headers-env-{test_id}"), &compiled.observers)
        .await
        .expect("the declared observer is stored");

    temp_env::async_with_vars([(var.as_str(), Some(TOKEN))], async {
        let mut runtime = start_runtime(&pool).await;
        change(&pool, &entity).await;
        wait_for_webhook(&receiver, 1, Duration::from_secs(10)).await;
        runtime.stop().await.expect("the runtime stops");
    })
    .await;

    assert_eq!(authorizations(&receiver).await, [Some(TOKEN.to_owned())]);
    assert!(compiled_json.contains(&var), "the compiled schema names the variable");
    assert!(!compiled_json.contains(TOKEN), "the compiled schema holds no value");
    let stored = stored_actions(&pool, &format!("hook-{entity}")).await;
    assert!(stored.contains(&var), "core.tb_observer names the variable: {stored}");
    assert!(!stored.contains(TOKEN), "core.tb_observer holds no value: {stored}");
}

#[tokio::test]
async fn a_variable_that_is_not_set_dead_letters_the_event_and_sends_nothing() {
    let test_id = Uuid::new_v4().simple().to_string();
    let pool = create_test_pool().await;
    setup_observer_schema(&pool).await.unwrap();
    let receiver = MockWebhookServer::start().await;
    receiver.mock_success().await;
    let entity = format!("Order{test_id}");
    let var = variable(&test_id);

    let compiled = compile_declaring(
        &entity,
        &json!({"type": "webhook", "url": receiver.webhook_url(),
                "headers_env": {"Authorization": var}}),
    );
    ObserverRepository::new(pool.clone())
        .reconcile_declared(&format!("headers-env-{test_id}"), &compiled.observers)
        .await
        .unwrap();

    let entity_id = temp_env::async_with_vars([(var.as_str(), None::<&str>)], async {
        let mut runtime = start_runtime(&pool).await;
        let entity_id = change(&pool, &entity).await;
        wait_for_observer_log_status(&pool, &entity_id, "failed", Duration::from_secs(10)).await;
        runtime.stop().await.expect("the runtime stops");
        entity_id
    })
    .await;

    assert_eq!(receiver.server.received_requests().await.unwrap_or_default().len(), 0);
    assert!(get_observer_log_count_for_entity(&pool, &entity_id, "failed").await.unwrap() >= 1);
    assert_eq!(
        get_observer_log_count_for_entity(&pool, &entity_id, "success").await.unwrap(),
        0
    );
}

/// The observer admin API, mounted over the real routers with the operator principal the
/// admin bearer gate would have admitted.
fn admin_api(pool: &PgPool, adapter: Arc<PostgresAdapter>) -> Router {
    let executor =
        Arc::new(ArcSwap::from_pointee(Executor::read_only(CompiledSchema::new(), adapter)));
    let operator = AuthUser(fraiseql_core::security::AuthenticatedUser {
        user_id:      fraiseql_core::types::UserId::new("admin-user"),
        scopes:       vec!["fraiseql:admin".to_owned()],
        expires_at:   chrono::Utc::now() + chrono::Duration::hours(1),
        email:        None,
        display_name: None,
        extra_claims: HashMap::new(),
        provenance:   fraiseql_core::security::Provenance::BearerToken,
    });
    Router::new()
        .nest(
            "/api/observers",
            observer_routes(ObserverState {
                repository: ObserverRepository::new(pool.clone()),
                runtime: None,
                executor,
            }),
        )
        .layer(axum::Extension(operator))
        .layer(axum::Extension(TenantClaim(Arc::from("tenant_id"))))
}

async fn post(api: &Router, body: &Value) -> (StatusCode, Value) {
    let request = Request::builder()
        .method(Method::POST)
        .uri("/api/observers")
        .header("content-type", "application/json")
        .body(Body::from(body.to_string()))
        .unwrap();
    let response = api.clone().oneshot(request).await.unwrap();
    let status = response.status();
    let bytes = axum::body::to_bytes(response.into_body(), 1 << 20).await.unwrap();
    // A refusal from the request extractor is plain text, not JSON.
    let body = serde_json::from_slice(&bytes)
        .unwrap_or_else(|_| Value::String(String::from_utf8_lossy(&bytes).into_owned()));
    (status, body)
}

#[tokio::test]
async fn an_observer_created_through_the_admin_api_keeps_headers_env_and_delivers_it() {
    let test_id = Uuid::new_v4().simple().to_string();
    let pool = create_test_pool().await;
    setup_observer_schema(&pool).await.unwrap();
    let adapter =
        Arc::new(PostgresAdapter::new(&fraiseql_test_support::database_url()).await.unwrap());
    let api = admin_api(&pool, adapter);
    let receiver = MockWebhookServer::start().await;
    receiver.mock_success().await;
    let entity = format!("Order{test_id}");
    let name = format!("admin-hook-{test_id}");
    let var = variable(&test_id);

    let (status, created) = post(
        &api,
        &json!({
            "name": name,
            "entity_type": entity,
            "event_type": "INSERT",
            "actions": [{"type": "webhook", "url": receiver.webhook_url(),
                         "headers_env": {"Authorization": var}}],
        }),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "{created}");

    temp_env::async_with_vars([(var.as_str(), Some(TOKEN))], async {
        let mut runtime = start_runtime(&pool).await;
        change(&pool, &entity).await;
        wait_for_webhook(&receiver, 1, Duration::from_secs(10)).await;
        runtime.stop().await.expect("the runtime stops");
    })
    .await;

    assert_eq!(authorizations(&receiver).await, [Some(TOKEN.to_owned())]);
    let stored = stored_actions(&pool, &name).await;
    assert!(stored.contains(&var), "the admin API stored the variable's name: {stored}");
    assert!(!stored.contains(TOKEN), "core.tb_observer holds no value: {stored}");
    assert_eq!(
        created["actions"][0]["headers_env"]["Authorization"], var,
        "the response shows the variable's name, which is not a secret"
    );
}

#[tokio::test]
async fn the_admin_api_refuses_a_header_named_in_both_headers_and_headers_env() {
    let test_id = Uuid::new_v4().simple().to_string();
    let pool = create_test_pool().await;
    setup_observer_schema(&pool).await.unwrap();
    let adapter =
        Arc::new(PostgresAdapter::new(&fraiseql_test_support::database_url()).await.unwrap());
    let api = admin_api(&pool, adapter);

    let (status, refused) = post(
        &api,
        &json!({
            "name": format!("both-{test_id}"),
            "entity_type": format!("Order{test_id}"),
            "event_type": "INSERT",
            "actions": [{"type": "webhook", "url": "https://example.test/hook",
                         "headers": {"x-trace": "a"},
                         "headers_env": {"X-Trace": "FRAISEQL_TEST_TRACE"}}],
        }),
    )
    .await;

    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{refused}");
    let message = refused.to_string();
    // The header, not merely an unknown key: `headers_env` must be read and the clash found.
    assert!(message.to_ascii_lowercase().contains("x-trace"), "names the header: {message}");
    assert!(message.contains("headers_env"), "the refusal names headers_env: {message}");
}
