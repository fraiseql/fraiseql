//! Shared SQL types used across the compiler, schema, and database layers.
//!
//! These types are defined here (rather than in `compiler::aggregation` or
//! `schema`) so that `db/` can import them without creating a dependency on
//! the compilation or schema layers — a prerequisite for eventually extracting
//! `db/` into its own crate (`fraiseql-db`).

use fraiseql_error::{FraiseQLError, Result};
use serde::{Deserialize, Serialize};

use crate::types::db_types::DatabaseType;

/// ORDER BY clause
///
/// # Numeric field sorting
///
/// When sorting on a JSONB field via relay pagination, the value is
/// extracted as `text` using `data->>'field'`. This means **numeric
/// JSON fields sort lexicographically** (`"9" > "10"`), which is
/// incorrect for integer and float data.
///
/// Workaround: expose integer sort keys as a dedicated typed column
/// in the database view. String and ISO-8601 date/time fields sort
/// correctly without this workaround.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OrderByClause {
    /// Field to order by (can be dimension, aggregate, or temporal bucket)
    pub field:     String,
    /// Sort direction
    pub direction: OrderDirection,
}

/// Sort direction
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[non_exhaustive]
pub enum OrderDirection {
    /// Ascending (A-Z, 0-9)
    Asc,
    /// Descending (Z-A, 9-0)
    Desc,
}

impl OrderByClause {
    /// Validate that a field name matches the GraphQL identifier pattern `[_A-Za-z][_0-9A-Za-z]*`.
    ///
    /// This is a security boundary: field names are interpolated into SQL `data->>'field'`
    /// expressions. Any character outside the GraphQL identifier set must be rejected before
    /// the `OrderByClause` is constructed.
    ///
    /// # Errors
    ///
    /// Returns `FraiseQLError::Validation` if the field contains invalid characters.
    fn validate_field_name(field: &str) -> Result<()> {
        let mut chars = field.chars();
        let first_ok = chars.next().map(|c| c.is_ascii_alphabetic() || c == '_').unwrap_or(false);
        let rest_ok = chars.all(|c| c.is_ascii_alphanumeric() || c == '_');
        if first_ok && rest_ok {
            Ok(())
        } else {
            Err(FraiseQLError::Validation {
                message: format!(
                    "orderBy field name '{field}' contains invalid characters; \
                     only [_A-Za-z][_0-9A-Za-z]* is allowed"
                ),
                path:    None,
            })
        }
    }

    /// Parse `orderBy` from a GraphQL variables JSON value.
    ///
    /// Accepts two formats:
    /// - Object: `{ "name": "DESC", "created_at": "ASC" }`
    /// - Array:  `[{ "field": "name", "direction": "DESC" }]`
    ///
    /// Direction strings are case-insensitive.
    ///
    /// # Errors
    ///
    /// Returns `FraiseQLError::Validation` for invalid structure or direction values.
    pub fn from_graphql_json(value: &serde_json::Value) -> Result<Vec<Self>> {
        if let Some(obj) = value.as_object() {
            // Object format: { "name": "DESC", "created_at": "ASC" }
            obj.iter()
                .map(|(field, dir_val)| {
                    let dir_str = dir_val.as_str().ok_or_else(|| FraiseQLError::Validation {
                        message: format!("orderBy direction for '{field}' must be a string"),
                        path:    None,
                    })?;
                    let direction = match dir_str.to_ascii_uppercase().as_str() {
                        "ASC" => OrderDirection::Asc,
                        "DESC" => OrderDirection::Desc,
                        _ => {
                            return Err(FraiseQLError::Validation {
                                message: format!(
                                    "orderBy direction '{dir_str}' must be ASC or DESC"
                                ),
                                path:    None,
                            });
                        },
                    };
                    Self::validate_field_name(field)?;
                    Ok(Self {
                        field: field.clone(),
                        direction,
                    })
                })
                .collect()
        } else if let Some(arr) = value.as_array() {
            // Array format: [{ "field": "name", "direction": "DESC" }]
            arr.iter()
                .map(|item| {
                    let obj = item.as_object().ok_or_else(|| FraiseQLError::Validation {
                        message: "orderBy array items must be objects".to_string(),
                        path:    None,
                    })?;
                    let field = obj
                        .get("field")
                        .and_then(|v| v.as_str())
                        .ok_or_else(|| FraiseQLError::Validation {
                            message: "orderBy item missing 'field' string".to_string(),
                            path:    None,
                        })?
                        .to_string();
                    let dir_str = obj.get("direction").and_then(|v| v.as_str()).unwrap_or("ASC");
                    let direction = match dir_str.to_ascii_uppercase().as_str() {
                        "ASC" => OrderDirection::Asc,
                        "DESC" => OrderDirection::Desc,
                        _ => {
                            return Err(FraiseQLError::Validation {
                                message: format!(
                                    "orderBy direction '{dir_str}' must be ASC or DESC"
                                ),
                                path:    None,
                            });
                        },
                    };
                    Self::validate_field_name(&field)?;
                    Ok(Self { field, direction })
                })
                .collect()
        } else {
            Err(FraiseQLError::Validation {
                message: "orderBy must be an object or array".to_string(),
                path:    None,
            })
        }
    }
}

/// SQL projection hint for database-specific field projection optimization.
///
/// When a type has a large JSONB payload, the compiler can generate
/// SQL that projects only the requested fields, reducing network payload
/// and JSON deserialization overhead.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SqlProjectionHint {
    /// Database type — typed to prevent silent typos (e.g. `"postgresq"`) that
    /// would cause adapters to silently ignore the hint.
    pub database: DatabaseType,

    /// The projection SQL template.
    /// Example for PostgreSQL:
    /// `jsonb_build_object('id', data->>'id', 'email', data->>'email')`
    pub projection_template: String,

    /// Estimated reduction in payload size (percentage 0-100).
    pub estimated_reduction_percent: u32,
}
