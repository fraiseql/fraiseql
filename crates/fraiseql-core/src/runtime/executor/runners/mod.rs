//! Focused sub-executors for each query family.
//!
//! Each runner holds an `Arc<ExecutorContext<A>>` and is responsible for
//! one class of database operation. Runners do not call each other directly —
//! all cross-runner coordination goes through [`Executor`].

pub(super) mod query;
// pub(super) mod mutation;    ← added in Phase 3
// pub(super) mod aggregate;   ← added in Phase 4
