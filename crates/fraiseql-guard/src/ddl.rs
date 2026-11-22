//! The one lock every FraiseQL-owned DDL script runs under.
//!
//! The scripts that create FraiseQL's own tables are idempotent: `CREATE SCHEMA IF NOT
//! EXISTS`, `CREATE TABLE IF NOT EXISTS`, then `ALTER TABLE … ENABLE ROW LEVEL SECURITY`
//! and `DROP POLICY IF EXISTS` / `CREATE POLICY`. Idempotent is not concurrency-safe:
//!
//! - **cold**, `IF NOT EXISTS` checks the catalogue and then creates, so two sessions both pass the
//!   check and one loses on a catalogue unique index (`pg_namespace_nspname_index`,
//!   `pg_class_relname_nsp_index`, `pg_type_typname_nsp_index`);
//! - **warm**, the `ALTER TABLE` and `CREATE POLICY` statements take `ACCESS EXCLUSIVE`, and two
//!   sessions can deadlock on one relation.
//!
//! Two server instances booting against one database, or two tests initialising one store,
//! are enough. [`lock_prefixed`] makes a transaction-scoped advisory lock the script's first
//! statement, so no two scripts hold a catalogue entry or a relation lock at once.
//!
//! # Why one key for every script
//!
//! The catalogue indexes the cold race loses on are database-wide, not per relation: two
//! different scripts creating two different tables contend on them as two runs of one
//! script do. A key per script, or per crate, leaves the cross-script race open, and that
//! is the one a boot runs into, with several subsystems initialising at once. So the key
//! lives here, where every crate that owns a script can reach it.

/// The advisory-lock key every FraiseQL-owned DDL script takes before its statements.
///
/// The ASCII bytes of `FRAISE` followed by a version byte, so a key seen in `pg_locks`
/// during an incident is recognisable rather than an arbitrary number. Advisory locks
/// share one 64-bit space per database with every other application on it, which is the
/// reason to pick a value nothing else plausibly picks.
pub const MIGRATION_LOCK_KEY: i64 = 0x4652_4149_5345_0001;

/// `ddl` with the advisory-lock acquisition as its first statement.
///
/// Run the result as one multi-statement simple query (`sqlx::raw_sql`, `psql -c`): such a
/// query executes in a single implicit transaction, so the lock is held for the whole
/// script and released when it commits or fails, with no unlock to forget on an error
/// path. Run statement by statement, outside one transaction, the lock would be released
/// before the DDL it guards.
#[must_use]
pub fn lock_prefixed(ddl: &str) -> String {
    format!("SELECT pg_advisory_xact_lock({MIGRATION_LOCK_KEY});\n{ddl}")
}

#[cfg(test)]
mod tests;
