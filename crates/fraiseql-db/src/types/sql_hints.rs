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
    /// Identity comparison (`ID`/`UUID`) — **no cast**.
    ///
    /// Renders exactly like [`Self::Text`]; the variant exists to mark *which*
    /// fields are identities, not to introduce a cast. `cast_type_name` returns
    /// `None` for it deliberately.
    ///
    /// Casting would be the obvious repair and is the wrong one:
    /// `(data->>'id')::uuid` is evaluated **per row**, so one row holding a
    /// non-UUID identity raises SQLSTATE 22P02 for every query — and `ID` is
    /// documented as intentionally spanning uuid / integer / text keys
    /// (`docs/adr/0017-entity-identity-contract.md`), with fixtures holding
    /// `'user-1'` and a BIGINT pk. Equality is instead made case-insensitive at
    /// the *literal*, which needs no knowledge of the column's type.
    Uuid,
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
    /// Full-text relevance ordering (#1284): when set, this clause orders by
    /// `ts_rank(to_tsvector(document), websearch_to_tsquery($n))`, where the
    /// document is built from the searchable fields the same `?search=`
    /// predicate matched on.
    ///
    /// [`field`](Self::field) is **not read** for such a clause — there is no
    /// single field a relevance rank belongs to — and is empty by construction.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub relevance:     Option<RelevanceOrder>,
}

/// The full-text operand of an ORDER BY clause (#1284).
///
/// Carries the search text as a *value*, not as SQL: unlike
/// [`VectorDistanceOrder`], whose literal is built from parsed numbers and is
/// therefore safe to interpolate, this is arbitrary client input and is bound
/// as a parameter by the renderer.
///
/// # Why the fields are storage keys
///
/// The rank has to be computed over the same expression the search predicate
/// matched on, or the ordering describes a different document from the one that
/// was searched. `build_fts_where_clause` emits a `WhereClause::Field`, whose
/// path `WhereClause::from_graphql_json` lowers to the snake_case storage key
/// and the generator renders as `data->>'key'` — so these are storage keys and
/// the renderer extracts them the same way.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RelevanceOrder {
    /// The snake_case storage keys of the searchable fields the rank is computed
    /// over, in the order the predicate ORs them. Never empty — a rank over no
    /// document is refused at render time.
    pub fields: Vec<String>,
    /// The raw `websearch_to_tsquery` text, exactly as the client sent it.
    pub query:  String,
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
            relevance: None,
        }
    }

    /// A clause that orders by full-text relevance, most relevant first (#1284).
    ///
    /// [`field`](Self::field) is left empty deliberately. A relevance rank is
    /// computed over several fields at once, so naming one of them would be a
    /// lie, and naming a sentinel like `_relevance` would put the sort kind back
    /// into a string — which is the defect this constructor exists to replace:
    /// `[{"_relevance": "desc"}]` type-checked at every layer that touched it
    /// and failed only in the ORDER BY parser, three layers below the handler
    /// that wrote it.
    #[must_use]
    pub fn by_relevance(relevance: RelevanceOrder) -> Self {
        Self {
            field:         String::new(),
            direction:     OrderDirection::Desc,
            field_type:    ScalarFieldType::default(),
            native_column: None,
            vector:        None,
            relevance:     Some(relevance),
        }
    }

    /// Whether rendering this clause binds a SQL parameter.
    ///
    /// True only for full-text relevance (#1284), whose operand is arbitrary
    /// client text and is therefore bound rather than interpolated. A SQL
    /// builder that assembles `ORDER BY` as a bare string — the relay keyset
    /// query, the fraiseql-wire adapter — asks this before rendering, so that
    /// "this ordering cannot be expressed here" is a named refusal instead of a
    /// dropped sort or an escaped literal.
    #[must_use]
    pub const fn binds_parameter(&self) -> bool {
        self.relevance.is_some()
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
