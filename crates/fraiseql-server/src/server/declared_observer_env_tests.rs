//! A declared observer's `*_env` variables are checked when the server is built (#2106).
//!
//! The environment of a running process cannot change, so a variable missing at boot is
//! missing at every delivery. In production the server refuses to start, naming the
//! observer, the action, the key and the variable; in development it starts and warns.

#![allow(clippy::unwrap_used)] // Reason: test code
#![allow(clippy::print_stderr)] // Reason: skip message when no backing Postgres is available.

use std::sync::Arc;

use fraiseql_core::schema::{CompiledSchema, ObserverDefinition};
use fraiseql_test_utils::failing_adapter::FailingAdapter;
use serde_json::{Value, json};
use sqlx::postgres::PgPoolOptions;

use crate::{Server, server_config::ServerConfig};

/// A value no variable is ever set to in these tests' windows: a refusal that quoted it
/// would show here.
const VALUE: &str = "https://hooks.example.test/p2106-value";

/// A schema declaring one observer, `hook`, with `action` as its only action.
fn declaring(action: Value) -> CompiledSchema {
    let mut schema = CompiledSchema::new();
    schema.project_name = Some("p2106".to_string());
    schema.observers_config = Some(fraiseql_core::schema::ObserversConfig {
        enabled: true,
        ..Default::default()
    });
    schema.observers = vec![ObserverDefinition::new("hook", "Order", "INSERT").with_action(action)];
    schema
}

/// Build a server over `schema` with `vars` set (or unset) for the duration of the build.
/// The pool is lazy and never connects: the check runs before any database work.
// Reason: `temp_env`'s lock guard is not `Send`; a `#[tokio::test]` runs on one thread.
#[allow(clippy::future_not_send)]
async fn build(
    schema: CompiledSchema,
    vars: Vec<(&'static str, Option<&'static str>)>,
) -> Result<(), String> {
    let mut window = vec![
        ("FRAISEQL_PROFILE", None),
        ("FRAISEQL_ALLOW_PRIVATE_WEBHOOKS", None),
    ];
    window.extend(super::extensions::UNRUN_TRANSPORT_VARS.map(|var| (var, None)));
    window.extend(vars);
    Box::pin(temp_env::async_with_vars(window, async {
        let pool = PgPoolOptions::new()
            .connect_lazy("postgres://fraiseql@127.0.0.1:1/p2106")
            .unwrap();
        let config = ServerConfig {
            admin_token: Some("a".repeat(32)),
            cors_enabled: false,
            ..ServerConfig::default()
        };
        Box::pin(Server::new(config, schema, Arc::new(FailingAdapter::new()), Some(pool)))
            .await
            .map(drop)
            .map_err(|e| e.to_string())
    }))
    .await
}

const PRODUCTION: (&str, Option<&str>) = ("FRAISEQL_ENV", None);
const DEVELOPMENT: (&str, Option<&str>) = ("FRAISEQL_ENV", Some("development"));

/// The refusal names the observer, the key and the variable, and never a value.
fn assert_names(error: &str, key: &str, variable: &str) {
    assert!(error.contains("`hook`"), "names the observer: {error}");
    assert!(error.contains(key), "names the key {key}: {error}");
    assert!(error.contains(variable), "names the variable {variable}: {error}");
    assert!(!error.contains(VALUE), "quotes a value: {error}");
}

#[tokio::test]
#[allow(clippy::future_not_send)] // Reason: see `build`
async fn production_refuses_a_webhook_url_env_that_is_not_set() {
    let schema = declaring(json!({"type": "webhook", "url_env": "P2106_URL"}));
    let error = build(schema, vec![PRODUCTION, ("P2106_URL", None)])
        .await
        .expect_err("booted with the webhook URL variable unset");
    assert_names(&error, "url_env", "P2106_URL");
}

