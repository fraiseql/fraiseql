//! Observer admin API tenant scope, driven through the real routers against PostgreSQL.
//!
//! A principal whose token names an organisation manages that organisation's observers and
//! sees their executions, nothing else. Global observers (`fk_customer_org IS NULL`) are
//! visible to it as definitions but belong to the operator: they fire on every tenant's
//! events, so their logs and statistics mix tenants, and changing one changes every tenant's
//! delivery. The change log is an operator surface and refuses an organisation-scoped token.
//!
//! The admin bearer gate (`admin_auth_middleware`) is not mounted here: these tests inject the
//! principal it would have admitted, so what they pin is what the handlers do with it.
//!
//! # Running
//!
//! ```bash
//! cargo test -p fraiseql-server --features observers --test observer_admin_scope_pg -- --test-threads=1
//! ```

#![cfg(feature = "observers")]
#![allow(clippy::unwrap_used)] // Reason: test code, panics are acceptable
#![allow(clippy::missing_panics_doc)] // Reason: test functions, panics are expected
#![allow(missing_docs)] // Reason: test code does not require documentation

mod observer_test_helpers;

use std::{collections::HashMap, sync::Arc};

use axum::{
    Router,
    body::Body,
    http::{Method, Request, StatusCode},
};
use chrono::{Duration, Utc};
use fraiseql_core::security::AuthenticatedUser;
use fraiseql_server::{
    middleware::{AuthUser, TenantClaim},
    observers::{
        ChangelogState, ObserverRepository, ObserverState, observer_changelog_routes,
        observer_routes,
    },
};
use observer_test_helpers::setup_observer_schema;
use serde_json::{Value, json};
use sqlx::PgPool;
use tower::ServiceExt;
use uuid::Uuid;

const ORG_A: i64 = 1;
const ORG_B: i64 = 2;

async fn setup_pg() -> (PgPool, fraiseql_test_support::Service) {
    let svc = fraiseql_test_support::postgres()
        .await
        .expect("DATABASE_URL must be set (or enable fraiseql-test-support/local-testcontainers)");
    let pool = PgPool::connect(svc.url()).await.unwrap();
    setup_observer_schema(&pool).await.unwrap();
    sqlx::query(
        "TRUNCATE tb_observer_log, tb_observer, observer_checkpoints, core.tb_entity_change_log \
         RESTART IDENTITY CASCADE",
    )
    .execute(&pool)
    .await
    .unwrap();
    (pool, svc)
}

