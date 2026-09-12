//! The resume reader, against a real PostgreSQL.
//!
//! Every test here is `#[ignore]`d and fails loudly without `DATABASE_URL`, rather than
//! returning early: the CI line that runs them (`--lib -- --ignored`) always binds a
//! database, so a silent skip could only ever hide a real failure.
//!
//! ```bash
//! DATABASE_URL=postgresql://fraiseql_test:fraiseql_test_password@localhost:5433/test_fraiseql \
//!   cargo test -p fraiseql-observers --features postgres --lib listener::replay -- \
//!   --ignored --test-threads=1
//! ```

#![allow(clippy::unwrap_used, clippy::panic)] // Reason: test code, panics are the failure mechanism

use serde_json::json;
use sqlx::postgres::PgPool;
use uuid::Uuid;

use super::{ChangeLogReplayReader, ReplayScope, ResumeAnchor, ResumePosition};

const LISTENER: &str = "replay-tests";

async fn pool() -> PgPool {
    let url = fraiseql_test_support::try_database_url()
        .expect("DATABASE_URL must be set for listener::replay tests");
    let pool = PgPool::connect(&url).await.expect("connect to the test database");
    sqlx::raw_sql(crate::migrations::entity_change_log_contract_sql())
        .execute(&pool)
        .await
        .expect("change-log contract DDL");
    sqlx::raw_sql(crate::migrations::observer_dispatch_sql())
        .execute(&pool)
        .await
        .expect("dispatch ledger DDL");
    pool
}

/// An entity type nothing else in the suite writes, so these tests never read each
/// other's rows (or a previous run's).
fn unique_type(label: &str) -> String {
    format!("Replay{label}_{}", Uuid::new_v4().simple())
}

/// Write one contract row and return `(pk, seq)`.
async fn insert_row(
    pool: &PgPool,
    object_type: &str,
    tenant: Option<Uuid>,
    marker: &str,
) -> (i64, i64) {
    sqlx::query_as(
        "INSERT INTO core.tb_entity_change_log \
           (object_type, modification_type, object_id, object_data, tenant_id) \
         VALUES ($1, 'INSERT', gen_random_uuid(), $2, $3) \
         RETURNING pk_entity_change_log, seq",
    )
    .bind(object_type)
    .bind(json!({ "marker": marker }))
    .bind(tenant)
    .fetch_one(pool)
    .await
    .expect("insert change-log row")
}

/// Record one row as dispatched, exactly as `record_dispatched` does — the ledger key
/// is the row's stable UUID, and `dispatched_at` defaults to the recording statement's
/// clock.
async fn record_dispatched(pool: &PgPool, pk: i64) {
    sqlx::query(
        "INSERT INTO core.tb_observer_dispatch (listener_id, change_log_id, created_at) \
         SELECT $1, e.id, e.created_at FROM core.tb_entity_change_log e \
         WHERE e.pk_entity_change_log = $2 \
         ON CONFLICT (listener_id, change_log_id) DO NOTHING",
    )
    .bind(LISTENER)
    .bind(pk)
    .execute(pool)
    .await
    .expect("record dispatch");
}

fn reader(pool: &PgPool) -> ChangeLogReplayReader {
    ChangeLogReplayReader::new(pool.clone(), LISTENER.to_string())
}

fn scope(object_type: &str, tenant: Option<&str>) -> ReplayScope {
    ReplayScope {
        object_type: object_type.to_string(),
        tenant:      tenant.map(String::from),
    }
}

async fn markers(
    reader: &ChangeLogReplayReader,
    scope: &ReplayScope,
    from: &ResumePosition,
) -> Vec<String> {
    reader
        .page(scope, from, 100)
        .await
        .expect("read a replay page")
        .into_iter()
        .map(|replayed| replayed.event.data["marker"].as_str().unwrap().to_string())
        .collect()
}

