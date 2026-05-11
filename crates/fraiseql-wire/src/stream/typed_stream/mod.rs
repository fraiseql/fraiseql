//! Typed JSON stream implementation
//!
//! `TypedJsonStream` wraps a raw JSON stream and deserializes each item to a target type T.
//! Type T is **consumer-side only** - it does NOT affect SQL generation, filtering,
//! ordering, or wire protocol. Deserialization happens lazily at `poll_next()`.

use crate::{Result, WireError};
use futures::stream::Stream;
use futures::StreamExt;
use serde::de::DeserializeOwned;
use serde_json::Value;
use std::marker::PhantomData;
use std::pin::Pin;
use std::task::{Context, Poll};

/// Typed JSON stream that deserializes rows to type T
///
/// This stream wraps a raw JSON stream and deserializes each Value to the target type T.
///
/// **Important**: Type T is **consumer-side only**.
/// - T does NOT affect SQL generation (still `SELECT data FROM v_{entity}`)
/// - T does NOT affect filtering (`where_sql`, `where_rust`, `order_by`)
/// - T does NOT affect wire protocol (identical for all T)
/// - T ONLY affects consumer-side deserialization at `poll_next()`
///
/// # Examples
///
/// ```text
/// // Requires: live Postgres connection via FraiseClient.
/// // Note: FraiseClient::query() takes ownership of self; create separate clients for
/// // separate queries in production code.
/// use serde::Deserialize;
/// use futures::stream::StreamExt;
///
/// #[derive(Deserialize)]
/// struct Project { id: String, name: String }
///
/// let mut stream = client.query::<Project>("projects").execute().await?;
/// while let Some(result) = stream.next().await {
///     let project: Project = result?;
///     println!("Project: {}", project.name);
/// }
/// ```
pub struct TypedJsonStream<T: DeserializeOwned> {
    /// Inner stream of JSON values.
    ///
    /// The `Send` bound ensures that `TypedJsonStream` itself is `Send`,
    /// allowing it to be held across `.await` points in async tasks and
    /// transferred between threads (e.g. via `tokio::spawn`).
    inner: Box<dyn Stream<Item = Result<Value>> + Send + Unpin>,
    /// Phantom data for type T (zero runtime cost)
    _phantom: PhantomData<T>,
}

impl<T: DeserializeOwned> TypedJsonStream<T> {
    /// Create a new typed stream from a raw JSON stream
    pub fn new(inner: Box<dyn Stream<Item = Result<Value>> + Send + Unpin>) -> Self {
        Self {
            inner,
            _phantom: PhantomData,
        }
    }

    /// Deserialize a JSON value to type T
    ///
    /// This is the only place type T matters. Deserialization is lazy (per-item)
    /// to skip deserializing filtered-out rows.
    fn deserialize_value(value: Value) -> Result<T> {
        let type_name = std::any::type_name::<T>().to_string();
        let deser_start = std::time::Instant::now();

        match serde_json::from_value::<T>(value) {
            Ok(result) => {
                let duration_ms = deser_start.elapsed().as_millis() as u64;
                crate::metrics::histograms::deserialization_duration(
                    "unknown",
                    &type_name,
                    duration_ms,
                );
                crate::metrics::counters::deserialization_success("unknown", &type_name);
                Ok(result)
            }
            Err(e) => {
                crate::metrics::counters::deserialization_failure(
                    "unknown",
                    &type_name,
                    "serde_error",
                );
                Err(WireError::Deserialization {
                    type_name,
                    details: e.to_string(),
                })
            }
        }
    }
}

impl<T: DeserializeOwned + Unpin> Stream for TypedJsonStream<T> {
    type Item = Result<T>;

    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        match self.inner.poll_next_unpin(cx) {
            Poll::Ready(Some(Ok(value))) => {
                // Deserialize happens HERE, at poll_next
                // This is the only place type T affects behavior
                Poll::Ready(Some(Self::deserialize_value(value)))
            }
            Poll::Ready(Some(Err(e))) => Poll::Ready(Some(Err(e))),
            Poll::Ready(None) => Poll::Ready(None),
            Poll::Pending => Poll::Pending,
        }
    }
}

#[cfg(test)]
mod tests;
