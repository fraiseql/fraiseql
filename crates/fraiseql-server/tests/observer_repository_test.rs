//! Observer repository integration tests.
#![allow(clippy::doc_markdown)]
//! These tests verify the SQL layer end-to-end against a real PostgreSQL
//! database spun up via testcontainers: correct row returns, pagination,
//! tenant isolation, and — the critical case — that injection payloads do
//! not affect other rows.
//!
//! No external database or environment variables are required; each test
//! creates its own container automatically.
//!
//! # Running
//!
//! ```bash
//! cargo test --test observer_repository_test --features observers
//! ```

#![cfg(feature = "observers")]
#![allow(clippy::unwrap_used)] // Reason: test code, panics are acceptable
#![allow(clippy::missing_panics_doc)] // Reason: test functions, panics are expected
#![allow(missing_docs)] // Reason: test code does not require documentation

mod observer_test_helpers;

use fraiseql_server::observers::{
    ListObserverLogsQuery, ListObserversQuery, ObserverRepository, ObserverScope,
};
use observer_test_helpers::setup_observer_schema;
use sqlx::PgPool;
use uuid::Uuid;

// ---------------------------------------------------------------------------
// Harness-backed pool setup
// ---------------------------------------------------------------------------

/// Connect to the harness Postgres (Dagger-bound in CI; a local spawn with the
/// `local-testcontainers` feature), create the observer schema, and truncate the
/// observer tables so each test starts clean. The server suite runs these with
/// --test-threads=1, so the shared bound database gives per-test isolation without
/// per-test DBs. Returns the pool plus the service guard, held for the test lifetime.
async fn setup_pg() -> (PgPool, fraiseql_test_support::Service) {
    let svc = fraiseql_test_support::postgres()
        .await
        .expect("DATABASE_URL must be set (or enable fraiseql-test-support/local-testcontainers)");
    let pool = sqlx::PgPool::connect(svc.url()).await.unwrap();
    setup_observer_schema(&pool).await.unwrap();
    sqlx::query(
        "TRUNCATE core.tb_observer_log, core.tb_observer, core.observer_checkpoints, core.tb_entity_change_log \
         RESTART IDENTITY CASCADE",
    )
    .execute(&pool)
    .await
    .unwrap();
    (pool, svc)
}

// ---------------------------------------------------------------------------
// Row insertion helpers
// ---------------------------------------------------------------------------

