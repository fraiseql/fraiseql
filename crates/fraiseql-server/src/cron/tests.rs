//! Tests for server-side cron scheduling (#595).
//!
//! The DB-backed tests self-skip without `DATABASE_URL`; they pin the durable
//! `_fraiseql_cron_state` record and the advisory-lease single-firing that make cron
//! fire exactly once across replicas.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::print_stderr)] // Reason: test module

use fraiseql_observers::{LeaseGuardedRunner, RunOutcome};

use super::PgCronState;

async fn pool() -> Option<sqlx::PgPool> {
    let url = fraiseql_test_support::try_database_url()?;
    sqlx::PgPool::connect(&url).await.ok()
}

#[tokio::test]
async fn pg_cron_state_records_and_increments_fire_count() {
    let Some(pool) = pool().await else {
        eprintln!("skipping #595 cron-state test: no DATABASE_URL");
        return;
    };
    let state = PgCronState::new(pool.clone());
    state.init().await.expect("cron state DDL");

    // A unique function name so parallel runs / prior runs do not collide.
    let function = format!("purge_test_{}", std::process::id());
    let now = chrono::Utc::now();

    state.record_fire(&function, "0 2 * * *", now).await.expect("first fire");
    state.record_fire(&function, "0 2 * * *", now).await.expect("second fire");

    let count: i64 = sqlx::query_scalar(
        "SELECT fire_count FROM _fraiseql_cron_state WHERE function_name = $1 AND cron_expr = $2",
    )
    .bind(&function)
    .bind("0 2 * * *")
    .fetch_one(&pool)
    .await
    .expect("row present");

    assert_eq!(count, 2, "two fires upsert to fire_count = 2");

    // Cleanup this test's row.
    let _ = sqlx::query("DELETE FROM _fraiseql_cron_state WHERE function_name = $1")
        .bind(&function)
        .execute(&pool)
        .await;
}

#[tokio::test]
async fn cron_state_read_back_after_restart_suppresses_refire_in_same_window() {
    use fraiseql_functions::triggers::{CronDecision, CronSchedule};

    let Some(pool) = pool().await else {
        eprintln!("skipping #796 cron-state loader test: no DATABASE_URL");
        return;
    };
    let state = PgCronState::new(pool.clone());
    state.init().await.expect("cron state DDL");

    let function = format!("resume_test_{}", std::process::id());
    let expr = "0 2 * * *";
    let schedule = CronSchedule::parse(expr).expect("parse cron");

    // The process fired at 02:00:07.312 and recorded it durably.
    let fired_at = chrono::DateTime::parse_from_rfc3339("2026-03-01T02:00:07.312+00:00")
        .expect("parse")
        .with_timezone(&chrono::Utc);
    state.record_fire(&function, expr, fired_at).await.expect("record fire");

    // A restart rebuilds its in-memory window state from `_fraiseql_cron_state`
    // (pre-fix: `PgCronState` had `record_fire` and no loader, so the documented
    // cross-restart guard read nothing back).
    let resumed = state.resume_state(&function, expr).await.expect("resume state");

    // A tick later in the SAME window must be suppressed by the window guard…
    let same_window: fraiseql_time::Timestamp =
        "2026-03-01T02:00:41.900+00:00".parse().expect("parse");
    assert!(
        matches!(resumed.decide(&schedule, &same_window), CronDecision::AlreadyFired { .. }),
        "restart inside an already-fired window must not re-fire it"
    );

    // …and the NEXT window must fire.
    let next_window: fraiseql_time::Timestamp =
        "2026-03-02T02:00:11.500+00:00".parse().expect("parse");
    assert!(
        matches!(resumed.decide(&schedule, &next_window), CronDecision::Fire),
        "the next day's window fires normally after a restart"
    );

    // A function with no persisted row resumes with a clean state.
    let fresh = state
        .resume_state(&format!("{function}_never_fired"), expr)
        .await
        .expect("resume state for unfired function");
    assert!(
        matches!(fresh.decide(&schedule, &next_window), CronDecision::Fire),
        "no persisted firing ⇒ the first matching window fires"
    );

    let _ = sqlx::query("DELETE FROM _fraiseql_cron_state WHERE function_name = $1")
        .bind(&function)
        .execute(&pool)
        .await;
}

