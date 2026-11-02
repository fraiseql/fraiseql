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
        last_processed_at: chrono::Utc::now(),
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
