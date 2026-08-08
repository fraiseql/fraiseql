//! #935: the change-log poller must deliver rows that commit out of pk order.
//!
//! `pk_entity_change_log` is `GENERATED ALWAYS AS IDENTITY` — allocated at
//! INSERT time inside the writer's transaction, but only *visible* at commit.
//! The two orders diverge under any concurrent mutation load, so a strict
//! `WHERE pk > watermark` cursor permanently drops a row whose transaction
//! commits after a higher-pk row was already polled: its observers never fire,
//! with no error and no trace. This is the same defect family as #797
//! (cdc-sinks' `MAX(seq)` enqueue cursor).
//!
//! ## Running
//!
//! These tests own `core.tb_entity_change_log`, so run them serially in their
//! own binary:
//!
//! ```bash
//! DATABASE_URL=postgresql://fraiseql_test:fraiseql_test_password@localhost:5433/test_fraiseql \
//!   cargo test -p fraiseql-observers --features postgres \
//!   --test change_log_commit_order_pg -- --ignored --test-threads=1
//! ```

#![cfg(feature = "postgres")]
#![allow(clippy::unwrap_used, clippy::panic)] // Reason: integration test file

use std::str::FromStr;

use fraiseql_observers::{
    listener::{ChangeLogListener, ChangeLogListenerConfig},
    migrations::entity_change_log_contract_sql,
};
use fraiseql_test_utils::database_url;
use sqlx::{
    PgPool,
    postgres::{PgConnectOptions, PgPoolOptions},
};

/// Four connections: session A's held transaction, session B's autocommit
/// write, and the poller's own queries.
async fn pool() -> PgPool {
    PgPoolOptions::new()
        .max_connections(4)
        .connect(&database_url())
        .await
        .expect("connect to test database")
}

/// Own the fixture (P03): rebuild the contract table *and* the dispatch ledger
/// so a previous run's rows cannot mask or fake this one's result. Both restart
/// their identity sequences, so a stale ledger row would otherwise anti-join
/// out a fresh change-log row that reuses its pk.
async fn fresh_contract(pool: &PgPool) {
    sqlx::query("CREATE SCHEMA IF NOT EXISTS core").execute(pool).await.unwrap();
    sqlx::query("DROP VIEW IF EXISTS core.v_entity_change_log")
        .execute(pool)
        .await
        .unwrap();
    sqlx::query("DROP TABLE IF EXISTS core.tb_entity_change_log CASCADE")
        .execute(pool)
        .await
        .unwrap();
    sqlx::query("DROP TABLE IF EXISTS core.tb_observer_dispatch")
        .execute(pool)
        .await
        .unwrap();
    sqlx::raw_sql(entity_change_log_contract_sql()).execute(pool).await.unwrap();
}

/// The insert every session in this suite performs; returns the allocated pk.
const INSERT_RETURNING_PK: &str = "INSERT INTO core.tb_entity_change_log \
     (object_type, modification_type, object_id, object_data) \
     VALUES ($1, 'INSERT', gen_random_uuid(), $2) \
     RETURNING pk_entity_change_log";

/// The #935 two-session repro: session A allocates the lower pk inside a
/// still-open transaction, session B commits the higher pk first, and the
/// poller runs in between. Both rows must eventually reach the dispatcher.
#[tokio::test]
#[ignore = "requires PostgreSQL — run with --ignored --test-threads=1"]
async fn row_committing_after_a_higher_pk_was_polled_is_still_dispatched() {
    let pool = pool().await;
    fresh_contract(&pool).await;

    // Session A: the pk is allocated now, inside a transaction that stays open.
    let mut session_a = pool.begin().await.unwrap();
    let pk_a: i64 = sqlx::query_scalar(INSERT_RETURNING_PK)
        .bind("User")
        .bind(serde_json::json!({ "n": 1 }))
        .fetch_one(&mut *session_a)
        .await
        .unwrap();

    // Session B: a later mutation, committing first (autocommit).
    let pk_b: i64 = sqlx::query_scalar(INSERT_RETURNING_PK)
        .bind("User")
        .bind(serde_json::json!({ "n": 2 }))
        .fetch_one(&pool)
        .await
        .unwrap();
    assert!(pk_b > pk_a, "test setup: B must hold the higher pk (a={pk_a}, b={pk_b})");

    let mut listener = ChangeLogListener::new(ChangeLogListenerConfig::new(pool.clone()));

    // Poll while A is uncommitted. Only B is visible — this half also proves the
    // rig: an empty batch here means the fixture is broken, not that the cursor
    // is fixed.
    let first: Vec<i64> = listener.next_batch().await.unwrap().iter().map(|e| e.id).collect();
    assert_eq!(first, vec![pk_b], "only the committed row is visible to the first poll");

    // A commits late — after the poller has already advanced past its pk.
    session_a.commit().await.unwrap();

    // The late-committing lower-pk row must still be dispatched.
    let second: Vec<i64> = listener.next_batch().await.unwrap().iter().map(|e| e.id).collect();
    assert!(
        second.contains(&pk_a),
        "pk {pk_a} committed after pk {pk_b} was polled and was never dispatched \
         (second poll returned {second:?})"
    );
}

