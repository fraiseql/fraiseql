//! Typed JSON stream implementation
//!
//! TypedJsonStream wraps a raw JSON stream and deserializes each item to a target type T.
//! Type T is **consumer-side only** - it does NOT affect SQL generation, filtering,
//! ordering, or wire protocol. Deserialization happens lazily at poll_next().

use crate::{Error, Result};
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
/// - T does NOT affect filtering (where_sql, where_rust, order_by)
/// - T does NOT affect wire protocol (identical for all T)
/// - T ONLY affects consumer-side deserialization at poll_next()
///
/// # Examples
///
/// ```ignore
/// use serde::Deserialize;
///
/// #[derive(Deserialize)]
/// struct Project {
///     id: String,
///     name: String,
/// }
///
/// let mut stream = client.query::<Project>("projects").execute().await?;
/// while let Some(result) = stream.next().await {
///     let project: Project = result?;
///     println!("Project: {}", project.name);
/// }
///
/// // Escape hatch: Always use Value if needed
/// let mut stream = client.query::<serde_json::Value>("projects").execute().await?;
/// while let Some(result) = stream.next().await {
///     let json: Value = result?;
///     println!("Raw JSON: {:?}", json);
/// }
/// ```
pub struct TypedJsonStream<T: DeserializeOwned> {
    /// Inner stream of JSON values
    inner: Box<dyn Stream<Item = Result<Value>> + Unpin>,
    /// Phantom data for type T (zero runtime cost)
    _phantom: PhantomData<T>,
}

impl<T: DeserializeOwned> TypedJsonStream<T> {
    /// Create a new typed stream from a raw JSON stream
    pub fn new(inner: Box<dyn Stream<Item = Result<Value>> + Unpin>) -> Self {
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
                Err(Error::Deserialization {
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
mod tests {
    use super::*;

    #[test]
    fn test_typed_stream_creation() {
        // Verify TypedJsonStream can be created with different types
        let _stream: TypedJsonStream<serde_json::Value> =
            TypedJsonStream::new(Box::new(futures::stream::empty()));

        #[derive(serde::Deserialize, Debug)]
        #[allow(dead_code)]
        struct TestType {
            id: String,
        }

        let _stream: TypedJsonStream<TestType> =
            TypedJsonStream::new(Box::new(futures::stream::empty()));
    }

    #[test]
    fn test_deserialize_valid_value() {
        let json = serde_json::json!({
            "id": "123",
            "name": "Test"
        });

        #[derive(serde::Deserialize)]
        #[allow(dead_code)]
        struct TestType {
            id: String,
            name: String,
        }

        let result = TypedJsonStream::<TestType>::deserialize_value(json);
        assert!(result.is_ok());
        let item = result.unwrap();
        assert_eq!(item.id, "123");
        assert_eq!(item.name, "Test");
    }

    #[test]
    fn test_deserialize_missing_field() {
        let json = serde_json::json!({
            "id": "123"
            // missing "name" field
        });

        #[derive(Debug, serde::Deserialize)]
        #[allow(dead_code)]
        struct TestType {
            id: String,
            name: String,
        }

        let result = TypedJsonStream::<TestType>::deserialize_value(json);
        assert!(result.is_err());

        let err = result.unwrap_err();
        match err {
            Error::Deserialization { type_name, details } => {
                assert!(type_name.contains("TestType"));
                assert!(details.contains("name"));
            }
            _ => panic!("Expected Deserialization error"),
        }
    }

    #[test]
    fn test_deserialize_type_mismatch() {
        let json = serde_json::json!({
            "id": "123",
            "count": "not a number"  // should be i32
        });

        #[derive(Debug, serde::Deserialize)]
        #[allow(dead_code)]
        struct TestType {
            id: String,
            count: i32,
        }

        let result = TypedJsonStream::<TestType>::deserialize_value(json);
        assert!(result.is_err());

        let err = result.unwrap_err();
        match err {
            Error::Deserialization { type_name, details } => {
                assert!(type_name.contains("TestType"));
                assert!(details.contains("invalid") || details.contains("type"));
            }
            _ => panic!("Expected Deserialization error"),
        }
    }

    #[test]
    fn test_deserialize_value_type() {
        let json = serde_json::json!({
            "id": "123",
            "name": "Test"
        });

        // Test that Value (escape hatch) works
        let result = TypedJsonStream::<serde_json::Value>::deserialize_value(json.clone());
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), json);
    }

    #[test]
    fn test_phantom_data_has_no_size() {
        use std::mem::size_of;

        // Verify PhantomData adds zero size
        let size_without_phantom = size_of::<Box<dyn Stream<Item = Result<Value>> + Unpin>>();
        let size_with_phantom = size_of::<TypedJsonStream<serde_json::Value>>();

        // PhantomData should not increase size
        // (might be equal or slightly different due to alignment, but not significantly larger)
        assert!(
            size_with_phantom <= size_without_phantom + 8,
            "PhantomData added too much size: {} vs {}",
            size_with_phantom,
            size_without_phantom
        );
    }
}