#[tokio::test]
#[allow(clippy::future_not_send)] // Reason: see `build`
async fn production_refuses_a_signing_secret_env_that_is_not_set() {
    let schema = declaring(json!({"type": "webhook", "url": "https://hooks.example.test/a",
                                  "signing_secret_env": "P2106_KEY"}));
    let error = build(schema, vec![PRODUCTION, ("P2106_KEY", None)])
        .await
        .expect_err("booted with the signing secret variable unset");
    assert_names(&error, "signing_secret_env", "P2106_KEY");
}

#[tokio::test]
#[allow(clippy::future_not_send)] // Reason: see `build`
async fn production_refuses_a_headers_env_variable_that_is_not_set() {
    let schema = declaring(json!({"type": "webhook", "url": "https://hooks.example.test/a",
                                  "headers_env": {"Authorization": "P2106_TOKEN"}}));
    let error = build(schema, vec![PRODUCTION, ("P2106_TOKEN", None)])
        .await
        .expect_err("booted with a header variable unset");
    assert_names(&error, "headers_env", "P2106_TOKEN");
    assert!(error.contains("Authorization"), "names the header: {error}");
}

#[tokio::test]
#[allow(clippy::future_not_send)] // Reason: see `build`
async fn production_refuses_a_slack_webhook_url_env_that_is_not_set() {
    let schema = declaring(json!({"type": "slack", "webhook_url_env": "P2106_SLACK",
                                  "message_template": "hi"}));
    let error = build(schema, vec![PRODUCTION, ("P2106_SLACK", None)])
        .await
        .expect_err("booted with the Slack URL variable unset");
    assert_names(&error, "webhook_url_env", "P2106_SLACK");
}

/// An empty value cannot be used either, so it is refused as an unset one is.
#[tokio::test]
#[allow(clippy::future_not_send)] // Reason: see `build`
async fn production_refuses_a_variable_that_is_set_but_empty() {
    let schema = declaring(json!({"type": "webhook", "url_env": "P2106_URL"}));
    let error = build(schema, vec![PRODUCTION, ("P2106_URL", Some(""))])
        .await
        .expect_err("booted with the webhook URL variable empty");
    assert_names(&error, "url_env", "P2106_URL");
}

/// Every missing variable is named in the one refusal, so a deployment fixes them in
/// one pass.
#[tokio::test]
#[allow(clippy::future_not_send)] // Reason: see `build`
async fn production_names_every_missing_variable_at_once() {
    let schema = declaring(json!({"type": "webhook", "url_env": "P2106_URL",
                                  "signing_secret_env": "P2106_KEY"}));
    let error = build(schema, vec![PRODUCTION, ("P2106_URL", None), ("P2106_KEY", None)])
        .await
        .expect_err("booted with both variables unset");
    assert_names(&error, "url_env", "P2106_URL");
    assert_names(&error, "signing_secret_env", "P2106_KEY");
}

/// Development starts: the deliveries dead-letter, as before.
#[tokio::test]
#[allow(clippy::future_not_send)] // Reason: see `build`
async fn development_starts_with_a_variable_that_is_not_set() {
    let schema = declaring(json!({"type": "webhook", "url_env": "P2106_URL"}));
    let built = build(schema, vec![DEVELOPMENT, ("P2106_URL", None)]).await;
    assert!(built.is_ok(), "development refused: {built:?}");
}

/// Every variable set: production starts, so the check refuses only what it names.
#[tokio::test]
#[allow(clippy::future_not_send)] // Reason: see `build`
async fn production_starts_when_every_variable_is_set() {
    let schema = declaring(json!({"type": "webhook", "url_env": "P2106_URL",
                                  "signing_secret_env": "P2106_KEY",
                                  "headers_env": {"Authorization": "P2106_TOKEN"}}));
    let built = build(
        schema,
        vec![
            PRODUCTION,
            ("P2106_URL", Some(VALUE)),
            ("P2106_KEY", Some("k")),
            ("P2106_TOKEN", Some("t")),
        ],
    )
    .await;
    assert!(built.is_ok(), "production refused a complete declaration: {built:?}");
}