/// The principals the admin gate admits: an operator token names no tenant.
#[derive(Clone, Copy)]
enum Caller {
    Operator,
    Org(i64),
    /// A tenant the observer tables cannot express (`fk_customer_org` is a BIGINT).
    NamedTenant(&'static str),
}

fn auth_user(caller: Caller) -> AuthUser {
    let mut extra_claims = HashMap::new();
    match caller {
        Caller::Operator => {},
        Caller::Org(org) => {
            extra_claims.insert("tenant_id".to_owned(), json!(org));
        },
        Caller::NamedTenant(name) => {
            extra_claims.insert("tenant_id".to_owned(), json!(name));
        },
    }
    AuthUser(AuthenticatedUser {
        user_id: fraiseql_core::types::UserId::new("admin-user"),
        scopes: vec!["fraiseql:admin".to_owned()],
        expires_at: Utc::now() + Duration::hours(1),
        email: None,
        display_name: None,
        extra_claims,
    })
}

fn admin_api(pool: &PgPool, caller: Caller) -> Router {
    let observers = observer_routes(ObserverState {
        repository: ObserverRepository::new(pool.clone()),
        runtime:    None,
    });
    let changelog = observer_changelog_routes(ChangelogState { pool: pool.clone() });
    Router::new()
        .nest("/api/observers", observers)
        .nest("/api/observers", changelog)
        .layer(axum::Extension(auth_user(caller)))
        .layer(axum::Extension(TenantClaim(Arc::from("tenant_id"))))
}

async fn call(
    pool: &PgPool,
    caller: Caller,
    method: Method,
    uri: &str,
    body: Option<Value>,
) -> (StatusCode, Value) {
    let request = Request::builder().method(method).uri(uri);
    let request = match body {
        Some(body) => request
            .header("content-type", "application/json")
            .body(Body::from(body.to_string()))
            .unwrap(),
        None => request.body(Body::empty()).unwrap(),
    };
    let response = admin_api(pool, caller).oneshot(request).await.unwrap();
    let status = response.status();
    let bytes = axum::body::to_bytes(response.into_body(), 1 << 20).await.unwrap();
    (status, serde_json::from_slice(&bytes).unwrap_or(Value::Null))
}

async fn insert_observer(pool: &PgPool, name: &str, org: Option<i64>) -> (i64, Uuid) {
    sqlx::query_as(
        "INSERT INTO tb_observer (name, entity_type, event_type, actions, fk_customer_org)
         VALUES ($1, 'Order', 'INSERT', '[]', $2)
         RETURNING pk_observer, id",
    )
    .bind(name)
    .bind(org)
    .fetch_one(pool)
    .await
    .unwrap()
}

/// A log row as the runtime writes it: the runtime stamps no organisation on a log.
async fn insert_log(pool: &PgPool, pk_observer: i64) -> Uuid {
    let event_id = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO tb_observer_log
            (fk_observer, event_id, entity_type, entity_id, event_type, status)
         VALUES ($1, $2, 'Order', $3, 'INSERT', 'success')",
    )
    .bind(pk_observer)
    .bind(event_id)
    .bind(Uuid::new_v4())
    .execute(pool)
    .await
    .unwrap();
    event_id
}

async fn is_live(pool: &PgPool, id: Uuid) -> bool {
    let (live,): (bool,) =
        sqlx::query_as("SELECT deleted_at IS NULL FROM tb_observer WHERE id = $1")
            .bind(id)
            .fetch_one(pool)
            .await
            .unwrap();
    live
}

async fn is_enabled(pool: &PgPool, id: Uuid) -> bool {
    let (enabled,): (bool,) = sqlx::query_as("SELECT enabled FROM tb_observer WHERE id = $1")
        .bind(id)
        .fetch_one(pool)
        .await
        .unwrap();
    enabled
}

fn event_ids(logs: &Value) -> Vec<String> {
    logs["data"]
        .as_array()
        .unwrap()
        .iter()
        .map(|log| log["event_id"].as_str().unwrap().to_owned())
        .collect()
}

fn observer_names(page: &Value) -> Vec<String> {
    page["data"]
        .as_array()
        .unwrap()
        .iter()
        .map(|o| o["name"].as_str().unwrap().to_owned())
        .collect()
}

fn stats_ids(stats: &Value) -> Vec<String> {
    stats
        .as_array()
        .unwrap()
        .iter()
        .map(|s| s["observer_id"].as_str().unwrap().to_owned())
        .collect()
}

mod another_organisations_observer {
    use super::*;

