//! The ONE provisioner for the shared sample `test` schema (#996).
//!
//! The sample entities (`test.tb_project`, `test.tb_user`, `test.tb_task`,
//! `test.tb_document` and their `v_*` views) back the integration suites of
//! both `fraiseql-core` and `fraiseql-wire`. They used to be provisioned from
//! **byte-identical copies** of the same two files vendored into each crate's
//! `tests/fixtures/`, with no single owner and two contradictory seeding
//! strategies: the wire helper seeded only when the tables were empty, the core
//! helper seeded unconditionally on every provisioning.
//!
//! On a database both suites touch — a full-workspace run, which is the run that
//! makes a red mean a regression — the core helper appended a fresh copy of the
//! seed for every test process, and the wire suite's count and ordering
//! assertions failed against rows it never created.
//!
//! One owner, one shape: [`SAMPLE_SCHEMA_SQL`] and [`SAMPLE_SEED_SQL`] are the
//! only definitions, and both are safe to apply repeatedly, in any order, from
//! any number of processes:
//!
//! - the schema is idempotent (`CREATE … IF NOT EXISTS` / `CREATE OR REPLACE`);
//! - the seed is idempotent **by construction** — every row carries a fixed id, so its `ON CONFLICT
//!   (id) DO NOTHING` clauses fire. With `gen_random_uuid()` ids they were dead code and each
//!   application duplicated the whole seed.
//!
//! So consumers need no "seed only when empty" guard and cannot drift from each
//! other by getting one wrong. Execute both as multi-statement batches
//! (`tokio_postgres::Client::batch_execute` or equivalent).
//!
//! Tests must treat these relations as **read-only shared fixtures**: a suite
//! needing writable rows creates its own, uniquely named (the phase-03 ownership
//! rule, gated by `fraiseql-db`'s `seed_fixture_integrity` suite).

/// The sample `test` schema DDL. Idempotent; apply as a batch.
pub const SAMPLE_SCHEMA_SQL: &str = include_str!("../fixtures/sample_schema.sql");

/// The sample `test` schema seed data. Idempotent by construction (fixed ids +
/// `ON CONFLICT (id) DO NOTHING`); apply as a batch, any number of times.
pub const SAMPLE_SEED_SQL: &str = include_str!("../fixtures/sample_seed.sql");

#[cfg(test)]
mod tests;
