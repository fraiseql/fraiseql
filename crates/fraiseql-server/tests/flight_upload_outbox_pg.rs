//! #953 — an allow-listed Flight `Upload` writes its rows **and** their Change Spine
//! outbox rows, in one transaction, against a real PostgreSQL.
//!
//! `fraiseql-arrow`'s `flight_upload_gate_pg` pins the decision half over a real
//! Flight socket: which tables are refused, and that an adapter without this seam
//! refuses even an allow-listed one. This suite pins the half that only exists here,
//! where the adapter that implements the seam lives — that the write actually happens,
//! that the Change Spine sees it, and that the two cannot come apart.
//!
//! Before #953 an Upload ran `adapter.execute_raw_query(insert_sql)` and nothing else:
//! the rows landed and the change log stayed empty, so every Upload was invisible to
//! CDC, to the observers and to any consumer reading the spine.
//!
//! `#[ignore]` — needs `DATABASE_URL`. Named explicitly by the Dagger
//! `integration` leg (`observers` suite, which binds a Postgres), so it either runs
//! or the leg fails. Run with:
//! `cargo test -p fraiseql-server --features arrow --test flight_upload_outbox_pg --
//! --ignored --test-threads=1`.

// PostgreSQL build only. Under `wire-backend`, `FlightDatabaseAdapter` wraps a
// `FraiseWireAdapter`, which does not implement `execute_gated_upload` — so an
// allow-listed Upload is refused there for want of an atomic write path, and there
// is no outbox behaviour to assert. ⚠ This means the leg that runs this suite must
// **not** pass `wire-backend`, or the whole binary compiles to zero tests and reads
// green; the Dagger line uses `--features arrow` for exactly that reason.
#![cfg(all(feature = "arrow", not(feature = "wire-backend")))]
#![allow(clippy::unwrap_used, clippy::expect_used)] // Reason: test code, panics acceptable

use fraiseql_arrow::db::{ArrowDatabaseAdapter, GatedUpload};
use fraiseql_server::arrow::FlightDatabaseAdapter;
use sqlx::{
    Row,
    postgres::{PgPool, PgPoolOptions},
};

const TENANT: &str = "6f1b2c34-5d6e-4f70-8a91-b2c3d4e5f607";

async fn pool() -> PgPool {
    let url = std::env::var("DATABASE_URL").expect("DATABASE_URL must be set for --ignored runs");
    PgPoolOptions::new().max_connections(4).connect(&url).await.unwrap()
}

/// The change-log table from the ONE shared provisioner (#942/#982) — the
/// migration-08 contract byte-for-byte, so this suite cannot assert against a
/// shape no deployment has.
async fn provision_changelog(pool: &PgPool) {
    sqlx::raw_sql(&fraiseql_test_support::changelog::entity_change_log_provision_sql())
        .execute(pool)
        .await
        .unwrap();
}

fn unique_table() -> String {
    format!("ta_upload_outbox_{}", uuid::Uuid::new_v4().simple())
}

async fn create_table(pool: &PgPool, table: &str) {
    sqlx::query(&format!("CREATE TABLE \"{table}\" (id UUID PRIMARY KEY, note TEXT)"))
        .execute(pool)
        .await
        .unwrap();
}

async fn drop_table(pool: &PgPool, table: &str) {
    let _ = sqlx::query(&format!("DROP TABLE IF EXISTS \"{table}\"")).execute(pool).await;
}

async fn count(pool: &PgPool, sql: &str) -> i64 {
    sqlx::query(sql).fetch_one(pool).await.unwrap().get::<i64, _>("n")
}

async fn adapter() -> FlightDatabaseAdapter {
    let url = std::env::var("DATABASE_URL").expect("DATABASE_URL must be set for --ignored runs");
    FlightDatabaseAdapter::new(fraiseql_core::db::PostgresAdapter::new(&url).await.unwrap())
}

/// Two rows with real UUID keys, the shape `build_insert_query` emits.
fn insert_two(table: &str) -> String {
    format!(
        "INSERT INTO \"{table}\" (id, note) VALUES \
         ('11111111-1111-4111-8111-111111111111', 'one'), \
         ('22222222-2222-4222-8222-222222222222', 'two')"
    )
}