/// The declared observers stored under `owner`.
async fn stored_under(pool: &sqlx::PgPool, owner: &str) -> i64 {
    sqlx::query_scalar("SELECT count(*) FROM core.tb_observer WHERE managed_by = $1")
        .bind(owner)
        .fetch_one(pool)
        .await
        .unwrap()
}

/// Build a server over `schema` on `pool` with `var` as given, and serve it until its
/// declared observers are stored under `owner` or ten seconds pass. Returns whether the
/// build was refused.
// Reason: `temp_env`'s lock guard is not `Send`; a `#[tokio::test]` runs on one thread.
#[allow(clippy::future_not_send)]
async fn boot_on(
    pool: sqlx::PgPool,
    schema: CompiledSchema,
    var: (&'static str, Option<&'static str>),
    owner: &str,
) -> bool {
    let mut window = vec![PRODUCTION, ("FRAISEQL_PROFILE", None), var];
    window.extend(super::extensions::UNRUN_TRANSPORT_VARS.map(|var| (var, None)));
    Box::pin(temp_env::async_with_vars(window, async {
        let config = ServerConfig {
            admin_token: Some("a".repeat(32)),
            cors_enabled: false,
            ..ServerConfig::default()
        };
        let built = Box::pin(Server::new(
            config,
            schema,
            Arc::new(FailingAdapter::new()),
            Some(pool.clone()),
        ))
        .await;
        let Ok(server) = built else { return true };
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let (stop, stopped) = tokio::sync::oneshot::channel::<()>();
        let serving = tokio::spawn(server.serve_on_listener(listener, async {
            let _ = stopped.await;
        }));
        let deadline = tokio::time::Instant::now() + std::time::Duration::from_secs(10);
        while stored_under(&pool, owner).await == 0
            && !serving.is_finished()
            && tokio::time::Instant::now() < deadline
        {
            tokio::time::sleep(std::time::Duration::from_millis(100)).await;
        }
        let _ = stop.send(());
        let _ = serving.await;
        false
    }))
    .await
}

/// The check runs before the boot writes the declared observers, so a refused boot
/// stores none of them. The same boot with the variable set stores the observer, so the
/// count below can see one.
#[tokio::test]
#[allow(clippy::future_not_send)] // Reason: see `build`
async fn a_refused_boot_stores_no_declared_observer() {
    let Some(svc) = fraiseql_test_support::postgres().await else {
        eprintln!("SKIP a_refused_boot_stores_no_declared_observer: no postgres");
        return;
    };
    let pool = sqlx::PgPool::connect(svc.url()).await.unwrap();
    crate::observers::install_runtime_schema_for_tests(&pool).await;
    for sql in [
        fraiseql_observers::migrations::observer_management_sql(),
        fraiseql_observers::migrations::observer_dispatch_sql(),
        fraiseql_observers::checkpoint::migration_sql(),
    ] {
        crate::migration_lock::run_migration(&pool, sql).await.unwrap();
    }
    let project = format!("p2106{}", uuid::Uuid::new_v4().simple());
    let owner = format!("schema:{project}");
    let schema = || {
        let mut schema = declaring(json!({"type": "webhook", "url_env": "P2106_STORE_URL"}));
        schema.project_name = Some(project.clone());
        schema.observers[0].name = format!("hook-{project}");
        schema
    };

    let refused = boot_on(pool.clone(), schema(), ("P2106_STORE_URL", None), &owner).await;
    assert_eq!(stored_under(&pool, &owner).await, 0, "a refused boot stored its observer");
    assert!(refused, "booted with the webhook URL variable unset");

    let refused = boot_on(pool.clone(), schema(), ("P2106_STORE_URL", Some(VALUE)), &owner).await;
    assert!(!refused, "refused with the webhook URL variable set");
    assert_eq!(stored_under(&pool, &owner).await, 1, "the boot stored its observer");
}
