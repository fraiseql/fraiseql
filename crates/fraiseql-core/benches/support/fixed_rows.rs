//! The in-memory adapter and rows the instruction benches read, shared by
//! `fraiseql-core/benches/executor_instructions.rs` and
//! `fraiseql-server/benches/rest_instructions.rs` so both measure the same reads.

use std::collections::HashMap;

use async_trait::async_trait;
use fraiseql_core::{
    db::{
        DatabaseAdapter, DatabaseType, WhereClause,
        traits::{
            MutationRowGate, ProjectionRequest, RawRows, WriteRequest, Writer, quote_decimals,
        },
        types::{JsonbValue, OrderByClause, PoolMetrics, ReadRouting},
    },
    error::{FraiseQLError, Result},
    schema::SqlProjectionHint,
};
use serde_json::{Value, json};

/// The compiled schema both benches load: `User` with nested `posts`, a list query and a
/// by-id query.
pub const SCHEMA: &str = include_str!("../fixtures/executor_schema.json");

/// Returns the same rows for every read.
/// Fixed rows, and optionally each row's text as PostgreSQL writes a `json` object, so a
/// read answered as text costs what the database's own bytes cost: nothing to build.
pub struct FixedRows(pub Vec<JsonbValue>, pub Option<Vec<Vec<u8>>>);

impl FixedRows {
    /// Rows answered as `Value`s only, as an adapter that does not answer as text.
    pub const fn new(rows: Vec<JsonbValue>) -> Self {
        Self(rows, None)
    }

    /// Rows answered as `Value`s, or as text.
    #[allow(dead_code)] // Reason: one of the three benches that include this file reads as text
    pub fn with_text(rows: Vec<JsonbValue>) -> Self {
        let texts = rows.iter().map(|row| json_object_text(row.as_value())).collect();
        Self(rows, Some(texts))
    }

    /// Rows given as their text, each also parsed for the `Value` path.
    #[allow(dead_code)] // Reason: one of the three benches that include this file reads it
    pub fn from_texts(texts: Vec<Vec<u8>>) -> Self {
        let rows = texts
            .iter()
            .map(|text| JsonbValue::new(serde_json::from_slice(text).expect("bench row is JSON")))
            .collect();
        Self(rows, Some(texts))
    }

    /// How many rows a read returns: one for a lookup, else up to `limit`.
    fn count(&self, where_clause: Option<&WhereClause>, limit: Option<u32>) -> usize {
        let n = if where_clause.is_some() {
            1
        } else {
            self.0.len()
        };
        limit.map_or(n, |l| n.min(l as usize))
    }
}

/// An object as PostgreSQL's `JSON_OBJECT` writes it: `{"k" : v, "k2" : v2}`.
#[allow(dead_code)] // Reason: reached only through `FixedRows::with_text`
fn json_object_text(value: &Value) -> Vec<u8> {
    let members = value.as_object().map_or_else(Vec::new, |object| {
        object
            .iter()
            .map(|(k, v)| format!("{} : {v}", Value::String(k.clone())))
            .collect()
    });
    format!("{{{}}}", members.join(", ")).into_bytes()
}

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
        Ok(self.0[..self.count(where_clause, limit)].to_vec())
    }

    async fn execute_with_projection_raw_with_session(
        &self,
        request: &ProjectionRequest<'_>,
        _session_vars: &[(&str, &str)],
        _routing: ReadRouting,
    ) -> Result<Option<RawRows>> {
        let Some(texts) = &self.1 else {
            return Ok(None);
        };
        let mut rows = RawRows::new();
        for text in &texts[..self.count(request.where_clause, request.limit)] {
            rows.push(text);
        }
        Ok(Some(rows))
    }

    async fn execute_with_projection_arc(
        &self,
        request: &ProjectionRequest<'_>,
    ) -> Result<std::sync::Arc<Vec<JsonbValue>>> {
        // A read with a `Decimal` shape quotes its rows' text first, as the database adapter
        // does, so the count includes the pass.
        let (Some(texts), Some(shape)) = (&self.1, request.decimals) else {
            return self
                .execute_with_projection(
                    request.view,
                    request.projection,
                    request.where_clause,
                    request.limit,
                    request.offset,
                    request.order_by,
                )
                .await
                .map(std::sync::Arc::new);
        };
        let mut quoted = Vec::new();
        texts[..self.count(request.where_clause, request.limit)]
            .iter()
            .map(|text| {
                quoted.clear();
                quote_decimals(text, shape, &mut quoted);
                serde_json::from_slice(&quoted).map(JsonbValue::new).map_err(|e| {
                    FraiseQLError::Internal {
                        message: format!("bench row is not JSON: {e}"),
                        source:  None,
                    }
                })
            })
            .collect::<Result<Vec<_>>>()
            .map(std::sync::Arc::new)
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
        // With text, a projected read parses it, as the database adapter parses its rows.
        let Some(texts) = &self.1 else {
            return self.execute_where_query(view, where_clause, limit, offset, order_by).await;
        };
        texts[..self.count(where_clause, limit)]
            .iter()
            .map(|text| {
                serde_json::from_slice(text).map(JsonbValue::new).map_err(|e| {
                    FraiseQLError::Internal {
                        message: format!("bench row is not JSON: {e}"),
                        source:  None,
                    }
                })
            })
            .collect()
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

/// A user row holding only the fields a plain read selects, in its selection order.
#[allow(dead_code)] // Reason: one of the three benches that include this file reads it
pub fn plain_user_row(i: usize, fat: bool) -> JsonbValue {
    let bio = if fat {
        "A longer biography paragraph, repeated to make the row fat. ".repeat(10)
    } else {
        "Short bio.".to_string()
    };
    JsonbValue::new(json!({
        "id": format!("00000000-0000-4000-8000-{i:012}"),
        "name": format!("User {i}"),
        "email": format!("user{i}@example.com"),
        "bio": bio,
    }))
}

/// A reading's text as PostgreSQL writes its projection: each `Decimal` a string of its
/// digits, read with `->>`, and each `Float` a number with a fraction, which `serde_json`
/// writes again.
#[allow(dead_code)] // Reason: one of the three benches that include this file reads it
pub fn reading_text(i: usize) -> Vec<u8> {
    format!(
        "{{\"id\" : \"00000000-0000-4000-8000-{i:012}\", \"price\" : \"{}.{:02}\", \
         \"cost\" : \"{}.50\", \"tax\" : \"0.{:02}0\", \"total\" : \"1{i:04}.00\", \
         \"score\" : 0.{:03}, \"ratio\" : 3.14159265358979323846, \"weight\" : {}.125, \
         \"count\" : {}}}",
        10 + i,
        (i * 7) % 100,
        i * 3,
        (i * 13) % 100,
        (i * 37) % 1000,
        i,
        i * 11,
    )
    .into_bytes()
}