async fn found(reader: &ChangeLogReplayReader, scope: &ReplayScope, seq: i64) -> ResumePosition {
    match reader.anchor(scope, seq).await.expect("resolve the anchor") {
        ResumeAnchor::Found(position) => position,
        ResumeAnchor::Unknown => panic!("seq {seq} should resolve to a position"),
    }
}

// ---------------------------------------------------------------------------
// The defect this reader exists for
// ---------------------------------------------------------------------------

/// **The acceptance test.** A row whose transaction committed late is delivered *after*
/// a row with a higher `seq`, so the client's `Last-Event-ID` names the higher one. A
/// resume that reads `seq > last` loses the late row for ever; this reads the recorded
/// delivery order instead and returns it.
///
/// The state built here — B dispatched before A, though A holds the lower sequence — is
/// exactly what `change_log_commit_order_pg` produces with two real concurrent
/// transactions and the real poller. It is built directly here so the assertion is about
/// the reader and not about the scheduler.
#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn a_row_dispatched_after_the_anchor_is_replayed_even_though_its_seq_is_lower() {
    let pool = pool().await;
    let object_type = unique_type("LateCommit");

    // A takes the lower sequence (its INSERT ran first) …
    let (a_pk, a_seq) = insert_row(&pool, &object_type, None, "A_slow").await;
    let (b_pk, b_seq) = insert_row(&pool, &object_type, None, "B_fast").await;
    assert!(a_seq < b_seq, "A must hold the lower sequence for this test to mean anything");

    // … but B commits first, so the poller dispatches B, and only later A.
    record_dispatched(&pool, b_pk).await;
    tokio::time::sleep(std::time::Duration::from_millis(10)).await;
    record_dispatched(&pool, a_pk).await;

    let reader = reader(&pool);
    let scope = scope(&object_type, None);
    let anchor = found(&reader, &scope, b_seq).await;

    assert_eq!(
        markers(&reader, &scope, &anchor).await,
        vec!["A_slow".to_string()],
        "the row dispatched after the anchor must be replayed; reading `seq > {b_seq}` \
         would return nothing and lose it silently"
    );
}

/// The mirror of the test above: nothing the client already received comes back. The
/// anchor's own row is excluded, and so is everything dispatched before it.
#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn a_replay_does_not_re_send_what_was_delivered_before_the_anchor() {
    let pool = pool().await;
    let object_type = unique_type("NoDuplicates");

    let (first_pk, _) = insert_row(&pool, &object_type, None, "first").await;
    record_dispatched(&pool, first_pk).await;
    tokio::time::sleep(std::time::Duration::from_millis(10)).await;
    let (anchor_pk, anchor_seq) = insert_row(&pool, &object_type, None, "anchor").await;
    record_dispatched(&pool, anchor_pk).await;
    tokio::time::sleep(std::time::Duration::from_millis(10)).await;
    let (after_pk, _) = insert_row(&pool, &object_type, None, "after").await;
    record_dispatched(&pool, after_pk).await;

    let reader = reader(&pool);
    let scope = scope(&object_type, None);
    let anchor = found(&reader, &scope, anchor_seq).await;

    assert_eq!(markers(&reader, &scope, &anchor).await, vec!["after".to_string()]);
}

/// Two rows recorded in the **same** batch share a `dispatched_at`, so the position has
/// to break the tie by pk — otherwise resuming from the first row of a batch re-sends
/// the whole batch, or skips the rest of it.
#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn a_resume_inside_one_batch_continues_after_its_own_row() {
    let pool = pool().await;
    let object_type = unique_type("SameBatch");

    let (first_pk, first_seq) = insert_row(&pool, &object_type, None, "first").await;
    let (second_pk, _) = insert_row(&pool, &object_type, None, "second").await;

    // One statement, one `now()` — the shape `record_dispatched` writes per batch.
    sqlx::query(
        "INSERT INTO core.tb_observer_dispatch (listener_id, change_log_id, created_at) \
         SELECT $1, e.id, e.created_at FROM core.tb_entity_change_log e \
         WHERE e.pk_entity_change_log = ANY($2::bigint[]) \
         ON CONFLICT DO NOTHING",
    )
    .bind(LISTENER)
    .bind(vec![first_pk, second_pk])
    .execute(&pool)
    .await
    .unwrap();

    let reader = reader(&pool);
    let scope = scope(&object_type, None);
    let anchor = found(&reader, &scope, first_seq).await;

    assert_eq!(markers(&reader, &scope, &anchor).await, vec!["second".to_string()]);
}

