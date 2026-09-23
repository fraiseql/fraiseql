//! The allowances one **request** holds, lent to every read it issues.
//!
//! A ceiling written as a per-request bound and enforced per read is not the control
//! the operator declared. `[validation] max_response_bytes` was that until `a09bc0dae`:
//! the REST `?select=` representation answers one request with the parent read plus one
//! sub-read per parent row per level, each read built a budget of its own, and the
//! ceiling therefore bounded every read and bounded their sum never.
//!
//! `[security.cost_budget] per_request_max` is the same control with the same defect —
//! its name says *per request* and `resolve_direct_read` scores each read alone. This
//! module holds the running total that fixes it ([`CostBudget`]) and the object a
//! transport builds once per request and lends to each of its reads
//! ([`RequestBudget`]).
//!
//! # Why the two allowances travel as one object
//!
//! Because the read chokepoint already takes one of them, and a second
//! `Option<&_>` beside it would be positional. `RowReadPlan` records the same
//! preference one file over — "a named field is harder to pass in the wrong position
//! than the fourth element of a tuple" — and the composed `LATERAL` statement has more
//! per-request state to come. A third allowance is a field here, not a fifth argument
//! there.
//!
//! The members stay `Option`, because "the operator declared no ceiling" is a shape
//! the code has to handle either way, and it is not the same as a ceiling of zero.
//!
//! # Why a cost aggregate is accumulated rather than scored
//!
//! A GraphQL document is scored whole, nesting included (`estimate_query_cost`),
//! because the document *states* its shape before anything runs. A `?select=` does
//! not: how many sub-reads it issues is one per parent row per level, and the parent
//! rows are not back yet. There is no point at which the request's total cost could be
//! computed in advance.
//!
//! So it is accumulated — charged by each read as that read resolves. The property the
//! gate exists for survives: the charge happens in `resolve_direct_read`, *before* the
//! statement is sent, so the read that crosses the ceiling is refused without reaching
//! the database. What changes is only which reads count against it.
//!
//! This is also a control the composed statement keeps. Under `LATERAL` the embedded
//! levels arrive in **one** read, and a running total charged once still bounds the
//! same work under the same ceiling. The arithmetic for scoring a nested projection is
//! the composition's to supply; the budget that score charges is this one, unchanged.

use std::sync::atomic::{AtomicU64, Ordering};

use super::response_budget::ResponseBudget;
use crate::error::{FraiseQLError, Result};

/// A running total of estimated operation cost, against
/// `[security.cost_budget] per_request_max`.
///
/// Charged before each read runs, by whoever resolves it. **Whoever holds one decides
/// what the ceiling bounds**, the same rule [`ResponseBudget`] states:
///
/// * a read that *is* the whole request resolves one of its own, so the ceiling bounds that read —
///   which is every direct read that is not part of a fan-out, and is the behaviour this type
///   preserves rather than changes;
/// * a request whose transport issues **several** reads on its behalf lends one budget to all of
///   them, so the ceiling bounds the request, which is what `per_request_max` has always said it
///   does.
///
/// `used` is atomic and [`charge`](Self::charge) takes `&self` for that second case, so
/// sharing a budget is holding it by reference rather than a second type with a second
/// copy of the rule.
///
/// Deliberately **not** `Clone`, for the reason [`ResponseBudget`] is not: a clone is an
/// unshared copy with its own zeroed total, which is exactly the defect above, and it
/// would compile in silence.
#[derive(Debug)]
pub struct CostBudget {
    limit: u64,
    used:  AtomicU64,
}

impl CostBudget {
    /// A budget for a declared ceiling, or `None` when the operator declared none.
    ///
    /// `Option` rather than an unbounded budget keeps "no ceiling configured" a shape
    /// the caller has to handle, instead of a `u64::MAX` that behaves like one until it
    /// overflows.
    #[must_use]
    pub const fn new(limit: Option<u64>) -> Option<Self> {
        match limit {
            Some(limit) => Some(Self {
                limit,
                used: AtomicU64::new(0),
            }),
            None => None,
        }
    }

    /// Cost charged so far.
    #[must_use]
    pub fn used(&self) -> u64 {
        self.used.load(Ordering::Relaxed)
    }

    /// The ceiling this budget enforces.
    #[must_use]
    pub const fn limit(&self) -> u64 {
        self.limit
    }

    /// Add `cost` to the running total, refusing once it crosses the ceiling.
    ///
    /// The total is updated before the comparison, so the `cost` reported in the error
    /// is the figure that crossed the line rather than the last one under it.
    ///
    /// `retry_after_secs` is `None`: a per-request ceiling is permanent for the request
    /// as issued, which is what makes it a `400` rather than the `429` a spent rolling
    /// window earns.
    ///
    /// # Errors
    ///
    /// [`FraiseQLError::CostExceeded`] once the accumulated total exceeds the ceiling.
    pub fn charge(&self, cost: u64) -> Result<()> {
        // `fetch_add` returns the previous total, so the figure compared — and reported —
        // is the one this charge produced. `Relaxed` is the whole ordering requirement:
        // the reads sharing a budget are sequential, so there is never contention, and the
        // atomic is here to allow `&self` across a `Send` future rather than to
        // synchronise anything.
        let used = self.used.fetch_add(cost, Ordering::Relaxed).saturating_add(cost);
        if used > self.limit {
            return Err(FraiseQLError::CostExceeded {
                message:          format!(
                    "operation cost {used} exceeds the schema-wide per-request maximum of {} \
                     ([security.cost_budget] per_request_max)",
                    self.limit
                ),
                cost:             used,
                limit:            self.limit,
                retry_after_secs: None,
            });
        }
        Ok(())
    }
}

/// Everything **one request** is allowed to spend, held by the transport that answers
/// it and borrowed by each read it issues.
///
/// Built by `Executor::request_budget` so the ceilings are read from the compiled
/// configuration in the one place that owns them, and a transport cannot supply a
/// budget with ceilings of its own choosing.
///
/// Not `Clone`, because neither member is, and for the same reason.
#[derive(Debug)]
pub struct RequestBudget {
    bytes: Option<ResponseBudget>,
    cost:  Option<CostBudget>,
}

impl RequestBudget {
    /// The allowances for one request, from the two compiled ceilings.
    ///
    /// Either may be absent, independently: an operator declares `[validation]
    /// max_response_bytes` and `[security.cost_budget] per_request_max` separately, and
    /// a budget holding neither is the shape a deployment that declared neither gets.
    #[must_use]
    pub const fn new(max_response_bytes: Option<u64>, max_operation_cost: Option<u64>) -> Self {
        Self {
            bytes: ResponseBudget::new(max_response_bytes),
            cost:  CostBudget::new(max_operation_cost),
        }
    }

    /// The response-bytes allowance, or `None` when no ceiling is configured.
    #[must_use]
    pub const fn bytes(&self) -> Option<&ResponseBudget> {
        self.bytes.as_ref()
    }

    /// The operation-cost allowance, or `None` when no ceiling is configured.
    #[must_use]
    pub const fn cost(&self) -> Option<&CostBudget> {
        self.cost.as_ref()
    }
}

#[cfg(test)]
mod tests;
