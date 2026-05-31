//! Changelog GraphQL-exposure migration tests (issue #149).
//!
//! Verifies that `migrations/07_create_changelog_views.sql` installs working,
//! idempotent views + checkpoint upsert function over the observer change-log.
//!
//! ## Running
//!
//! ```bash
//! docker run -d --name postgres -p 5432:5432 -e POSTGRES_PASSWORD=postgres postgres:16
//! psql -h localhost -U postgres -c "CREATE DATABASE fraiseql_test"
//! DATABASE_URL=postgres://postgres:postgres@localhost/fraiseql_test \
//!   cargo test -p fraiseql-observers --test changelog_views -- --ignored
//! ```

#![cfg(feature = "postgres")]
#![allow(clippy::unwrap_used, clippy::print_stdout)] // Reason: integration test file

use fraiseql_test_utils::database_url;
use sqlx::{PgPool, Row, postgres::PgPoolOptions};

const MIGRATION: &str = include_str!("../migrations/07_create_changelog_views.sql");

async fn pool() -> PgPool {
    PgPoolOptions::new()
        .max_connections(2)
        .connect(&database_url())
        .await
        .expect("connect to test database")
}

/// Install the prerequisite tables + the `app.mutation_response` contract, then a
/// clean copy of the change-log table for this test run.
async fn install_prerequisites(pool: &PgPool) {
    sqlx::query("CREATE SCHEMA IF NOT EXISTS core").execute(pool).await.unwrap();
    sqlx::query("CREATE SCHEMA IF NOT EXISTS app").execute(pool).await.unwrap();

    // app.mutation_response contract (see docs/architecture/mutation-response.md).
    sqlx::query(
        r"
        DO $$ BEGIN
            CREATE TYPE app.mutation_error_class AS ENUM (
                'validation','conflict','not_found','unauthorized','forbidden',
                'internal','transaction_failed','timeout','rate_limited','service_unavailable'
            );
        EXCEPTION WHEN duplicate_object THEN NULL; END $$;
        ",
    )
    .execute(pool)
    .await
    .unwrap();
    sqlx::query(
        r"
        DO $$ BEGIN
            CREATE TYPE app.mutation_response AS (
                succeeded BOOLEAN, state_changed BOOLEAN, error_class app.mutation_error_class,
                status_detail TEXT, http_status SMALLINT, message TEXT, entity_id UUID,
                entity_type TEXT, entity JSONB, updated_fields TEXT[], cascade JSONB,
                error_detail JSONB, metadata JSONB
            );
        EXCEPTION WHEN duplicate_object THEN NULL; END $$;
        ",
    )
    .execute(pool)
    .await
    .unwrap();

    // Source tables (the observer/install convention — drop for a clean per-run slate).
    sqlx::query("DROP TABLE IF EXISTS core.tb_entity_change_log CASCADE")
        .execute(pool)
        .await
        .unwrap();
    sqlx::query(
        r"
        CREATE TABLE core.tb_entity_change_log (
            pk_entity_change_log BIGSERIAL PRIMARY KEY,
            id UUID NOT NULL DEFAULT gen_random_uuid(),
            fk_customer_org TEXT,
            fk_contact TEXT,
            object_type TEXT NOT NULL,
            object_id TEXT NOT NULL,
            modification_type TEXT NOT NULL,
            change_status TEXT,
            object_data JSONB NOT NULL,
            extra_metadata JSONB,
            created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
        )
        ",
    )
    .execute(pool)
    .await
    .unwrap();
    sqlx::query(
        r"
        CREATE TABLE IF NOT EXISTS core.tb_transport_checkpoint (
            transport_name TEXT PRIMARY KEY,
            last_pk BIGINT NOT NULL,
            updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
        )
        ",
    )
    .execute(pool)
    .await
    .unwrap();
}

#[tokio::test]
#[ignore = "requires PostgreSQL - run with: cargo test -p fraiseql-observers --test changelog_views -- --ignored"]
async fn migration_installs_views_and_upsert_fn() {
    let pool = pool().await;
    install_prerequisites(&pool).await;

    // Apply migration — and a second time to prove idempotency.
    sqlx::raw_sql(MIGRATION).execute(&pool).await.expect("first migration apply");
    sqlx::raw_sql(MIGRATION).execute(&pool).await.expect("idempotent re-apply");

    // Insert two change-log rows.
    sqlx::query(
        r#"
        INSERT INTO core.tb_entity_change_log
            (object_type, object_id, modification_type, object_data, extra_metadata)
        VALUES
            ('User',  gen_random_uuid()::text, 'INSERT', '{"a":1}'::jsonb, NULL),
            ('Order', gen_random_uuid()::text, 'UPDATE', '{"b":2}'::jsonb, '{"src":"t"}'::jsonb)
        "#,
    )
    .execute(&pool)
    .await
    .unwrap();

    // The `data` JSONB carries every GraphQL field; pk is a JSON number so the
    // runtime's `(data->>'pk_entity_change_log')::numeric` keyset comparison is sound.
    let rows = sqlx::query(
        "SELECT pk_entity_change_log,
                data->>'object_type'        AS object_type,
                (data->>'pk_entity_change_log')::bigint AS data_pk,
                jsonb_typeof(data->'object_data')       AS od_kind
         FROM core.v_entity_change_log
         ORDER BY pk_entity_change_log ASC",
    )
    .fetch_all(&pool)
    .await
    .unwrap();
    assert_eq!(rows.len(), 2);
    let pk0: i64 = rows[0].get("pk_entity_change_log");
    let data_pk0: i64 = rows[0].get("data_pk");
    assert_eq!(pk0, data_pk0, "pk must round-trip through the data JSONB as a number");
    let ot0: String = rows[0].get("object_type");
    assert_eq!(ot0, "User");
    let od_kind: String = rows[0].get("od_kind");
    assert_eq!(od_kind, "object", "object_data stays JSONB inside data");

    // Checkpoint upsert: insert path then update path, asserting state_changed semantics.
    let r = sqlx::query(
        "SELECT (core.fn_upsert_transport_checkpoint('sidecar-1', 5)).succeeded AS ok,
                (core.fn_upsert_transport_checkpoint('sidecar-1', 5)).state_changed AS again",
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    let ok: bool = r.get("ok");
    let again: bool = r.get("again");
    assert!(ok, "upsert succeeds");
    assert!(
        !again,
        "re-upserting the same cursor is an idempotent no-op (state_changed=false)"
    );

    sqlx::query("SELECT core.fn_upsert_transport_checkpoint('sidecar-1', 42)")
        .execute(&pool)
        .await
        .unwrap();
    let cp = sqlx::query(
        "SELECT last_pk, (data->>'last_pk')::bigint AS data_last_pk
         FROM core.v_transport_checkpoint WHERE transport_name = 'sidecar-1'",
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    let last_pk: i64 = cp.get("last_pk");
    let data_last_pk: i64 = cp.get("data_last_pk");
    assert_eq!(last_pk, 42);
    assert_eq!(data_last_pk, 42);

    // Cleanup so reruns start clean.
    sqlx::query("DELETE FROM core.tb_transport_checkpoint WHERE transport_name = 'sidecar-1'")
        .execute(&pool)
        .await
        .ok();
}
