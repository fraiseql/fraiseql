//! External-write capture trigger tests (#366 + `changelog_pre_image`).
//!
//! Proves `migrations/11_create_change_log_capture_trigger.sql`
//! (`fraiseql_observers::migrations::entity_change_log_capture_trigger_sql`):
//!
//! * a raw external write (no `fraiseql.cdc_mediated` marker) is captured as a contract-conforming
//!   `core.tb_entity_change_log` row whose `object_data` is the **after-image** (uniform with the
//!   executor outbox — NOT a `{op,before,after}` envelope);
//! * an app-mediated write (marker `= 'on'`) is **suppressed** — no duplicate;
//! * a bulk statement captures all its rows in one set-based INSERT;
//! * with the per-table pre-image opt-in (`TG_ARGV[3] = 'true'`), UPDATE/DELETE also record the
//!   pre-image (OLD) into `object_data_before`; without it, only the after-image is captured;
//! * per-tenant `tenant_id` is stamped from the configured column;
//! * a non-UUID PK is skipped (the poller decodes `object_id` as a non-null `uuid`, so a bad value
//!   would otherwise stall the whole batch).
//!
//! ## Running
//!
//! These tests share `core.tb_entity_change_log`, so run them serially:
//!
//! ```bash
//! DATABASE_URL=postgresql://fraiseql_test:fraiseql_test_password@localhost:5433/test_fraiseql \
//!   cargo test -p fraiseql-observers --test capture_trigger \
//!   -- --ignored --test-threads=1
//! ```

#![cfg(feature = "postgres")]
#![allow(clippy::unwrap_used)] // Reason: integration test file

use fraiseql_observers::{
    event::EventKind,
    listener::{ChangeLogListener, ChangeLogListenerConfig},
    migrations::{entity_change_log_capture_trigger_sql, entity_change_log_contract_sql},
};
use fraiseql_test_utils::database_url;
use serde_json::Value;
use sqlx::{PgPool, Row, postgres::PgPoolOptions};

async fn pool() -> PgPool {
    PgPoolOptions::new()
        .max_connections(2)
        .connect(&database_url())
        .await
        .expect("connect to test database")
}

/// Install the contract table + the capture function, a fresh source table, and
/// its three statement-level transition-table triggers (exactly the shape the
/// `fraiseql generate capture-triggers` DDL generator emits). Truncates the
/// shared change-log so each test starts clean. When `pre_image` is set, the
/// triggers pass `'true'` as `TG_ARGV[3]` so the capture function records OLD
/// into `object_data_before` (the `changelog_pre_image` out-of-band parity).
async fn setup(pool: &PgPool, src: &str, pk: &str, tenant: &str, pre_image: bool) {
    sqlx::raw_sql(entity_change_log_contract_sql()).execute(pool).await.unwrap();
    sqlx::raw_sql(entity_change_log_capture_trigger_sql())
        .execute(pool)
        .await
        .unwrap();
    sqlx::query("TRUNCATE core.tb_entity_change_log").execute(pool).await.unwrap();

    sqlx::query(&format!("DROP TABLE IF EXISTS {src} CASCADE"))
        .execute(pool)
        .await
        .unwrap();
    sqlx::query(&format!(
        "CREATE TABLE {src} (
            {pk}      UUID PRIMARY KEY DEFAULT gen_random_uuid(),
            {tenant}  UUID,
            name      TEXT
        )"
    ))
    .execute(pool)
    .await
    .unwrap();

    // Opt-in tables pass the pre-image flag as the 4th capture-function arg.
    let pre_image_arg = if pre_image { ", 'true'" } else { "" };
    for (suffix, when, referencing) in [
        ("ins", "INSERT", "NEW TABLE AS new_table"),
        ("upd", "UPDATE", "OLD TABLE AS old_table NEW TABLE AS new_table"),
        ("del", "DELETE", "OLD TABLE AS old_table"),
    ] {
        sqlx::query(&format!(
            "CREATE TRIGGER tr_cdc_capture_{suffix} AFTER {when} ON {src}
             REFERENCING {referencing} FOR EACH STATEMENT
             EXECUTE FUNCTION core.fn_entity_change_log_capture('Post', '{pk}', '{tenant}'{pre_image_arg})"
        ))
        .execute(pool)
        .await
        .unwrap();
    }
}

/// A captured change-log row: type, verb, the after-image (`object_data`), the
/// pre-image (`object_data_before`), tenant text, and the source marker.
struct Captured {
    object_type:        String,
    modification_type:  String,
    object_data:        Value,
    object_data_before: Value,
    tenant_text:        Option<String>,
    extra_metadata:     Value,
}

