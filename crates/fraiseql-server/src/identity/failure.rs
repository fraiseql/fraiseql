//! The identity-resolution failure model (DESIGN §5) — one result type produced
//! by the resolver and interpreted by each call site.
//!
//! The load-bearing decision is *fail-closed at source*: anything other than
//! exactly one row with every mapped field present and non-null is a denial, and
//! a denial fails the operation — never a silent skip, never an empty-string GUC,
//! the mapped set applied whole or not at all.

use std::fmt;

/// Why a subject was **permanently** denied. The request must fail closed and
/// never proceed.
///
/// A `DenyReason` is logged server-side with the subject (DESIGN §5.4) but is
/// **never** surfaced to the client: a client that could distinguish
/// unknown-subject from ambiguous from null-field would have an existence oracle
/// over the actor table. The outward response is a uniform "forbidden".
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum DenyReason {
    /// No row matched the subject — unknown or unprovisioned.
    ZeroRows,
    /// More than one row matched — ambiguous identity, a misconfiguration.
    /// Refines #242's silent `LIMIT 1`: for identity, we refuse to pick one.
    Ambiguous,
    /// A declared mapped column was NULL or absent in the resolved row. Carries
    /// the column name (log-only). Denying here is what prevents an
    /// empty-string GUC a predicate could read as authorized.
    NullField(String),
    /// The query references a `$param` absent from the token claims. Carries the
    /// bare parameter name (log-only).
    MissingParam(String),
}

impl DenyReason {
    /// A short, **log-only** label (DESIGN §5.4): `zero-rows`, `ambiguous`,
    /// `null-field <name>`, or `missing-param <name>`. Never returned to a
    /// client.
    pub(super) fn log_label(&self) -> String {
        match self {
            Self::ZeroRows => "zero-rows".to_owned(),
            Self::Ambiguous => "ambiguous".to_owned(),
            Self::NullField(col) => format!("null-field {col}"),
            Self::MissingParam(name) => format!("missing-param {name}"),
        }
    }
}

/// A **transient** failure to resolve — DB unreachable, query error, pool
/// exhausted. Never cached (a blip must not pin a denial): the read path fails
/// the request (503) rather than falling through to an unscoped query, and the
/// send path retries.
#[derive(Debug, Clone)]
pub(super) struct ResolveError {
    message: String,
}

impl ResolveError {
    /// Wrap an underlying transient failure with a server-side message.
    pub(super) fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
        }
    }
}

impl fmt::Display for ResolveError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.message)
    }
}

/// The outcome of a single `sub → DB` lookup (DESIGN §5.1). Produced by the
/// resolver; each call site maps it to its own surface (403/503 for the sync
/// read path, retry/DLQ for the durable send path).
#[derive(Debug)]
pub(super) enum IdentityResolution {
    /// Exactly one row, every mapped field present and non-null. The map holds
    /// the renamed enriched fields (column → field applied). Merged under the
    /// `fraiseql.enriched.*` namespace by the read-path consumer.
    Resolved(serde_json::Map<String, serde_json::Value>),
    /// Permanent — fail closed, never proceed.
    Denied(DenyReason),
    /// Transient — infra failure; do not cache.
    Unavailable(ResolveError),
}