#[tokio::test]
async fn two_replicas_single_fire_under_the_advisory_lease() {
    let Some(pool) = pool().await else {
        eprintln!("skipping #595 cron-lease test: no DATABASE_URL");
        return;
    };
    // Two runners keyed on the same cron function name model two replicas. One
    // acquires the session advisory lock; the other must skip (not-leader).
    let key = format!("cron:lease_test_{}", std::process::id());
    let a = LeaseGuardedRunner::postgres(pool.clone(), key.clone());
    let b = LeaseGuardedRunner::postgres(pool.clone(), key.clone());

    // Replica A holds the lease for the whole closure; while it runs, replica B's
    // attempt must be skipped. We coordinate with a barrier so B runs mid-A-closure.
    let (tx, rx) = tokio::sync::oneshot::channel::<()>();
    let (done_tx, done_rx) = tokio::sync::oneshot::channel::<()>();

    let a_task = tokio::spawn(async move {
        a.run(|| async move {
            // Signal B to attempt now, then hold the lease until B has finished.
            let _ = tx.send(());
            let _ = done_rx.await;
        })
        .await
    });

    rx.await.expect("A signalled");
    let b_outcome = b.run(|| async {}).await.expect("B lease attempt");
    let _ = done_tx.send(());
    let a_outcome = a_task.await.expect("A joined").expect("A lease attempt");

    assert!(matches!(a_outcome, RunOutcome::Ran(())), "replica A ran");
    assert!(
        matches!(b_outcome, RunOutcome::SkippedNotLeader),
        "replica B skipped while A held the lease — single-firing"
    );
}

/// #1627: a cron function runs under its declared `timeout_ms`, not the 5 s default
/// every poller used to get.
#[test]
fn a_cron_function_runs_under_its_declared_timeout() {
    use std::{sync::Arc, time::Duration};

    use fraiseql_functions::{
        FunctionDefinition, FunctionModule, FunctionObserver, RuntimeType, TriggerRegistry,
    };

    use crate::{schema::loader::FunctionsConfig, subsystems::FunctionsSubsystem};

    let definitions = vec![
        FunctionDefinition::new("purge", "cron:0 2 * * *", RuntimeType::Wasm).with_timeout(30_000),
    ];
    let hooks = FunctionsSubsystem {
        observer:         Arc::new(FunctionObserver::new()),
        trigger_registry: TriggerRegistry::load_from_definitions(&definitions).unwrap(),
        module_registry:  std::collections::HashMap::from([(
            "purge".to_string(),
            FunctionModule::from_source("purge".to_string(), String::new(), RuntimeType::Wasm),
        )]),
        config:           FunctionsConfig {
            module_dir: "/functions".into(),
            dlq_store: None,
            definitions,
        },
    }
    .into_before_mutation_hooks();

    let kept = super::schedulable(&hooks);
    assert_eq!(kept.len(), 1, "the cron function is schedulable");
    assert_eq!(kept[0].limits.max_duration, Duration::from_secs(30));
}

/// A replica's poller for the every-minute cron function `function`, with its own lease and
/// its own window state read from the durable record at its boot, as `build_cron_pollers`
/// builds one. No runtime is registered: a firing invokes the module and fails, which still
/// spends the window, as on one replica.
async fn replica(pool: &sqlx::PgPool, function: &str) -> super::ScheduledPoller {
    use std::sync::Arc;

    use fraiseql_functions::{
        FunctionModule, FunctionObserver, ResourceLimits, RuntimeType,
        host::live::HostContextConfig,
        triggers::{CronTrigger, Schedule},
    };

    let expression = "* * * * *";
    let state = PgCronState::new(pool.clone());
    let initial_state = state.resume_state(function, expression).await.unwrap();
    super::ScheduledPoller::cron(
        &CronTrigger {
            function_name: function.to_string(),
            schedule:      expression.to_string(),
            timezone:      "UTC".to_string(),
        },
        Schedule::parse(expression, None).unwrap(),
        FunctionModule::from_source(function.to_string(), String::new(), RuntimeType::Wasm),
        Arc::new(FunctionObserver::new()),
        Arc::new(NoQueries),
        fraiseql_core::security::SecurityContext::system_job(
            function,
            function,
            vec![],
            vec![],
            None,
        ),
        LeaseGuardedRunner::postgres(pool.clone(), format!("cron:{function}")),
        state,
        HostContextConfig::default(),
        ResourceLimits::default(),
        None,
        initial_state,
    )
}

/// A query bridge the counting runtime never calls.
struct NoQueries;

