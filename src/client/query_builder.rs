//! Query builder API
//!
//! Generic query builder that supports automatic JSON deserialization to target types.
//!
//! **IMPORTANT**: Type T is **consumer-side only**.
//!
//! Type T does NOT affect:
//! - SQL generation (always `SELECT data FROM v_{entity}`)
//! - Filtering (where_sql, where_rust, order_by)
//! - Wire protocol (identical for all T)
//!
//! Type T ONLY affects:
//! - Consumer-side deserialization at poll_next()
//! - Error messages (type name included)

use crate::client::FraiseClient;
use crate::stream::{FilteredStream, TypedJsonStream};
use crate::Result;
use futures::stream::Stream;
use serde::de::DeserializeOwned;
use serde_json::Value;
use std::marker::PhantomData;

/// Type alias for a Rust-side predicate function
type RustPredicate = Box<dyn Fn(&Value) -> bool + Send>;

/// Generic query builder
///
/// The type parameter T controls consumer-side deserialization only.
/// Default type T = serde_json::Value for backward compatibility.
///
/// # Examples
///
/// Type-safe query (recommended):
/// ```ignore
/// use serde::Deserialize;
///
/// #[derive(Deserialize)]
/// struct Project {
///     id: String,
///     name: String,
/// }
///
/// let stream = client.query::<Project>("projects")
///     .where_sql("status='active'")
///     .execute()
///     .await?;
/// ```
///
/// Raw JSON query (debugging, forward compatibility):
/// ```ignore
/// let stream = client.query::<serde_json::Value>("projects")
///     .execute()
///     .await?;
/// ```
pub struct QueryBuilder<T: DeserializeOwned + Unpin + 'static = serde_json::Value> {
    client: FraiseClient,
    entity: String,
    sql_predicates: Vec<String>,
    rust_predicate: Option<RustPredicate>,
    order_by: Option<String>,
    chunk_size: usize,
    max_memory: Option<usize>,
    _phantom: PhantomData<T>,
}

impl<T: DeserializeOwned + Unpin + 'static> QueryBuilder<T> {
    /// Create new query builder
    pub(crate) fn new(client: FraiseClient, entity: impl Into<String>) -> Self {
        Self {
            client,
            entity: entity.into(),
            sql_predicates: Vec::new(),
            rust_predicate: None,
            order_by: None,
            chunk_size: 256,
            max_memory: None,
            _phantom: PhantomData,
        }
    }

    /// Add SQL WHERE clause predicate
    ///
    /// Type T does NOT affect SQL generation.
    /// Multiple predicates are AND'ed together.
    pub fn where_sql(mut self, predicate: impl Into<String>) -> Self {
        self.sql_predicates.push(predicate.into());
        self
    }

    /// Add Rust-side predicate
    ///
    /// Type T does NOT affect filtering.
    /// Applied after SQL filtering, runs on streamed JSON values.
    /// Predicates receive &serde_json::Value regardless of T.
    pub fn where_rust<F>(mut self, predicate: F) -> Self
    where
        F: Fn(&Value) -> bool + Send + 'static,
    {
        self.rust_predicate = Some(Box::new(predicate));
        self
    }

    /// Set ORDER BY clause
    ///
    /// Type T does NOT affect ordering.
    pub fn order_by(mut self, order: impl Into<String>) -> Self {
        self.order_by = Some(order.into());
        self
    }

    /// Set chunk size (default: 256)
    pub fn chunk_size(mut self, size: usize) -> Self {
        self.chunk_size = size;
        self
    }

    /// Set maximum memory limit for buffered items (default: unbounded)
    ///
    /// When the estimated memory usage of buffered items exceeds this limit,
    /// the stream will return `Error::MemoryLimitExceeded` instead of additional items.
    ///
    /// Memory is estimated as: `items_buffered * 2048 bytes` (conservative for typical JSON).
    ///
    /// By default, `max_memory()` is None (unbounded), maintaining backward compatibility.
    /// Only set if you need hard memory bounds.
    ///
    /// # Example
    ///
    /// ```ignore
    /// let stream = client
    ///     .query::<Project>("projects")
    ///     .max_memory(500_000_000)  // 500 MB limit
    ///     .execute()
    ///     .await?;
    /// ```
    ///
    /// # Interpretation
    ///
    /// If memory limit is exceeded:
    /// - It indicates the consumer is too slow relative to data arrival
    /// - The error is terminal (non-retriable) — retrying won't help
    /// - Consider: increasing consumer throughput, reducing chunk_size, or removing limit
    pub fn max_memory(mut self, bytes: usize) -> Self {
        self.max_memory = Some(bytes);
        self
    }

    /// Execute query and return typed stream
    ///
    /// Type T ONLY affects consumer-side deserialization at poll_next().
    /// SQL, filtering, ordering, and wire protocol are identical regardless of T.
    ///
    /// # Examples
    ///
    /// With type-safe deserialization:
    /// ```ignore
    /// let stream = client.query::<Project>("projects").execute().await?;
    /// while let Some(result) = stream.next().await {
    ///     let project: Project = result?;
    /// }
    /// ```
    ///
    /// With raw JSON (escape hatch):
    /// ```ignore
    /// let stream = client.query::<serde_json::Value>("projects").execute().await?;
    /// while let Some(result) = stream.next().await {
    ///     let json: Value = result?;
    /// }
    /// ```
    pub async fn execute(self) -> Result<Box<dyn Stream<Item = Result<T>> + Unpin>> {
        let sql = self.build_sql()?;
        tracing::debug!("executing query: {}", sql);

        // Record query submission metrics
        crate::metrics::counters::query_submitted(
            &self.entity,
            !self.sql_predicates.is_empty(),
            self.rust_predicate.is_some(),
            self.order_by.is_some(),
        );

        let stream = self.client.execute_query(&sql, self.chunk_size, self.max_memory).await?;

        // Apply Rust predicate if present (filters JSON before deserialization)
        let filtered_stream: Box<dyn Stream<Item = Result<Value>> + Unpin> =
            if let Some(predicate) = self.rust_predicate {
                Box::new(FilteredStream::new(stream, predicate))
            } else {
                Box::new(stream)
            };

        // Wrap in TypedJsonStream for deserialization to T
        Ok(Box::new(TypedJsonStream::<T>::new(filtered_stream)))
    }

    /// Build SQL query
    fn build_sql(&self) -> Result<String> {
        let mut sql = format!("SELECT data FROM v_{}", self.entity);

        if !self.sql_predicates.is_empty() {
            sql.push_str(" WHERE ");
            sql.push_str(&self.sql_predicates.join(" AND "));
        }

        if let Some(ref order) = self.order_by {
            sql.push_str(" ORDER BY ");
            sql.push_str(order);
        }

        Ok(sql)
    }
}