/// The other half of the contract: recovering the straggler must not re-deliver
/// rows already handed to the dispatcher. A fix that simply re-scans the recent
/// window would pass the test above and flood every observer with duplicates.
#[tokio::test]
#[ignore = "requires PostgreSQL — run with --ignored --test-threads=1"]
async fn recovering_a_straggler_does_not_redeliver_rows_already_dispatched() {
    let pool = pool().await;
    fresh_contract(&pool).await;

    let mut session_a = pool.begin().await.unwrap();
    let pk_a: i64 = sqlx::query_scalar(INSERT_RETURNING_PK)
        .bind("Order")
        .bind(serde_json::json!({ "n": 1 }))
        .fetch_one(&mut *session_a)
        .await
        .unwrap();
    let pk_b: i64 = sqlx::query_scalar(INSERT_RETURNING_PK)
        .bind("Order")
        .bind(serde_json::json!({ "n": 2 }))
        .fetch_one(&pool)
        .await
        .unwrap();

    let mut listener = ChangeLogListener::new(ChangeLogListenerConfig::new(pool.clone()));
    let first: Vec<i64> = listener.next_batch().await.unwrap().iter().map(|e| e.id).collect();
    assert_eq!(first, vec![pk_b]);

    session_a.commit().await.unwrap();

    // Drain every remaining batch: across all of them each pk appears exactly once.
    let mut delivered = first;
    for _ in 0..4 {
        delivered.extend(listener.next_batch().await.unwrap().iter().map(|e| e.id));
    }

    let mut unique = delivered.clone();
    unique.sort_unstable();
    unique.dedup();
    assert_eq!(
        unique,
        vec![pk_a, pk_b],
        "both rows must be delivered (delivered: {delivered:?})"
    );
    assert_eq!(
        delivered.len(),
        2,
        "each row must be delivered exactly once within a process (delivered: {delivered:?})"
    );
}

/// The durable half of the fix. A restarted poller resumes from the checkpoint
/// its predecessor saved, so the straggler is *below* its resume floor: only the
/// dispatch ledger can distinguish "committed late, never dispatched" from "the
/// predecessor already handled this". An in-process-only fix passes the two
/// tests above and re-delivers the whole commit-lag window on every boot.
#[tokio::test]
#[ignore = "requires PostgreSQL — run with --ignored --test-threads=1"]
async fn a_restarted_poller_recovers_the_straggler_without_replaying_the_window() {
    let pool = pool().await;
    fresh_contract(&pool).await;

    let listener_id = format!("restart-{}", uuid::Uuid::new_v4());

    let mut session_a = pool.begin().await.unwrap();
    let pk_a: i64 = sqlx::query_scalar(INSERT_RETURNING_PK)
        .bind("Product")
        .bind(serde_json::json!({ "n": 1 }))
        .fetch_one(&mut *session_a)
        .await
        .unwrap();
    let pk_b: i64 = sqlx::query_scalar(INSERT_RETURNING_PK)
        .bind("Product")
        .bind(serde_json::json!({ "n": 2 }))
        .fetch_one(&pool)
        .await
        .unwrap();

    // Incarnation 1: polls, dispatches B, records it, saves checkpoint = pk_b.
    let mut first_run = ChangeLogListener::new(
        ChangeLogListenerConfig::new(pool.clone()).with_listener_id(&listener_id),
    );
    let batch = first_run.next_batch().await.unwrap();
    assert_eq!(batch.iter().map(|e| e.id).collect::<Vec<_>>(), vec![pk_b]);
    first_run.record_dispatched(&batch).await.unwrap();
    let checkpoint = first_run.checkpoint();
    assert_eq!(checkpoint, pk_b, "the saved checkpoint is the highest pk polled");
    drop(first_run);

    // A commits during the restart window.
    session_a.commit().await.unwrap();

    // Incarnation 2: same identity, resuming from the saved checkpoint.
    let mut second_run = ChangeLogListener::new(
        ChangeLogListenerConfig::new(pool.clone())
            .with_listener_id(&listener_id)
            .with_resume_from(checkpoint),
    );
    let recovered: Vec<i64> = second_run.next_batch().await.unwrap().iter().map(|e| e.id).collect();

    assert_eq!(
        recovered,
        vec![pk_a],
        "the restarted poller must deliver exactly the straggler: pk {pk_a} recovered, \
         pk {pk_b} not replayed (got {recovered:?})"
    );
}

