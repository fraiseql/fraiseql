//! Query stream with pause/resume/stats capabilities
//!
//! This stream combines JsonStream (with control methods) with optional filtering
//! and type-safe deserialization. It exposes pause(), resume(), and stats() methods
//! while implementing `Stream<Item = Result<T>>`.

use crate::stream::JsonStream;
use crate::{Error, Result};
use futures::stream::Stream;
use serde::de::DeserializeOwned;
use serde_json::Value;
use std::marker::PhantomData;
use std::pin::Pin;
use std::task::{Context, Poll};

/// Type alias for Rust-side predicate function
type Predicate = Box<dyn Fn(&Value) -> bool + Send>;

/// Query stream with pause/resume/stats capabilities
///
/// This stream combines JsonStream (with control methods) with optional filtering
/// and type-safe deserialization. It exposes pause(), resume(), and stats() methods
/// while implementing `Stream<Item = Result<T>>`.
pub struct QueryStream<T: DeserializeOwned + Unpin> {
    /// Inner JSON stream (provides pause/resume/stats)
    inner: JsonStream,
    /// Optional Rust-side predicate for filtering
    predicate: Option<Predicate>,
    /// Type marker for deserialization target
    _phantom: PhantomData<T>,
}

impl<T: DeserializeOwned + Unpin> QueryStream<T> {
    /// Create a new query stream
    pub fn new(inner: JsonStream, predicate: Option<Predicate>) -> Self {
        Self {
            inner,
            predicate,
            _phantom: PhantomData,
        }
    }

    /// Pause the stream
    pub async fn pause(&mut self) -> Result<()> {
        self.inner.pause().await
    }

    /// Resume the stream
    pub async fn resume(&mut self) -> Result<()> {
        self.inner.resume().await
    }

    /// Get stream statistics
    pub fn stats(&self) -> crate::stream::StreamStats {
        self.inner.stats()
    }

    /// Get current stream state snapshot
    pub fn state_snapshot(&self) -> crate::stream::StreamState {
        self.inner.state_snapshot()
    }

    /// Get buffered rows when paused
    pub fn paused_occupancy(&self) -> usize {
        self.inner.paused_occupancy()
    }

    /// Pause with diagnostic reason
    pub async fn pause_with_reason(&mut self, reason: &str) -> Result<()> {
        self.inner.pause_with_reason(reason).await
    }

    /// Deserialize a JSON value to type T
    fn deserialize_value(value: Value) -> Result<T> {
        match serde_json::from_value::<T>(value) {
            Ok(result) => Ok(result),
            Err(e) => Err(Error::Deserialization {
                type_name: std::any::type_name::<T>().to_string(),
                details: e.to_string(),
            }),
        }
    }
}

impl<T: DeserializeOwned + Unpin> Stream for QueryStream<T> {
    type Item = Result<T>;

    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        loop {
            // Poll the inner JsonStream
            match Pin::new(&mut self.inner).poll_next(cx) {
                Poll::Ready(Some(Ok(value))) => {
                    // Apply predicate if present
                    if let Some(ref predicate) = self.predicate {
                        if !predicate(&value) {
                            // Filtered out, try next value
                            continue;
                        }
                    }

                    // Deserialize to target type T
                    return Poll::Ready(Some(Self::deserialize_value(value)));
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

impl<T: DeserializeOwned + Unpin> Unpin for QueryStream<T> {}
