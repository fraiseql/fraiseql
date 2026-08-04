//! Durable long-running operations: submit / status / cancel (#391).
//!
//! Fire-and-poll for operations that cannot finish inside a request timeout:
//! `POST /operations/v1/{operation}` persists the submission and returns an
//! `op_id` immediately; background workers execute it **through the same
//! `execute_with_security` pipeline as `/graphql`** (never a second execution
//! path — the `execute_query_direct` drift class); `GET`/`DELETE` read and
//! cancel by id, scoped to the submitting principal.
//!
//! Designed against P19's six saga-recovery failure modes, each pinned by a
//! test in `tests/async_operations_e2e_pg.rs`:
//!
//! 1. **Replay re-executing completed work** — terminal states are never claimable; the state
//!    machine lives in the store's conditional UPDATEs.
//! 2. **Claiming without a staleness threshold** — a `running` row is only reclaimable when its
//!    heartbeat is older than `stuck_threshold_secs`.
//! 3. **Discarded results** — failures record their error terminally; a superseded worker's late
//!    completion is a claim-guarded no-op.
//! 4. **Missing idempotency tokens** — `Idempotency-Key` on submission deduplicates through the
//!    same store `/graphql` uses (#747).
//! 5. **Wrong-database replay** — the resolved tenant key is persisted at submission and execution
//!    dispatches through the shared tenant seam.
//! 6. **Fabricated status** — `GET` reads the stored row; nothing is inferred, and a cancel that
//!    did not cancel is never reported as cancelled (#746).

pub mod store;
pub mod worker;

use std::sync::Arc;

pub use store::{AsyncOperation, AsyncOperationStore};

use crate::server_config::AsyncOperationsConfig;

/// Everything the routes and workers share: the durable store plus the
/// validated configuration (allowlist, attempt ceiling, worker cadence).
#[derive(Debug, Clone)]
pub struct AsyncOperationsRuntime {
    /// The durable operation store.
    pub store:  Arc<AsyncOperationStore>,
    /// The validated `[async_operations]` configuration.
    pub config: Arc<AsyncOperationsConfig>,
}