// ---------------------------------------------------------------------------
// Anchors that cannot be honoured
// ---------------------------------------------------------------------------

/// A sequence no row carries — pruned by retention, or an id from another deployment.
/// Unresumable: with the anchor gone, nothing can establish what came after it.
#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn a_sequence_no_row_carries_is_unknown() {
    let pool = pool().await;
    let reader = reader(&pool);

    assert_eq!(
        reader.anchor(&scope(&unique_type("Missing"), None), 1).await.unwrap(),
        ResumeAnchor::Unknown
    );
}

/// An id the client picked up on a *different* resource's stream. The anchor is scoped
/// like the stream is, so it does not resolve — a stream must not resume from a position
/// in someone else's delivery order.
#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn an_anchor_from_another_entity_type_is_unknown() {
    let pool = pool().await;
    let mine = unique_type("Mine");
    let theirs = unique_type("Theirs");

    let (pk, seq) = insert_row(&pool, &theirs, None, "not-mine").await;
    record_dispatched(&pool, pk).await;

    assert_eq!(
        reader(&pool).anchor(&scope(&mine, None), seq).await.unwrap(),
        ResumeAnchor::Unknown
    );
}

/// The same rule for the tenant gate: an anchor outside the caller's tenant does not
/// resolve, so a multi-tenant caller cannot resume into another tenant's ordering.
#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn an_anchor_in_another_tenant_is_unknown() {
    let pool = pool().await;
    let object_type = unique_type("TenantAnchor");
    let theirs = Uuid::new_v4();
    let mine = Uuid::new_v4();

    let (pk, seq) = insert_row(&pool, &object_type, Some(theirs), "theirs").await;
    record_dispatched(&pool, pk).await;

    assert_eq!(
        reader(&pool)
            .anchor(&scope(&object_type, Some(&mine.to_string())), seq)
            .await
            .unwrap(),
        ResumeAnchor::Unknown
    );
}

// ---------------------------------------------------------------------------
// Scope gates on the replay itself
// ---------------------------------------------------------------------------

/// A replayed stream carries what a live stream carries: its own entity type, and — in a
/// multi-tenant deployment — its own tenant. An event stamped with **no** tenant does not
/// match a tenant-scoped replay, the same fail-closed rule the live gate applies.
#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn a_replay_carries_only_its_own_scope() {
    let pool = pool().await;
    let object_type = unique_type("Scoped");
    let other_type = unique_type("ScopedOther");
    let mine = Uuid::new_v4();
    let theirs = Uuid::new_v4();

    let (anchor_pk, anchor_seq) = insert_row(&pool, &object_type, Some(mine), "anchor").await;
    record_dispatched(&pool, anchor_pk).await;
    tokio::time::sleep(std::time::Duration::from_millis(10)).await;

    for (ty, tenant, marker) in [
        (&object_type, Some(mine), "mine"),
        (&object_type, Some(theirs), "another tenant"),
        (&object_type, None, "no tenant"),
        (&other_type, Some(mine), "another type"),
    ] {
        let (pk, _) = insert_row(&pool, ty, tenant, marker).await;
        record_dispatched(&pool, pk).await;
    }

    let reader = reader(&pool);
    let scope = scope(&object_type, Some(&mine.to_string()));
    let anchor = found(&reader, &scope, anchor_seq).await;

    assert_eq!(markers(&reader, &scope, &anchor).await, vec!["mine".to_string()]);
}

// ---------------------------------------------------------------------------
// The anchor that has been delivered but not yet recorded
// ---------------------------------------------------------------------------

