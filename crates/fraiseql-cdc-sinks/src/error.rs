//! Error type for the outbound CDC sink crate.

use thiserror::Error;

/// Errors raised while draining the change-log outbox or connecting a sink.
///
/// Per-event publish failures are *not* modelled here — those flow through
/// [`crate::PublishOutcome`] so the drain worker can classify them as transient
/// (retry with backoff) or permanent (dead-letter). `CdcError` is reserved for
/// failures of the drain machinery itself (database, sink connection, config).
#[derive(Debug, Error)]
#[non_exhaustive]
pub enum CdcError {
    /// A database error while reading the outbox or updating delivery state.
    #[error("database error: {0}")]
    Database(#[from] sqlx::Error),

    /// Failed to connect to (or initialise) a broker sink.
    #[error("sink connection failed: {0}")]
    Connection(String),

    /// Invalid sink configuration (e.g. an unsafe plaintext endpoint).
    #[error("configuration error: {0}")]
    Config(String),
}

/// Convenience alias for results in this crate.
pub type Result<T> = std::result::Result<T, CdcError>;
