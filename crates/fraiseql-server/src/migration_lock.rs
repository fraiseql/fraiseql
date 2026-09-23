//! Mutual exclusion for the server's own DDL migrations.
//!
//! Every subsystem that owns a `_fraiseql_*` operational table runs an idempotent
//! `CREATE TABLE IF NOT EXISTS` / `CREATE INDEX IF NOT EXISTS` / `ALTER TABLE … ENABLE
//! ROW LEVEL SECURITY` script at startup — the cron state, the inbound spine, the
//! e-mail send tracker, the function DLQ, the observer checkpoints and the CDC outbox
//! sink state. Six scripts, six subsystems, all booting at once against one database.
//!
//! **`IF NOT EXISTS` is not atomic.** It checks the catalogue and then creates, so two
//! sessions running the same script concurrently both pass the check and one loses on a
//! catalogue unique index. Measured against the docker test stack, four concurrent runs
//! of the send-tracking script from an empty database: *three of the four failed, in ten
//! rounds out of ten*, with
//!
//! ```text
//! ERROR:  duplicate key value violates unique constraint "pg_type_typname_nsp_index"
//! ERROR:  duplicate key value violates unique constraint "pg_class_relname_nsp_index"
//! ```
//!
//! Against a database where the tables already exist the same concurrency fails
//! differently — the later `ALTER TABLE` and `CREATE POLICY` statements take
//! `ACCESS EXCLUSIVE`, and two runs deadlock waiting on each other for one relation:
//!
//! ```text
//! ERROR:  deadlock detected
//! DETAIL:  Process A waits for AccessExclusiveLock on relation R; blocked by process B.
//!          Process B waits for AccessExclusiveLock on relation R; blocked by process A.
//! ```
//!
//! Both are one defect: the scripts are idempotent but not **concurrency-safe**, and
//! nothing serialises them. A second server instance starting against a live database is
//! enough — the loser's subsystem fails to initialise, for a script whose only job was to
//! be a no-op.
//!
//! [`run_migration`] takes a transaction-scoped advisory lock as the script's first
//! statement, so no two migrations hold a relation lock at the same time and neither
//! failure mode has a window to occur in.
//!
//! # Why one key for every script rather than one per script
//!
//! The catalogue indexes the cold-start race loses on — `pg_class_relname_nsp_index` and
//! `pg_type_typname_nsp_index` — are **database-wide**, not per relation. Two *different*
//! scripts creating two *different* tables contend on them exactly as two runs of one
//! script do. A per-script key would serialise the case that is easiest to notice and
//! leave the cross-script case — the one that actually happens at startup, six scripts at
//! once — racing as before.

use sqlx::PgPool;

/// The advisory-lock key every server-owned migration script takes before its DDL.
///
/// The ASCII bytes of `FRAISE` followed by a version byte, so a key seen in `pg_locks`
/// during an incident is recognisable rather than an arbitrary number. Advisory locks
/// share one 64-bit space per database with every other application on it, which is the
/// reason to pick a value nothing else plausibly picks.
pub const MIGRATION_LOCK_KEY: i64 = 0x4652_4149_5345_0001;

/// Run a server-owned migration script under [`MIGRATION_LOCK_KEY`].
///
/// `ddl` is the script as its owning crate publishes it, unchanged — the constants stay
/// pure DDL for the e2e suites that apply them directly on one connection. The lock
/// statement is prepended rather than issued separately because a multi-statement simple
/// query executes in a single implicit transaction: the lock is therefore held for the
/// whole script and released on commit, with no explicit transaction to plumb through six
/// call sites and no path on which an early return leaves it held.
///
/// # Errors
///
/// The `sqlx::Error` of the lock acquisition or of any statement in `ddl`, so each
/// caller's existing error mapping applies unchanged.
pub async fn run_migration(pool: &PgPool, ddl: &str) -> Result<(), sqlx::Error> {
    sqlx::raw_sql(&lock_prefixed(ddl)).execute(pool).await.map(|_| ())
}

/// `ddl` with the advisory-lock acquisition as its first statement.
///
/// Separate from `run_migration` so a test can assert the composed script's shape
/// without a database: that the lock comes *first*, and that the caller's DDL is
/// carried through unaltered.
fn lock_prefixed(ddl: &str) -> String {
    format!("SELECT pg_advisory_xact_lock({MIGRATION_LOCK_KEY});\n{ddl}")
}

#[cfg(test)]
mod tests;
