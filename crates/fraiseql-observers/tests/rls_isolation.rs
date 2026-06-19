//! Change-log Row-Level Security isolation tests (#443 / #437 F6).
//!
//! Proves `migrations/12_enable_change_log_rls.sql`
//! (`fraiseql_observers::migrations::entity_change_log_rls_sql`) under a role that
//! does **not** bypass RLS — the CI superuser (`fraiseql_test`) would mask every
//! policy, so these assertions run on a dedicated `NOBYPASSRLS` role created by the
//! test itself.
//!
//! Covered:
//! * **deny-by-default / fail-closed** — a NOBYPASSRLS role with no `fraiseql.tenant_id` GUC reads
//!   zero change-log rows (table + both views);
//! * **forward-compat per-tenant** — with the GUC set it sees exactly that tenant's rows, and
//!   switching the GUC switches the visible set;
//! * **cross-tenant consumer** — a `BYPASSRLS` role (the poller/bridge stance) reads every tenant's
//!   rows;
//! * **capture under RLS** — an external write by a NOBYPASSRLS role still produces a change-log
//!   row, because the capture function is `SECURITY DEFINER` (migration 11).
//!
//! ## Running
//!
//! Shares `core.tb_entity_change_log`; run serially against a superuser `DATABASE_URL`
//! (the test creates the two subordinate roles itself):
//!
//! ```bash
//! DATABASE_URL=postgresql://fraiseql_test:fraiseql_test_password@localhost:5433/test_fraiseql \
//!   cargo test -p fraiseql-observers --features postgres --test rls_isolation \
//!   -- --ignored --test-threads=1
//! ```

#![cfg(feature = "postgres")]
#![allow(clippy::unwrap_used, clippy::panic)] // Reason: integration test file

use std::str::FromStr;

use fraiseql_observers::migrations::{
    entity_change_log_capture_trigger_sql, entity_change_log_contract_sql,
    entity_change_log_rls_sql,
};
use fraiseql_test_utils::database_url;
use sqlx::{
    PgPool, Row,
    postgres::{PgConnectOptions, PgPoolOptions},
};

/// NOBYPASSRLS reader — the RLS-subject role the policy must bind.
const TENANT_ROLE: &str = "fraiseql_rls_tenant";
/// BYPASSRLS consumer — the trusted cross-tenant poller/bridge stance.
const CONSUMER_ROLE: &str = "fraiseql_rls_consumer";
const ROLE_PASSWORD: &str = "rls_test_password";
const TENANT_A: &str = "aaaaaaaa-aaaa-aaaa-aaaa-aaaaaaaaaaaa";
const TENANT_B: &str = "bbbbbbbb-bbbb-bbbb-bbbb-bbbbbbbbbbbb";

async fn admin_pool() -> PgPool {
    PgPoolOptions::new()
        .max_connections(2)
        .connect(&database_url())
        .await
        .expect("connect to test database as the superuser DATABASE_URL")
}

/// A pool connected as `role` — same host/port/database as `DATABASE_URL`, with the
/// credentials swapped (no second env var or init script needed; works against the
/// warm local PG and the Dagger-bound Postgres alike).
async fn role_pool(role: &str) -> PgPool {
    let opts = PgConnectOptions::from_str(&database_url())
        .expect("parse DATABASE_URL")
        .username(role)
        .password(ROLE_PASSWORD);
    PgPoolOptions::new()
        .max_connections(2)
        .connect_with(opts)
        .await
        .unwrap_or_else(|e| panic!("connect as {role}: {e}"))
}