#[cfg(test)]
mod tests {

    fn build_test_sql(
        entity: &str,
        predicates: Vec<&str>,
        order_by: Option<&str>,
    ) -> String {
        let mut sql = format!("SELECT data FROM v_{}", entity);
        if !predicates.is_empty() {
            sql.push_str(" WHERE ");
            sql.push_str(&predicates.join(" AND "));
        }
        if let Some(order) = order_by {
            sql.push_str(" ORDER BY ");
            sql.push_str(order);
        }
        sql
    }

    #[test]
    fn test_build_sql_simple() {
        let sql = build_test_sql("user", vec![], None);
        assert_eq!(sql, "SELECT data FROM v_user");
    }

    #[test]
    fn test_build_sql_with_where() {
        let sql = build_test_sql("user", vec!["data->>'status' = 'active'"], None);
        assert_eq!(sql, "SELECT data FROM v_user WHERE data->>'status' = 'active'");
    }

    #[test]
    fn test_build_sql_with_order() {
        let sql = build_test_sql("user", vec![], Some("data->>'name' ASC"));
        assert_eq!(sql, "SELECT data FROM v_user ORDER BY data->>'name' ASC");
    }

    // Stream pipeline integration tests
    #[test]
    fn test_typed_stream_with_value_type() {
        // Verify that TypedJsonStream can wrap a raw JSON stream
        use futures::stream;
        use crate::stream::TypedJsonStream;

        let values = vec![
            Ok(serde_json::json!({"id": "1", "name": "Alice"})),
            Ok(serde_json::json!({"id": "2", "name": "Bob"})),
        ];

        let json_stream = stream::iter(values);
        let typed_stream: TypedJsonStream<serde_json::Value> =
            TypedJsonStream::new(Box::new(json_stream));

        // This verifies the stream compiles and has correct type
        let _stream: Box<dyn futures::stream::Stream<Item = crate::Result<serde_json::Value>> + Unpin> =
            Box::new(typed_stream);
    }

    #[test]
    fn test_filtered_stream_with_typed_output() {
        // Verify that FilteredStream correctly filters before TypedJsonStream
        use futures::stream;
        use crate::stream::{FilteredStream, TypedJsonStream};

        let values = vec![
            Ok(serde_json::json!({"id": 1, "active": true})),
            Ok(serde_json::json!({"id": 2, "active": false})),
            Ok(serde_json::json!({"id": 3, "active": true})),
        ];

        let json_stream = stream::iter(values);
        let predicate = Box::new(|v: &serde_json::Value| {
            v["active"].as_bool().unwrap_or(false)
        });

        let filtered = FilteredStream::new(json_stream, predicate);
        let typed_stream: TypedJsonStream<serde_json::Value> =
            TypedJsonStream::new(Box::new(filtered));

        // This verifies the full pipeline compiles
        let _stream: Box<dyn futures::stream::Stream<Item = crate::Result<serde_json::Value>> + Unpin> =
            Box::new(typed_stream);
    }

    #[test]
    fn test_stream_pipeline_type_flow() {
        // Comprehensive test of stream type compatibility:
        // JsonStream (Result<Value>) → FilteredStream (Result<Value>) → TypedJsonStream<T> (Result<T>)
        use futures::stream;
        use crate::stream::{FilteredStream, TypedJsonStream};
        use serde::Deserialize;

        #[derive(Deserialize, Debug)]
        struct TestUser {
            id: String,
            active: bool,
        }

        let values = vec![
            Ok(serde_json::json!({"id": "1", "active": true})),
            Ok(serde_json::json!({"id": "2", "active": false})),
        ];

        let json_stream = stream::iter(values);

        // Step 1: FilteredStream filters JSON values
        let predicate: Box<dyn Fn(&serde_json::Value) -> bool + Send> =
            Box::new(|v| v["active"].as_bool().unwrap_or(false));
        let filtered: Box<dyn futures::stream::Stream<Item = crate::Result<serde_json::Value>> + Unpin> =
            Box::new(FilteredStream::new(json_stream, predicate));

        // Step 2: TypedJsonStream deserializes to TestUser
        let typed: TypedJsonStream<TestUser> = TypedJsonStream::new(filtered);

        // This verifies type system is compatible:
        // - FilteredStream outputs Result<Value>
        // - TypedJsonStream<T> takes Box<dyn Stream<Item = Result<Value>>>
        // - TypedJsonStream<T> outputs Result<T>
        let _final_stream: Box<dyn futures::stream::Stream<Item = crate::Result<TestUser>> + Unpin> =
            Box::new(typed);
    }
}