    #[tokio::test]
    async fn is_not_found_by_id() {
        let (pool, _svc) = setup_pg().await;
        let (_, theirs) = insert_observer(&pool, "theirs", Some(ORG_B)).await;

        let (status, _) = call(
            &pool,
            Caller::Org(ORG_A),
            Method::GET,
            &format!("/api/observers/{theirs}"),
            None,
        )
        .await;

        assert_eq!(status, StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn cannot_be_deleted() {
        let (pool, _svc) = setup_pg().await;
        let (_, theirs) = insert_observer(&pool, "theirs", Some(ORG_B)).await;

        let (status, _) = call(
            &pool,
            Caller::Org(ORG_A),
            Method::DELETE,
            &format!("/api/observers/{theirs}"),
            None,
        )
        .await;

        assert_eq!(status, StatusCode::NOT_FOUND);
        assert!(is_live(&pool, theirs).await, "the other organisation's observer survives");
    }

    #[tokio::test]
    async fn cannot_be_disabled() {
        let (pool, _svc) = setup_pg().await;
        let (_, theirs) = insert_observer(&pool, "theirs", Some(ORG_B)).await;

        let (status, _) = call(
            &pool,
            Caller::Org(ORG_A),
            Method::POST,
            &format!("/api/observers/{theirs}/disable"),
            None,
        )
        .await;

        assert_eq!(status, StatusCode::NOT_FOUND);
        assert!(is_enabled(&pool, theirs).await, "the other organisation's observer still fires");
    }

    #[tokio::test]
    async fn cannot_be_enabled() {
        let (pool, _svc) = setup_pg().await;
        let (_, theirs) = insert_observer(&pool, "theirs", Some(ORG_B)).await;
        sqlx::query("UPDATE tb_observer SET enabled = false WHERE id = $1")
            .bind(theirs)
            .execute(&pool)
            .await
            .unwrap();

        let (status, _) = call(
            &pool,
            Caller::Org(ORG_A),
            Method::POST,
            &format!("/api/observers/{theirs}/enable"),
            None,
        )
        .await;

        assert_eq!(status, StatusCode::NOT_FOUND);
        assert!(!is_enabled(&pool, theirs).await);
    }

    #[tokio::test]
    async fn executions_are_not_in_the_log() {
        let (pool, _svc) = setup_pg().await;
        let (pk_ours, _) = insert_observer(&pool, "ours", Some(ORG_A)).await;
        let (pk_theirs, theirs) = insert_observer(&pool, "theirs", Some(ORG_B)).await;
        let ours_event = insert_log(&pool, pk_ours).await;
        let theirs_event = insert_log(&pool, pk_theirs).await;

        let (status, all) =
            call(&pool, Caller::Org(ORG_A), Method::GET, "/api/observers/logs", None).await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(all["total_count"], json!(1), "the count is scoped as the page is: {all}");
        let all = event_ids(&all);
        assert!(all.contains(&ours_event.to_string()), "its own executions are listed: {all:?}");
        assert!(!all.contains(&theirs_event.to_string()), "B's execution leaked: {all:?}");

        let (status, by_id) = call(
            &pool,
            Caller::Org(ORG_A),
            Method::GET,
            &format!("/api/observers/{theirs}/logs"),
            None,
        )
        .await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(event_ids(&by_id), Vec::<String>::new());
    }

    #[tokio::test]
    async fn statistics_are_not_reported() {
        let (pool, _svc) = setup_pg().await;
        let (_, ours) = insert_observer(&pool, "ours", Some(ORG_A)).await;
        let (_, theirs) = insert_observer(&pool, "theirs", Some(ORG_B)).await;

        let (status, all) =
            call(&pool, Caller::Org(ORG_A), Method::GET, "/api/observers/stats", None).await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(stats_ids(&all), vec![ours.to_string()]);

        let (status, by_id) = call(
            &pool,
            Caller::Org(ORG_A),
            Method::GET,
            &format!("/api/observers/{theirs}/stats"),
            None,
        )
        .await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(stats_ids(&by_id), Vec::<String>::new());
    }
}

mod a_global_observer {
    use super::*;

    #[tokio::test]
    async fn is_listed_with_an_organisations_own() {
        let (pool, _svc) = setup_pg().await;
        insert_observer(&pool, "global", None).await;
        insert_observer(&pool, "ours", Some(ORG_A)).await;
        insert_observer(&pool, "theirs", Some(ORG_B)).await;

        let (status, page) =
            call(&pool, Caller::Org(ORG_A), Method::GET, "/api/observers", None).await;

        assert_eq!(status, StatusCode::OK);
        let mut names = observer_names(&page);
        names.sort();
        assert_eq!(names, vec!["global", "ours"]);
        assert_eq!(page["total_count"], json!(2), "the count is scoped as the page is: {page}");
    }

    #[tokio::test]
    async fn is_visible_to_an_organisation() {
        let (pool, _svc) = setup_pg().await;
        let (_, global) = insert_observer(&pool, "global", None).await;

        let (status, _) = call(
            &pool,
            Caller::Org(ORG_A),
            Method::GET,
            &format!("/api/observers/{global}"),
            None,
        )
        .await;

        assert_eq!(status, StatusCode::OK);
    }

    #[tokio::test]
    async fn cannot_be_changed_by_an_organisation() {
        let (pool, _svc) = setup_pg().await;
        let (_, global) = insert_observer(&pool, "global", None).await;

        let (disable, _) = call(
            &pool,
            Caller::Org(ORG_A),
            Method::POST,
            &format!("/api/observers/{global}/disable"),
            None,
        )
        .await;
        let (patch, _) = call(
            &pool,
            Caller::Org(ORG_A),
            Method::PATCH,
            &format!("/api/observers/{global}"),
            Some(json!({ "name": "renamed" })),
        )
        .await;
        let (delete, _) = call(
            &pool,
            Caller::Org(ORG_A),
            Method::DELETE,
            &format!("/api/observers/{global}"),
            None,
        )
        .await;

        assert_eq!(
            (disable, patch, delete),
            (StatusCode::NOT_FOUND, StatusCode::NOT_FOUND, StatusCode::NOT_FOUND)
        );
        assert!(is_live(&pool, global).await && is_enabled(&pool, global).await);
    }

    #[tokio::test]
    async fn executions_are_not_shown_to_an_organisation() {
        let (pool, _svc) = setup_pg().await;
        let (pk_global, _) = insert_observer(&pool, "global", None).await;
        let event = insert_log(&pool, pk_global).await;

        let (status, logs) =
            call(&pool, Caller::Org(ORG_A), Method::GET, "/api/observers/logs", None).await;

        assert_eq!(status, StatusCode::OK);
        assert!(
            !event_ids(&logs).contains(&event.to_string()),
            "a global observer fires on every tenant's events"
        );
    }

    #[tokio::test]
    async fn is_changed_by_the_operator() {
        let (pool, _svc) = setup_pg().await;
        let (_, global) = insert_observer(&pool, "global", None).await;

        let (status, _) = call(
            &pool,
            Caller::Operator,
            Method::POST,
            &format!("/api/observers/{global}/disable"),
            None,
        )
        .await;

        assert_eq!(status, StatusCode::OK);
        assert!(!is_enabled(&pool, global).await);
    }
}

mod the_operator {
    use super::*;

    #[tokio::test]
    async fn sees_every_organisations_observers_and_executions() {
        let (pool, _svc) = setup_pg().await;
        let (pk_a, _) = insert_observer(&pool, "a", Some(ORG_A)).await;
        let (pk_b, _) = insert_observer(&pool, "b", Some(ORG_B)).await;
        let event_a = insert_log(&pool, pk_a).await;
        let event_b = insert_log(&pool, pk_b).await;

        let (_, page) = call(&pool, Caller::Operator, Method::GET, "/api/observers", None).await;
        let (_, logs) =
            call(&pool, Caller::Operator, Method::GET, "/api/observers/logs", None).await;

        let mut names = observer_names(&page);
        names.sort();
        assert_eq!(names, vec!["a", "b"]);
        let logs = event_ids(&logs);
        assert!(logs.contains(&event_a.to_string()) && logs.contains(&event_b.to_string()));
    }

    #[tokio::test]
    async fn reads_the_change_log() {
        let (pool, _svc) = setup_pg().await;

        let (status, _) =
            call(&pool, Caller::Operator, Method::GET, "/api/observers/changelog", None).await;

        assert_eq!(status, StatusCode::OK);
    }
}

mod an_organisation {
    use super::*;

    #[tokio::test]
    async fn creates_observers_it_owns() {
        let (pool, _svc) = setup_pg().await;

        let (status, created) = call(
            &pool,
            Caller::Org(ORG_A),
            Method::POST,
            "/api/observers",
            Some(json!({
                "name": "ours",
                "entity_type": "Order",
                "event_type": "INSERT",
                "actions": [{ "type": "log", "level": "info", "message_template": "fired" }],
            })),
        )
        .await;

        assert_eq!(status, StatusCode::CREATED, "{created}");
        assert_eq!(created["fk_customer_org"], json!(ORG_A));
    }

    #[tokio::test]
    async fn manages_its_own_observer() {
        let (pool, _svc) = setup_pg().await;
        let (_, ours) = insert_observer(&pool, "ours", Some(ORG_A)).await;

        let (disable, _) = call(
            &pool,
            Caller::Org(ORG_A),
            Method::POST,
            &format!("/api/observers/{ours}/disable"),
            None,
        )
        .await;
        let (delete, _) = call(
            &pool,
            Caller::Org(ORG_A),
            Method::DELETE,
            &format!("/api/observers/{ours}"),
            None,
        )
        .await;

        assert_eq!((disable, delete), (StatusCode::OK, StatusCode::NO_CONTENT));
        assert!(!is_live(&pool, ours).await);
    }

    /// The change log holds every tenant's entity changes, and the checkpoints are the
    /// operator's consumers' cursors.
    #[tokio::test]
    async fn is_refused_the_change_log_and_its_checkpoints() {
        let (pool, _svc) = setup_pg().await;

        for (method, uri, body) in [
            (Method::GET, "/api/observers/changelog", None),
            (Method::GET, "/api/observers/checkpoint/consumer", None),
            (
                Method::PUT,
                "/api/observers/checkpoint/consumer",
                Some(json!({ "last_cursor": 9 })),
            ),
        ] {
            let (status, _) = call(&pool, Caller::Org(ORG_A), method.clone(), uri, body).await;
            assert_eq!(status, StatusCode::FORBIDDEN, "{method} {uri}");
        }
        let (count,): (i64,) = sqlx::query_as("SELECT COUNT(*) FROM observer_checkpoints")
            .fetch_one(&pool)
            .await
            .unwrap();
        assert_eq!(count, 0, "no checkpoint was written");
    }
}

mod a_tenant_that_is_not_an_organisation_id {
    use super::*;

    /// `fk_customer_org` is a BIGINT. A token whose tenant cannot be one names an
    /// organisation the observer tables cannot scope by; reading it as "no tenant" made it
    /// the operator.
    #[tokio::test]
    async fn is_refused_rather_than_treated_as_the_operator() {
        let (pool, _svc) = setup_pg().await;
        insert_observer(&pool, "a", Some(ORG_A)).await;
        insert_observer(&pool, "b", Some(ORG_B)).await;

        let (status, body) =
            call(&pool, Caller::NamedTenant("acme"), Method::GET, "/api/observers", None).await;

        assert_eq!(status, StatusCode::FORBIDDEN, "{body}");
    }
}

/// A failed query answers with a generic message; the database's own text goes to the log.
#[tokio::test]
async fn a_database_error_is_not_echoed_to_the_caller() {
    let (pool, _svc) = setup_pg().await;
    sqlx::query("DROP VIEW vw_observer_stats").execute(&pool).await.unwrap();

    let (status, body) =
        call(&pool, Caller::Operator, Method::GET, "/api/observers/stats", None).await;

    setup_observer_schema(&pool).await.unwrap();
    assert_eq!(status, StatusCode::INTERNAL_SERVER_ERROR);
    let text = body.to_string();
    assert!(
        !text.contains("vw_observer_stats") && !text.contains("relation"),
        "database text reached the caller: {text}"
    );
}