async fn changelog_rows(pool: &PgPool) -> Vec<Captured> {
    sqlx::query(
        "SELECT object_type, modification_type, object_data, object_data_before,
                tenant_id::text AS tenant_text, extra_metadata
         FROM core.tb_entity_change_log ORDER BY pk_entity_change_log",
    )
    .fetch_all(pool)
    .await
    .unwrap()
    .into_iter()
    .map(|r| Captured {
        object_type:        r.get::<String, _>("object_type"),
        modification_type:  r.get::<String, _>("modification_type"),
        // object_data / object_data_before are nullable → decode as Option then
        // normalise to Value::Null so assertions read uniformly.
        object_data:        r.get::<Option<Value>, _>("object_data").unwrap_or(Value::Null),
        object_data_before: r.get::<Option<Value>, _>("object_data_before").unwrap_or(Value::Null),
        tenant_text:        r.get::<Option<String>, _>("tenant_text"),
        extra_metadata:     r.get::<Value, _>("extra_metadata"),
    })
    .collect()
}

#[tokio::test]
#[ignore = "requires PostgreSQL — run with --ignored --test-threads=1"]
async fn external_insert_is_captured_as_the_after_image() {
    let pool = pool().await;
    setup(&pool, "public.tc_post", "id", "tenant_id", false).await;

    let tenant = "11111111-1111-1111-1111-111111111111";
    sqlx::query(&format!(
        "INSERT INTO public.tc_post (tenant_id, name) VALUES ('{tenant}', 'hello')"
    ))
    .execute(&pool)
    .await
    .unwrap();

    let rows = changelog_rows(&pool).await;
    assert_eq!(rows.len(), 1, "external INSERT produces exactly one change-log row");
    let row = &rows[0];
    assert_eq!(row.object_type, "Post", "object_type is the GraphQL type name (TG_ARGV[0])");
    assert_eq!(row.modification_type, "INSERT");
    // object_data is the after-image directly — not a {op,before,after} envelope.
    assert_eq!(
        row.object_data["name"], "hello",
        "object_data carries the new row (after-image)"
    );
    assert!(row.object_data.get("op").is_none(), "no inline Debezium op key");
    assert!(row.object_data_before.is_null(), "an insert has no before-image");
    assert_eq!(row.tenant_text.as_deref(), Some(tenant), "tenant_id stamped from the column");
    assert_eq!(row.extra_metadata["cdc_source"], "fallback_trigger", "marked as a captured row");
}

#[tokio::test]
#[ignore = "requires PostgreSQL — run with --ignored --test-threads=1"]
async fn app_mediated_write_is_suppressed() {
    let pool = pool().await;
    setup(&pool, "public.tc_post", "id", "tenant_id", false).await;

    // Mimic the executor: set the marker transaction-locally, then write.
    let mut tx = pool.begin().await.unwrap();
    sqlx::query("SELECT set_config('fraiseql.cdc_mediated', 'on', true)")
        .execute(&mut *tx)
        .await
        .unwrap();
    sqlx::query("INSERT INTO public.tc_post (name) VALUES ('app-write')")
        .execute(&mut *tx)
        .await
        .unwrap();
    tx.commit().await.unwrap();

    assert!(
        changelog_rows(&pool).await.is_empty(),
        "the marker suppresses the fallback trigger — no duplicate row"
    );
}

#[tokio::test]
#[ignore = "requires PostgreSQL — run with --ignored --test-threads=1"]
async fn bulk_insert_captures_every_row_in_one_statement() {
    let pool = pool().await;
    setup(&pool, "public.tc_post", "id", "tenant_id", false).await;

    // One statement, 500 rows → 500 change-log rows via a single set-based INSERT.
    sqlx::query(
        "INSERT INTO public.tc_post (name)
         SELECT 'row-' || g FROM generate_series(1, 500) g",
    )
    .execute(&pool)
    .await
    .unwrap();

    let rows = changelog_rows(&pool).await;
    assert_eq!(rows.len(), 500, "every bulk-inserted row is captured");
    assert!(
        rows.iter()
            .all(|r| r.modification_type == "INSERT" && r.object_data["name"].is_string())
    );
}

#[tokio::test]
#[ignore = "requires PostgreSQL — run with --ignored --test-threads=1"]
async fn opted_in_update_and_delete_record_the_pre_image() {
    let pool = pool().await;
    // pre_image = true → OLD reaches object_data_before for UPDATE/DELETE.
    setup(&pool, "public.tc_post", "id", "tenant_id", true).await;

    let id = "22222222-2222-2222-2222-222222222222";
    sqlx::query(&format!("INSERT INTO public.tc_post (id, name) VALUES ('{id}', 'v1')"))
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query(&format!("UPDATE public.tc_post SET name = 'v2' WHERE id = '{id}'"))
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query(&format!("DELETE FROM public.tc_post WHERE id = '{id}'"))
        .execute(&pool)
        .await
        .unwrap();

    let rows = changelog_rows(&pool).await;
    assert_eq!(rows.len(), 3, "insert + update + delete each capture one row");

    // UPDATE: object_data = after (NEW), object_data_before = before (OLD).
    let upd = &rows[1];
    assert_eq!(upd.modification_type, "UPDATE");
    assert_eq!(upd.object_data["name"], "v2", "update after = new row");
    assert_eq!(upd.object_data_before["name"], "v1", "update before = old row (opted in)");

    // DELETE: object_data = NULL (no after), object_data_before = OLD.
    let del = &rows[2];
    assert_eq!(del.modification_type, "DELETE");
    assert!(del.object_data.is_null(), "delete has no after-image");
    assert_eq!(
        del.object_data_before["name"], "v2",
        "delete before = last row state (opted in)"
    );
}

