//! The function-dispatch ledger, reached through the executor that writes it (#1628).
//!
//! A write records its dispatches through the executor's write handle; the sweep that runs
//! a dispatch its process never finished, and the settlement of each one, go through the same
//! handle, so a tenant's dispatches are claimed and settled where its writes recorded them.

use std::time::Duration;

use super::Executor;
use crate::{
    backend::{DispatchLedger, DispatchSettlement, DueDispatch, PendingDispatch},
    error::Result,
};

impl Executor {
    fn dispatch_ledger(&self) -> Result<&dyn DispatchLedger> {
        self.ctx
            .writer
            .as_deref()
            .ok_or_else(crate::backend::dispatch_ledger_unsupported)?
            .dispatch_ledger()
    }

    /// Create the ledger table where this executor's writes record into.
    ///
    /// # Errors
    ///
    /// When the executor cannot write or its adapter keeps no ledger, or the DDL fails.
    pub async fn provision_dispatch_ledger(&self) -> Result<()> {
        self.dispatch_ledger()?.provision_dispatch_ledger().await
    }

    /// Record the durable ones of `dispatches` outside any write, leased for `lease`.
    ///
    /// # Errors
    ///
    /// As [`provision_dispatch_ledger`](Self::provision_dispatch_ledger), or the insert fails.
    pub async fn record_dispatches(
        &self,
        dispatches: &[PendingDispatch],
        lease: Duration,
    ) -> Result<()> {
        self.dispatch_ledger()?.record_dispatches(dispatches, lease).await
    }

    /// Claim up to `batch` dispatches recorded for `tenant` whose lease ran out, renewing it
    /// for `lease`.
    ///
    /// # Errors
    ///
    /// As [`provision_dispatch_ledger`](Self::provision_dispatch_ledger), or the claim fails.
    pub async fn claim_due_dispatches(
        &self,
        tenant: Option<&str>,
        batch: u32,
        lease: Duration,
    ) -> Result<Vec<DueDispatch>> {
        self.dispatch_ledger()?.claim_due_dispatches(tenant, batch, lease).await
    }

    /// Settle a recorded dispatch.
    ///
    /// # Errors
    ///
    /// As [`provision_dispatch_ledger`](Self::provision_dispatch_ledger), or the update fails.
    pub async fn settle_dispatch(
        &self,
        source: &str,
        ref_key: &str,
        function_name: &str,
        settlement: &DispatchSettlement,
    ) -> Result<()> {
        self.dispatch_ledger()?
            .settle_dispatch(source, ref_key, function_name, settlement)
            .await
    }
}