/// Create the two subordinate roles (idempotent), apply migrations 08/11/12, grant
/// the roles read access, and seed `n_a` rows for tenant A + `n_b` for tenant B as
/// the superuser (which bypasses RLS for seeding).
async fn setup(admin: &PgPool, n_a: usize, n_b: usize) {
    for (role, bypass) in [(TENANT_ROLE, "NOBYPASSRLS"), (CONSUMER_ROLE, "BYPASSRLS")] {
        sqlx::query(&format!(
            "DO $$ BEGIN
                 IF NOT EXISTS (SELECT FROM pg_roles WHERE rolname = '{role}') THEN
                     CREATE ROLE {role} LOGIN PASSWORD '{ROLE_PASSWORD}' NOSUPERUSER {bypass};
                 END IF;
             END $$"
        ))
        .execute(admin)
        .await
        .unwrap();
        // Re-assert the attributes in case the role pre-existed from an earlier run.
        sqlx::query(&format!(
            "ALTER ROLE {role} NOSUPERUSER {bypass} LOGIN PASSWORD '{ROLE_PASSWORD}'"
        ))
        .execute(admin)
        .await
        .unwrap();
    }

    sqlx::raw_sql(entity_change_log_contract_sql()).execute(admin).await.unwrap();
    sqlx::raw_sql(entity_change_log_capture_trigger_sql())
        .execute(admin)
        .await
        .unwrap();
    sqlx::raw_sql(entity_change_log_rls_sql()).execute(admin).await.unwrap();
    sqlx::query("TRUNCATE core.tb_entity_change_log").execute(admin).await.unwrap();

    sqlx::query(&format!("GRANT USAGE ON SCHEMA core TO {TENANT_ROLE}, {CONSUMER_ROLE}"))
        .execute(admin)
        .await
        .unwrap();
    sqlx::query(&format!(
        "GRANT SELECT ON core.tb_entity_change_log, core.v_entity_change_log, \
         core.v_entity_change_log_debezium TO {TENANT_ROLE}"
    ))
    .execute(admin)
    .await
    .unwrap();
    // The consumer also marks rows published in production, so grant UPDATE too.
    sqlx::query(&format!("GRANT SELECT, UPDATE ON core.tb_entity_change_log TO {CONSUMER_ROLE}"))
        .execute(admin)
        .await
        .unwrap();

    for (tenant, count) in [(TENANT_A, n_a), (TENANT_B, n_b)] {
        for i in 0..count {
            sqlx::query(
                "INSERT INTO core.tb_entity_change_log
                     (object_type, modification_type, object_id, object_data, tenant_id)
                 VALUES ('Post', 'INSERT', gen_random_uuid(), jsonb_build_object('i', $1::int), $2::uuid)",
            )
            .bind(i32::try_from(i).unwrap())
            .bind(tenant)
            .execute(admin)
            .await
            .unwrap();
        }
    }
}

/// Count rows from `sql` on `pool`, optionally setting `fraiseql.tenant_id` for the
/// transaction first (so `SET LOCAL` scoping mirrors a real request).
async fn count_with_guc(pool: &PgPool, guc: Option<&str>, sql: &str) -> i64 {
    let mut tx = pool.begin().await.unwrap();
    if let Some(tenant) = guc {
        sqlx::query("SELECT set_config('fraiseql.tenant_id', $1, true)")
            .bind(tenant)
            .execute(&mut *tx)
            .await
            .unwrap();
    }
    let count: i64 = sqlx::query(sql).fetch_one(&mut *tx).await.unwrap().get(0);
    tx.commit().await.unwrap();
    count
}

#[tokio::test]
#[ignore = "requires PostgreSQL — run with --ignored --test-threads=1"]
async fn tenant_sees_only_its_own_rows_via_table_and_views() {
    let admin = admin_pool().await;
    setup(&admin, 3, 2).await;
    let tenant = role_pool(TENANT_ROLE).await;

    // GUC = A → exactly A's 3 rows, through the base table AND both views (which
    // inherit the base-table RLS).
    for sql in [
        "SELECT count(*) FROM core.tb_entity_change_log",
        "SELECT count(*) FROM core.v_entity_change_log",
        "SELECT count(*) FROM core.v_entity_change_log_debezium",
    ] {
        assert_eq!(
            count_with_guc(&tenant, Some(TENANT_A), sql).await,
            3,
            "tenant A must see exactly its 3 rows via `{sql}`"
        );
    }

    // Switching the GUC to B switches the visible set to B's 2 rows.
    assert_eq!(
        count_with_guc(&tenant, Some(TENANT_B), "SELECT count(*) FROM core.tb_entity_change_log")
            .await,
        2,
        "switching the GUC to B reveals exactly B's 2 rows"
    );
}

