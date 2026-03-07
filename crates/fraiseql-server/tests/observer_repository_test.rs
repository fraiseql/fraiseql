//! Observer repository integration tests.
//!
//! These tests verify the SQL layer end-to-end against a real PostgreSQL
//! database: correct row returns, pagination, tenant isolation, and — the
//! critical case — that injection payloads do not affect other rows.
//!
//! # Running
//!
//! ```bash
//! export DATABASE_URL="postgresql://postgres:postgres@localhost/fraiseql_test"
//! cargo test --test observer_repository_test --features observers -- --ignored --nocapture
//! ```

#![cfg(feature = "observers")]

#![allow(clippy::unwrap_used)] // Reason: test code, panics are acceptable
#![allow(clippy::missing_panics_doc)] // Reason: test functions, panics are expected
#![allow(missing_docs)] // Reason: test code does not require documentation

mod observer_test_helpers;

use fraiseql_server::observers::{
    CreateObserverRequest, ListObserverLogsQuery, ListObserversQuery, ObserverRepository,
};
use observer_test_helpers::{cleanup_test_data, create_test_pool, setup_observer_schema};
use serde_json::json;
use uuid::Uuid;

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Insert a minimal observer row directly via SQL, bypassing the repository
/// create path so we can control fk_customer_org precisely.
async fn insert_observer(
    pool: &sqlx::PgPool,
    name: &str,
    entity_type: &str,
    event_type: &str,
    org_id: Option<i64>,
) -> i64 {
    let row: (i64,) = sqlx::query_as(
        r"INSERT INTO tb_observer
            (name, entity_type, event_type, actions, fk_customer_org)
          VALUES ($1, $2, $3, '[]', $4)
          RETURNING pk_observer",
    )
    .bind(name)
    .bind(entity_type)
    .bind(event_type)
    .bind(org_id)
    .fetch_one(pool)
    .await
    .unwrap();
    row.0
}

/// Insert a minimal observer log row linked to a given pk_observer.
async fn insert_log(
    pool: &sqlx::PgPool,
    pk_observer: i64,
    status: &str,
    trace_id: Option<&str>,
) -> Uuid {
    let event_id = Uuid::new_v4();
    let entity_id = Uuid::new_v4();
    sqlx::query(
        r"INSERT INTO tb_observer_log
            (fk_observer, event_id, entity_type, entity_id, event_type, status, trace_id)
          VALUES ($1, $2, 'TestEntity', $3, 'INSERT', $4, $5)",
    )
    .bind(pk_observer)
    .bind(event_id)
    .bind(entity_id)
    .bind(status)
    .bind(trace_id)
    .execute(pool)
    .await
    .unwrap();
    event_id
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

/// Verify that list() with a customer_org filter only returns observers
/// belonging to that org (or org-agnostic rows).
#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn test_list_tenant_isolation() {
    let pool = create_test_pool().await;
    setup_observer_schema(&pool).await.unwrap();

    let test_id = Uuid::new_v4().to_string();
    let name_a = format!("obs-a-{test_id}");
    let name_b = format!("obs-b-{test_id}");

    insert_observer(&pool, &name_a, "Order", "INSERT", Some(1)).await;
    insert_observer(&pool, &name_b, "Payment", "INSERT", Some(2)).await;

    let repo = ObserverRepository::new(pool.clone());
    let query = ListObserversQuery {
        page: 1,
        page_size: 50,
        include_deleted: false,
        entity_type: None,
        event_type: None,
        enabled: None,
    };

    let (observers, count) = repo.list(&query, Some(1)).await.unwrap();
    let names: Vec<&str> = observers.iter().map(|o| o.name.as_str()).collect();

    assert!(names.contains(&name_a.as_str()), "org-1 observer should appear");
    assert!(!names.contains(&name_b.as_str()), "org-2 observer must not appear");
    assert!(count >= 1);

    cleanup_test_data(&pool, &test_id).await.unwrap();
}