/// Insert a minimal observer row, controlling fk_customer_org precisely.
async fn insert_observer(
    pool: &PgPool,
    name: &str,
    entity_type: &str,
    event_type: &str,
    org_id: Option<i64>,
) -> i64 {
    let row: (i64,) = sqlx::query_as(
        r"INSERT INTO core.tb_observer
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
async fn insert_log(pool: &PgPool, pk_observer: i64, status: &str, trace_id: Option<&str>) -> Uuid {
    let event_id = Uuid::new_v4();
    let entity_id = Uuid::new_v4();
    sqlx::query(
        r"INSERT INTO core.tb_observer_log
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
/// belonging to that org.
#[tokio::test]
async fn test_list_tenant_isolation() {
    let (pool, _container) = setup_pg().await;

    let id = Uuid::new_v4().to_string();
    let name_a = format!("obs-a-{id}");
    let name_b = format!("obs-b-{id}");
    insert_observer(&pool, &name_a, "Order", "INSERT", Some(1)).await;
    insert_observer(&pool, &name_b, "Payment", "INSERT", Some(2)).await;

    let repo = ObserverRepository::new(pool);
    let query = ListObserversQuery {
        page:            1,
        page_size:       50,
        include_deleted: false,
        entity_type:     None,
        event_type:      None,
        enabled:         None,
    };

    let (observers, count) = repo.list(&query, ObserverScope::Organisation(1)).await.unwrap();
    let names: Vec<&str> = observers.iter().map(|o| o.name.as_str()).collect();

    assert!(names.contains(&name_a.as_str()), "org-1 observer should appear");
    assert!(!names.contains(&name_b.as_str()), "org-2 observer must not appear");
    assert!(count >= 1);
}

/// The critical injection test: a malicious entity_type string is treated as
/// a literal value, returning 0 rows rather than leaking cross-tenant data.
#[tokio::test]
async fn test_list_injection_payload_returns_no_rows() {
    let (pool, _container) = setup_pg().await;

    let id = Uuid::new_v4().to_string();
    insert_observer(&pool, &format!("inj-1-{id}"), "Order", "INSERT", Some(1)).await;
    insert_observer(&pool, &format!("inj-2-{id}"), "Payment", "INSERT", Some(2)).await;

    let repo = ObserverRepository::new(pool);
    let query = ListObserversQuery {
        page:            1,
        page_size:       50,
        include_deleted: false,
        // Classic injection payload
        entity_type:     Some("' OR 1=1 --".to_string()),
        event_type:      None,
        enabled:         None,
    };

    // Must return 0 rows: the payload is a literal string, not SQL.
    let (observers, count) = repo.list(&query, ObserverScope::Organisation(1)).await.unwrap();
    assert_eq!(
        observers.len(),
        0,
        "injection payload must not match any rows (got {}, count={})",
        observers.len(),
        count,
    );
    assert_eq!(count, 0);
}

/// Verify that all optional list() filters narrow the result to exactly the
/// matching row.
#[tokio::test]
async fn test_list_all_filters() {
    let (pool, _container) = setup_pg().await;

    let id = Uuid::new_v4().to_string();
    let target = format!("filter-target-{id}");
    let other = format!("filter-other-{id}");
    insert_observer(&pool, &target, "Invoice", "UPDATE", Some(10)).await;
    insert_observer(&pool, &other, "Order", "INSERT", Some(10)).await;

    let repo = ObserverRepository::new(pool);
    let query = ListObserversQuery {
        page:            1,
        page_size:       50,
        include_deleted: false,
        entity_type:     Some("Invoice".to_string()),
        event_type:      Some("UPDATE".to_string()),
        enabled:         Some(true),
    };

    let (observers, count) = repo.list(&query, ObserverScope::Organisation(10)).await.unwrap();
    let names: Vec<&str> = observers.iter().map(|o| o.name.as_str()).collect();

    assert!(names.contains(&target.as_str()));
    assert!(!names.contains(&other.as_str()));
    assert_eq!(count, 1);
}

/// Verify that pagination (page 1 + page 2) covers all rows without duplicates.
#[tokio::test]
async fn test_list_pagination_correctness() {
    let (pool, _container) = setup_pg().await;

    let id = Uuid::new_v4().to_string();
    for i in 0..3 {
        insert_observer(&pool, &format!("pag-{i}-{id}"), "Widget", "INSERT", Some(99)).await;
    }

    let repo = ObserverRepository::new(pool);
    let base = ListObserversQuery {
        page:            1,
        page_size:       2,
        include_deleted: false,
        entity_type:     Some("Widget".to_string()),
        event_type:      Some("INSERT".to_string()),
        enabled:         None,
    };

    let (page1, total) = repo.list(&base, ObserverScope::Organisation(99)).await.unwrap();
    let (page2, _) = repo
        .list(&ListObserversQuery { page: 2, ..base }, ObserverScope::Organisation(99))
        .await
        .unwrap();

    assert_eq!(total, 3);
    assert_eq!(page1.len(), 2);
    assert_eq!(page2.len(), 1);

    let ids1: std::collections::HashSet<_> = page1.iter().map(|o| o.pk_observer).collect();
    let ids2: std::collections::HashSet<_> = page2.iter().map(|o| o.pk_observer).collect();
    assert!(ids1.is_disjoint(&ids2), "pages must not share rows");
}

/// Verify list_logs() filters (status, trace_id, observer_id) narrow results
/// correctly, and that a malicious trace_id does not leak rows.
#[tokio::test]
async fn test_list_logs_filters() {
    let (pool, _container) = setup_pg().await;

    let id = Uuid::new_v4().to_string();
    let pk = insert_observer(&pool, &format!("log-obs-{id}"), "Item", "DELETE", None).await;

    let trace = format!("trace-{id}");
    insert_log(&pool, pk, "success", Some(&trace)).await;
    // "failed", not "failure": `ck_observer_log_status` accepts seven values and
    // the writer emits two of them (#932). The fixture carries that CHECK now,
    // so a status no deployment could ever hold is rejected here too.
    insert_log(&pool, pk, "failed", None).await;

    let obs_uuid: (Uuid,) =
        sqlx::query_as("SELECT id FROM core.tb_observer WHERE pk_observer = $1")
            .bind(pk)
            .fetch_one(&pool)
            .await
            .unwrap();

    let repo = ObserverRepository::new(pool);

    // Filter by status — all returned rows must be "success"
    let q_status = ListObserverLogsQuery {
        page:        1,
        page_size:   50,
        observer_id: Some(obs_uuid.0),
        status:      Some("success".to_string()),
        event_id:    None,
        trace_id:    None,
    };
    let (logs, _) = repo.list_logs(&q_status, ObserverScope::Operator).await.unwrap();
    assert!(logs.iter().all(|l| l.status == "success"));
    assert_eq!(logs.len(), 1);

    // Filter by trace_id — exactly one match
    let q_trace = ListObserverLogsQuery {
        page:        1,
        page_size:   50,
        observer_id: None,
        status:      None,
        event_id:    None,
        trace_id:    Some(trace.clone()),
    };
    let (logs, count) = repo.list_logs(&q_trace, ObserverScope::Operator).await.unwrap();
    assert_eq!(count, 1);
    assert_eq!(logs[0].trace_id.as_deref(), Some(trace.as_str()));

    // Malicious trace_id must return 0 rows
    let q_inject = ListObserverLogsQuery {
        page:        1,
        page_size:   50,
        observer_id: None,
        status:      None,
        event_id:    None,
        trace_id:    Some("x' OR 1=1 --".to_string()),
    };
    let (logs_inject, cnt_inject) =
        repo.list_logs(&q_inject, ObserverScope::Operator).await.unwrap();
    assert_eq!(logs_inject.len(), 0, "injection payload must not match rows");
    assert_eq!(cnt_inject, 0);

    // Filter by observer_id — both rows visible
    let q_obs = ListObserverLogsQuery {
        page:        1,
        page_size:   50,
        observer_id: Some(obs_uuid.0),
        status:      None,
        event_id:    None,
        trace_id:    None,
    };
    let (logs_obs, cnt_obs) = repo.list_logs(&q_obs, ObserverScope::Operator).await.unwrap();
    assert_eq!(cnt_obs, 2);
    assert!(logs_obs.iter().all(|l| l.fk_observer == pk));
}

/// Verify delete() only soft-deletes when the observer belongs to the
/// specified customer_org (tenant isolation).
#[tokio::test]
async fn test_delete_tenant_isolation() {
    let (pool, _container) = setup_pg().await;

    let id = Uuid::new_v4().to_string();
    insert_observer(&pool, &format!("del-org1-{id}"), "Thing", "INSERT", Some(1)).await;
    insert_observer(&pool, &format!("del-org2-{id}"), "Thing", "INSERT", Some(2)).await;

    let (uuid_org2,): (Uuid,) = sqlx::query_as("SELECT id FROM core.tb_observer WHERE name = $1")
        .bind(format!("del-org2-{id}"))
        .fetch_one(&pool)
        .await
        .unwrap();

    let repo = ObserverRepository::new(pool.clone());

    // Attempt to delete org-2's observer while acting as org-1 — must return false
    let deleted = repo.delete(uuid_org2, ObserverScope::Organisation(1)).await.unwrap();
    assert!(!deleted, "cross-tenant delete must be rejected");

    // The row must still be alive
    let still_there: Option<(Uuid,)> =
        sqlx::query_as("SELECT id FROM core.tb_observer WHERE name = $1 AND deleted_at IS NULL")
            .bind(format!("del-org2-{id}"))
            .fetch_optional(&pool)
            .await
            .unwrap();
    assert!(still_there.is_some(), "observer must not have been deleted");
}

// ---------------------------------------------------------------------------
// Auto-reload: an observer another replica creates reaches this one
// ---------------------------------------------------------------------------

/// Start a runtime with a 1-second reload interval and `auto_reload` as given, over
/// an empty `core.tb_observer`; then insert one observer behind its back and return how
/// many observers it has loaded 3 seconds later.
async fn observers_loaded_after_an_insert(auto_reload: bool) -> usize {
    use fraiseql_server::observers::runtime::{ObserverRuntime, ObserverRuntimeConfig};

    let (pool, _svc) = setup_pg().await;
    let mut config = ObserverRuntimeConfig::new(pool.clone()).with_poll_interval(50);
    config.auto_reload = auto_reload;
    config.reload_interval_secs = 1;
    let mut runtime = ObserverRuntime::new(config);
    runtime.start().await.expect("the observer runtime starts");
    assert_eq!(runtime.health().observer_count, 0, "fixture: nothing to load at start");

    insert_observer(&pool, "created-elsewhere", "Order", "INSERT", None).await;
    tokio::time::sleep(std::time::Duration::from_secs(3)).await;

    let loaded = runtime.health().observer_count;
    runtime.stop().await.expect("the observer runtime stops");
    loaded
}

#[tokio::test]
async fn an_observer_inserted_after_start_is_loaded_within_the_reload_interval() {
    assert_eq!(
        observers_loaded_after_an_insert(true).await,
        1,
        "auto_reload must load an observer created after start within its interval"
    );
}

#[tokio::test]
async fn auto_reload_false_never_reloads() {
    assert_eq!(
        observers_loaded_after_an_insert(false).await,
        0,
        "with auto_reload off, only an explicit reload loads new observers"
    );
}

// ---------------------------------------------------------------------------
// #1893: the observer tables have one home, `core`, whatever the search_path
// ---------------------------------------------------------------------------

/// A schema an application puts first on its `search_path`.
const APP_FIRST: &str = "app_first_1893";

/// The DDL the server provisions the observer tables with at boot
/// (`capabilities/observers.rs`), applied through `pool`.
async fn apply_boot_ddl(pool: &PgPool) {
    for migration in [
        fraiseql_observers::migrations::observer_management_sql(),
        fraiseql_observers::migrations::observer_dispatch_sql(),
        fraiseql_observers::checkpoint::migration_sql(),
    ] {
        sqlx::raw_sql(migration).execute(pool).await.unwrap();
    }
}

/// A pool on the same database whose every connection resolves names through
/// `search_path`.
async fn pool_with_search_path(svc: &fraiseql_test_support::Service, search_path: &str) -> PgPool {
    let options: sqlx::postgres::PgConnectOptions = svc.url().parse().unwrap();
    sqlx::postgres::PgPoolOptions::new()
        .max_connections(2)
        .connect_with(options.options([("search_path", search_path)]))
        .await
        .unwrap()
}

/// Install the observer tables in `core`, as an application that ships them with
/// the rest of its schema does, and empty them. Returns that pool.
async fn observer_tables_in_core(svc: &fraiseql_test_support::Service) -> PgPool {
    let core_first = pool_with_search_path(svc, "core").await;
    apply_boot_ddl(&core_first).await;
    sqlx::query(
        "TRUNCATE core.tb_observer_log, core.tb_observer, core.observer_checkpoints \
         RESTART IDENTITY CASCADE",
    )
    .execute(&core_first)
    .await
    .unwrap();
    core_first
}

async fn reset_app_first_schema(pool: &PgPool) {
    sqlx::raw_sql(&format!("DROP SCHEMA IF EXISTS {APP_FIRST} CASCADE; CREATE SCHEMA {APP_FIRST}"))
        .execute(pool)
        .await
        .unwrap();
}

async fn start_runtime_and_count(pool: &PgPool) -> Result<usize, fraiseql_server::ServerError> {
    use fraiseql_server::observers::runtime::{ObserverRuntime, ObserverRuntimeConfig};

    let mut runtime = ObserverRuntime::new(ObserverRuntimeConfig::new(pool.clone()));
    runtime.start().await?;
    let loaded = runtime.health().observer_count;
    runtime.stop().await.expect("the observer runtime stops");
    Ok(loaded)
}

#[tokio::test]
async fn observers_in_core_load_when_the_search_path_starts_with_another_schema() {
    let (pool, svc) = setup_pg().await;
    reset_app_first_schema(&pool).await;
    let core_first = observer_tables_in_core(&svc).await;
    sqlx::query(
        "INSERT INTO core.tb_observer (name, entity_type, event_type, actions) \
         VALUES ('lives-in-core', 'Order', 'INSERT', '[]')",
    )
    .execute(&core_first)
    .await
    .unwrap();

    // The server boots on a database whose search_path puts an application schema
    // first: it provisions its tables, then loads.
    let app_first = pool_with_search_path(&svc, &format!("{APP_FIRST},core,public")).await;
    apply_boot_ddl(&app_first).await;
    let loaded = start_runtime_and_count(&app_first).await.expect("the runtime starts");

    let shadows: Vec<(String,)> = sqlx::query_as(
        "SELECT c.relname::text FROM pg_class c JOIN pg_namespace n ON n.oid = c.relnamespace \
         WHERE n.nspname = $1 ORDER BY 1",
    )
    .bind(APP_FIRST)
    .fetch_all(&pool)
    .await
    .unwrap();
    reset_app_first_schema(&pool).await;

    assert_eq!(loaded, 1, "the enabled observer in core.tb_observer must load");
    assert!(shadows.is_empty(), "boot must create nothing in {APP_FIRST}: {shadows:?}");
}

#[tokio::test]
async fn an_observer_created_through_the_repository_lands_in_core() {
    let (pool, svc) = setup_pg().await;
    reset_app_first_schema(&pool).await;
    let core_first = observer_tables_in_core(&svc).await;
    let app_first = pool_with_search_path(&svc, &format!("{APP_FIRST},core,public")).await;
    apply_boot_ddl(&app_first).await;

    let request: fraiseql_server::observers::CreateObserverRequest =
        serde_json::from_value(serde_json::json!({
            "name": "created-through-the-api",
            "entity_type": "Order",
            "event_type": "INSERT",
            "actions": [],
        }))
        .unwrap();
    ObserverRepository::new(app_first.clone())
        .create(&request, ObserverScope::Operator, None)
        .await
        .expect("the repository creates the observer");

    let in_core: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM core.tb_observer")
        .fetch_one(&core_first)
        .await
        .unwrap();
    reset_app_first_schema(&pool).await;

    assert_eq!(in_core.0, 1, "the created observer must be stored in core.tb_observer");
}

/// A deployment that ran an earlier release has its observers in `public`. A
/// release that reads `core` must refuse to start over an empty `core.tb_observer`
/// rather than run with none, and say how to move them.
#[tokio::test]
async fn a_populated_legacy_tb_observer_beside_an_empty_core_one_refuses_to_start() {
    let (pool, svc) = setup_pg().await;
    let core_first = observer_tables_in_core(&svc).await;
    sqlx::raw_sql(
        "CREATE TABLE IF NOT EXISTS public.tb_observer (LIKE core.tb_observer INCLUDING ALL);
         INSERT INTO public.tb_observer (name, entity_type, event_type, actions)
             VALUES ('left-behind', 'Order', 'INSERT', '[]');",
    )
    .execute(&core_first)
    .await
    .unwrap();

    let message = start_runtime_and_count(&pool)
        .await
        .expect_err("an empty core.tb_observer beside a populated legacy one")
        .to_string();
    assert!(
        message.contains("ALTER TABLE public.tb_observer SET SCHEMA core"),
        "the refusal must name the migration to run: {message}"
    );

    // The statements it names are the whole migration: run them, boot again.
    let (_, statements) = message.split_once("restart:\n").expect("the refusal lists statements");
    sqlx::raw_sql(statements).execute(&pool).await.unwrap();
    apply_boot_ddl(&pool).await;
    let loaded = start_runtime_and_count(&pool).await.expect("the runtime starts once moved");
    assert_eq!(loaded, 1, "the moved observer must load from core.tb_observer");
}

/// The same for the change-log cursor: a runtime that starts on an empty
/// `core.observer_checkpoints` while `public.observer_checkpoints` holds this
/// listener's position would replay the whole change log.
#[tokio::test]
async fn a_populated_legacy_checkpoint_table_beside_an_empty_core_one_refuses_to_start() {
    let (pool, svc) = setup_pg().await;
    let core_first = observer_tables_in_core(&svc).await;
    sqlx::query(
        "INSERT INTO core.tb_observer (name, entity_type, event_type, actions) \
         VALUES ('lives-in-core', 'Order', 'INSERT', '[]')",
    )
    .execute(&core_first)
    .await
    .unwrap();
    sqlx::raw_sql(
        "CREATE TABLE IF NOT EXISTS public.observer_checkpoints
             (LIKE core.observer_checkpoints INCLUDING ALL);
         INSERT INTO public.observer_checkpoints (listener_id, last_processed_id)
             VALUES ('change_log', 42);",
    )
    .execute(&core_first)
    .await
    .unwrap();

    let outcome = start_runtime_and_count(&pool).await;
    sqlx::raw_sql("DROP TABLE IF EXISTS public.observer_checkpoints CASCADE")
        .execute(&pool)
        .await
        .unwrap();

    let message = outcome
        .expect_err("an empty core cursor beside a populated legacy one")
        .to_string();
    assert!(
        message.contains("ALTER TABLE public.observer_checkpoints SET SCHEMA core"),
        "the refusal must name the migration to run: {message}"
    );
}

/// The refusal's whole text, with both observer tables stranded: the operator copies
/// these statements, so a change to a byte of them is a change to the migration.
#[tokio::test]
async fn the_stranded_observer_table_refusal_reads_exactly() {
    let (pool, svc) = setup_pg().await;
    let core_first = observer_tables_in_core(&svc).await;
    sqlx::raw_sql(
        "CREATE TABLE IF NOT EXISTS public.tb_observer (LIKE core.tb_observer INCLUDING ALL);
         INSERT INTO public.tb_observer (name, entity_type, event_type, actions)
             VALUES ('left-behind', 'Order', 'INSERT', '[]');
         CREATE TABLE IF NOT EXISTS public.observer_checkpoints
             (LIKE core.observer_checkpoints INCLUDING ALL);
         INSERT INTO public.observer_checkpoints (listener_id, last_processed_id)
             VALUES ('change_log', 42);",
    )
    .execute(&core_first)
    .await
    .unwrap();

    let outcome = ObserverRepository::new(pool.clone()).refuse_stranded_legacy_tables().await;
    sqlx::raw_sql(
        "DROP TABLE IF EXISTS public.tb_observer CASCADE;
         DROP TABLE IF EXISTS public.observer_checkpoints CASCADE;",
    )
    .execute(&pool)
    .await
    .unwrap();

    let message = match outcome {
        Err(fraiseql_server::ServerError::ConfigError(message)) => message,
        other => format!("expected the refusal, got {other:?}"),
    };
    assert_eq!(
        message,
        "observer tables from an earlier release hold rows outside `core`, and their `core` \
         tables are empty. FraiseQL keeps its observer tables in `core` whatever the \
         search_path (#1893); starting now would run with none of them. Move them, then \
         restart:\n  \
         BEGIN;\n  \
         DROP TABLE IF EXISTS core.tb_observer_log, core.tb_observer CASCADE;\n  \
         ALTER TABLE public.tb_observer SET SCHEMA core;\n  \
         ALTER TABLE IF EXISTS public.tb_observer_log SET SCHEMA core;\n  \
         COMMIT;\n  \
         BEGIN;\n  \
         DROP TABLE IF EXISTS core.observer_checkpoints_history, core.observer_checkpoints \
         CASCADE;\n  \
         ALTER TABLE public.observer_checkpoints SET SCHEMA core;\n  \
         ALTER TABLE IF EXISTS public.observer_checkpoints_history SET SCHEMA core;\n  \
         COMMIT;"
    );
}

/// The earlier boot created its tables in whatever schema came first on that
/// deployment's `search_path`, which need not be on today's: the check must find
/// them anywhere outside `core`.
#[tokio::test]
async fn a_populated_legacy_tb_observer_off_the_search_path_refuses_to_start() {
    let (pool, svc) = setup_pg().await;
    let core_first = observer_tables_in_core(&svc).await;
    sqlx::raw_sql(
        "DROP SCHEMA IF EXISTS old_home_1893 CASCADE;
         CREATE SCHEMA old_home_1893;
         CREATE TABLE old_home_1893.tb_observer (LIKE core.tb_observer INCLUDING ALL);
         INSERT INTO old_home_1893.tb_observer (name, entity_type, event_type, actions)
             VALUES ('left-behind', 'Order', 'INSERT', '[]');",
    )
    .execute(&core_first)
    .await
    .unwrap();

    let outcome = start_runtime_and_count(&pool).await;
    sqlx::raw_sql("DROP SCHEMA IF EXISTS old_home_1893 CASCADE")
        .execute(&pool)
        .await
        .unwrap();

    let message = outcome
        .expect_err("an empty core.tb_observer beside a populated legacy one")
        .to_string();
    assert!(
        message.contains("ALTER TABLE old_home_1893.tb_observer SET SCHEMA core"),
        "the refusal must name the migration to run: {message}"
    );
}

/// Once `core` holds the observers, a copy left elsewhere is not a reason to
/// refuse: the runtime starts and reads `core`.
#[tokio::test]
async fn a_populated_core_tb_observer_starts_whatever_copy_is_left_elsewhere() {
    let (pool, svc) = setup_pg().await;
    let core_first = observer_tables_in_core(&svc).await;
    sqlx::raw_sql(
        "INSERT INTO core.tb_observer (name, entity_type, event_type, actions)
             VALUES ('lives-in-core', 'Order', 'INSERT', '[]');
         CREATE TABLE IF NOT EXISTS public.tb_observer (LIKE core.tb_observer INCLUDING ALL);
         INSERT INTO public.tb_observer (name, entity_type, event_type, actions)
             VALUES ('stale-copy', 'Order', 'INSERT', '[]'), ('stale-copy-2', 'Order', 'INSERT', '[]');",
    )
    .execute(&core_first)
    .await
    .unwrap();

    let outcome = start_runtime_and_count(&pool).await;
    sqlx::raw_sql("DROP TABLE IF EXISTS public.tb_observer CASCADE")
        .execute(&pool)
        .await
        .unwrap();

    assert_eq!(outcome.expect("a populated core starts"), 1, "only core.tb_observer is read");
}

/// The runtime's checkpoint store reads and writes `core.observer_checkpoints` on its
/// load, save, swap and delete paths.
#[tokio::test]
async fn the_checkpoint_store_keeps_its_cursor_in_core() {
    use fraiseql_observers::checkpoint::{
        CheckpointState, CheckpointStore, PostgresCheckpointStore,
    };

    let (pool, _svc) = setup_pg().await;
    let store = PostgresCheckpointStore::new(pool.clone());
    let in_core = || async {
        sqlx::query_as::<_, (i64,)>(
            "SELECT last_processed_id FROM core.observer_checkpoints WHERE listener_id = 'cas'",
        )
        .fetch_optional(&pool)
        .await
        .unwrap()
        .map(|(id,)| id)
    };
    let state = CheckpointState {
        listener_id:       "cas".to_string(),
        last_processed_id: 5,
        last_processed_at: fraiseql_time::Timestamp::now(),
        batch_size:        1,
        event_count:       1,
    };

    store.save("cas", &state).await.unwrap();
    assert_eq!(in_core().await, Some(5));
    assert!(store.compare_and_swap("cas", 5, 7).await.unwrap(), "a current swap updates");
    assert_eq!(in_core().await, Some(7));
    assert!(!store.compare_and_swap("cas", 5, 9).await.unwrap(), "a stale swap is refused");
    store.delete("cas").await.unwrap();
    assert_eq!(in_core().await, None, "delete resets the listener");
}

// ---------------------------------------------------------------------------
// Observers the compiled schema declares are written into `core.tb_observer`
// ---------------------------------------------------------------------------

use fraiseql_core::schema::ObserverDefinition;
use fraiseql_server::{
    ServerError,
    observers::{Reconciled, UpdateObserverRequest},
};

/// A declared observer on `Order` inserts, posting to `url`.
fn declared(name: &str, url: &str) -> ObserverDefinition {
    ObserverDefinition::new(name, "Order", "INSERT")
        .with_action(serde_json::json!({"type": "webhook", "url": url}))
}

/// `(name, organisation, managed_by, enabled, first action's url)` of every live observer.
async fn live_observers(
    pool: &PgPool,
) -> Vec<(String, Option<i64>, Option<String>, bool, Option<String>)> {
    sqlx::query_as(
        "SELECT name::text, fk_customer_org, managed_by::text, enabled, actions -> 0 ->> 'url' \
         FROM core.tb_observer WHERE deleted_at IS NULL ORDER BY name, fk_customer_org NULLS FIRST",
    )
    .fetch_all(pool)
    .await
    .unwrap()
}

async fn id_of(pool: &PgPool, name: &str) -> Uuid {
    let (id,): (Uuid,) = sqlx::query_as(
        "SELECT id FROM core.tb_observer WHERE name = $1 AND fk_customer_org IS NULL \
         AND deleted_at IS NULL",
    )
    .bind(name)
    .fetch_one(pool)
    .await
    .unwrap();
    id
}

fn schema_row(
    name: &str,
    enabled: bool,
    url: &str,
) -> (String, Option<i64>, Option<String>, bool, Option<String>) {
    (name.to_owned(), None, Some("schema".to_owned()), enabled, Some(url.to_owned()))
}

#[tokio::test]
async fn a_declared_observer_is_stored_as_a_global_schema_managed_row() {
    let (pool, _svc) = setup_pg().await;
    let repo = ObserverRepository::new(pool.clone());

    let outcome = repo
        .reconcile_declared("schema", &[declared("order_created", "https://a.test")])
        .await
        .unwrap();

    assert_eq!(outcome.created, ["order_created"]);
    assert_eq!(
        live_observers(&pool).await,
        [schema_row("order_created", true, "https://a.test")]
    );
    // The runtime loads it: the stored actions and retry policy are ones it reads.
    let (stored, _) = repo
        .list(
            &ListObserversQuery {
                page:            1,
                page_size:       10,
                entity_type:     None,
                event_type:      None,
                enabled:         Some(true),
                include_deleted: false,
            },
            ObserverScope::Operator,
        )
        .await
        .unwrap();
    let actions: Vec<fraiseql_observers::ActionConfig> =
        serde_json::from_value(stored[0].actions.clone()).unwrap();
    assert_eq!(actions.len(), 1);
    serde_json::from_value::<fraiseql_observers::RetryConfig>(stored[0].retry_config.clone())
        .expect("the runtime reads the stored retry policy");
}

#[tokio::test]
async fn reconciling_the_same_declarations_again_changes_nothing() {
    let (pool, _svc) = setup_pg().await;
    let repo = ObserverRepository::new(pool.clone());
    let declarations = [declared("order_created", "https://a.test")];
    repo.reconcile_declared("schema", &declarations).await.unwrap();
    let before = id_of(&pool, "order_created").await;

    let outcome = repo.reconcile_declared("schema", &declarations).await.unwrap();

    assert_eq!(outcome, Reconciled::default());
    assert_eq!(id_of(&pool, "order_created").await, before);
}

#[tokio::test]
async fn a_changed_declaration_rewrites_its_row_in_place() {
    let (pool, _svc) = setup_pg().await;
    let repo = ObserverRepository::new(pool.clone());
    repo.reconcile_declared("schema", &[declared("order_created", "https://a.test")])
        .await
        .unwrap();
    let before = id_of(&pool, "order_created").await;

    let outcome = repo
        .reconcile_declared("schema", &[declared("order_created", "https://b.test")])
        .await
        .unwrap();

    assert_eq!(outcome.updated, ["order_created"]);
    assert_eq!(
        live_observers(&pool).await,
        [schema_row("order_created", true, "https://b.test")]
    );
    assert_eq!(id_of(&pool, "order_created").await, before, "the same row, so its log stays");
}

#[tokio::test]
async fn a_declaration_that_is_removed_soft_deletes_its_row_and_leaves_admin_rows() {
    let (pool, _svc) = setup_pg().await;
    let repo = ObserverRepository::new(pool.clone());
    insert_observer(&pool, "by_admin", "Order", "INSERT", None).await;
    repo.reconcile_declared(
        "schema",
        &[
            declared("order_created", "https://a.test"),
            declared("order_audited", "https://a.test"),
        ],
    )
    .await
    .unwrap();

    let outcome = repo
        .reconcile_declared("schema", &[declared("order_audited", "https://a.test")])
        .await
        .unwrap();

    assert_eq!(outcome.removed, ["order_created"]);
    let names: Vec<String> = live_observers(&pool).await.into_iter().map(|row| row.0).collect();
    assert_eq!(names, ["by_admin", "order_audited"]);
}

#[tokio::test]
async fn a_global_admin_row_of_the_same_name_is_adopted_and_a_tenant_row_is_untouched() {
    let (pool, _svc) = setup_pg().await;
    let repo = ObserverRepository::new(pool.clone());
    insert_observer(&pool, "order_created", "Legacy", "DELETE", None).await;
    insert_observer(&pool, "order_created", "Legacy", "DELETE", Some(7)).await;

    let outcome = repo
        .reconcile_declared("schema", &[declared("order_created", "https://a.test")])
        .await
        .unwrap();

    assert_eq!(outcome.adopted, ["order_created"]);
    assert!(outcome.created.is_empty() && outcome.updated.is_empty());
    assert_eq!(
        live_observers(&pool).await,
        [
            schema_row("order_created", true, "https://a.test"),
            ("order_created".to_owned(), Some(7), None, true, None),
        ]
    );
    let tenant_entity: (String,) =
        sqlx::query_as("SELECT entity_type::text FROM core.tb_observer WHERE fk_customer_org = 7")
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(tenant_entity.0, "Legacy");
}

#[tokio::test]
async fn an_admin_row_identical_to_the_declaration_is_still_taken_over() {
    let (pool, _svc) = setup_pg().await;
    let repo = ObserverRepository::new(pool.clone());
    let declarations = [declared("order_created", "https://a.test")];
    repo.reconcile_declared("schema", &declarations).await.unwrap();
    // The same content, as an operator who seeded it by hand would have written it.
    sqlx::query("UPDATE core.tb_observer SET managed_by = NULL")
        .execute(&pool)
        .await
        .unwrap();

    let outcome = repo.reconcile_declared("schema", &declarations).await.unwrap();

    assert_eq!(outcome.adopted, ["order_created"]);
    assert_eq!(
        live_observers(&pool).await,
        [schema_row("order_created", true, "https://a.test")]
    );
}

#[tokio::test]
async fn a_declared_observer_an_operator_disabled_stays_disabled_across_a_redeploy() {
    let (pool, _svc) = setup_pg().await;
    let repo = ObserverRepository::new(pool.clone());
    repo.reconcile_declared("schema", &[declared("order_created", "https://a.test")])
        .await
        .unwrap();
    let id = id_of(&pool, "order_created").await;
    let disable = UpdateObserverRequest {
        enabled: Some(false),
        ..Default::default()
    };
    let disabled = repo.update(id, &disable, ObserverScope::Operator, None).await.unwrap();
    assert!(
        disabled.is_some_and(|o| !o.enabled),
        "the admin API disables a declared observer"
    );

    let outcome = repo
        .reconcile_declared("schema", &[declared("order_created", "https://b.test")])
        .await
        .unwrap();

    assert_eq!(outcome.disabled, ["order_created"]);
    assert_eq!(
        live_observers(&pool).await,
        [schema_row("order_created", false, "https://b.test")]
    );
}

#[tokio::test]
async fn the_admin_api_changes_nothing_of_a_declared_observer_but_whether_it_is_enabled() {
    let (pool, _svc) = setup_pg().await;
    let repo = ObserverRepository::new(pool.clone());
    repo.reconcile_declared("schema", &[declared("order_created", "https://a.test")])
        .await
        .unwrap();
    let id = id_of(&pool, "order_created").await;

    let rename = UpdateObserverRequest {
        name: Some("renamed".to_owned()),
        enabled: Some(false),
        ..Default::default()
    };
    let refused = repo.update(id, &rename, ObserverScope::Operator, None).await;
    let deleted = repo.delete(id, ObserverScope::Operator).await;

    for refusal in [refused.map(|_| ()), deleted.map(|_| ())] {
        let message = match refusal {
            Err(ServerError::Conflict(message)) => message,
            other => format!("not a conflict: {other:?}"),
        };
        assert!(message.contains("'order_created'"), "names the observer: {message}");
        assert!(message.contains("declared in the schema"), "says why: {message}");
    }
    assert_eq!(
        live_observers(&pool).await,
        [schema_row("order_created", true, "https://a.test")]
    );
}

#[tokio::test]
async fn an_organisation_is_told_nothing_of_a_declared_observer_it_cannot_change() {
    let (pool, _svc) = setup_pg().await;
    let repo = ObserverRepository::new(pool.clone());
    repo.reconcile_declared("schema", &[declared("order_created", "https://a.test")])
        .await
        .unwrap();
    let id = id_of(&pool, "order_created").await;

    let deleted = repo.delete(id, ObserverScope::Organisation(7)).await.unwrap();

    assert!(
        !deleted,
        "a global observer is not the organisation's: not found, not a conflict"
    );
}

#[tokio::test]
async fn an_admin_observer_is_still_changed_and_deleted_through_the_admin_api() {
    let (pool, _svc) = setup_pg().await;
    let repo = ObserverRepository::new(pool.clone());
    insert_observer(&pool, "by_admin", "Order", "INSERT", None).await;
    let id = id_of(&pool, "by_admin").await;

    let rename = UpdateObserverRequest {
        name: Some("renamed".to_owned()),
        ..Default::default()
    };
    let renamed = repo.update(id, &rename, ObserverScope::Operator, None).await.unwrap();
    assert_eq!(renamed.map(|o| o.name).as_deref(), Some("renamed"));
    assert!(repo.delete(id, ObserverScope::Operator).await.unwrap());
}

#[tokio::test]
async fn two_owners_on_one_database_keep_their_own_declared_observers() {
    let (pool, _svc) = setup_pg().await;
    let repo = ObserverRepository::new(pool.clone());
    repo.reconcile_declared("schema:billing", &[declared("invoice_sent", "https://a.test")])
        .await
        .unwrap();

    let first_boot = repo
        .reconcile_declared("schema:shipping", &[declared("parcel_sent", "https://b.test")])
        .await
        .unwrap();
    // Shipping drops its only observer: billing's is not shipping's to remove.
    let second_boot = repo.reconcile_declared("schema:shipping", &[]).await.unwrap();

    assert!(first_boot.removed.is_empty(), "shipping removed {:?}", first_boot.removed);
    assert_eq!(second_boot.removed, ["parcel_sent"]);
    assert_eq!(
        second_boot.removed_with_owner("schema:shipping").collect::<Vec<_>>(),
        [("parcel_sent", "schema:shipping")]
    );
    let owners: Vec<(String, Option<String>)> =
        live_observers(&pool).await.into_iter().map(|row| (row.0, row.2)).collect();
    assert_eq!(owners, [("invoice_sent".to_owned(), Some("schema:billing".to_owned()))]);
}

#[tokio::test]
async fn a_name_another_owner_declares_refuses_the_boot_naming_both_and_writes_nothing() {
    let (pool, _svc) = setup_pg().await;
    let repo = ObserverRepository::new(pool.clone());
    repo.reconcile_declared("schema:billing", &[declared("order_created", "https://a.test")])
        .await
        .unwrap();

    let refused = repo
        .reconcile_declared(
            "schema:shipping",
            &[
                declared("parcel_sent", "https://b.test"),
                declared("order_created", "https://b.test"),
            ],
        )
        .await;

    let message = match refused {
        Err(ServerError::ConfigError(message)) => message,
        other => format!("not a configuration error: {other:?}"),
    };
    assert!(message.contains("`order_created`"), "names the observer: {message}");
    assert!(
        message.contains("project `billing`") && message.contains("project `shipping`"),
        "names both projects: {message}"
    );
    assert!(
        message.contains("[project] name"),
        "names the key that sets the owner: {message}"
    );
    let stored: Vec<(String, Option<String>, Option<String>)> = live_observers(&pool)
        .await
        .into_iter()
        .map(|row| (row.0, row.2, row.4))
        .collect();
    assert_eq!(
        stored,
        [(
            "order_created".to_owned(),
            Some("schema:billing".to_owned()),
            Some("https://a.test".to_owned())
        )],
        "billing's observer is as it was, and shipping's first one was not stored"
    );
}

#[tokio::test]
async fn two_replicas_booting_together_store_one_set_of_declared_observers() {
    let (pool, _svc) = setup_pg().await;
    let declarations: Vec<ObserverDefinition> =
        (0..80).map(|n| declared(&format!("observer_{n}"), "https://a.test")).collect();
    // Open every connection first: a replica that had to connect while another wrote
    // would start after it finished, and the two would never overlap.
    let mut warm = Vec::new();
    for _ in 0..6 {
        warm.push(pool.acquire().await.unwrap());
    }
    drop(warm);

    // Collected, so every replica is running before the first is awaited.
    let replicas: Vec<_> = (0..6)
        .map(|_| {
            let repo = ObserverRepository::new(pool.clone());
            let declarations = declarations.clone();
            tokio::spawn(async move { repo.reconcile_declared("schema", &declarations).await })
        })
        .collect();
    let mut created = 0;
    for replica in replicas {
        created += replica.await.unwrap().expect("no replica fails").created.len();
    }

    assert_eq!(created, 80, "each observer is created by exactly one replica");
    assert_eq!(live_observers(&pool).await.len(), 80);
}

/// A project name of any length: the owner column an earlier release created held 50
/// characters, and `schema:` takes seven of them.
const LONG_OWNER: &str =
    "schema:a-project-whose-name-is-longer-than-the-forty-three-characters-left";

#[tokio::test]
async fn an_owner_longer_than_the_first_column_width_is_stored_whole() {
    let (pool, _svc) = setup_pg().await;
    let repo = ObserverRepository::new(pool.clone());
    let very_long = format!("schema:{}", "n".repeat(400));

    for (name, owner) in [("long", LONG_OWNER), ("very_long", very_long.as_str())] {
        let stored = repo.reconcile_declared(owner, &[declared(name, "https://a.test")]).await;
        assert!(stored.is_ok(), "{} characters: {stored:?}", owner.len());
    }

    let owners: Vec<Option<String>> =
        live_observers(&pool).await.into_iter().map(|row| row.2).collect();
    assert_eq!(owners, [Some(LONG_OWNER.to_owned()), Some(very_long)]);
}

#[tokio::test]
async fn a_table_an_earlier_release_created_gains_the_wide_owner_columns_at_boot() {
    let (pool, svc) = setup_pg().await;
    // The shape the earlier release left, the statistics view over it included.
    sqlx::raw_sql(
        "ALTER TABLE core.tb_observer ALTER COLUMN managed_by TYPE VARCHAR(50),
             ALTER COLUMN created_by TYPE VARCHAR(255), ALTER COLUMN updated_by TYPE VARCHAR(255)",
    )
    .execute(&pool)
    .await
    .unwrap();
    // On connections of its own: a statement prepared against the narrow column is not
    // one a server booted after the change holds.
    let earlier = pool_with_search_path(&svc, "public").await;
    let narrow = ObserverRepository::new(earlier.clone())
        .reconcile_declared(LONG_OWNER, &[declared("order_created", "https://a.test")])
        .await;
    earlier.close().await;
    assert!(
        matches!(&narrow, Err(ServerError::Database(e)) if e.contains("too long")),
        "the earlier shape holds a long owner: {narrow:?}"
    );
    let repo = ObserverRepository::new(pool.clone());

    apply_boot_ddl(&pool).await;
    apply_boot_ddl(&pool).await;

    let widths: Vec<(String, String)> = sqlx::query_as(
        "SELECT column_name::text, data_type::text FROM information_schema.columns \
         WHERE table_schema = 'core' AND table_name = 'tb_observer' \
           AND column_name IN ('managed_by', 'created_by', 'updated_by') ORDER BY 1",
    )
    .fetch_all(&pool)
    .await
    .unwrap();
    assert_eq!(
        widths,
        ["created_by", "managed_by", "updated_by"].map(|c| (c.to_owned(), "text".to_owned()))
    );
    repo.reconcile_declared(LONG_OWNER, &[declared("order_created", "https://a.test")])
        .await
        .unwrap();
}

#[tokio::test]
async fn the_first_named_boot_takes_over_the_unnamed_owners_rows_it_still_declares() {
    let (pool, _svc) = setup_pg().await;
    let repo = ObserverRepository::new(pool.clone());
    repo.reconcile_declared(
        "schema",
        &[
            declared("order_created", "https://a.test"),
            declared("order_audited", "https://a.test"),
        ],
    )
    .await
    .unwrap();
    let id = id_of(&pool, "order_created").await;
    sqlx::query("UPDATE core.tb_observer SET enabled = false WHERE id = $1")
        .bind(id)
        .execute(&pool)
        .await
        .unwrap();

    let named = repo
        .reconcile_declared("schema:billing", &[declared("order_created", "https://a.test")])
        .await
        .unwrap();

    assert_eq!(named.taken_over, ["order_created"], "{named:?}");
    assert_eq!(named.removed_unnamed, ["order_audited"], "{named:?}");
    assert_eq!(
        named.removed_with_owner("schema:billing").collect::<Vec<_>>(),
        [("order_audited", "schema")],
        "the removal is reported under the owner that held the row"
    );
    assert!(named.created.is_empty() && named.removed.is_empty(), "{named:?}");
    assert_eq!(named.disabled, ["order_created"], "{named:?}");
    assert_eq!(id_of(&pool, "order_created").await, id, "the row is kept, with its id");
    let stored: Vec<(String, Option<String>, bool)> = live_observers(&pool)
        .await
        .into_iter()
        .map(|row| (row.0, row.2, row.3))
        .collect();
    assert_eq!(
        stored,
        [("order_created".to_owned(), Some("schema:billing".to_owned()), false)],
        "taken over in place as the operator left it; the undeclared one is gone"
    );
}

#[tokio::test]
async fn a_named_owner_never_takes_over_another_named_owners_row() {
    let (pool, _svc) = setup_pg().await;
    let repo = ObserverRepository::new(pool.clone());
    repo.reconcile_declared("schema:billing", &[declared("invoice_sent", "https://a.test")])
        .await
        .unwrap();

    let other = repo.reconcile_declared("schema:shipping", &[]).await.unwrap();

    assert!(other.removed.is_empty() && other.removed_unnamed.is_empty(), "{other:?}");
    assert_eq!(live_observers(&pool).await.len(), 1);
}

// ---------------------------------------------------------------------------
// The retry policy a row takes when none is given
// ---------------------------------------------------------------------------

/// The policy the admin API and a declared observer store when none is given.
fn stored_policy() -> serde_json::Value {
    serde_json::to_value(fraiseql_server::observers::RetryConfig::default()).unwrap()
}

/// The default an earlier release gave the column: a document the runtime refuses.
const UNREADABLE_DEFAULT: &str =
    r#"{"max_attempts": 3, "backoff": "exponential", "initial_delay_ms": 1000}"#;

async fn retry_config_column_default(pool: &PgPool) -> serde_json::Value {
    let (expression,): (String,) = sqlx::query_as(
        "SELECT column_default::text FROM information_schema.columns \
         WHERE table_schema = 'core' AND table_name = 'tb_observer' \
           AND column_name = 'retry_config'",
    )
    .fetch_one(pool)
    .await
    .expect("retry_config has a default");
    assert!(
        expression.starts_with('\'') && expression.ends_with("'::jsonb"),
        "the default is one jsonb literal: {expression}"
    );
    let document = &expression[1..expression.len() - "'::jsonb".len()];
    serde_json::from_str(document).unwrap()
}

async fn retry_configs_by_name(pool: &PgPool) -> Vec<(String, serde_json::Value)> {
    sqlx::query_as("SELECT name::text, retry_config FROM core.tb_observer ORDER BY name")
        .fetch_all(pool)
        .await
        .unwrap()
}

#[tokio::test]
async fn the_retry_config_column_default_is_the_policy_the_server_stores() {
    let (pool, _svc) = setup_pg().await;

    let column_default = retry_config_column_default(&pool).await;

    assert_eq!(column_default, stored_policy());
    let read = serde_json::from_value::<fraiseql_observers::RetryConfig>(column_default)
        .expect("the runtime reads the column default");
    assert_eq!(
        (
            read.max_attempts,
            read.initial_delay_ms,
            read.max_delay_ms,
            read.backoff_strategy
        ),
        (3, 1000, 60000, fraiseql_observers::BackoffStrategy::Exponential),
        "the runtime runs the policy the document states, not its own fallback"
    );
}

#[tokio::test]
async fn a_row_inserted_without_a_retry_policy_holds_one_the_runtime_reads() {
    let (pool, _svc) = setup_pg().await;
    sqlx::query(
        r#"INSERT INTO core.tb_observer (name, entity_type, event_type, actions)
           VALUES ('by_sql', 'Order', 'INSERT',
                   '[{"type": "webhook", "url": "https://hooks.test/order"}]')"#,
    )
    .execute(&pool)
    .await
    .unwrap();

    let stored = retry_configs_by_name(&pool).await;

    assert_eq!(stored, [("by_sql".to_owned(), stored_policy())]);
    serde_json::from_value::<fraiseql_observers::RetryConfig>(stored[0].1.clone())
        .expect("the runtime reads the row's retry policy");
    assert_eq!(start_runtime_and_count(&pool).await.unwrap(), 1);
}

#[tokio::test]
async fn a_table_an_earlier_release_created_takes_the_policy_and_so_do_the_rows_no_one_wrote() {
    let (pool, _svc) = setup_pg().await;
    // The shape the earlier release left: its default, a row that took it, and two rows
    // someone wrote, one of them as unreadable as the default.
    sqlx::raw_sql(&format!(
        r#"ALTER TABLE core.tb_observer ALTER COLUMN retry_config SET DEFAULT '{UNREADABLE_DEFAULT}'::jsonb;
           INSERT INTO core.tb_observer (name, actions) VALUES ('took_the_default', '[]');
           INSERT INTO core.tb_observer (name, actions, retry_config) VALUES
             ('wrote_backoff', '[]', '{{"max_attempts": 9, "backoff": "exponential", "initial_delay_ms": 1000}}'),
             ('wrote_a_policy', '[]', '{{"max_attempts": 5, "backoff_strategy": "fixed", "initial_delay_ms": 250, "max_delay_ms": 250}}');"#
    ))
    .execute(&pool)
    .await
    .unwrap();
    let unreadable: serde_json::Value = serde_json::from_str(UNREADABLE_DEFAULT).unwrap();
    assert_eq!(retry_config_column_default(&pool).await, unreadable);
    let before = retry_configs_by_name(&pool).await;
    assert_eq!(before[0], ("took_the_default".to_owned(), unreadable));

    apply_boot_ddl(&pool).await;
    let after_one_boot = retry_configs_by_name(&pool).await;
    apply_boot_ddl(&pool).await;

    assert_eq!(retry_config_column_default(&pool).await, stored_policy());
    assert_eq!(
        after_one_boot,
        [
            ("took_the_default".to_owned(), stored_policy()),
            before[1].clone(),
            before[2].clone(),
        ]
    );
    assert_eq!(
        retry_configs_by_name(&pool).await,
        after_one_boot,
        "a second boot changes nothing"
    );
}

// ---------------------------------------------------------------------------
// The checkpoint store's compare-and-swap
// ---------------------------------------------------------------------------

/// A listener's first compare-and-swap, from no row, stores the cursor.
#[tokio::test]
async fn first_compare_and_swap_creates_the_checkpoint() {
    use fraiseql_observers::checkpoint::{CheckpointStore as _, PostgresCheckpointStore};

    let (pool, _svc) = setup_pg().await;
    let store = PostgresCheckpointStore::new(pool.clone());

    assert!(store.compare_and_swap("new", 0, 5).await.expect("the insert is accepted"));

    let row: (i64, i32, i32) = sqlx::query_as(
        "SELECT last_processed_id, batch_size, event_count FROM core.observer_checkpoints \
         WHERE listener_id = 'new'",
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(row, (5, 100, 0));
    assert!(store.compare_and_swap("new", 5, 6).await.unwrap(), "and the next one moves it");
}

/// Two first compare-and-swaps race: exactly one creates the row.
#[tokio::test]
async fn of_two_first_compare_and_swaps_one_wins() {
    use fraiseql_observers::checkpoint::{CheckpointStore as _, PostgresCheckpointStore};

    let (pool, _svc) = setup_pg().await;
    let store = PostgresCheckpointStore::new(pool.clone());

    let (a, b) =
        tokio::join!(store.compare_and_swap("race", 0, 3), store.compare_and_swap("race", 0, 4));
    let (a, b) = (a.expect("accepted"), b.expect("accepted"));

    assert!(a ^ b, "exactly one wins: {a} {b}");
    let stored: i64 = sqlx::query_scalar(
        "SELECT last_processed_id FROM core.observer_checkpoints WHERE listener_id = 'race'",
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(stored, if a { 3 } else { 4 });
}

/// The default state is one the table accepts.
#[tokio::test]
async fn a_default_checkpoint_state_saves() {
    use fraiseql_observers::checkpoint::{
        CheckpointState, CheckpointStore as _, PostgresCheckpointStore,
    };

    let (pool, _svc) = setup_pg().await;
    let store = PostgresCheckpointStore::new(pool.clone());
    let state = CheckpointState {
        listener_id: "default".to_string(),
        ..CheckpointState::default()
    };

    store
        .save("default", &state)
        .await
        .expect("the table accepts the default state");

    let loaded = store.load("default").await.unwrap().expect("saved");
    assert_eq!((loaded.last_processed_id, loaded.batch_size), (0, 100));
}

/// A runtime over `pool` that is never started: what a reload reconciles through.
fn reloading_runtime(pool: &PgPool) -> fraiseql_server::observers::ObserverRuntime {
    fraiseql_server::observers::ObserverRuntime::new(
        fraiseql_server::observers::ObserverRuntimeConfig::new(pool.clone())
            .with_deployment(fraiseql_guard::deployment::Deployment::Development),
    )
}

/// Replicas reload one edited schema at their own moments, each on its own connections: the
/// reconcile serializes them, so each round rewrites each row once, stores each added
/// observer once (a name with no row yet has nothing for `FOR UPDATE` to lock: the
/// advisory lock is what holds the second replica back), and the rows end as the schema
/// declares them, whichever replica ran first.
#[tokio::test]
async fn two_replicas_reloading_an_edited_schema_rewrite_each_row_once() {
    let (pool, svc) = setup_pg().await;
    let replicas = [
        PgPool::connect(svc.url()).await.unwrap(),
        PgPool::connect(svc.url()).await.unwrap(),
    ];
    let names: Vec<String> = (0..40).map(|n| format!("observer_{n}")).collect();
    // Each round adds twenty observers and edits every other one. The added ones come
    // first: a row that exists is locked by `FOR UPDATE`, and the first one would hold the
    // second replica back on its own.
    let version = |round: usize| -> Vec<ObserverDefinition> {
        (0..round * 20)
            .map(|added| format!("added_{added}"))
            .chain(names.iter().cloned())
            .map(|name| declared(&name, &format!("https://v{round}.test")))
            .collect()
    };
    ObserverRepository::new(pool.clone())
        .reconcile_declared("schema:app", &version(0))
        .await
        .unwrap();

    for round in 1..=15 {
        let barrier = std::sync::Arc::new(tokio::sync::Barrier::new(2));
        let reloads: Vec<_> = replicas
            .iter()
            .map(|replica| {
                let runtime = reloading_runtime(replica);
                let declared = version(round);
                let barrier = std::sync::Arc::clone(&barrier);
                tokio::spawn(async move {
                    barrier.wait().await;
                    runtime.reconcile_and_prepare("schema:app", &declared).await
                })
            })
            .collect();
        let (mut created, mut updated) = (0, 0);
        for reload in reloads {
            let (outcome, _prepared) = reload.await.unwrap().expect("no replica's reload fails");
            assert!(outcome.removed.is_empty(), "{outcome:?}");
            created += outcome.created.len();
            updated += outcome.updated.len();
        }
        assert_eq!(created, 20, "round {round}: each added observer is stored by one replica");
        assert_eq!(
            updated,
            40 + (round - 1) * 20,
            "round {round}: each row is rewritten by exactly one replica"
        );
        let urls: Vec<Option<String>> =
            live_observers(&pool).await.into_iter().map(|row| row.4).collect();
        assert_eq!(
            urls,
            vec![Some(format!("https://v{round}.test")); 40 + round * 20],
            "round {round}"
        );
    }
}

/// A reload that drops a declared observer soft-deletes its row, as boot does, and builds
/// the runtime without it; one an operator disabled stays disabled across the edit.
#[tokio::test]
async fn a_reload_that_drops_a_declared_observer_soft_deletes_its_row() {
    let (pool, _svc) = setup_pg().await;
    let runtime = reloading_runtime(&pool);
    let repo = ObserverRepository::new(pool.clone());
    repo.reconcile_declared(
        "schema:app",
        &[
            declared("kept", "https://a.test"),
            declared("paused", "https://a.test"),
            declared("dropped", "https://a.test"),
        ],
    )
    .await
    .unwrap();
    sqlx::query("UPDATE core.tb_observer SET enabled = false WHERE name = 'paused'")
        .execute(&pool)
        .await
        .unwrap();

    let (outcome, _prepared) = runtime
        .reconcile_and_prepare(
            "schema:app",
            &[
                declared("kept", "https://b.test"),
                declared("paused", "https://b.test"),
            ],
        )
        .await
        .unwrap();

    assert_eq!(outcome.removed, ["dropped"]);
    assert_eq!(outcome.disabled, ["paused"]);
    let live: Vec<(String, bool, Option<String>)> = live_observers(&pool)
        .await
        .into_iter()
        .map(|row| (row.0, row.3, row.4))
        .collect();
    assert_eq!(
        live,
        [
            ("kept".to_owned(), true, Some("https://b.test".to_owned())),
            ("paused".to_owned(), false, Some("https://b.test".to_owned())),
        ]
    );
    let (deleted,): (i64,) = sqlx::query_as(
        "SELECT count(*) FROM core.tb_observer WHERE name = 'dropped' AND deleted_at IS NOT NULL",
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(deleted, 1, "the dropped observer's row is kept, soft-deleted");
}

/// In production, a reload declaring an observer that reads an unset variable is refused
/// before anything is written, as the boot is.
#[tokio::test]
async fn a_production_reload_declaring_an_unset_variable_writes_nothing() {
    let (pool, _svc) = setup_pg().await;
    ObserverRepository::new(pool.clone())
        .reconcile_declared("schema:app", &[declared("order_created", "https://a.test")])
        .await
        .unwrap();
    let runtime = fraiseql_server::observers::ObserverRuntime::new(
        fraiseql_server::observers::ObserverRuntimeConfig::new(pool.clone())
            .with_deployment(fraiseql_guard::deployment::Deployment::Production),
    );
    let reading_unset = ObserverDefinition::new("order_created", "Order", "INSERT").with_action(
        serde_json::json!({"type": "webhook", "url_env": "FRAISEQL_TEST_UNSET_HOOK_URL_2037"}),
    );

    let refused = runtime.reconcile_and_prepare("schema:app", &[reading_unset]).await;

    let message = match refused {
        Err(ServerError::ConfigError(message)) => message,
        other => format!("not a configuration error: {:?}", other.map(|(outcome, _)| outcome)),
    };
    assert!(message.contains("FRAISEQL_TEST_UNSET_HOOK_URL_2037"), "{message}");
    let urls: Vec<Option<String>> =
        live_observers(&pool).await.into_iter().map(|row| row.4).collect();
    assert_eq!(urls, [Some("https://a.test".to_owned())], "the stored row is as it was");
}
