//! The aggregate bound on what one `?select=` request may read.
//!
//! Every other control on this path sees a single sub-read. The cost gate scores one,
//! `[validation] max_response_bytes` charges one, `[rest] max_page_size` clamps one. The
//! fan-out that makes embedding expensive lives *above* all of them, in
//! `executor::embed_into_rows`: one sub-read per parent row, recursing per row, so the
//! reads a request performs multiply per level while each individual read stays cheap
//! enough to pass every per-read check.
//!
//! This is the counter that closes that gap. It is deliberately **not** an estimate
//! derived from the request — see [`RestConfig::max_embedded_reads`] for why the
//! document path's multiply-per-level arithmetic is the wrong model here — but a tally
//! of the sub-reads actually issued, shared by every level and every relationship of one
//! request.
//!
//! [`RestConfig::max_embedded_reads`]: fraiseql_core::schema::RestConfig::max_embedded_reads

use std::sync::atomic::{AtomicU64, Ordering};

use axum::http::StatusCode;

use crate::routes::rest::handler::RestError;

/// How many embedded sub-reads one request has spent, against its ceiling.
///
/// One budget is built per request and travels on
/// [`EmbeddingRequest`](super::EmbeddingRequest) and `executor::EmbedCtx`, including into
/// the nested requests the recursion builds — so a request's second level is charged
/// against the same tally as its first, which is the whole point. A budget rebuilt per
/// level would bound each level's width and leave their product unbounded, which is the
/// defect this type exists to close.
///
/// The row embeds and the `.count` embeds of one request share it too: both issue reads
/// against the same pool on behalf of the same request, and a tally that counted only
/// one of them would be bounded in name only.
#[derive(Debug)]
pub struct EmbedReadBudget {
    /// The ceiling, or `0` for no bound.
    limit: u64,
    /// Sub-reads charged so far.
    ///
    /// Atomic rather than a `Cell` because the futures this is borrowed across must be
    /// `Send`; the embed pass itself is sequential, so there is never contention and
    /// `Relaxed` is the whole ordering requirement.
    spent: AtomicU64,
}

impl EmbedReadBudget {
    /// A budget of `limit` sub-reads, or an unbounded one when `limit` is `0`.
    #[must_use]
    pub const fn new(limit: u64) -> Self {
        Self {
            limit,
            spent: AtomicU64::new(0),
        }
    }

    /// Charge one sub-read, or refuse the request because this one would cross the
    /// ceiling.
    ///
    /// Called immediately **before** each read is issued, so the read that would exceed
    /// the budget is never performed.
    ///
    /// # Errors
    ///
    /// Returns a `413 TOO_MANY_EMBEDDED_READS` [`RestError`] once the tally would exceed
    /// the configured ceiling.
    pub fn charge(&self) -> Result<(), RestError> {
        if self.limit == 0 {
            return Ok(());
        }
        // `fetch_add` returns the previous value, so this read is the (previous + 1)-th.
        let this_read = self.spent.fetch_add(1, Ordering::Relaxed).saturating_add(1);
        if this_read > self.limit {
            return Err(too_many_embedded_reads(self.limit));
        }
        Ok(())
    }

    /// How many sub-reads have been charged.
    #[must_use]
    pub fn spent(&self) -> u64 {
        self.spent.load(Ordering::Relaxed)
    }
}

/// The refusal owed to a request whose embedding would read more than the deployment
/// allows.
///
/// Refused rather than answered with the rows gathered so far: an embed served in part
/// is indistinguishable from a parent that genuinely has fewer related rows, which is
/// #1230's failure shape under a `200`. Same posture as `resume_too_far_behind`, and the
/// same status — what was asked for is too big.
fn too_many_embedded_reads(limit: u64) -> RestError {
    RestError {
        status:  StatusCode::PAYLOAD_TOO_LARGE,
        code:    "TOO_MANY_EMBEDDED_READS",
        message: format!(
            "This request's `?select=` embedding would perform more than {limit} sub-reads, \
             which is this deployment's bound (`[rest].max_embedded_reads`). Embedding \
             resolves one sub-query per parent row per level, so the reads multiply with \
             the page size at each level. Narrow `?select=`, lower `?limit=`, or raise the \
             bound."
        ),
        details: None,
    }
}