/// The critical injection test: a malicious entity_type string is treated as
/// a literal value, returning 0 rows rather than leaking cross-tenant data or
/// throwing an error.
#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn test_list_injection_payload_returns_no_rows() {
    let pool = create_test_pool().await;
    setup_observer_schema(&pool).await.unwrap();

    let test_id = Uuid::new_v4().to_string();
    let name_1 = format!("inj-1-{test_id}");
    let name_2 = format!("inj-2-{test_id}");

    insert_observer(&pool, &name_1, "Order", "INSERT", Some(1)).await;
    insert_observer(&pool, &name_2, "Payment", "INSERT", Some(2)).await;

    let repo = ObserverRepository::new(pool.clone());
    let query = ListObserversQuery {
        page: 1,
        page_size: 50,
        include_deleted: false,
        // Classic injection payload
        entity_type: Some("' OR 1=1 --".to_string()),
        event_type: None,
        enabled: None,
    };

    // Must return 0 rows: the payload is a literal string, not SQL.
    let (observers, count) = repo.list(&query, Some(1)).await.unwrap();
    assert_eq!(
        observers.len(),
        0,
        "injection payload must not match any rows (got {} rows, count={})",
        observers.len(),
        count,
    );
    assert_eq!(count, 0);

    cleanup_test_data(&pool, &test_id).await.unwrap();
}

/// Verify that all optional list() filters, when set, narrow the result to the
/// matching single row.
#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn test_list_all_filters() {
    let pool = create_test_pool().await;
    setup_observer_schema(&pool).await.unwrap();

    let test_id = Uuid::new_v4().to_string();
    let target_name = format!("filter-target-{test_id}");
    let other_name = format!("filter-other-{test_id}");

    insert_observer(&pool, &target_name, "Invoice", "UPDATE", Some(10)).await;
    insert_observer(&pool, &other_name, "Order", "INSERT", Some(10)).await;

    let repo = ObserverRepository::new(pool.clone());
    let query = ListObserversQuery {
        page: 1,
        page_size: 50,
        include_deleted: false,
        entity_type: Some("Invoice".to_string()),
        event_type: Some("UPDATE".to_string()),
        enabled: Some(true),
    };

    let (observers, count) = repo.list(&query, Some(10)).await.unwrap();
    let names: Vec<&str> = observers.iter().map(|o| o.name.as_str()).collect();

    assert!(names.contains(&target_name.as_str()));
    assert!(!names.contains(&other_name.as_str()));
    assert_eq!(count, 1);

    cleanup_test_data(&pool, &test_id).await.unwrap();
}

/// Verify that pagination (page 1 + page 2) covers all rows without duplicates.
#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn test_list_pagination_correctness() {
    let pool = create_test_pool().await;
    setup_observer_schema(&pool).await.unwrap();

    let test_id = Uuid::new_v4().to_string();
    // Insert 3 observers; paginate with page_size=2
    for i in 0..3 {
        insert_observer(&pool, &format!("pag-{i}-{test_id}"), "Widget", "INSERT", Some(99)).await;
    }

    let repo = ObserverRepository::new(pool.clone());
    let base = ListObserversQuery {
        page: 1,
        page_size: 2,
        include_deleted: false,
        entity_type: Some("Widget".to_string()),
        event_type: Some("INSERT".to_string()),
        enabled: None,
    };

    let (page1, total) = repo.list(&base, Some(99)).await.unwrap();
    let (page2, _) = repo
        .list(&ListObserversQuery { page: 2, ..base }, Some(99))
        .await
        .unwrap();

    assert_eq!(total, 3);
    assert_eq!(page1.len(), 2);
    assert_eq!(page2.len(), 1);

    // No duplicates across pages
    let ids1: std::collections::HashSet<_> = page1.iter().map(|o| o.pk_observer).collect();
    let ids2: std::collections::HashSet<_> = page2.iter().map(|o| o.pk_observer).collect();
    assert!(ids1.is_disjoint(&ids2), "pages must not share rows");

    cleanup_test_data(&pool, &test_id).await.unwrap();
}