/// A poller resuming from a checkpoint left by a pre-ledger release finds an
/// empty ledger. It must not read that as "nothing was ever dispatched" and
/// replay the whole change log — the upgrade path, bounded by the window.
#[tokio::test]
#[ignore = "requires PostgreSQL — run with --ignored --test-threads=1"]
async fn an_empty_ledger_under_an_old_checkpoint_does_not_replay_history() {
    let pool = pool().await;
    fresh_contract(&pool).await;

    // History a previous release already dispatched, backdated past the window
    // exactly as a real upgrade would find it.
    let mut history = Vec::new();
    for n in 0..5 {
        history.push(
            sqlx::query_scalar::<_, i64>(INSERT_RETURNING_PK)
                .bind("Legacy")
                .bind(serde_json::json!({ "n": n }))
                .fetch_one(&pool)
                .await
                .unwrap(),
        );
    }
    sqlx::query("UPDATE core.tb_entity_change_log SET created_at = now() - interval '30 minutes'")
        .execute(&pool)
        .await
        .unwrap();

    let last = *history.last().unwrap();

    // Upgraded process: checkpoint from the old release, ledger empty. The first
    // poll is a sweep — the worst case for replay.
    let mut listener = ChangeLogListener::new(
        ChangeLogListenerConfig::new(pool.clone())
            .with_listener_id(format!("upgrade-{}", uuid::Uuid::new_v4()))
            .with_resume_from(last),
    );
    let replayed: Vec<i64> = listener.next_batch().await.unwrap().iter().map(|e| e.id).collect();

    assert!(
        replayed.is_empty(),
        "an empty ledger under an old checkpoint replayed already-dispatched history: {replayed:?}"
    );
}

/// Rebuilding the change log restarts its IDENTITY at 1, so fresh rows reuse the
/// pks a previous incarnation dispatched. A ledger keyed by pk would anti-join
/// them straight out — the same permanent silent skip #935 is about, arriving by
/// a different route. Keying on the row's stable UUID makes that impossible.
#[tokio::test]
#[ignore = "requires PostgreSQL — run with --ignored --test-threads=1"]
async fn a_rebuilt_change_log_is_not_suppressed_by_the_previous_incarnations_ledger() {
    let pool = pool().await;
    fresh_contract(&pool).await;

    // Deliberately do NOT clear the ledger between incarnations: an operator who
    // rebuilds the log has no reason to know it exists.
    let listener_id = format!("rebuild-{}", uuid::Uuid::new_v4());

    let first_pk: i64 = sqlx::query_scalar(INSERT_RETURNING_PK)
        .bind("Shipment")
        .bind(serde_json::json!({ "n": 1 }))
        .fetch_one(&pool)
        .await
        .unwrap();
    let mut before = ChangeLogListener::new(
        ChangeLogListenerConfig::new(pool.clone()).with_listener_id(&listener_id),
    );
    let batch = before.next_batch().await.unwrap();
    before.record_dispatched(&batch).await.unwrap();

    // The log is rebuilt (a reset, a restore, a migration) — identity restarts.
    sqlx::query("DROP VIEW IF EXISTS core.v_entity_change_log")
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query("DROP TABLE core.tb_entity_change_log CASCADE")
        .execute(&pool)
        .await
        .unwrap();
    sqlx::raw_sql(entity_change_log_contract_sql()).execute(&pool).await.unwrap();

    let reused_pk: i64 = sqlx::query_scalar(INSERT_RETURNING_PK)
        .bind("Shipment")
        .bind(serde_json::json!({ "n": 2 }))
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(reused_pk, first_pk, "test setup: the rebuilt log must reuse the pk");

    let mut after = ChangeLogListener::new(
        ChangeLogListenerConfig::new(pool.clone()).with_listener_id(&listener_id),
    );
    let recovered: Vec<i64> = after.next_batch().await.unwrap().iter().map(|e| e.id).collect();
    assert_eq!(
        recovered,
        vec![reused_pk],
        "a rebuilt change log's rows were suppressed by the old ledger (got {recovered:?})"
    );
}

