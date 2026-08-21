//! Per-query read routing (#957).
//!
//! The read/write partition FraiseQL enforces is *structural*: a compiled query
//! is read-only by construction, so it is replica-eligible, and a mutation is
//! not. [`ReadRouting`] is the one place a schema author can say something the
//! structure cannot — that a particular read must not be stale, or that a
//! particular read may be staler than the rest.

use serde::{Deserialize, Serialize};

/// Where one compiled query's reads may be served from.
///
/// # Why this exists as a schema field
///
/// Read-replica routing is otherwise a whole-server decision: the pin window and
/// the staleness budget apply to every query alike, so an operator sizing them
/// for the strictest query gives up the offload on all the others, and sizing
/// them for the common case silently serves the strict one stale. A per-query
/// override is the only way to have both.
///
/// # The contract
///
/// FraiseQL defines and enforces this shape; an authoring language emits it.
/// A `@reads_from(...)` directive is one spelling
/// of it, and the spelling is that project's to choose — what is fixed here is
/// what the runtime can actually guarantee, which is exactly these three
/// answers. Replica **topology** deliberately stays out: URLs are server
/// configuration and secrets, not compiled schema.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
#[non_exhaustive]
pub enum ReadRouting {
    /// Server policy decides: replicas when configured and eligible, primary
    /// inside the read-your-writes pin window or when no replica qualifies.
    ///
    /// The default, and what every query did before this field existed.
    #[default]
    Any,

    /// Never serve this query from a replica.
    ///
    /// For reads where staleness is a correctness problem rather than a
    /// performance trade — a balance before a transfer, an authorization
    /// decision, the row a client is about to act on.
    ///
    /// **Also bypasses the result cache.** A cached row is stale data by
    /// construction, so a query that asked not to be served stale from a replica
    /// and was then served stale from memory would have got the opposite of what
    /// it asked for. `Primary` means "fresh", not "from a particular host".
    Primary,

    /// Prefer a replica even inside the read-your-writes pin window, optionally
    /// under a tighter (or looser) staleness budget than the server's.
    ///
    /// For reads that are explicitly allowed to be behind — feeds, dashboards,
    /// exports — where paying primary capacity for freshness nobody needs is the
    /// waste worth naming.
    ///
    /// Still falls back to the primary when no replica qualifies: this is a
    /// statement about which server *should* answer, and refusing to answer at
    /// all would turn a capacity preference into an outage.
    Replica {
        /// Staleness budget for this query alone, in milliseconds.
        ///
        /// `None` uses the server's `read_replica_max_lag_ms`. A value here
        /// replaces it rather than intersecting with it — a query that knows it
        /// tolerates ten minutes is as legitimate as one that needs 50 ms, and
        /// both are the schema author's call.
        ///
        /// A budget on a server with no measurement configured is still honoured:
        /// probing is unconditional, so the reading exists whether or not the
        /// server set a global budget.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        max_lag_ms: Option<u64>,
    },
}

impl ReadRouting {
    /// Whether this is [`Any`](Self::Any) — the serde skip predicate, so a schema
    /// that never mentions routing round-trips byte-identically.
    #[must_use]
    pub const fn is_default(&self) -> bool {
        matches!(self, Self::Any)
    }

    /// Whether a replica may serve this query at all.
    #[must_use]
    pub const fn allows_replica(&self) -> bool {
        !matches!(self, Self::Primary)
    }

    /// Whether the read-your-writes pin window applies to this query.
    ///
    /// [`Replica`](Self::Replica) opts out of it: the pin exists so a client
    /// reads its own writes, and a query annotated as tolerating staleness has
    /// said that is not what it is for.
    #[must_use]
    pub const fn honours_write_pin(&self) -> bool {
        !matches!(self, Self::Replica { .. })
    }

    /// Whether a result-cache hit may serve this query.
    ///
    /// See [`Primary`](Self::Primary) for why it may not.
    #[must_use]
    pub const fn allows_cached_result(&self) -> bool {
        !matches!(self, Self::Primary)
    }

    /// This query's staleness budget in milliseconds, given the server's.
    #[must_use]
    pub const fn effective_max_lag_ms(&self, server_max_lag_ms: Option<u64>) -> Option<u64> {
        match self {
            Self::Replica {
                max_lag_ms: Some(ms),
            } => Some(*ms),
            _ => server_max_lag_ms,
        }
    }
}

#[cfg(test)]
mod tests;