#[tokio::test]
#[ignore = "requires PostgreSQL — run with --ignored --test-threads=1"]
async fn cross_tenant_hidden_and_unset_guc_is_fail_closed() {
    let admin = admin_pool().await;
    setup(&admin, 3, 2).await;
    let tenant = role_pool(TENANT_ROLE).await;

    // With A's GUC, none of B's rows are visible even when explicitly filtered.
    assert_eq!(
        count_with_guc(
            &tenant,
            Some(TENANT_A),
            &format!(
                "SELECT count(*) FROM core.tb_entity_change_log WHERE tenant_id = '{TENANT_B}'"
            ),
        )
        .await,
        0,
        "A's GUC must hide every one of B's rows"
    );

    // Deny-by-default: an unset GUC reads zero rows, table + view.
    for sql in [
        "SELECT count(*) FROM core.tb_entity_change_log",
        "SELECT count(*) FROM core.v_entity_change_log",
    ] {
        assert_eq!(
            count_with_guc(&tenant, None, sql).await,
            0,
            "an unset fraiseql.tenant_id GUC is fail-closed (0 rows) via `{sql}`"
        );
    }
}

#[tokio::test]
#[ignore = "requires PostgreSQL — run with --ignored --test-threads=1"]
async fn bypassrls_consumer_sees_all_tenants() {
    let admin = admin_pool().await;
    setup(&admin, 3, 2).await;
    let consumer = role_pool(CONSUMER_ROLE).await;

    // The trusted cross-tenant consumer (BYPASSRLS), no GUC, reads every tenant's
    // rows — exactly what the poller / NATS bridges must keep doing under RLS.
    assert_eq!(
        count_with_guc(&consumer, None, "SELECT count(*) FROM core.tb_entity_change_log").await,
        5,
        "a BYPASSRLS consumer reads all tenants' rows (3 + 2)"
    );
}

#[tokio::test]
#[ignore = "requires PostgreSQL — run with --ignored --test-threads=1"]
async fn definer_capture_writes_under_rls_for_a_nonbypassrls_writer() {
    let admin = admin_pool().await;
    setup(&admin, 0, 0).await;

    // Admin owns the source table + capture trigger; the capture function is
    // SECURITY DEFINER (owned by the superuser admin).
    sqlx::query("DROP TABLE IF EXISTS public.tc_rls_post CASCADE")
        .execute(&admin)
        .await
        .unwrap();
    sqlx::query(
        "CREATE TABLE public.tc_rls_post \
         (id UUID PRIMARY KEY DEFAULT gen_random_uuid(), tenant_id UUID, name TEXT)",
    )
    .execute(&admin)
    .await
    .unwrap();
    sqlx::query(
        "CREATE TRIGGER tr_rls_capture_ins AFTER INSERT ON public.tc_rls_post \
         REFERENCING NEW TABLE AS new_table FOR EACH STATEMENT \
         EXECUTE FUNCTION core.fn_entity_change_log_capture('Post', 'id', 'tenant_id')",
    )
    .execute(&admin)
    .await
    .unwrap();
    sqlx::query(&format!("GRANT USAGE ON SCHEMA public TO {TENANT_ROLE}"))
        .execute(&admin)
        .await
        .unwrap();
    sqlx::query(&format!("GRANT INSERT ON public.tc_rls_post TO {TENANT_ROLE}"))
        .execute(&admin)
        .await
        .unwrap();

    // The NOBYPASSRLS tenant role makes the external write. The SECURITY DEFINER
    // capture fn runs as the owner and writes the change-log row exempt from RLS.
    let tenant = role_pool(TENANT_ROLE).await;
    sqlx::query(&format!(
        "INSERT INTO public.tc_rls_post (tenant_id, name) VALUES ('{TENANT_A}', 'ext')"
    ))
    .execute(&tenant)
    .await
    .unwrap();

    // The admin (BYPASSRLS) sees the captured row → DEFINER capture wrote through RLS.
    let captured: i64 = sqlx::query(
        "SELECT count(*) FROM core.tb_entity_change_log \
         WHERE extra_metadata->>'cdc_source' = 'fallback_trigger'",
    )
    .fetch_one(&admin)
    .await
    .unwrap()
    .get(0);
    assert_eq!(
        captured, 1,
        "the SECURITY DEFINER capture fn wrote a change-log row even though the writer is NOBYPASSRLS"
    );

    // And under RLS that captured row is readable by the tenant only with its GUC set.
    assert_eq!(
        count_with_guc(&tenant, Some(TENANT_A), "SELECT count(*) FROM core.tb_entity_change_log")
            .await,
        1,
        "the captured row is visible to the tenant role with its GUC set"
    );
}
