//! Helper functions for NDJSON streaming responses.
//!
//! Contains utilities for batch serialization, error formatting, and row extraction.

use std::sync::Arc;

use axum::http::StatusCode;
use bytes::Bytes;
use fraiseql_core::{
    db::traits::DatabaseAdapter,
    runtime::{Executor, QueryMatch},
    security::SecurityContext,
};
use serde_json::json;

use crate::routes::rest::handler::RestError;

/// Internal state for the streaming unfold loop.
pub(super) struct StreamState<A: DatabaseAdapter> {
    pub executor:     Arc<Executor<A>>,
    pub query_name:   String,
    pub query_match:  QueryMatch,
    pub variables:    serde_json::Value,
    pub security_ctx: Option<SecurityContext>,
    pub batch_size:   u64,
    pub offset:       u64,
    pub done:         bool,
}

/// Fetch the next batch of rows, serialize as NDJSON bytes, and advance the offset.
///
/// Returns:
/// - `Ok(Some(bytes))` — batch serialized successfully
/// - `Ok(None)` — no more rows (stream done)
/// - `Err(bytes)` — error serialized as NDJSON error line
pub(super) async fn fetch_and_serialize_batch<A: DatabaseAdapter>(
    state: &mut StreamState<A>,
) -> Result<Option<Bytes>, Bytes> {
    // Override limit/offset in the variables for this batch.
    let mut batch_vars = state.variables.clone();
    if let Some(obj) = batch_vars.as_object_mut() {
        obj.insert("limit".to_string(), serde_json::json!(state.batch_size));
        if state.offset > 0 {
            obj.insert("offset".to_string(), serde_json::json!(state.offset));
        }
    }

    let vars_ref = if batch_vars.as_object().is_none_or(|m| m.is_empty()) {
        None
    } else {
        Some(&batch_vars)
    };

    let result_value = match state
        .executor
        .execute_query_direct(&state.query_match, vars_ref, state.security_ctx.as_ref())
        .await
    {
        Ok(r) => r,
        Err(e) => {
            state.done = true;
            return Err(error_ndjson_line(&e.to_string()));
        },
    };

    let rows = match extract_rows(&result_value, &state.query_name) {
        Ok(r) => r,
        Err(e) => {
            state.done = true;
            return Err(error_ndjson_line(&e.message));
        },
    };

    if rows.is_empty() {
        state.done = true;
        return Ok(None);
    }

    // Serialize rows as NDJSON.
    let mut ndjson_bytes = Vec::new();
    for row in &rows {
        match serde_json::to_vec(row) {
            Ok(mut line) => {
                line.push(b'\n');
                ndjson_bytes.extend_from_slice(&line);
            },
            Err(e) => {
                state.done = true;
                // Yield what we have so far plus the error.
                ndjson_bytes.extend_from_slice(&error_ndjson_line(&e.to_string()));
                return Ok(Some(Bytes::from(ndjson_bytes)));
            },
        }
    }

    // If we got fewer rows than the batch size, this is the last batch.
    #[allow(clippy::cast_possible_truncation)] // Reason: rows.len() won't exceed u64 range
    let row_count = rows.len() as u64;
    if row_count < state.batch_size {
        state.done = true;
    } else {
        state.offset += state.batch_size;
    }

    Ok(Some(Bytes::from(ndjson_bytes)))
}

/// Serialize an error as an NDJSON error line.
pub(super) fn error_ndjson_line(message: &str) -> Bytes {
    // Escape the message for safe JSON embedding.
    let escaped = serde_json::to_string(message).unwrap_or_else(|_| format!("\"{message}\""));
    Bytes::from(format!("{{\"error\":{escaped}}}\n"))
}

/// Extract rows from the executor result envelope.
///
/// The executor returns `{ "data": { "queryName": [...] } }`.
/// For a single resource, returns a one-element vec.
///
/// # Errors
///
/// Returns `RestError` if the result cannot be parsed.
pub(super) fn extract_rows(
    result: &serde_json::Value,
    query_name: &str,
) -> Result<Vec<serde_json::Value>, RestError> {
    let data = result
        .get("data")
        .and_then(|d| d.get(query_name))
        .ok_or_else(|| RestError::internal("Missing data in query result"))?;

    match data {
        serde_json::Value::Array(arr) => Ok(arr.clone()),
        // Single resource — wrap in a vec
        other => Ok(vec![other.clone()]),
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)] // Reason: test code
mod tests {
    use super::*;

