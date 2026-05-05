//! Helper functions for REST response formatting.
//!
//! Contains utility functions for `ETag` computation, data extraction, and link building.

use axum::http::{HeaderMap, HeaderValue};
use serde_json::{json, Value};
use xxhash_rust::xxh3::xxh3_64;

/// Compute an `ETag` for response body data.
pub(super) fn compute_etag(body: &[u8]) -> String {
    let hash = xxh3_64(body);
    format!("W/\"{hash:016x}\"")
}

/// Check `If-None-Match` header against computed `ETag`.
///
/// Returns `Some(true)` if the `ETag` matches (304 should be returned),
/// `Some(false)` if it doesn't match, `None` if no `If-None-Match` header.
pub(super) fn check_if_none_match(headers: &HeaderMap, etag: &str) -> Option<bool> {
    let inm = headers.get("if-none-match")?.to_str().ok()?;
    // Handle wildcard
    if inm.trim() == "*" {
        return Some(true);
    }
    // Compare ETags (may be comma-separated)
    Some(inm.split(',').any(|tag| tag.trim() == etag))
}

/// Extract single resource data from executor result envelope.
///
/// The executor returns `{ "data": { "queryName": { ... } } }`.
/// Extracts the inner value (first field of the data object).
///
/// # Errors
///
/// Returns `RestError` if JSON parsing fails.
pub(super) fn extract_single_data(
    result: &Value,
) -> Result<Value, super::RestError> {
    if let Some(data_obj) = result.get("data") {
        if let Value::Object(map) = data_obj {
            Ok(map.values().next().cloned().unwrap_or(Value::Null))
        } else {
            Ok(data_obj.clone())
        }
    } else {
        Ok(result.clone())
    }
}

/// Extract collection data from executor result envelope.
///
/// # Errors
///
/// Returns `RestError` if JSON parsing fails.
pub(super) fn extract_collection_data(
    result: &Value,
) -> Result<Value, super::RestError> {
    extract_single_data(result)
}

/// Extract mutation data from executor result envelope.
///
/// Mutation results have `{ "data": { "mutationName": { ... } } }` structure.
///
/// # Errors
///
/// Returns `RestError` if JSON parsing fails.
pub(super) fn extract_mutation_data(
    result: &Value,
) -> Result<Value, super::RestError> {
    if let Some(data_obj) = result.get("data") {
        if let Value::Object(map) = data_obj {
            // For mutations, extract the entity from mutation_response
            if let Some(mutation_result) = map.values().next() {
                // Try to extract entity from mutation_response structure
                if let Some(entity) = mutation_result.get("entity") {
                    if !entity.is_null() {
                        return Ok(entity.clone());
                    }
                }
                return Ok(mutation_result.clone());
            }
        }
        Ok(data_obj.clone())
    } else {
        Ok(result.clone())
    }
}

/// Extract entity data from a DELETE mutation response.
///
/// Parses `data.{mutation_name}.entity` from the mutation result.
pub(super) fn extract_delete_entity(
    result: &Value,
    mutation_name: &str,
) -> Option<Value> {
    let entity = result.get("data")?.get(mutation_name)?.get("entity")?;

    if entity.is_null() {
        None
    } else {
        Some(entity.clone())
    }
}

/// Extract `pageInfo` from a Relay connection response.
pub(super) fn extract_relay_page_info(data: &Value) -> Option<&Value> {
    data.get("pageInfo")
}

/// Try to extract an `id` field from mutation response data.
pub(super) fn extract_id_from_data(data: &Value) -> Option<&Value> {
    data.get("id")
}

/// Format an ID value for use in a URL path segment.
pub(super) fn format_id_for_url(id: &Value) -> String {
    match id {
        Value::String(s) => s.clone(),
        Value::Number(n) => n.to_string(),
        other => other.to_string(),
    }
}

/// Build pagination links for offset-based pagination.
pub(super) fn build_offset_links(
    base: &str,
    limit: u64,
    offset: u64,
    total: Option<u64>,
) -> Value {
    let mut links = serde_json::Map::new();

    // self
    links.insert(
        "self".to_string(),
        json!(format!("{base}?limit={limit}&offset={offset}")),
    );

    // first
    links.insert(
        "first".to_string(),
        json!(format!("{base}?limit={limit}&offset=0")),
    );

    // next (if there could be more items)
    let next_offset = offset + limit;
    let has_next = total.is_none_or(|t| next_offset < t);
    if has_next {
        links.insert(
            "next".to_string(),
            json!(format!("{base}?limit={limit}&offset={next_offset}")),
        );
    } else {
        links.insert("next".to_string(), Value::Null);
    }

    // prev
    if offset > 0 {
        let prev_offset = offset.saturating_sub(limit);
        links.insert(
            "prev".to_string(),
            json!(format!("{base}?limit={limit}&offset={prev_offset}")),
        );
    } else {
        links.insert("prev".to_string(), Value::Null);
    }

    // last (only if total is known)
    if let Some(total) = total {
        if total > 0 {
            let last_offset = ((total - 1) / limit) * limit;
            links.insert(
                "last".to_string(),
                json!(format!("{base}?limit={limit}&offset={last_offset}")),
            );
        } else {
            links.insert("last".to_string(), json!(format!("{base}?limit={limit}&offset=0")));
        }
    }
    // Omit `last` entirely when total is unknown (not null — absent)

    Value::Object(links)
}

/// Build pagination links for cursor-based (Relay) pagination.
pub(super) fn build_cursor_links(
    base: &str,
    first: Option<u64>,
    after: Option<&str>,
    data: &Value,
) -> Value {
    let mut links = serde_json::Map::new();

    // self
    let mut self_url = base.to_string();
    if let Some(f) = first {
        self_url = format!("{self_url}?first={f}");
        if let Some(a) = after {
            self_url = format!("{self_url}&after={a}");
        }
    }
    links.insert("self".to_string(), json!(self_url));

    // next: use last edge's cursor if hasNextPage
    let has_next = data
        .get("pageInfo")
        .and_then(|pi| pi.get("hasNextPage"))
        .and_then(Value::as_bool)
        .unwrap_or(false);

    if has_next {
        if let Some(end_cursor) = extract_end_cursor(data) {
            let mut next_url = base.to_string();
            if let Some(f) = first {
                next_url = format!("{next_url}?first={f}&after={end_cursor}");
            } else {
                next_url = format!("{next_url}?after={end_cursor}");
            }
            links.insert("next".to_string(), json!(next_url));
        }
    }

    Value::Object(links)
}

/// Extract the end cursor from a Relay connection response.
pub(super) fn extract_end_cursor(data: &Value) -> Option<&str> {
    data.get("pageInfo")?
        .get("endCursor")?
        .as_str()
}

/// Create a `HeaderValue` from an `ETag` string.
///
/// # Panics
///
/// Panics if `s` contains non-visible-ASCII characters. This is a programmer
/// invariant: callers must only pass values produced by [`compute_etag`], which
/// returns `W/"<16 lowercase hex chars>"` — always valid ASCII.
pub(super) fn header_value(s: &str) -> HeaderValue {
    // Reason: `s` is always the output of `compute_etag`, which produces
    // `W/"<16 hex chars>"` — guaranteed valid ASCII for HeaderValue.
    HeaderValue::from_str(s).expect("ETag string must be valid ASCII")
}