/// The rows land **and** the Change Spine records each of them.
#[tokio::test]
#[ignore = "requires DATABASE_URL"]
async fn an_allow_listed_upload_writes_its_rows_and_one_outbox_row_per_row() {
    let pool = pool().await;
    provision_changelog(&pool).await;
    let table = unique_table();
    create_table(&pool, &table).await;

    let sql = insert_two(&table);
    let written = adapter()
        .await
        .execute_gated_upload(&GatedUpload {
            table:      &table,
            insert_sql: &sql,
            user_id:    "upload-outbox-user",
            tenant_id:  Some(TENANT),
        })
        .await;

    let written = written.expect("the gated Upload must succeed");

    let rows = count(&pool, &format!("SELECT count(*) AS n FROM \"{table}\"")).await;
    let logged = count(
        &pool,
        &format!(
            "SELECT count(*) AS n FROM core.tb_entity_change_log WHERE object_type = '{table}'"
        ),
    )
    .await;
    let log_row = sqlx::query(
        "SELECT object_id, object_data, tenant_id, modification_type, extra_metadata
         FROM core.tb_entity_change_log WHERE object_type = $1 ORDER BY object_id LIMIT 1",
    )
    .bind(&table)
    .fetch_one(&pool)
    .await
    .unwrap();
    drop_table(&pool, &table).await;

    assert_eq!(written, 2, "the seam must report the rows it wrote");
    assert_eq!(rows, 2, "the Upload's rows must land");
    assert_eq!(logged, 2, "the Change Spine must record one row per uploaded row");

    // The outbox row must be usable by a consumer, not just present.
    assert_eq!(log_row.get::<String, _>("modification_type"), "INSERT");
    assert_eq!(
        log_row.get::<uuid::Uuid, _>("object_id").to_string(),
        "11111111-1111-4111-8111-111111111111",
        "a UUID key must reach object_id, so consumers can address the entity"
    );
    assert_eq!(
        log_row.get::<uuid::Uuid, _>("tenant_id").to_string(),
        TENANT,
        "the write's tenant must partition its outbox row"
    );
    let data: serde_json::Value = log_row.get("object_data");
    assert_eq!(data["note"], "one", "object_data must carry the row that was written");
    let meta: serde_json::Value = log_row.get("extra_metadata");
    assert_eq!(
        meta["transport"], "flight",
        "the ingress transport must be recorded, so an operator can tell Flight writes apart"
    );
    assert_eq!(meta["flight_user_id"], "upload-outbox-user");
}

/// **Atomicity.** With the change-log table absent, the outbox write fails — and the
/// data rows must not be there either.
///
/// Without this, "in the same transaction" is untested prose: two sequential
/// statements would pass every other assertion in this file and still leave the rows
/// committed with the spine blind, which is the exact defect #953 names.
#[tokio::test]
#[ignore = "requires DATABASE_URL"]
async fn a_failing_outbox_write_takes_the_rows_with_it() {
    let pool = pool().await;
    let table = unique_table();
    create_table(&pool, &table).await;
    // Remove the outbox table the CTE writes to — the second half of the
    // transaction can no longer succeed.
    sqlx::raw_sql("DROP TABLE IF EXISTS core.tb_entity_change_log CASCADE")
        .execute(&pool)
        .await
        .unwrap();

    let sql = insert_two(&table);
    let result = adapter()
        .await
        .execute_gated_upload(&GatedUpload {
            table:      &table,
            insert_sql: &sql,
            user_id:    "upload-outbox-user",
            tenant_id:  None,
        })
        .await;

    let rows = count(&pool, &format!("SELECT count(*) AS n FROM \"{table}\"")).await;
    drop_table(&pool, &table).await;
    provision_changelog(&pool).await; // restore for whatever runs next

    result.expect_err("the Upload must fail when its outbox row cannot be written");
    assert_eq!(
        rows, 0,
        "the rows must roll back with the outbox write — {rows} row(s) committed while the \
         Change Spine recorded nothing"
    );
}

/// A non-UUID primary key must not fail the write: `object_id` is NULL and the whole
/// row still reaches `object_data`. An Upload target is an arbitrary allow-listed
/// table and need not use UUID keys.
#[tokio::test]
#[ignore = "requires DATABASE_URL"]
async fn a_non_uuid_key_logs_a_null_object_id_rather_than_failing() {
    let pool = pool().await;
    provision_changelog(&pool).await;
    let table = unique_table();
    sqlx::query(&format!("CREATE TABLE \"{table}\" (id TEXT PRIMARY KEY, note TEXT)"))
        .execute(&pool)
        .await
        .unwrap();

    let sql = format!("INSERT INTO \"{table}\" (id, note) VALUES ('not-a-uuid', 'one')");
    let written = adapter()
        .await
        .execute_gated_upload(&GatedUpload {
            table:      &table,
            insert_sql: &sql,
            user_id:    "upload-outbox-user",
            tenant_id:  None,
        })
        .await;

    let logged = sqlx::query(
        "SELECT object_id, object_data FROM core.tb_entity_change_log WHERE object_type = $1",
    )
    .bind(&table)
    .fetch_one(&pool)
    .await
    .unwrap();
    drop_table(&pool, &table).await;

    assert_eq!(written.unwrap(), 1);
    assert!(
        logged.get::<Option<uuid::Uuid>, _>("object_id").is_none(),
        "a TEXT key must log NULL, not abort the Upload"
    );
    let data: serde_json::Value = logged.get("object_data");
    assert_eq!(data["id"], "not-a-uuid", "the key is still recoverable from object_data");
}
