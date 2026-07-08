//! Scheduled-ingress coordination primitives (#573 `Source`).
//!
//! `Source` is the dual of `Observer`: on a trigger, pull from an external system
//! and drive the results into the database via mutations, resuming from a durable
//! cursor and firing on exactly one replica. This module holds the two generic,
//! driver-light primitives that make that rock-solid, placed beside the observer
//! [`CheckpointStore`](crate::checkpoint::CheckpointStore) /
//! [`CheckpointLease`](crate::listener::CheckpointLease) family because they are
//! the same concern — durable watermarks and multi-replica coordination — and the
//! functions subsystem that drives sources already depends on this crate.
//!
//! - [`SourceCursorStore`] / [`PostgresSourceCursorStore`] — the durable, opaque per-source cursor
//!   with compare-and-swap advance.
//! - [`LeaseGuardedRunner`] — single-firing: run a source's work on one replica.

pub mod cursor;
pub mod runner;

#[cfg(test)]
mod tests;

pub use cursor::{CursorSnapshot, PostgresSourceCursorStore, SourceCursorStore};
pub use runner::{LeaseGuardedRunner, RunOutcome, lock_id};
