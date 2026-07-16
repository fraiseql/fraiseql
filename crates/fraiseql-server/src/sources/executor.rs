//! The source query-executor bridge (#573).
//!
//! Extracted into the shared [`crate::query_bridge`] module in #594 so scheduled
//! sources and event-dispatched functions run their `fraiseql_query` mutations
//! through the exact same authority + hot-reload seam. `SourceQueryExecutor` is a
//! type alias for the shared [`RunAsQueryExecutor`](crate::query_bridge::RunAsQueryExecutor)
//! so the sources wiring and its integration tests are unchanged.

pub use crate::query_bridge::{RunAsQueryExecutor as SourceQueryExecutor, SOURCE_TENANT_VAR};
