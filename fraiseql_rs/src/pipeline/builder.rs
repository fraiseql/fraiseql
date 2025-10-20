//! Pipeline response builder for GraphQL responses
//!
//! This module provides the high-level API for building complete GraphQL
//! responses from PostgreSQL JSON rows using the zero-copy transformation engine.

use pyo3::prelude::*;
use crate::core::arena::Arena;
use crate::core::transform::{TransformConfig, ZeroCopyTransformer, ByteBuf};
use crate::pipeline::projection::FieldSet;

/// Build complete GraphQL response from PostgreSQL JSON rows
///
/// This is the TOP-LEVEL API called from Python:
/// ```python
/// response_bytes = fraiseql_rs.build_graphql_response(
///     json_rows=["{'id':1}", "{'id':2}"],
///     field_name="users",
///     typename="User",
///     field_paths=[["id"], ["firstName"]],
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
///        ▼
/// ┌──────────────┐
/// │ HTTP Bytes   │ → Return to Python (zero-copy)
/// │ (Vec<u8>)    │
/// └──────────────┘
///
#[pyfunction]
pub fn build_graphql_response(
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
