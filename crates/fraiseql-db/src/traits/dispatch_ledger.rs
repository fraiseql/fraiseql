//! The function-dispatch ledger (#1628): what makes `after:mutation` and `after:capture`
//! dispatch at-least-once.
//!
//! A write that triggers a durable function records one `pending` row per function in
//! `_fraiseql_function_dispatch`, in the write's own transaction (a [`DispatchLedgerSink`] on
//! the [`WriteRequest`](super::WriteRequest)), leased to the process about to run it. When the
//! dispatch ends, the row is settled: deleted once the function ran, kept as `dead_lettered`
//! (with its reason) until an operator clears it. A process that dies first leaves the row
//! `pending`; once its lease runs out, a sweep claims it ([`DispatchLedger::claim_due_dispatches`])
//! and runs it again. A dispatch can therefore run twice (a lease that expires while the first
//! run is still in flight), never zero times.

use std::{sync::Mutex, time::Duration};

use async_trait::async_trait;
use fraiseql_error::{FraiseQLError, Result};

/// The ledger table: in the schema the writing connection resolves first, so a tenant whose
/// connections put its own schema first keeps its own ledger.
pub const FUNCTION_DISPATCH_TABLE: &str = "_fraiseql_function_dispatch";

/// The ledger's DDL. Idempotent: `IF NOT EXISTS` throughout.
///
/// `state` is `pending` until the dispatch settles, then `dead_lettered`; a dispatch that ran
/// is deleted, so no payload outlives its delivery. `(source, ref_key, function_name)` is the
/// dispatch's identity: a write mints a fresh `ref_key`, a captured change carries its
/// change-log row id, so a change read twice records its dispatches once.
pub const FUNCTION_DISPATCH_LEDGER_DDL: &str = "\
CREATE TABLE IF NOT EXISTS _fraiseql_function_dispatch (
    pk_function_dispatch BIGINT GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
    source               TEXT NOT NULL,
    ref_key              TEXT NOT NULL,
    function_name        TEXT NOT NULL,
    payload              JSONB NOT NULL,
    idempotency_token    TEXT NOT NULL,
    caller               JSONB,
    tenant               TEXT,
    state                TEXT NOT NULL DEFAULT 'pending'
                         CHECK (state IN ('pending', 'dead_lettered')),
    reason               TEXT,
    attempts             INTEGER NOT NULL DEFAULT 1,
    lease_until          TIMESTAMPTZ NOT NULL,
    created_at           TIMESTAMPTZ NOT NULL DEFAULT now(),
    settled_at           TIMESTAMPTZ,
    UNIQUE (source, ref_key, function_name)
);
CREATE INDEX IF NOT EXISTS idx_function_dispatch_due
    ON _fraiseql_function_dispatch (lease_until) WHERE state = 'pending';
";

/// One function dispatch a write or a captured change calls for.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PendingDispatch {
    /// The trigger kind: `after:mutation` or `after:capture`.
    pub source:            String,
    /// What the dispatch is for: a key minted per committed write, or the change-log row id.
    pub ref_key:           String,
    /// The function to run.
    pub function_name:     String,
    /// The event payload it runs with.
    pub payload:           serde_json::Value,
    /// The idempotency token every run of this dispatch hands the function.
    pub idempotency_token: String,
    /// The principal whose write triggered it, when one did.
    pub caller:            Option<serde_json::Value>,
    /// The tenant whose executor committed the write, when one did.
    pub tenant:            Option<String>,
    /// Whether the dispatch is recorded. A `re_runnable` function asks for one attempt and no
    /// record: it is dispatched, never recorded.
    pub durable:           bool,
}

/// The dispatches one write call records, staged by the commit gate and inserted by the
/// adapter in the same attempt's transaction, after the gate passes.
///
/// The gate stages afresh on every attempt (a deadlock retry runs it again), replacing what
/// an earlier, rolled-back attempt staged.
#[derive(Debug)]
pub struct DispatchLedgerSink {
    lease:  Duration,
    staged: Mutex<Vec<PendingDispatch>>,
}

