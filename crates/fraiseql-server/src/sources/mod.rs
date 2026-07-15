//! Scheduled ingress sources (#573) — the runtime half of the `Source` primitive.
//!
//! A `Source` is the dual of an observer: on a cron schedule, under a single-firing
//! lease, a Model B (Deno) connector fetches an external system and drives the
//! results into the database via mutations, resuming from a durable cursor. This
//! module holds the server-side wiring:
//!
//! - [`SourceQueryExecutor`] — the [`QueryExecutor`](fraiseql_functions::host::live::QueryExecutor)
//!   bridge that lets a source's `fraiseql_query` mutations execute against the server's
//!   [`Executor`](fraiseql_core::runtime::Executor) under the source's `run_as` identity (#573 D6).
//!
//! The source scheduler that drives connectors on their schedule lands alongside
//! this (Phase 06 Step 3); native pull sources (poll-IMAP email) run under the
//! `inbound-email` feature independently.

mod executor;

pub use executor::{SOURCE_TENANT_VAR, SourceQueryExecutor};
