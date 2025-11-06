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
) -> PyResult<Vec<u8>> {
    // Check if schema registry is available for schema-aware transformation
    let registry = schema_registry::get_registry();

    if let (Some(registry), Some(type_name_str)) = (registry, type_name) {
        // SCHEMA-AWARE PATH: Use transform_with_selections() for aliases or transform_with_schema()
        return build_with_schema_awareness(json_rows, field_name, type_name_str, field_paths, field_selections, registry);
    }

    // FALLBACK PATH: Use zero-copy transformer (original behavior)
    build_legacy_response(json_rows, field_name, type_name, field_paths)
}

/// Schema-aware transformation path (NEW)
///
/// Uses the schema registry to correctly resolve nested object types and apply aliases.
/// This fixes Issue #112 where nested JSONB objects had wrong __typename.
fn build_with_schema_awareness(
    json_rows: Vec<String>,
    field_name: &str,
    type_name: &str,
    _field_paths: Option<Vec<Vec<String>>>,  // TODO: Implement field projection (deprecated)
    field_selections: Option<Vec<Value>>,
    registry: &schema_registry::SchemaRegistry,
) -> PyResult<Vec<u8>> {
    // Parse, transform, and serialize each row
    let transformed_items: Result<Vec<Value>, _> = json_rows
        .iter()
        .map(|row_str| {
            // Parse JSON
            serde_json::from_str::<Value>(row_str)
                .map_err(|e| pyo3::exceptions::PyValueError::new_err(
                    format!("Failed to parse JSON: {}", e)
                ))
                .map(|value| {
                    // Transform with schema awareness and optional alias support
                    if let Some(ref selections) = field_selections {
                        // Use transform_with_selections for alias support
                        json_transform::transform_with_selections(&value, type_name, selections, registry)
                    } else {
                        // Fallback to transform_with_schema (no aliases)
                        json_transform::transform_with_schema(&value, type_name, registry)
                    }
                })
        })
        .collect();

    let transformed_items = transformed_items?;

    // Build GraphQL response structure
    let response_data = if json_rows.len() == 1 {
        // Single object
        json!({
            "data": {
                field_name: transformed_items.get(0).cloned().unwrap_or(Value::Null)
            }
        })
    } else {
        // Array of objects
        json!({
            "data": {
                field_name: transformed_items
            }
        })
    };

    // Serialize to bytes
    serde_json::to_vec(&response_data)
        .map_err(|e| pyo3::exceptions::PyValueError::new_err(
            format!("Failed to serialize response: {}", e)
        ))
}

/// Legacy zero-copy transformation (FALLBACK)
///
/// Used when schema registry is not available or type_name is not provided.
/// This is the original implementation before schema-aware transformation.
fn build_legacy_response(
    json_rows: Vec<String>,
    field_name: &str,
    type_name: Option<&str>,
    field_paths: Option<Vec<Vec<String>>>,
) -> PyResult<Vec<u8>> {
    // Setup arena (request-scoped)
    let arena = Arena::with_capacity(estimate_arena_size(&json_rows));

    // Setup transformer
    let config = TransformConfig {
        add_typename: type_name.is_some(),
        camel_case: true,
        project_fields: field_paths.is_some(),
        add_graphql_wrapper: false,  // Pipeline adds its own wrapper
    };

    let field_set = field_paths
        .map(|paths| FieldSet::from_paths(&paths, &arena));

    let transformer = ZeroCopyTransformer::new(
        &arena,
        config,
        type_name,
        field_set.as_ref(),
    );

    // Estimate output size (include wrapper overhead)
    let total_input_size: usize = json_rows.iter().map(|s| s.len()).sum();
    let wrapper_overhead = 50 + field_name.len(); // {"data":{"field":...}}
    let estimated_size = total_input_size + wrapper_overhead;

    // Pre-allocate output buffer with proper capacity
    let mut result = Vec::with_capacity(estimated_size);

    // Build GraphQL response structure manually for clarity and correctness
    // Format: {"data":{"<field_name>":<transformed_data>}}

    result.extend_from_slice(b"{\"data\":{\"");
    result.extend_from_slice(field_name.as_bytes());
    result.extend_from_slice(b"\":");

    // Transform and append data
    if json_rows.len() == 1 {
        // Single object - no array wrapper
        let row = &json_rows[0];
        let mut temp_buf = ByteBuf::with_estimated_capacity(row.len(), &config);
        transformer.transform_bytes(row.as_bytes(), &mut temp_buf)?;
        result.extend_from_slice(&temp_buf.into_vec());
    } else {
        // Multiple objects - array wrapper
        result.push(b'[');

        for (i, row) in json_rows.iter().enumerate() {
            let mut temp_buf = ByteBuf::with_estimated_capacity(row.len(), &config);
            transformer.transform_bytes(row.as_bytes(), &mut temp_buf)?;
            result.extend_from_slice(&temp_buf.into_vec());

            // Add comma between rows
            if i < json_rows.len() - 1 {
                result.push(b',');
            }
        }

        result.push(b']');
    }

    // Close data object and root object
    result.push(b'}');  // Close data object
    result.push(b'}');  // Close root object

    Ok(result)
}

/// Estimate arena size based on input workload
///
/// Arena is used for temporary allocations during transformation:
/// - Transformed field names (camelCase)
/// - Intermediate string buffers
/// - Field projection bitmaps
fn estimate_arena_size(json_rows: &[String]) -> usize {
    let total_input_size: usize = json_rows.iter().map(|s| s.len()).sum();

    // Estimate: 25% of input size for temporary buffers
    // Minimum 8KB, maximum 64KB
    let estimated = (total_input_size / 4).max(8192).min(65536);

    estimated
}