impl fraiseql_functions::host::live::QueryExecutor for NoQueries {
    fn execute_query(
        &self,
        _query: &str,
        _variables: Option<&serde_json::Value>,
    ) -> std::pin::Pin<
        Box<
            dyn std::future::Future<Output = fraiseql_error::Result<serde_json::Value>> + Send + '_,
        >,
    > {
        Box::pin(std::future::ready(Ok(serde_json::Value::Null)))
    }
}

/// Two replicas booted before a window, ticking in it one after the other (each ticks once
/// a minute from its own start, so their ticks rarely overlap): the window fires once. The
/// second replica's lease is free by then, so only the durable record, read under the
/// lease, can tell it the window already fired.
#[tokio::test]
async fn two_replicas_ticking_one_after_the_other_fire_a_window_once() {
    let Some(pool) = pool().await else {
        eprintln!("skipping cron window test: no DATABASE_URL");
        return;
    };
    PgCronState::new(pool.clone()).init().await.unwrap();
    let function = format!("window_once_{}", uuid::Uuid::new_v4().simple());
    let mut a = replica(&pool, &function).await;
    let mut b = replica(&pool, &function).await;

    let at = fraiseql_time::Timestamp::now();
    assert!(matches!(a.tick(at).await, super::Tick::Ran { .. }));
    assert_eq!(
        b.tick(at).await,
        super::Tick::AlreadyFired,
        "the window fires once across both replicas"
    );
    let _ = sqlx::query("DELETE FROM _fraiseql_cron_state WHERE function_name = $1")
        .bind(&function)
        .execute(&pool)
        .await;
}

/// A replica resumed from a stored firing at 02:00:07.312 treats the rest of that minute as
/// fired and fires again at the next one, wherever in it the tick lands: the stored
/// instant crosses from the state table into the schedule with its fraction.
#[tokio::test]
async fn a_replica_resumed_from_a_stored_firing_fires_at_the_next_minute() {
    let Some(pool) = pool().await else {
        eprintln!("skipping cron resume test: no DATABASE_URL");
        return;
    };
    PgCronState::new(pool.clone()).init().await.unwrap();
    let function = format!("resume_next_{}", uuid::Uuid::new_v4().simple());
    let at = |text: &str| text.parse::<fraiseql_time::Timestamp>().unwrap();
    PgCronState::new(pool.clone())
        .record_fire(&function, "* * * * *", at("2026-03-01T02:00:07.312Z").into())
        .await
        .unwrap();
    for (tick, expected) in [
        ("2026-03-01T02:00:07.312Z", super::Tick::AlreadyFired),
        ("2026-03-01T02:00:59.999Z", super::Tick::AlreadyFired),
    ] {
        assert_eq!(replica(&pool, &function).await.tick(at(tick)).await, expected, "{tick}");
    }
    let next = replica(&pool, &function).await.tick(at("2026-03-01T02:01:00.500Z")).await;
    assert!(matches!(next, super::Tick::Ran { .. }), "{next:?}");
    let _ = sqlx::query("DELETE FROM _fraiseql_cron_state WHERE function_name = $1")
        .bind(&function)
        .execute(&pool)
        .await;
}

/// Two replicas firing one window tick at different seconds of its minute, and both carry
/// the window's scheduled instant, so the same idempotency token: a window fired again
/// after a replica died before recording it is one the module can recognise.
#[tokio::test]
async fn ticks_in_one_window_carry_the_same_idempotency_token() {
    use std::sync::Arc;

    use fraiseql_functions::{
        FunctionModule, FunctionObserver, ResourceLimits, RuntimeType,
        host::live::HostContextConfig,
        triggers::{CronTrigger, Schedule},
    };

    // A lazy pool never connects: this derives tokens, it does not query.
    let pool = sqlx::PgPool::connect_lazy("postgres://unused/unused").unwrap();
    let expression = "* * * * *";
    let poller = super::ScheduledPoller::cron(
        &CronTrigger {
            function_name: "tokens".to_string(),
            schedule:      expression.to_string(),
            timezone:      "UTC".to_string(),
        },
        Schedule::parse(expression, None).unwrap(),
        FunctionModule::from_source("tokens".to_string(), String::new(), RuntimeType::Wasm),
        Arc::new(FunctionObserver::new()),
        Arc::new(NoQueries),
        fraiseql_core::security::SecurityContext::system_job(
            "tokens",
            "tokens",
            vec![],
            vec![],
            None,
        ),
        LeaseGuardedRunner::in_process("cron:tokens"),
        PgCronState::new(pool),
        HostContextConfig::default(),
        ResourceLimits::default(),
        None,
        fraiseql_functions::triggers::CronExecutionState::new(),
    );
    let token = |second: i64| {
        poller.idempotency_token(
            &poller.payload(fraiseql_time::Timestamp::from_second(second).unwrap()),
        )
    };

    // The start of a minute: 1_800_000_000 is a multiple of 60.
    let minute = 1_800_000_000;
    assert_eq!(token(minute + 7), token(minute + 43), "one window, one token");
    assert_ne!(token(minute + 7), token(minute + 67), "the next window has its own");
}

