//! Shared SQL types used across the compiler, schema, and database layers.
//!
//! These types are defined here (rather than in `compiler::aggregation` or
//! `schema`) so that `db/` can import them without creating a dependency on
//! the compilation or schema layers — a prerequisite for eventually extracting
//! `db/` into its own crate (`fraiseql-db`).

use fraiseql_error::{FraiseQLError, Result};
use serde::{Deserialize, Serialize};

use crate::{types::db_types::DatabaseType, utils::to_snake_case};

/// Declared scalar type of a JSON field, used to pick the SQL cast.
///
/// A JSON/JSONB extraction is always `text`, so both ORDER BY and WHERE have to
/// cast it before the database can compare it as anything else. Sorting `"9"`
/// after `"10"` is the visible symptom for numbers; for WHERE the symptom is a
/// wrong row set (or a hard cast error).
///
/// The variant → SQL-type-name mapping lives in exactly one place —
/// [`SqlDialect::cast_type_name`] — so ORDER BY and WHERE cannot drift apart.
///
/// [`SqlDialect::cast_type_name`]: crate::dialect::SqlDialect::cast_type_name
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[non_exhaustive]
pub enum ScalarFieldType {
    /// No cast — text comparison (correct for strings, UUIDs, IDs, enum values).
    #[default]
    Text,
    /// Integer (`::bigint` / `CAST(… AS BIGINT)`).
    Integer,
    /// Fixed/floating point (`::numeric` / `CAST(… AS DECIMAL(38,12))`).
    Numeric,
    /// Boolean (`::boolean`).
    Boolean,
    /// Instant (`::timestamptz` / `CAST(… AS DATETIME2)`).
    DateTime,
    /// Calendar date (`::date`).
    Date,
    /// Wall-clock time (`::time`).
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
#[non_exhaustive]
pub struct OrderByClause {
    /// Field to order by (GraphQL camelCase name).
    pub field:         String,
    /// Sort direction.
    pub direction:     OrderDirection,
    /// Field type for SQL cast generation. `Text` (default) means no cast.
    #[serde(default)]
    pub field_type:    ScalarFieldType,
    /// Native column name if the view exposes this field as a typed column.
    /// When set, ORDER BY uses this column directly instead of JSONB extraction,
    /// enabling index support and correct typing without casts.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub native_column: Option<String>,
    /// Vector-distance ordering (#386): when set, this clause orders by
    /// `{column} {operator} '{query_vector}'::vector` — the pgvector ANN shape.
    /// Requires [`native_column`](Self::native_column) (a JSONB-extracted text
    /// value would defeat every vector index and re-parse per row).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub vector:        Option<VectorDistanceOrder>,
}

/// The vector-distance operand of an ORDER BY clause (#386).
///
/// `query_vector` is carried as the *body* of a pgvector text literal
/// (`[0.1,0.2,…]` for floats, `1011…` for bits) rather than a bind parameter:
/// it is constructed exclusively by formatting values parsed from the request,
/// so it can only contain digits, `.`, `-`, `e`, commas and brackets — and the
/// SQL builder re-validates that character set before interpolating (defence in
/// depth).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct VectorDistanceOrder {
    /// pgvector distance operator: `<=>` (cosine), `<->` (L2), `<#>` (inner
    /// product), `<~>` (hamming), `<%>` (jaccard).
    pub operator:     String,
    /// The query vector literal body, e.g. `[0.1,0.2,0.3]` or `1011`.
    pub query_vector: String,
    /// Which pgvector operand type the literal is cast to (#959).
    #[serde(default)]
    pub kind:         VectorOperandKind,
}

/// The operand type a vector-distance comparison is made in (#959).
///
/// The cast is load-bearing on the binary side: `'1011'::bit` is `bit(1)`, so a
/// length-less cast silently keeps the first bit and reports the distance
/// between two one-bit values. `varbit` carries whatever width the value has,
/// which makes a width disagreement `different bit lengths 8 and 4` from the
/// server rather than a plausible wrong answer.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[non_exhaustive]
pub enum VectorOperandKind {
    /// Float vector: `'[…]'::vector`, ordered by `<=>` / `<->` / `<#>`.
    #[default]
    Float,
    /// Binary vector: `'1011'::varbit`, ordered by `<~>` / `<%>`.
    Bit,
    /// Sparse float vector: `'{1:0.5,7:0.25}/1000'::sparsevec`, same operators
    /// as [`Self::Float`] (#959).
    ///
    /// A sparse literal has no other reading: `'{1:1}/5'::vector` is
    /// `Vector contents must start with "["`, so this kind is the difference
    /// between a query and an error.
    Sparse,
}

impl VectorOperandKind {
    /// The pgvector type a literal of this kind is cast to.
    ///
    /// `varbit` and not `bit` for the binary kind — see the type's own note.
    ///
    /// There is deliberately **no half-precision kind**: a `halfvec` column
    /// compared against `'[…]'::vector` resolves the literal to `halfvec` and
    /// uses the same `halfvec_*_ops` index — verified on the rig, where both
    /// spellings produce the identical plan. A kind that changes nothing the
    /// server can see would be a distinction the code carries and the database
    /// ignores.
    #[must_use]
    pub const fn cast(self) -> &'static str {
        match self {
            Self::Bit => "varbit",
            Self::Sparse => "sparsevec",
            _ => "vector",
        }
    }
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
            field_type: ScalarFieldType::default(),
            native_column: None,
            vector: None,
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
        crate::utils::validate_graphql_identifier(field, "orderBy")
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
#[non_exhaustive]
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

impl SqlProjectionHint {
    /// Creates a new `SqlProjectionHint`.
    #[must_use]
    pub const fn new(
        database: DatabaseType,
        projection_template: String,
        estimated_reduction_percent: u32,
    ) -> Self {
        Self {
            database,
            projection_template,
            estimated_reduction_percent,
        }
    }
}

#[cfg(test)]
mod tests;