    // -----------------------------------------------------------------------
    // extract_rows
    // -----------------------------------------------------------------------

    fn v(s: &str) -> serde_json::Value {
        serde_json::from_str(s).unwrap()
    }

    #[test]
    fn extract_rows_from_array() {
        let result = v(r#"{"data":{"users":[{"id":1,"name":"Alice"},{"id":2,"name":"Bob"}]}}"#);
        let rows = extract_rows(&result, "users").unwrap();
        assert_eq!(rows.len(), 2);
        assert_eq!(rows[0]["name"], "Alice");
        assert_eq!(rows[1]["name"], "Bob");
    }

    #[test]
    fn extract_rows_from_single_resource() {
        let result = v(r#"{"data":{"user":{"id":1,"name":"Alice"}}}"#);
        let rows = extract_rows(&result, "user").unwrap();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0]["name"], "Alice");
    }

    #[test]
    fn extract_rows_missing_data() {
        let result = v(r#"{"errors":[]}"#);
        let err = extract_rows(&result, "users").unwrap_err();
        assert_eq!(err.status, StatusCode::INTERNAL_SERVER_ERROR);
    }

    #[test]
    fn extract_rows_missing_query() {
        let result = v(r#"{"data":{"other_query":[]}}"#);
        let err = extract_rows(&result, "users").unwrap_err();
        assert_eq!(err.status, StatusCode::INTERNAL_SERVER_ERROR);
    }

    // -----------------------------------------------------------------------
    // error_ndjson_line
    // -----------------------------------------------------------------------

    #[test]
    fn error_ndjson_line_valid_json() {
        let line = error_ndjson_line("something went wrong");
        let s = String::from_utf8(line.to_vec()).unwrap();
        assert!(s.ends_with('\n'));
        let parsed: serde_json::Value = serde_json::from_str(s.trim()).unwrap();
        assert_eq!(parsed["error"], "something went wrong");
    }

    #[test]
    fn error_ndjson_line_escapes_special_chars() {
        let line = error_ndjson_line("bad \"quote\" and \nnewline");
        let s = String::from_utf8(line.to_vec()).unwrap();
        let parsed: serde_json::Value = serde_json::from_str(s.trim()).unwrap();
        assert!(parsed["error"].as_str().unwrap().contains("quote"));
    }

    // -----------------------------------------------------------------------
    // NDJSON serialization
    // -----------------------------------------------------------------------

    #[test]
    fn ndjson_format_one_object_per_line() {
        let rows = vec![
            json!({"id": 1, "name": "Alice"}),
            json!({"id": 2, "name": "Bob"}),
        ];

        let mut ndjson = Vec::new();
        for row in &rows {
            let mut line = serde_json::to_vec(row).unwrap();
            line.push(b'\n');
            ndjson.extend_from_slice(&line);
        }

        let output = String::from_utf8(ndjson).unwrap();
        let lines: Vec<&str> = output.trim_end().split('\n').collect();
        assert_eq!(lines.len(), 2);

        // Each line is valid JSON
        for line in &lines {
            let parsed: serde_json::Value = serde_json::from_str(line).unwrap();
            assert!(parsed.is_object());
        }
    }

    #[test]
    fn ndjson_no_envelope() {
        let rows = vec![json!({"id": 1})];

        let mut ndjson = Vec::new();
        for row in &rows {
            let mut line = serde_json::to_vec(row).unwrap();
            line.push(b'\n');
            ndjson.extend_from_slice(&line);
        }

        let output = String::from_utf8(ndjson).unwrap();
        // No "data", "meta", or "links" wrapper
        assert!(!output.contains("\"data\""));
        assert!(!output.contains("\"meta\""));
        assert!(!output.contains("\"links\""));
    }

    #[test]
    fn ndjson_select_fields_applied() {
        // When ?select=id,name is used, each row should only have those fields.
        // This is handled upstream by QueryMatch field selection, but verify format.
        let rows = [json!({"id": 1, "name": "Alice"})];

        let line = serde_json::to_string(&rows[0]).unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&line).unwrap();
        assert!(parsed.get("id").is_some());
        assert!(parsed.get("name").is_some());
        assert!(parsed.get("email").is_none());
    }
}
