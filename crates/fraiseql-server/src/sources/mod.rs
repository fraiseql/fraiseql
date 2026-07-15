//! Scheduled ingress sources (#573) — the runtime half of the `Source` primitive.
//!
//! A `Source` is the dual of an observer: on a cron schedule, under a single-firing
//! lease, a Model B (Deno) connector fetches an external system and drives the
//! results into the database via mutations, resuming from a durable cursor. This
//! module holds the server-side wiring:
//!
//! - [`SourceQueryExecutor`] — the [`QueryExecutor`](fraiseql_functions::host::live::QueryExecutor)
//!   bridge that lets a source's `fraiseql_query` mutations execute against the server's
//!   [`Executor`](fraiseql_core::runtime::Executor) under the source's `run_as` identity.
//! - [`SourcePoller`] — the per-source scheduler loop: cron-tick → single-firing lease → a cursor +
//!   executor host → invoke the Deno connector.
//!
//! Native pull sources (poll-IMAP email) run under the `inbound-email` feature
//! independently. Lifecycle wiring — reading `sources` from the compiled schema and
//! spawning a poller per enabled source — is assembled by [`build_source_pollers`].

mod executor;
mod metrics;
mod poller;
mod scheduler;

pub use executor::{SOURCE_TENANT_VAR, SourceQueryExecutor};
pub use poller::SourcePoller;
pub use scheduler::{build_source_pollers, source_host_config, sources_enabled};
