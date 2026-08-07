//! Seed-fixture integrity gate (#936).
//!
//! The shared fixtures in `tests/sql/postgres/init.sql` (+ `init-analytics.sql`)
//! are read-only for tests: a suite needing writable relations creates its own,
//! unique-named (`tb_pipeline_user`, `e2e_users`, `tests/saga_integration.rs`'s
//! pattern). Twice a suite instead redirected or dropped a shared fixture
//! (`v_user`), silently reddening ~24 consumers in every LATER run on the same
//! database — a red no crate-alone run could reproduce.
//!
//! # One owner, or the expectations below are unassertable
//!
//! Those two files are the ONE owner of the `public` integration fixtures: the
//! Dagger `pgService` and `docker/docker-compose.test.yml` mount the same paths.
//! They used to be twins, and had drifted structurally — the local rig seeded
//! `users`/`posts` with `v_user AS SELECT data FROM users`, CI seeded
//! `tb_user`/`tb_post` with `v_user AS SELECT id, data FROM tb_user`. Same view
//! name, same `public` schema, different shape. A fixture assertion could
//! therefore only ever hold on one of the two rigs, and this suite was green
//! locally and red in CI on its first run. Retiring the twin is what makes the
//! expectations below mean anything.
//!
//! This suite asserts the live database still serves the seeded shapes. It
//! runs LAST in the DB-bound legs, so any clobber introduced by a suite that
//! ran before it in the same leg fails loudly, naming the fixture — instead of
//! reddening two dozen unrelated tests on the next run. Run it locally after a
//! full-workspace run for the same verdict.

#![allow(clippy::unwrap_used, clippy::panic)] // Reason: test code, panics are acceptable
#![allow(clippy::print_stderr)] // Reason: skip message when no backing Postgres is available

use tokio_postgres::Client;

async fn connect() -> Option<Client> {
    let url = fraiseql_test_support::try_database_url()?;
    let (client, connection) = match tokio_postgres::connect(&url, tokio_postgres::NoTls).await {
        Ok(pair) => pair,
        Err(e) => {
            eprintln!("SKIP seed_fixture_integrity: postgres unreachable ({e})");
            return None;
        },
    };
    tokio::spawn(async move { connection.await.ok() });
    Some(client)
}

/// The seeded views must still read from their seeded base tables — a
/// redirected view (the #936 clobber) survives `\dt` inspection because the
/// base table remains.
#[tokio::test]
async fn seeded_views_still_read_their_seeded_tables() {
    let Some(client) = connect().await else {
        eprintln!("SKIP seeded_views_still_read_their_seeded_tables: no DATABASE_URL");
        return;
    };
    for (view, base) in [
        ("v_user", "tb_user"),
        ("v_post", "tb_post"),
        ("v_order", "tb_order"),
    ] {
        // Qualified to `public`: a suite that owns its relations in its OWN schema
        // (the wire suite's `test.v_user`, `p13_embed.v_post`) is the ownership rule
        // working, not a clobber — and an unqualified lookup would match several rows.
        let row = client
            .query_opt(
                "SELECT definition FROM pg_views WHERE schemaname = 'public' AND viewname = $1",
                &[&view],
            )
            .await
            .unwrap();
        let def: String = row
            .unwrap_or_else(|| {
                panic!(
                    "shared fixture view public.{view} is GONE — a suite dropped it (#936 class)"
                )
            })
            .get(0);
        assert!(
            def.contains(&format!("FROM {base}")),
            "shared fixture view public.{view} no longer reads {base} — a suite redirected \
             it (#936 class). Live definition: {def}"
        );
    }
}

/// The seeded tables must exist with their seeded columns and row counts.
#[tokio::test]
async fn seeded_tables_keep_their_shape_and_rows() {
    let Some(client) = connect().await else {
        eprintln!("SKIP seeded_tables_keep_their_shape_and_rows: no DATABASE_URL");
        return;
    };
    // (table, required columns, seeded row count) — from tests/sql/postgres/init.sql
    // and tests/sql/postgres/init-analytics.sql, the files BOTH rigs mount.
    let expectations: [(&str, &[&str], i64); 5] = [
        ("tb_user", &["id", "data"], 5),
        ("tb_post", &["id", "data"], 4),
        ("tb_order", &["id", "data"], 3),
        ("tf_sales", &["id"], 3),
        ("tf_events", &["id"], 3),
    ];
    for (table, cols, want) in expectations {
        for col in cols {
            let present: bool = client
                .query_one(
                    "SELECT EXISTS (SELECT 1 FROM information_schema.columns \
                     WHERE table_schema = 'public' AND table_name = $1 AND column_name = $2)",
                    &[&table, &col],
                )
                .await
                .unwrap()
                .get(0);
            assert!(
                present,
                "shared fixture table {table} lost its seeded column {col} — a suite \
                 recreated it with a different shape (#936 class)"
            );
        }
        let n: i64 = client
            .query_one(&format!(r#"SELECT count(*) FROM "{table}""#), &[])
            .await
            .unwrap_or_else(|e| panic!("shared fixture table {table} unreadable: {e}"))
            .get(0);
        assert!(
            n >= want,
            "shared fixture table {table} has {n} rows, seeded {want} — a suite deleted \
             from it (#936 class; fixtures are read-only for tests)"
        );
    }
}
