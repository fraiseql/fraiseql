//! Focused sub-executors for each query family.
//!
//! Each runner holds an `Arc<ExecutorContext<A>>` and is responsible for
//! one class of database operation. Runners do not call each other directly —
//! all cross-runner coordination goes through [`Executor`].

pub(super) mod aggregate;
pub(super) mod mutation;
pub(super) mod query;
