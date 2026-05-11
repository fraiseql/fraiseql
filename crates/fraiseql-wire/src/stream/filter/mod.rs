//! Filtered JSON stream

use crate::Result;
use futures::stream::Stream;
use serde_json::Value;
use std::pin::Pin;
use std::sync::atomic::{AtomicU64, Ordering};
use std::task::{Context, Poll};

/// Predicate function type
pub type Predicate = Box<dyn Fn(&Value) -> bool + Send>;

/// Filtered JSON stream
pub struct FilteredStream<S> {
    inner: S,
    predicate: Predicate,
    // Sampling counter for metrics recording
    eval_count: AtomicU64,
}

impl<S> FilteredStream<S> {
    /// Create new filtered stream
    pub fn new(inner: S, predicate: Predicate) -> Self {
        Self {
            inner,
            predicate,
            eval_count: AtomicU64::new(0),
        }
    }
}

impl<S> Stream for FilteredStream<S>
where
    S: Stream<Item = Result<Value>> + Unpin,
{
    type Item = Result<Value>;

    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        loop {
            match Pin::new(&mut self.inner).poll_next(cx) {
                Poll::Ready(Some(Ok(value))) => {
                    // Apply predicate
                    // Sample timing: only record 1 in 1000 evaluations
                    let eval_idx = self.eval_count.fetch_add(1, Ordering::Relaxed);
                    let passed = if eval_idx.is_multiple_of(1000) {
                        // Record timing for sampled evaluation
                        let filter_start = std::time::Instant::now();
                        let result = (self.predicate)(&value);
                        let filter_duration = filter_start.elapsed().as_millis() as u64;
                        crate::metrics::histograms::filter_duration("unknown", filter_duration);
                        result
                    } else {
                        // No timing, just evaluate
                        (self.predicate)(&value)
                    };

                    if passed {
                        return Poll::Ready(Some(Ok(value)));
                    }
                    // Predicate failed, try next value (filter out this row)
                    crate::metrics::counters::rows_filtered("unknown", 1);
                    continue;
                }
                Poll::Ready(Some(Err(e))) => {
                    // Propagate errors
                    return Poll::Ready(Some(Err(e)));
                }
                Poll::Ready(None) => {
                    // End of stream
                    return Poll::Ready(None);
                }
                Poll::Pending => {
                    return Poll::Pending;
                }
            }
        }
    }
}

#[cfg(test)]
mod tests;
