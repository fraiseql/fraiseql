//! The summarisation hook: collapse a long thread into one summary entry.

use std::{future::Future, pin::Pin};

use crate::{error::Result, session_state::store::SessionStateEntry};

/// The boxed future a [`Summarizer`] returns.
///
/// An explicit boxed form rather than native `async fn` in the trait: the
/// summarizer is injected as `Arc<dyn Summarizer>` (an embedder may back it
/// with an LLM call), and native async-fn-in-trait is not dyn-compatible.
pub type SummarizeFuture<'a> = Pin<Box<dyn Future<Output = Result<serde_json::Value>> + Send + 'a>>;

/// Collapses a thread's entries into a single summary value.
///
/// Invoked by [`SessionState::set`](super::SessionState::set) when a thread
/// exceeds the configured threshold — the returned value replaces every
/// ordinary entry under the reserved [`SUMMARY_KEY`](super::SUMMARY_KEY)
/// atomically. A summarizer is an *injection point* (typically the consuming
/// agent or an embedder-provided LLM call); when none is installed, threads
/// simply grow and expire by TTL.
///
/// # Errors
///
/// An implementation returning `Err` aborts the collapse — the just-written
/// entry survives and the thread stays intact (state loss is worse than an
/// oversized thread), and the error is propagated to the `set` caller.
pub trait Summarizer: Send + Sync {
    /// Produce the summary value for `entries` (every live ordinary entry of
    /// the thread, oldest write first).
    fn summarize(&self, entries: Vec<SessionStateEntry>) -> SummarizeFuture<'_>;
}

/// A [`Summarizer`] that records nothing but the entry count.
///
/// Useful in tests and as a documentation stub — installing it *does* collapse
/// threads at the threshold (into `{"entry_count": N}`); to keep threads
/// uncollapsed, install no summarizer at all.
pub struct NoOpSummarizer;

impl Summarizer for NoOpSummarizer {
    fn summarize(&self, entries: Vec<SessionStateEntry>) -> SummarizeFuture<'_> {
        Box::pin(async move { Ok(serde_json::json!({ "entry_count": entries.len() })) })
    }
}