/// The dispatch ledger is created lazily, but a least-privilege deployment runs
/// the poller as a role with no CREATE on `core` — and PostgreSQL refuses
/// `CREATE TABLE IF NOT EXISTS` on privilege grounds *before* it checks whether
/// the table exists. Treating that error as "ledger missing" would stop the
/// poller dead on every poll of an already-migrated database.
#[tokio::test]
#[ignore = "requires PostgreSQL — run with --ignored --test-threads=1"]
async fn a_role_without_ddl_rights_still_polls_an_already_migrated_ledger() {
    const ROLE: &str = "fraiseql_dispatch_nodll";
    const ROLE_PASSWORD: &str = "dispatch_probe_password";

    let admin = pool().await;
    fresh_contract(&admin).await;

    // Migrate the ledger as the owner, exactly as a deploy would.
    sqlx::raw_sql(fraiseql_observers::migrations::observer_dispatch_sql())
        .execute(&admin)
        .await
        .unwrap();

    sqlx::query(&format!(
        "DO $$ BEGIN
             IF NOT EXISTS (SELECT 1 FROM pg_roles WHERE rolname = '{ROLE}') THEN
                 CREATE ROLE {ROLE} LOGIN PASSWORD '{ROLE_PASSWORD}' NOSUPERUSER;
             END IF;
         END $$;"
    ))
    .execute(&admin)
    .await
    .unwrap();
    for grant in [
        format!("ALTER ROLE {ROLE} NOSUPERUSER LOGIN PASSWORD '{ROLE_PASSWORD}'"),
        format!("GRANT USAGE ON SCHEMA core TO {ROLE}"),
        // Read the log, write the ledger — and pointedly no CREATE on the schema.
        format!("REVOKE CREATE ON SCHEMA core FROM {ROLE}"),
        format!("GRANT SELECT ON core.tb_entity_change_log TO {ROLE}"),
        format!("GRANT SELECT, INSERT ON core.tb_observer_dispatch TO {ROLE}"),
    ] {
        sqlx::query(&grant).execute(&admin).await.unwrap();
    }

    let pk: i64 = sqlx::query_scalar(INSERT_RETURNING_PK)
        .bind("Invoice")
        .bind(serde_json::json!({ "n": 1 }))
        .fetch_one(&admin)
        .await
        .unwrap();

    let restricted = PgPoolOptions::new()
        .max_connections(2)
        .connect_with(
            PgConnectOptions::from_str(&database_url())
                .unwrap()
                .username(ROLE)
                .password(ROLE_PASSWORD),
        )
        .await
        .unwrap_or_else(|e| panic!("connect as {ROLE}: {e}"));

    let mut listener = ChangeLogListener::new(
        ChangeLogListenerConfig::new(restricted)
            .with_listener_id(format!("least-privilege-{}", uuid::Uuid::new_v4())),
    );
    let batch = listener
        .next_batch()
        .await
        .expect("a role without CREATE on core must still poll a migrated ledger");
    assert_eq!(batch.iter().map(|e| e.id).collect::<Vec<_>>(), vec![pk]);
    listener
        .record_dispatched(&batch)
        .await
        .expect("recording needs INSERT, not CREATE");

    // Leave no role behind: a lingering grantee blocks later DROP ROLE runs.
    for cleanup in [
        format!("REVOKE ALL ON core.tb_observer_dispatch FROM {ROLE}"),
        format!("REVOKE ALL ON core.tb_entity_change_log FROM {ROLE}"),
        format!("REVOKE ALL ON SCHEMA core FROM {ROLE}"),
        format!("DROP ROLE IF EXISTS {ROLE}"),
    ] {
        sqlx::query(&cleanup).execute(&admin).await.unwrap();
    }
}