/// Verify list_logs() filters (status, trace_id, observer_id) each narrow
/// results correctly and that a malicious trace_id does not leak rows.
#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn test_list_logs_filters() {
    let pool = create_test_pool().await;
    setup_observer_schema(&pool).await.unwrap();

    let test_id = Uuid::new_v4().to_string();
    let pk = insert_observer(&pool, &format!("log-obs-{test_id}"), "Item", "DELETE", None).await;

    let trace = format!("trace-{test_id}");
    insert_log(&pool, pk, "success", Some(&trace)).await;
    insert_log(&pool, pk, "failure", None).await;

    let obs_uuid: (Uuid,) =
        sqlx::query_as("SELECT id FROM tb_observer WHERE pk_observer = $1")
            .bind(pk)
            .fetch_one(&pool)
            .await
            .unwrap();

    let repo = ObserverRepository::new(pool.clone());

    // Filter by status
    let q_status = ListObserverLogsQuery {
        page: 1,
        page_size: 50,
        observer_id: None,
        status: Some("success".to_string()),
        event_id: None,
        trace_id: None,
    };
    let (logs, _) = repo.list_logs(&q_status, None).await.unwrap();
    let statuses: Vec<&str> = logs.iter().map(|l| l.status.as_str()).collect();
    assert!(statuses.iter().all(|s| *s == "success"));

    // Filter by trace_id
    let q_trace = ListObserverLogsQuery {
        page: 1,
        page_size: 50,
        observer_id: None,
        status: None,
        event_id: None,
        trace_id: Some(trace.clone()),
    };
    let (logs, count) = repo.list_logs(&q_trace, None).await.unwrap();
    assert_eq!(count, 1);
    assert_eq!(logs[0].trace_id.as_deref(), Some(trace.as_str()));

    // Malicious trace_id returns 0 rows
    let q_inject = ListObserverLogsQuery {
        page: 1,
        page_size: 50,
        observer_id: None,
        status: None,
        event_id: None,
        trace_id: Some("x' OR 1=1 --".to_string()),
    };
    let (logs_inject, cnt_inject) = repo.list_logs(&q_inject, None).await.unwrap();
    assert_eq!(logs_inject.len(), 0, "injection payload must not match rows");
    assert_eq!(cnt_inject, 0);

    // Filter by observer_id
    let q_obs = ListObserverLogsQuery {
        page: 1,
        page_size: 50,
        observer_id: Some(obs_uuid.0),
        status: None,
        event_id: None,
        trace_id: None,
    };
    let (logs_obs, cnt_obs) = repo.list_logs(&q_obs, None).await.unwrap();
    assert_eq!(cnt_obs, 2, "both logs should be found by observer_id");
    let all_for_obs = logs_obs.iter().all(|l| l.fk_observer == pk);
    assert!(all_for_obs);

    cleanup_test_data(&pool, &test_id).await.unwrap();
}

/// Verify delete() only soft-deletes when the observer belongs to the
/// specified customer_org (tenant isolation).
#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn test_delete_tenant_isolation() {
    let pool = create_test_pool().await;
    setup_observer_schema(&pool).await.unwrap();

    let test_id = Uuid::new_v4().to_string();
    let name_org1 = format!("del-org1-{test_id}");
    let name_org2 = format!("del-org2-{test_id}");

    insert_observer(&pool, &name_org1, "Thing", "INSERT", Some(1)).await;
    insert_observer(&pool, &name_org2, "Thing", "INSERT", Some(2)).await;

    // Retrieve UUIDs
    let (uuid_org2,): (Uuid,) =
        sqlx::query_as("SELECT id FROM tb_observer WHERE name = $1")
            .bind(&name_org2)
            .fetch_one(&pool)
            .await
            .unwrap();

    let repo = ObserverRepository::new(pool.clone());

    // Attempt to delete org-2's observer while acting as org-1 — must fail (false)
    let deleted = repo.delete(uuid_org2, Some(1)).await.unwrap();
    assert!(!deleted, "cross-tenant delete must be rejected");

    // Verify the row is still alive
    let still_there: Option<(Uuid,)> =
        sqlx::query_as("SELECT id FROM tb_observer WHERE name = $1 AND deleted_at IS NULL")
            .bind(&name_org2)
            .fetch_optional(&pool)
            .await
            .unwrap();
    assert!(still_there.is_some(), "observer must not have been deleted");

    cleanup_test_data(&pool, &test_id).await.unwrap();
}