#[tokio::test]
#[ignore = "requires PostgreSQL — run with --ignored --test-threads=1"]
async fn non_opted_in_update_captures_after_image_only() {
    let pool = pool().await;
    // pre_image = false (default) → no object_data_before, even for UPDATE.
    setup(&pool, "public.tc_post", "id", "tenant_id", false).await;

    let id = "55555555-5555-5555-5555-555555555555";
    sqlx::query(&format!("INSERT INTO public.tc_post (id, name) VALUES ('{id}', 'v1')"))
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query(&format!("UPDATE public.tc_post SET name = 'v2' WHERE id = '{id}'"))
        .execute(&pool)
        .await
        .unwrap();

    let rows = changelog_rows(&pool).await;
    let upd = &rows[1];
    assert_eq!(upd.modification_type, "UPDATE");
    assert_eq!(upd.object_data["name"], "v2", "after-image still captured");
    assert!(
        upd.object_data_before.is_null(),
        "no pre-image without the per-table opt-in (cost paid only where wanted)"
    );
}

#[tokio::test]
#[ignore = "requires PostgreSQL — run with --ignored --test-threads=1"]
async fn captured_row_decodes_into_a_subscriber_event() {
    // The real acceptance proof: a raw external write flows through the shipped
    // ChangeLogListener (poller) into an EntityEvent ready for subscription
    // fan-out — i.e. the trigger's after-image matches the reader's contract.
    let pool = pool().await;
    setup(&pool, "public.tc_post", "id", "tenant_id", false).await;

    let id = "33333333-3333-3333-3333-333333333333";
    let tenant = "44444444-4444-4444-4444-444444444444";
    sqlx::query(&format!(
        "INSERT INTO public.tc_post (id, tenant_id, name) VALUES ('{id}', '{tenant}', 'world')"
    ))
    .execute(&pool)
    .await
    .unwrap();

    let mut listener = ChangeLogListener::new(ChangeLogListenerConfig::new(pool.clone()));
    let batch = listener.next_batch().await.unwrap();
    assert_eq!(batch.len(), 1, "the poller reads the captured row");

    let event = batch[0].to_entity_event().expect("captured row decodes to an EntityEvent");
    assert_eq!(event.event_type, EventKind::Created);
    assert_eq!(event.entity_type, "Post", "matched against the subscription return type");
    assert_eq!(event.entity_id.to_string(), id);
    assert_eq!(event.data["name"], "world", "the resolved payload is the new row");
    assert_eq!(event.tenant_id.as_deref(), Some(tenant), "per-tenant fan-out key preserved");
}

#[tokio::test]
#[ignore = "requires PostgreSQL — run with --ignored --test-threads=1"]
async fn non_uuid_pk_is_skipped_not_stalling_the_poller() {
    let pool = pool().await;
    sqlx::raw_sql(entity_change_log_contract_sql()).execute(&pool).await.unwrap();
    sqlx::raw_sql(entity_change_log_capture_trigger_sql())
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query("TRUNCATE core.tb_entity_change_log").execute(&pool).await.unwrap();

    // A table whose PK is NOT a UUID — a misconfigured @subscribable target.
    sqlx::query("DROP TABLE IF EXISTS public.tc_bad CASCADE")
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query("CREATE TABLE public.tc_bad (id BIGINT PRIMARY KEY, name TEXT)")
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query(
        "CREATE TRIGGER tr_cdc_capture_ins AFTER INSERT ON public.tc_bad
         REFERENCING NEW TABLE AS new_table FOR EACH STATEMENT
         EXECUTE FUNCTION core.fn_entity_change_log_capture('Bad', 'id', 'tenant_id')",
    )
    .execute(&pool)
    .await
    .unwrap();

    // The write itself must succeed (the trigger must never abort the user's txn)…
    sqlx::query("INSERT INTO public.tc_bad (id, name) VALUES (1, 'x')")
        .execute(&pool)
        .await
        .unwrap();
    // …and capture nothing, so a non-null-uuid object_id never reaches the poller.
    assert!(
        changelog_rows(&pool).await.is_empty(),
        "a non-UUID PK is skipped rather than written with a NULL object_id"
    );
}
