//! Shared SQL types used across the compiler, schema, and database layers.
//!
//! These types are defined here (rather than in `compiler::aggregation` or
//! `schema`) so that `db/` can import them without creating a dependency on
//! the compilation or schema layers — a prerequisite for eventually extracting
//! `db/` into its own crate (`fraiseql-db`).

use fraiseql_error::{FraiseQLError, Result};
use serde::{Deserialize, Serialize};

use crate::projection_generator::to_snake_case;
use crate::types::db_types::DatabaseType;

/// SQL sort type for ORDER BY cast generation.
///
/// Determines whether the SQL generator wraps the extracted JSONB text in a
/// type cast (e.g., `(data->>'amount')::numeric`) to ensure correct sort order.
/// Without a cast, all JSONB extractions are `text` and sort lexicographically,
/// which is wrong for numeric and date/time fields (`"9" > "10"`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[non_exhaustive]
pub enum OrderByFieldType {
    /// No cast — text/string sort (correct for strings, UUIDs, enum values).
    #[default]
    Text,
    /// Cast to integer type (`::bigint` / `CAST(... AS BIGINT)`).
    Integer,
    /// Cast to floating-point/numeric type (`::numeric` / `CAST(... AS DECIMAL(38,12))`).
    Numeric,
    /// Cast to boolean (`::boolean` / `CAST(... AS UNSIGNED)`).
    Boolean,
    /// Cast to timestamp (`::timestamptz` / `CAST(... AS DATETIME)`).
    /// Also used for ISO-8601 date-time strings which sort correctly as text,
    /// but the cast ensures the database optimizer can use typed comparisons.
    DateTime,
    /// Cast to date (`::date` / `CAST(... AS DATE)`).
    Date,
    /// Cast to time (`::time` / `CAST(... AS TIME)`).
    Time,
}

/// ORDER BY clause with optional type and native column information.
///
/// The SQL generator uses `field_type` to emit the correct type cast for
/// JSONB-extracted values, and `native_column` to bypass JSONB extraction
/// entirely when the view exposes a dedicated typed column.
///
/// # Sort correctness by source
///
/// | Source | Text fields | Numeric fields | Date fields |
/// |--------|------------|----------------|-------------|
/// | JSONB (no cast) | Correct | **Wrong** (lexicographic) | Correct (ISO-8601) |
/// | JSONB (with cast) | Correct | Correct | Correct |
/// | Native column | Correct | Correct | Correct + indexable |
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OrderByClause {
    /// Field to order by (GraphQL camelCase name).
    pub field:     String,
    /// Sort direction.
    pub direction: OrderDirection,
    /// Field type for SQL cast generation. `Text` (default) means no cast.
    #[serde(default)]
    pub field_type: OrderByFieldType,
    /// Native column name if the view exposes this field as a typed column.
    /// When set, ORDER BY uses this column directly instead of JSONB extraction,
    /// enabling index support and correct typing without casts.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub native_column: Option<String>,
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

impl OrderDirection {
    /// Return the SQL keyword for this direction.
    #[must_use]
    pub const fn as_sql(self) -> &'static str {
        match self {
            Self::Asc => "ASC",
            Self::Desc => "DESC",
        }
    }
}

impl OrderByClause {
    /// Create a new `OrderByClause` with default field type (text) and no native column.
    #[must_use]
    pub fn new(field: String, direction: OrderDirection) -> Self {
        Self {
            field,
            direction,
            field_type:    OrderByFieldType::default(),
            native_column: None,
        }
    }

    /// Convert the GraphQL camelCase field name to the JSONB snake_case storage key.
    ///
    /// # Examples
    ///
    /// ```
    /// use fraiseql_db::OrderByClause;
    /// use fraiseql_db::OrderDirection;
    ///
    /// let clause = OrderByClause::new("createdAt".to_string(), OrderDirection::Asc);
    /// assert_eq!(clause.storage_key(), "created_at");
    /// ```
    #[must_use]
    pub fn storage_key(&self) -> String {
        to_snake_case(&self.field)
    }

    /// Validate that a field name matches the GraphQL identifier pattern `[_A-Za-z][_0-9A-Za-z]*`.
    ///
    /// This is a security boundary: field names are interpolated into SQL `data->>'field'`
    /// expressions. Any character outside the GraphQL identifier set must be rejected before
    /// the `OrderByClause` is constructed.
    ///
    /// # Errors
    ///
    /// Returns `FraiseQLError::Validation` if the field contains invalid characters.
    pub fn validate_field_name(field: &str) -> Result<()> {
        let mut chars = field.chars();
        let first_ok = chars.next().is_some_and(|c| c.is_ascii_alphabetic() || c == '_');
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
                    Ok(Self::new(field.clone(), direction))
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
                    Ok(Self::new(field, direction))
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

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)] // Reason: test code, panics are acceptable

    use super::*;

    // ── storage_key ───────────────────────────────────────────────────────

    #[test]
    fn test_storage_key_camel_to_snake() {
        let clause = OrderByClause::new("createdAt".into(), OrderDirection::Asc);
        assert_eq!(clause.storage_key(), "created_at");
    }

    #[test]
    fn test_storage_key_multi_word() {
        let clause = OrderByClause::new("firstName".into(), OrderDirection::Desc);
        assert_eq!(clause.storage_key(), "first_name");
    }

    #[test]
    fn test_storage_key_already_snake() {
        let clause = OrderByClause::new("id".into(), OrderDirection::Asc);
        assert_eq!(clause.storage_key(), "id");
    }

    #[test]
    fn test_storage_key_long_camel() {
        let clause =
            OrderByClause::new("updatedAtTimestamp".into(), OrderDirection::Asc);
        assert_eq!(clause.storage_key(), "updated_at_timestamp");
    }

    // ── OrderDirection::as_sql ────────────────────────────────────────────

    #[test]
    fn test_order_direction_as_sql() {
        assert_eq!(OrderDirection::Asc.as_sql(), "ASC");
        assert_eq!(OrderDirection::Desc.as_sql(), "DESC");
    }

    // ── validate_field_name ───────────────────────────────────────────────

    #[test]
    fn test_validate_field_name_accepts_valid() {
        assert!(OrderByClause::validate_field_name("id").is_ok());
        assert!(OrderByClause::validate_field_name("createdAt").is_ok());
        assert!(OrderByClause::validate_field_name("_private").is_ok());
        assert!(OrderByClause::validate_field_name("field123").is_ok());
    }

    #[test]
    fn test_validate_field_name_rejects_injection() {
        assert!(OrderByClause::validate_field_name("'; DROP TABLE users; --").is_err());
        assert!(OrderByClause::validate_field_name("field name").is_err());
        assert!(OrderByClause::validate_field_name("123start").is_err());
        assert!(OrderByClause::validate_field_name("").is_err());
    }
}
