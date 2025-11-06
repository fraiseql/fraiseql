//! Pipeline response builder for GraphQL responses
//!
//! This module provides the high-level API for building complete GraphQL
//! responses from PostgreSQL JSON rows using schema-aware transformation.

use pyo3::prelude::*;
use serde_json::{json, Value};
use crate::core::arena::Arena;
use crate::core::transform::{TransformConfig, ZeroCopyTransformer, ByteBuf};
use crate::pipeline::projection::FieldSet;
use crate::json_transform;
use crate::schema_registry;

/// Build complete GraphQL response from PostgreSQL JSON rows
///
/// This is the TOP-LEVEL API called from lib.rs (FFI layer):
/// ```rust
/// let response_bytes = pipeline::builder::build_graphql_response(
///     json_rows,
///     field_name,
///     type_name,
///     field_paths,
///     field_selections,
/// )
/// ```
///
/// Pipeline:
/// ┌──────────────┐
/// │ PostgreSQL   │ → JSON strings (already in memory)
/// │ json_rows    │
/// └──────┬───────┘
///        │
///        ▼
/// ┌──────────────┐
/// │ Arena        │ → Allocate scratch space
/// │ Setup        │
/// └──────┬───────┘
///        │
///        ▼
/// ┌──────────────┐
/// │ Estimate     │ → Size output buffer (eliminate reallocs)
/// │ Capacity     │
/// └──────┬───────┘
///        │
///        ▼
/// ┌──────────────┐
/// │ Zero-Copy    │ → Transform each row (no parsing!)
/// │ Transform    │    - Wrap in GraphQL structure
/// └──────┬───────┘    - Project fields
///        │            - Add __typename
///        │            - CamelCase keys
///        │            - Apply aliases
///        ▼
/// ┌──────────────┐
/// │ HTTP Bytes   │ → Return to Python (zero-copy)
/// │ (Vec<u8>)    │
/// └──────────────┘
///
pub fn build_graphql_response(
    json_rows: Vec<String>,
    field_name: &str,
    type_name: Option<&str>,
    field_paths: Option<Vec<Vec<String>>>,
    field_selections: Option<Vec<Value>>,
    is_list: Option<bool>,
) -> PyResult<Vec<u8>> {
    let registry = schema_registry::get_registry();

    if let (Some(registry), Some(type_name_str)) = (registry, type_name) {
        return build_with_schema(json_rows, field_name, type_name_str, field_paths, field_selections, registry, is_list);
    }

    build_zero_copy(json_rows, field_name, type_name, field_paths, is_list)
}

fn build_with_schema(
    json_rows: Vec<String>,
    field_name: &str,
    type_name: &str,
    _field_paths: Option<Vec<Vec<String>>>,
    field_selections: Option<Vec<Value>>,
    registry: &schema_registry::SchemaRegistry,
    is_list: Option<bool>,
) -> PyResult<Vec<u8>> {
    let transformed_items: Result<Vec<Value>, _> = json_rows
        .iter()
        .map(|row_str| {
            serde_json::from_str::<Value>(row_str)
                .map_err(|e| pyo3::exceptions::PyValueError::new_err(
                    format!("Failed to parse JSON: {}", e)
                ))
                .map(|value| {
                    if let Some(ref selections) = field_selections {
                        json_transform::transform_with_selections(&value, type_name, selections, registry)
                    } else {
                        json_transform::transform_with_schema(&value, type_name, registry)
                    }
                })
        })
        .collect();

    let transformed_items = transformed_items?;

    // Respect is_list parameter (defaults to true for backward compatibility)
    // When true: wrap in array regardless of item count
    // When false: return single unwrapped object
    let response_data = if is_list.unwrap_or(true) {
        json!({
            "data": {
                field_name: transformed_items
            }
        })
    } else if !transformed_items.is_empty() {
        json!({
            "data": {
                field_name: transformed_items.get(0).cloned().unwrap_or(Value::Null)
            }
        })
    } else {
        // Empty result for is_list=False: return [] so Python null detection works
        json!({
            "data": {
                field_name: Value::Array(vec![])
            }
        })
    };

    serde_json::to_vec(&response_data)
        .map_err(|e| pyo3::exceptions::PyValueError::new_err(
            format!("Failed to serialize response: {}", e)
        ))
}

fn build_zero_copy(
    json_rows: Vec<String>,
    field_name: &str,
    type_name: Option<&str>,
    field_paths: Option<Vec<Vec<String>>>,
    is_list: Option<bool>,
) -> PyResult<Vec<u8>> {
    let arena = Arena::with_capacity(estimate_arena_size(&json_rows));

    let config = TransformConfig {
        add_typename: type_name.is_some(),
        camel_case: true,
        project_fields: field_paths.is_some(),
        add_graphql_wrapper: false,
    };

    let field_set = field_paths
        .map(|paths| FieldSet::from_paths(&paths, &arena));

    let transformer = ZeroCopyTransformer::new(
        &arena,
        config,
        type_name,
        field_set.as_ref(),
    );

    let total_input_size: usize = json_rows.iter().map(|s| s.len()).sum();
    let wrapper_overhead = 50 + field_name.len();
    let estimated_size = total_input_size + wrapper_overhead;

    let mut result = Vec::with_capacity(estimated_size);

    // Build GraphQL response: {"data":{"<field_name>":<content>}}
    result.extend_from_slice(b"{\"data\":{\"");
    result.extend_from_slice(field_name.as_bytes());
    result.extend_from_slice(b"\":");

    // Respect is_list parameter (defaults to true for backward compatibility)
    // When true: wrap transformed items in array
    // When false: return single unwrapped item
    if is_list.unwrap_or(true) {
        result.push(b'[');

        for (i, row) in json_rows.iter().enumerate() {
            let mut temp_buf = ByteBuf::with_estimated_capacity(row.len(), &config);
            transformer.transform_bytes(row.as_bytes(), &mut temp_buf)?;
            result.extend_from_slice(&temp_buf.into_vec());

            if i < json_rows.len() - 1 {
                result.push(b',');
            }
        }

        result.push(b']');
    } else if !json_rows.is_empty() {
        let row = &json_rows[0];
        let mut temp_buf = ByteBuf::with_estimated_capacity(row.len(), &config);
        transformer.transform_bytes(row.as_bytes(), &mut temp_buf)?;
        result.extend_from_slice(&temp_buf.into_vec());
    } else {
        // Empty result for is_list=False: return [] so Python null detection works
        result.extend_from_slice(b"[]");
    }

    result.push(b'}');
    result.push(b'}');

    Ok(result)
}

fn estimate_arena_size(json_rows: &[String]) -> usize {
    let total_input_size: usize = json_rows.iter().map(|s| s.len()).sum();
    (total_input_size / 4).max(8192).min(65536)
}