/// A replica killed mid-firing, after its module ran and before its window is recorded, hands
/// the window to the next replica that ticks in it (#2000): the lock goes with the killed
/// session, the record it was writing on that session fails with it, and that replica fires
/// the window, with the idempotency token the killed one derived. Delivery is at least once.
///
/// The kill is the session ending, which is what `PostgreSQL` sees of a killed process. A row
/// lock the test holds on the window record stalls the killed replica's write, so it is killed
/// at that point and nowhere else.
#[tokio::test]
async fn a_replica_killed_before_recording_hands_its_window_to_the_next() {
    use std::time::{Duration, Instant};

    let Some(pool) = pool().await else {
        eprintln!("skipping cron handover test: no DATABASE_URL");
        return;
    };
    PgCronState::new(pool.clone()).init().await.unwrap();
    let function = format!("handover_{}", uuid::Uuid::new_v4().simple());
    let minute = 1_800_000_000;
    let at_a = fraiseql_time::Timestamp::from_second(minute + 10).unwrap();
    let at_b = fraiseql_time::Timestamp::from_second(minute + 40).unwrap();
    // An earlier window fired, so the record exists for the test to lock.
    let earlier = chrono::DateTime::from_timestamp(minute - 86_400, 0).unwrap();
    PgCronState::new(pool.clone())
        .record_fire(&function, "* * * * *", earlier)
        .await
        .unwrap();
    let mut held = pool.begin().await.unwrap();
    sqlx::query("SELECT 1 FROM _fraiseql_cron_state WHERE function_name = $1 FOR UPDATE")
        .bind(&function)
        .execute(&mut *held)
        .await
        .unwrap();

    let mut a = replica(&pool, &function).await;
    let token_a = a.idempotency_token(&a.payload(at_a));
    let firing = tokio::spawn(async move { a.tick(at_a).await });
    // A ran its module and waits on the record, on the session holding its lease.
    let pid = loop {
        let waiting: Option<i32> = sqlx::query_scalar(
            "SELECT pid FROM pg_stat_activity \
             WHERE wait_event_type = 'Lock' AND query LIKE 'INSERT INTO _fraiseql_cron_state%'",
        )
        .fetch_optional(&pool)
        .await
        .unwrap();
        if let Some(pid) = waiting {
            break pid;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    };
    let mut b = replica(&pool, &function).await;
    assert_eq!(
        b.tick(at_b).await,
        super::Tick::SkippedNotLeader,
        "while A holds the lease, B does not fire"
    );

    let killed_at = Instant::now();
    sqlx::query("SELECT pg_terminate_backend($1)")
        .bind(pid)
        .execute(&pool)
        .await
        .unwrap();
    // A write that outlived the session would wait on the test's row lock, so A never ends.
    let a_ended = tokio::time::timeout(Duration::from_secs(5), firing)
        .await
        .expect("A ends once its session is gone: its record is not retried on another")
        .unwrap();
    assert!(
        matches!(a_ended, super::Tick::Ran { .. }),
        "A's module ran before it was killed"
    );
    held.rollback().await.unwrap();

    // C ticks later in the same window: A's record died with its session, so C fires it. B
    // does not tick again in this window (it ticks once a minute), so a window whose leader
    // dies after every other replica ticked in it is not fired again.
    let mut c = replica(&pool, &function).await;
    assert!(matches!(c.tick(at_b).await, super::Tick::Ran { .. }), "the window is not lost");
    let handed_over_in = killed_at.elapsed();
    assert_eq!(
        c.idempotency_token(&c.payload(at_b)),
        token_a,
        "C fires the window with the token A derived for it"
    );
    assert!(handed_over_in < Duration::from_secs(1), "handed over in {handed_over_in:?}");
    let _ = sqlx::query("DELETE FROM _fraiseql_cron_state WHERE function_name = $1")
        .bind(&function)
        .execute(&pool)
        .await;
}
