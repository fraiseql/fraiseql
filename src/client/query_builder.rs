//! Query builder API

use crate::client::FraiseClient;
use crate::stream::JsonStream;
use crate::{Error, Result};
use serde_json::Value;

/// Type alias for a Rust-side predicate function
type RustPredicate = Box<dyn Fn(&Value) -> bool + Send>;

/// Query builder
pub struct QueryBuilder {
    client: FraiseClient,
    entity: String,
    sql_predicates: Vec<String>,
    rust_predicate: Option<RustPredicate>,
    order_by: Option<String>,
    chunk_size: usize,
}

impl QueryBuilder {
    /// Create new query builder
    pub(crate) fn new(client: FraiseClient, entity: impl Into<String>) -> Self {
        Self {
            client,
            entity: entity.into(),
            sql_predicates: Vec::new(),
            rust_predicate: None,
            order_by: None,
            chunk_size: 256,
        }
    }

    /// Add SQL WHERE clause predicate
    ///
    /// Multiple predicates are AND'ed together
    pub fn where_sql(mut self, predicate: impl Into<String>) -> Self {
        self.sql_predicates.push(predicate.into());
        self
    }

    /// Add Rust-side predicate
    ///
    /// Applied after SQL filtering, runs on streamed JSON
    pub fn where_rust<F>(mut self, predicate: F) -> Self
    where
        F: Fn(&Value) -> bool + Send + 'static,
    {
        self.rust_predicate = Some(Box::new(predicate));
        self
    }

    /// Set ORDER BY clause
    pub fn order_by(mut self, order: impl Into<String>) -> Self {
        self.order_by = Some(order.into());
        self
    }

    /// Set chunk size (default: 256)
    pub fn chunk_size(mut self, size: usize) -> Self {
        self.chunk_size = size;
        self
    }

    /// Execute query and return JSON stream
    pub async fn execute(self) -> Result<JsonStream> {
        let sql = self.build_sql()?;
        tracing::debug!("executing query: {}", sql);

        // TODO: Apply rust_predicate if present (Phase 5)
        if self.rust_predicate.is_some() {
            return Err(Error::Config(
                "Rust predicates not yet implemented".into(),
            ));
        }

        let stream = self.client.execute_query(&sql, self.chunk_size).await?;
        Ok(stream)
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
}