/// The ledger is written *after* a batch is forwarded, so a client reconnecting inside
/// that window names a real event with no recorded position. It resumes by insertion
/// order rather than being refused, which re-sends at worst and never skips.
#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn an_anchor_not_yet_recorded_resumes_by_insertion_order() {
    let pool = pool().await;
    let object_type = unique_type("Unrecorded");

    let (_, anchor_seq) = insert_row(&pool, &object_type, None, "anchor").await;
    insert_row(&pool, &object_type, None, "after").await;

    let reader = reader(&pool);
    let scope = scope(&object_type, None);
    let anchor = found(&reader, &scope, anchor_seq).await;

    assert_eq!(
        anchor.dispatched_at, None,
        "an unrecorded dispatch must be visible as such, not guessed at"
    );
    assert_eq!(markers(&reader, &scope, &anchor).await, vec!["after".to_string()]);
}

// ---------------------------------------------------------------------------
// The bound
// ---------------------------------------------------------------------------

/// The backlog count stops at the caller's limit, so a resume point far behind the head
/// costs one bounded index scan instead of a walk over everything since.
#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn the_backlog_count_stops_at_the_limit() {
    let pool = pool().await;
    let object_type = unique_type("Bounded");

    let (anchor_pk, anchor_seq) = insert_row(&pool, &object_type, None, "anchor").await;
    record_dispatched(&pool, anchor_pk).await;
    tokio::time::sleep(std::time::Duration::from_millis(10)).await;
    for i in 0..5 {
        let (pk, _) = insert_row(&pool, &object_type, None, &format!("after-{i}")).await;
        record_dispatched(&pool, pk).await;
    }

    let reader = reader(&pool);
    let anchor = found(&reader, &scope(&object_type, None), anchor_seq).await;

    assert_eq!(reader.count_since(&anchor, 3).await.unwrap(), 3, "counting stops at the limit");
    assert!(
        reader.count_since(&anchor, 1_000).await.unwrap() >= 6,
        "with room to count, the anchor's own batch and everything after it are counted"
    );
}

// ---------------------------------------------------------------------------
// One projection
// ---------------------------------------------------------------------------

/// A replayed event must be the event the poller would have delivered for the same row —
/// not a second projection of it that drifts a field at a time. Both sides decode
/// through `ChangeLogEntry::to_entity_event`, and this is what says so.
#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn a_replayed_event_is_the_event_the_poller_delivers() {
    use crate::listener::{ChangeLogListener, ChangeLogListenerConfig};

    let pool = pool().await;
    let object_type = unique_type("SameShape");
    let tenant = Uuid::new_v4();

    let (anchor_pk, anchor_seq) = insert_row(&pool, &object_type, Some(tenant), "anchor").await;
    record_dispatched(&pool, anchor_pk).await;
    tokio::time::sleep(std::time::Duration::from_millis(10)).await;
    let (subject_pk, _) = insert_row(&pool, &object_type, Some(tenant), "subject").await;

    // What the poller makes of the row.
    let mut listener = ChangeLogListener::new(
        ChangeLogListenerConfig::new(pool.clone())
            .with_batch_size(1_000)
            .with_listener_id(format!("poller-{}", Uuid::new_v4().simple())),
    );
    let live = listener
        .next_batch()
        .await
        .expect("poll")
        .into_iter()
        .find(|entry| entry.id == subject_pk)
        .expect("the poller must see the subject row")
        .to_entity_event()
        .expect("the poller decodes it");

    // What the resume reader makes of the same row.
    record_dispatched(&pool, subject_pk).await;
    let reader = reader(&pool);
    let scope = scope(&object_type, Some(&tenant.to_string()));
    let anchor = found(&reader, &scope, anchor_seq).await;
    let replayed = reader
        .page(&scope, &anchor, 100)
        .await
        .expect("read a replay page")
        .into_iter()
        .find(|replayed| replayed.event.data["marker"] == "subject")
        .expect("the replay must carry the subject row")
        .event;

    assert_eq!(
        serde_json::to_value(&replayed).unwrap(),
        serde_json::to_value(&live).unwrap(),
        "a replayed event and the live event for the same row must be one value"
    );
}

