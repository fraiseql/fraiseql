//! # FraiseQL Test Support
//!
//! Single source of truth for how a FraiseQL integration test obtains its backing
//! service (database, cache, message bus, secret store).
//!
//! ## One policy, no drift
//!
//! Every service getter follows the same rule:
//!
//! 1. If the service's env URL is set (e.g. `DATABASE_URL`) **and its host:port is reachable**
//!    (short-timeout TCP probe), use it. A configured-but-unreachable service reads as absent,
//!    announced on stderr — availability means *reachable*, not *configured* (#879).
//! 2. Otherwise, with the `local-testcontainers` feature, spawn an ephemeral container on the local
//!    Docker daemon (Ryuk reaper on) and use that.
//! 3. Otherwise return `None` — the caller skips.
//!
//! CI never enables `local-testcontainers`: Dagger provisions the services and
//! injects the URLs, identically to a local `dagger call test-integration`. The
//! testcontainers code path is therefore not compiled into CI binaries, so the
//! container-leak class is impossible there by construction.
//!
//! Getters that can spawn a local container are `async`; the pure env-reader
//! [`vault`] is sync. Env-only services in the spawnable family stay `async` so
//! adding local spawn later is not a signature change for callers.
//!
//! ```no_run
//! #[tokio::test]
//! async fn needs_postgres() {
//!     let Some(pg) = fraiseql_test_support::postgres().await else {
//!         eprintln!("SKIP: no postgres (set DATABASE_URL or enable local-testcontainers)");
//!         return;
//!     };
//!     let _url = pg.url();
//! }
//! ```

pub mod changelog;
pub mod db;
pub mod sample_schema;
pub mod services;

pub use db::{database_url, failover_standby_database_url, standby_database_url, try_database_url};
pub use services::{Service, Vault, azure_blob, gcs, minio, nats, postgres, redis, vault};