impl DispatchLedgerSink {
    /// An empty sink whose rows are leased for `lease` to the dispatch about to run.
    #[must_use]
    pub const fn new(lease: Duration) -> Self {
        Self {
            lease,
            staged: Mutex::new(Vec::new()),
        }
    }

    /// How long a recorded row is leased to the live dispatch.
    #[must_use]
    pub const fn lease(&self) -> Duration {
        self.lease
    }

    /// Replace the staged dispatches with `dispatches`.
    pub fn stage(&self, dispatches: Vec<PendingDispatch>) {
        *self.staged.lock().unwrap_or_else(std::sync::PoisonError::into_inner) = dispatches;
    }

    /// Every staged dispatch, durable or not.
    #[must_use]
    pub fn staged(&self) -> Vec<PendingDispatch> {
        self.staged.lock().unwrap_or_else(std::sync::PoisonError::into_inner).clone()
    }
}

/// A recorded dispatch the sweep claimed: its lease renewed, its attempt counted.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DueDispatch {
    /// The trigger kind.
    pub source:            String,
    /// What the dispatch is for.
    pub ref_key:           String,
    /// The function to run.
    pub function_name:     String,
    /// The event payload it runs with.
    pub payload:           serde_json::Value,
    /// The idempotency token recorded with it.
    pub idempotency_token: String,
    /// The principal whose write triggered it, when one did.
    pub caller:            Option<serde_json::Value>,
    /// The tenant whose executor committed the write, when one did.
    pub tenant:            Option<String>,
    /// How many times it has now been dispatched, this one included.
    pub attempts:          i32,
}

/// How a dispatch ended, for its row.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DispatchSettlement {
    /// The function ran: the row is deleted.
    Dispatched,
    /// It will not run again by itself: the row stays, `dead_lettered`, with this reason.
    DeadLettered(String),
}

/// Where a write's dispatches are recorded, claimed and settled.
// async_trait: dyn-dispatch required; remove when RTN + Send is stable (RFC 3425)
#[async_trait]
pub trait DispatchLedger: Send + Sync {
    /// Create the ledger table where this adapter's connections resolve it.
    ///
    /// # Errors
    ///
    /// A database error from the DDL.
    async fn provision_dispatch_ledger(&self) -> Result<()>;

    /// Record `dispatches` (their durable ones) outside any write, leased for `lease`: a
    /// captured change's, before the change-log reader moves past it. A dispatch already
    /// recorded is left as it is.
    ///
    /// # Errors
    ///
    /// A database error from the insert.
    async fn record_dispatches(
        &self,
        dispatches: &[PendingDispatch],
        lease: Duration,
    ) -> Result<()>;

    /// Claim up to `batch` `pending` dispatches of `tenant` (`None`: those recorded with no
    /// tenant) whose lease ran out, renewing it for `lease`. Concurrent sweepers skip each
    /// other's rows. Scoped by tenant as well as by schema: under row-level tenancy every
    /// tenant's connections resolve the same table.
    ///
    /// # Errors
    ///
    /// A database error from the claim.
    async fn claim_due_dispatches(
        &self,
        tenant: Option<&str>,
        batch: u32,
        lease: Duration,
    ) -> Result<Vec<DueDispatch>>;

    /// Settle the pending dispatch of `function_name` for `(source, ref_key)`. A row already
    /// settled is left as it is.
    ///
    /// # Errors
    ///
    /// A database error from the update.
    async fn settle_dispatch(
        &self,
        source: &str,
        ref_key: &str,
        function_name: &str,
        settlement: &DispatchSettlement,
    ) -> Result<()>;
}

/// The refusal of an adapter that keeps no dispatch ledger.
#[must_use]
pub fn dispatch_ledger_unsupported() -> FraiseQLError {
    FraiseQLError::Unsupported {
        message: "this database adapter keeps no function-dispatch ledger, so a durable \
                  after:mutation or after:capture dispatch cannot be recorded"
            .to_string(),
    }
}