// ---------------------------------------------------------------------------
// The in-flight tail
// ---------------------------------------------------------------------------

/// The race the tail closes: an event published to the fan-out while the client was
/// disconnected, and recorded in the ledger only after the client reconnected and read
/// its catch-up pages. The ledger-ordered page cannot see it — it is not in the ledger
/// yet — and the live receiver never will, because it subscribed after the publish.
#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn the_in_flight_tail_carries_what_the_ledger_has_not_placed_yet() {
    let pool = pool().await;
    let object_type = unique_type("InFlight");

    let (anchor_pk, anchor_seq) = insert_row(&pool, &object_type, None, "anchor").await;
    record_dispatched(&pool, anchor_pk).await;
    // Published to the fan-out, not yet recorded — the window this exists for.
    insert_row(&pool, &object_type, None, "published-not-recorded").await;

    let reader = reader(&pool);
    let scope = scope(&object_type, None);
    let anchor = found(&reader, &scope, anchor_seq).await;

    assert!(
        markers(&reader, &scope, &anchor).await.is_empty(),
        "the ledger-ordered page cannot see a row the ledger has not placed"
    );

    let tail: Vec<String> = reader
        .page_in_flight(&scope, &anchor, 100)
        .await
        .expect("read the in-flight tail")
        .into_iter()
        .map(|replayed| replayed.event.data["marker"].as_str().unwrap().to_string())
        .collect();
    assert_eq!(tail, vec!["published-not-recorded".to_string()]);
}

/// The tail carries only what the ledger has not placed: a recorded row is served by the
/// ordered page and must not be sent twice.
#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn the_in_flight_tail_excludes_rows_the_ledger_already_placed() {
    let pool = pool().await;
    let object_type = unique_type("TailExcludes");

    let (anchor_pk, anchor_seq) = insert_row(&pool, &object_type, None, "anchor").await;
    record_dispatched(&pool, anchor_pk).await;
    let (recorded_pk, _) = insert_row(&pool, &object_type, None, "recorded").await;
    record_dispatched(&pool, recorded_pk).await;
    insert_row(&pool, &object_type, None, "unrecorded").await;

    let reader = reader(&pool);
    let scope = scope(&object_type, None);
    let anchor = found(&reader, &scope, anchor_seq).await;

    let tail: Vec<String> = reader
        .page_in_flight(&scope, &anchor, 100)
        .await
        .unwrap()
        .into_iter()
        .map(|replayed| replayed.event.data["marker"].as_str().unwrap().to_string())
        .collect();
    assert_eq!(tail, vec!["unrecorded".to_string()]);
    assert_eq!(reader.count_in_flight(&scope, &anchor, 100).await.unwrap(), 1);
}

/// The tail obeys the same scope gates as the ordered page — it is the same stream.
#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn the_in_flight_tail_carries_only_its_own_scope() {
    let pool = pool().await;
    let object_type = unique_type("TailScoped");
    let other_type = unique_type("TailOther");
    let mine = Uuid::new_v4();
    let theirs = Uuid::new_v4();

    let (anchor_pk, anchor_seq) = insert_row(&pool, &object_type, Some(mine), "anchor").await;
    record_dispatched(&pool, anchor_pk).await;
    insert_row(&pool, &object_type, Some(mine), "mine").await;
    insert_row(&pool, &object_type, Some(theirs), "another tenant").await;
    insert_row(&pool, &object_type, None, "no tenant").await;
    insert_row(&pool, &other_type, Some(mine), "another type").await;

    let reader = reader(&pool);
    let scope = scope(&object_type, Some(&mine.to_string()));
    let anchor = found(&reader, &scope, anchor_seq).await;

    let tail: Vec<String> = reader
        .page_in_flight(&scope, &anchor, 100)
        .await
        .unwrap()
        .into_iter()
        .map(|replayed| replayed.event.data["marker"].as_str().unwrap().to_string())
        .collect();
    assert_eq!(tail, vec!["mine".to_string()]);
}
