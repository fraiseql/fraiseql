//! The in-memory adapter and rows the instruction benches read, shared by
//! `fraiseql-core/benches/executor_instructions.rs` and
//! `fraiseql-server/benches/rest_instructions.rs` so both measure the same reads.

use std::collections::HashMap;

use async_trait::async_trait;
use fraiseql_core::{
    db::{
        DatabaseAdapter, DatabaseType, WhereClause,
        traits::{MutationRowGate, WriteRequest, Writer},
        types::{JsonbValue, OrderByClause, PoolMetrics},
    },
    error::{FraiseQLError, Result},
    schema::SqlProjectionHint,
};
use serde_json::{Value, json};

/// The compiled schema both benches load: `User` with nested `posts`, a list query and a
/// by-id query.
pub const SCHEMA: &str = include_str!("../fixtures/executor_schema.json");

/// Returns the same rows for every read.
pub struct FixedRows(pub Vec<JsonbValue>);

// Reason: DatabaseAdapter is defined with #[async_trait]; implementations must match it.
#[async_trait]
impl DatabaseAdapter for FixedRows {
    fn applies_session_variables(&self) -> bool {
        true
    }

    async fn execute_where_query(
        &self,
        _view: &str,
        where_clause: Option<&WhereClause>,
        limit: Option<u32>,
        _offset: Option<u32>,
        _order_by: Option<&[OrderByClause]>,
    ) -> Result<Vec<JsonbValue>> {
        // A filtered read is the by-id shape: the database would answer one row.
        let n = if where_clause.is_some() {
            1
        } else {
            self.0.len()
        };
        let n = limit.map_or(n, |l| n.min(l as usize));
        Ok(self.0[..n].to_vec())
    }

    async fn execute_with_projection(
        &self,
        view: &str,
        _projection: Option<&SqlProjectionHint>,
        where_clause: Option<&WhereClause>,
        limit: Option<u32>,
        offset: Option<u32>,
        order_by: Option<&[OrderByClause]>,
    ) -> Result<Vec<JsonbValue>> {
        self.execute_where_query(view, where_clause, limit, offset, order_by).await
    }

    fn database_type(&self) -> DatabaseType {
        DatabaseType::PostgreSQL
    }

    async fn health_check(&self) -> Result<()> {
        Ok(())
    }

    fn pool_metrics(&self) -> PoolMetrics {
        PoolMetrics::default()
    }

    async fn execute_raw_query(&self, _sql: &str) -> Result<Vec<HashMap<String, Value>>> {
        Ok(vec![])
    }

    async fn execute_parameterized_aggregate(
        &self,
        _sql: &str,
        _params: &[Value],
    ) -> Result<Vec<HashMap<String, Value>>> {
        Ok(vec![])
    }
}

// Reason: Writer is defined with #[async_trait]; implementations must match it.
#[async_trait]
impl Writer for FixedRows {
    async fn execute_write(
        &self,
        _request: &WriteRequest<'_>,
        _gate: MutationRowGate<'_>,
    ) -> std::result::Result<Vec<HashMap<String, Value>>, FraiseQLError> {
        Err(FraiseQLError::Unsupported {
            message: "the instruction bench does not write".to_string(),
        })
    }
}

/// One user row; `fat` gives it a long bio and twenty posts.
pub fn user_row(i: usize, fat: bool) -> JsonbValue {
    let (bio, posts) = if fat {
        let posts: Vec<Value> = (0..20)
            .map(|p| {
                json!({
                    "id": format!("00000000-0000-4000-8000-{i:06}{p:06}"),
                    "title": format!("Post {p} by user {i}, a title of ordinary length"),
                    "published": p % 2 == 0,
                })
            })
            .collect();
        ("A longer biography paragraph, repeated to make the row fat. ".repeat(10), posts)
    } else {
        ("Short bio.".to_string(), Vec::new())
    };
    JsonbValue::new(json!({
        "id": format!("00000000-0000-4000-8000-{i:012}"),
        "name": format!("User {i}"),
        "email": format!("user{i}@example.com"),
        "bio": bio,
        "createdAt": "2026-10-22T12:00:00Z",
        "posts": posts,
    }))
}
