//! SQL Projection Query Generator
//!
//! Generates database-specific SQL for field projection optimization.
//!
//! # Overview
//!
//! When a schema type has a `SqlProjectionHint`, this module generates the actual SQL
//! to project only requested fields at the database level, reducing network payload
//! and JSON deserialization overhead.
//!
//! # Supported Databases
//!
//! - PostgreSQL: Uses `jsonb_build_object()` for efficient field selection
//! - MySQL, SQLite, SQL Server: Multi-database support
//!
//! # Example
//!
//! ```rust
//! use fraiseql_db::projection_generator::PostgresProjectionGenerator;
//! # use fraiseql_error::Result;
//! # fn example() -> Result<()> {
//! let generator = PostgresProjectionGenerator::new();
//! let fields = vec!["id".to_string(), "name".to_string(), "email".to_string()];
//! let sql = generator.generate_projection_sql(&fields)?;
//! assert!(sql.contains("jsonb_build_object"));
//! # Ok(())
//! # }
//! ```

use fraiseql_error::{FraiseQLError, Result};

/// The semantic kind of a projection field, determining which JSONB extraction
/// operator to use in generated SQL.
///
/// - `Text` → `->>` (extracts as text — for String and ID scalars)
/// - `Native` → `->` (preserves native JSON type — Int, Float, Boolean, DateTime, etc.)
/// - `Composite` → `->` (preserves full JSONB structure)
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FieldKind {
    /// Text scalar — extracted with `->>` (String, ID).
    Text,
    /// Native JSON scalar — extracted with `->` to preserve type (Int, Float, Boolean, DateTime,
    /// etc.).
    Native,
    /// Object or list — extracted with `->` to preserve JSONB structure.
    Composite,
}

/// A field in a SQL projection with type information.
///
/// Used by typed projection generators to choose the correct JSONB extraction
/// operator based on [`FieldKind`]: `->` (preserves JSONB) for composites and
/// native scalars, `->>` (text) for text scalars (String, ID).
///
/// When `sub_fields` is populated on a composite field, `generate_typed_projection_sql`
/// will recurse and emit a nested `jsonb_build_object(...)` instead of returning the full
/// composite blob.  Leave `sub_fields` as `None` to get the existing `data->'field'`
/// behaviour (full blob, no sub-selection).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProjectionField {
    /// GraphQL field name (camelCase).
    pub name: String,

    /// Semantic kind of the field, controlling the JSONB extraction operator.
    pub kind: FieldKind,

    /// Sub-fields to project for composite (Object) types.
    ///
    /// When `Some` and non-empty, the generator recurses and produces a nested
    /// `jsonb_build_object` instead of returning the entire composite blob.
    /// Set to `None` (or `Some([])`) to fall back to `data->'field'`.
    /// List fields should always use `None` — sub-projection inside aggregated
    /// JSONB arrays is out of scope for this first iteration.
    pub sub_fields: Option<Vec<ProjectionField>>,
}

impl ProjectionField {
    /// Create a text scalar projection field (uses `->>` text extraction).
    ///
    /// Use for String and ID fields only. Other scalars (Int, Float, Boolean,
    /// DateTime, etc.) should use [`Self::native`].
    #[must_use]
    pub fn scalar(name: impl Into<String>) -> Self {
        Self {
            name:       name.into(),
            kind:       FieldKind::Text,
            sub_fields: None,
        }
    }

    /// Create a native JSON scalar projection field (uses `->` to preserve type).
    ///
    /// Use for Int, Float, Boolean, DateTime, Date, Time, Decimal, Vector, and
    /// other non-text scalars. `->>` would coerce these to strings inside
    /// `jsonb_build_object`, losing type information.
    #[must_use]
    pub fn native(name: impl Into<String>) -> Self {
        Self {
            name:       name.into(),
            kind:       FieldKind::Native,
            sub_fields: None,
        }
    }

    /// Create a composite (object/list) projection field (uses `->` JSONB extraction).
    #[must_use]
    pub fn composite(name: impl Into<String>) -> Self {
        Self {
            name:       name.into(),
            kind:       FieldKind::Composite,
            sub_fields: None,
        }
    }

    /// Create a composite projection field with known sub-fields.
    ///
    /// The generator will recurse into `sub_fields` and emit a nested
    /// `jsonb_build_object(...)` rather than returning the full composite blob.
    #[must_use]
    pub fn composite_with_sub_fields(name: impl Into<String>, sub_fields: Vec<Self>) -> Self {
        Self {
            name:       name.into(),
            kind:       FieldKind::Composite,
            sub_fields: Some(sub_fields),
        }
    }

    /// Whether this field is a composite type (Object or List).
    #[must_use]
    pub const fn is_composite(&self) -> bool {
        matches!(self.kind, FieldKind::Composite)
    }
}

impl From<String> for ProjectionField {
    fn from(name: String) -> Self {
        Self::scalar(name)
    }
}

/// Validate that a GraphQL field name contains only characters that are safe
/// for use in SQL projections (alphanumeric characters and underscores only).
///
/// GraphQL field names in FraiseQL are either snake_case (schema definitions)
/// or camelCase (after the compiler's automatic conversion). Both forms are
/// subsets of `[a-zA-Z_][a-zA-Z0-9_]*`, so this function rejects any name
/// that falls outside that alphabet.
///
/// # Errors
///
/// Returns `FraiseQLError::Validation` if `field` contains a character outside
/// `[a-zA-Z0-9_]`.
fn validate_field_name(field: &str) -> Result<()> {
    if field.chars().all(|c| c.is_ascii_alphanumeric() || c == '_') {
        Ok(())
    } else {
        Err(FraiseQLError::Validation {
            message: format!(
                "field name '{}' contains characters that cannot be safely projected; \
                 only ASCII alphanumeric characters and underscores are allowed",
                field
            ),
            path:    None,
        })
    }
}

use crate::utils::to_snake_case;

/// Maximum nesting depth for recursive JSONB projection.
///
/// Prevents pathological schemas from producing unbounded SQL. Fields at depth ≥ this
/// value fall back to `data->'field'` (full composite blob), matching the pre-recursion
/// behaviour.
const MAX_PROJECTION_DEPTH: usize = 4;

/// PostgreSQL SQL projection generator using jsonb_build_object.
///
/// Generates efficient PostgreSQL SQL that projects only requested JSONB fields,
/// reducing payload size and JSON deserialization time.
pub struct PostgresProjectionGenerator {
    /// JSONB column name (typically "data")
    jsonb_column: String,
}

impl PostgresProjectionGenerator {
    /// Create new PostgreSQL projection generator with default JSONB column name.
    ///
    /// Default JSONB column: "data"
    #[must_use]
    pub fn new() -> Self {
        Self::with_column("data")
    }

    /// Create projection generator with custom JSONB column name.
    ///
    /// # Arguments
    ///
    /// * `jsonb_column` - Name of the JSONB column in the database table
    #[must_use]
    pub fn with_column(jsonb_column: &str) -> Self {
        Self {
            jsonb_column: jsonb_column.to_string(),
        }
    }

    /// Generate PostgreSQL projection SQL for specified fields.
    ///
    /// Generates a `jsonb_build_object()` call that selects only the requested fields
    /// from the JSONB column, drastically reducing payload size.
    ///
    /// # Arguments
    ///
    /// * `fields` - GraphQL field names to project from JSONB
    ///
    /// # Returns
    ///
    /// SQL fragment that can be used in a SELECT clause, e.g.:
    /// `jsonb_build_object('id', data->>'id', 'email', data->>'email')`
    ///
    /// # Example
    ///
    /// ```rust
    /// use fraiseql_db::projection_generator::PostgresProjectionGenerator;
    /// # use fraiseql_error::Result;
    /// # fn example() -> Result<()> {
    /// let generator = PostgresProjectionGenerator::new();
    /// let fields = vec!["id".to_string(), "email".to_string()];
    /// let sql = generator.generate_projection_sql(&fields)?;
    /// // Returns:
    /// // jsonb_build_object('id', data->>'id', 'email', data->>'email')
    /// assert!(sql.contains("jsonb_build_object"));
    /// # Ok(())
    /// # }
    /// ```
    ///
    /// # Errors
    ///
    /// Returns `FraiseQLError::Validation` if any field name contains characters
    /// that cannot be safely included in a SQL projection.
    pub fn generate_projection_sql(&self, fields: &[String]) -> Result<String> {
        if fields.is_empty() {
            // No fields to project, return pass-through
            return Ok(format!("\"{}\"", self.jsonb_column));
        }

        // Validate all field names before generating any SQL.
        for field in fields {
            validate_field_name(field)?;
        }

        // Build the jsonb_build_object() call with all requested fields
        let field_pairs: Vec<String> = fields
            .iter()
            .map(|field| {
                // Response key uses the GraphQL field name (camelCase).
                // Used as a SQL *string literal* key (inside single-quotes): escape ' → ''.
                let safe_field = Self::escape_sql_string(field);
                // JSONB key uses the original schema field name (snake_case).
                let jsonb_key = to_snake_case(field);
                let safe_jsonb_key = Self::escape_sql_string(&jsonb_key);
                format!("'{}', \"{}\"->>'{}' ", safe_field, self.jsonb_column, safe_jsonb_key)
            })
            .collect();

        // Format: jsonb_build_object('field1', data->>'field1', 'field2', data->>'field2', ...)
        Ok(format!("jsonb_build_object({})", field_pairs.join(",")))
    }

    /// Generate type-aware PostgreSQL projection SQL.
    ///
    /// Uses `->` (JSONB extraction) for composite fields (objects, lists) and
    /// `->>` (text extraction) for scalar fields. This avoids the unnecessary
    /// text→JSON round-trip that occurs when `->>` is used for nested objects.
    ///
    /// When a composite field carries `sub_fields`, the generator recurses and
    /// emits a nested `jsonb_build_object(...)` that selects only the requested
    /// sub-fields rather than returning the entire blob.  Recursion is capped at
    /// `MAX_PROJECTION_DEPTH` levels; deeper fields fall back to `data->'field'`.
    ///
    /// # Arguments
    ///
    /// * `fields` - Projection fields with type information
    ///
    /// # Errors
    ///
    /// Returns `FraiseQLError::Validation` if any field name contains characters
    /// that cannot be safely included in a SQL projection.
    pub fn generate_typed_projection_sql(&self, fields: &[ProjectionField]) -> Result<String> {
        if fields.is_empty() {
            return Ok(format!("\"{}\"", self.jsonb_column));
        }

        let path = format!("\"{}\"", self.jsonb_column);
        let field_pairs = fields
            .iter()
            .map(|field| Self::render_field(field, &path, 0))
            .collect::<Result<Vec<_>>>()?;

        Ok(format!("jsonb_build_object({})", field_pairs.join(",")))
    }

    /// Recursively render one projection field as a `'key', <expr>` pair for
    /// `jsonb_build_object`.
    ///
    /// * `field` — field to render
    /// * `path`  — JSONB path prefix built so far (e.g. `"data"` at depth 0, `"data"->'author'` at
    ///   depth 1)
    /// * `depth` — current recursion depth (capped at `MAX_PROJECTION_DEPTH`)
    fn render_field(field: &ProjectionField, path: &str, depth: usize) -> Result<String> {
        let resp_key = Self::escape_sql_string(&field.name);
        let jsonb_key = to_snake_case(&field.name);
        let safe_jsonb_key = Self::escape_sql_string(&jsonb_key);

        // Recurse into Object sub-fields when available and within depth limit.
        if depth < MAX_PROJECTION_DEPTH {
            if let Some(subs) = &field.sub_fields {
                if !subs.is_empty() {
                    let nested_path = format!("{}->'{}'", path, safe_jsonb_key);
                    let inner = subs
                        .iter()
                        .map(|sf| Self::render_field(sf, &nested_path, depth + 1))
                        .collect::<Result<Vec<_>>>()?;
                    return Ok(format!("'{}', jsonb_build_object({})", resp_key, inner.join(",")));
                }
            }
        }

        // Text: ->> (text cast, for String/ID).
        // Native / Composite: -> (preserves native JSONB type).
        let op = if field.kind == FieldKind::Text {
            "->>"
        } else {
            "->"
        };
        Ok(format!("'{}', {}{}'{}'", resp_key, path, op, safe_jsonb_key))
    }

    /// Generate complete SELECT clause with projection for a table.
    ///
    /// # Arguments
    ///
    /// * `table_alias` - Table alias or name in the FROM clause
    /// * `fields` - Fields to project
    ///
    /// # Returns
    ///
    /// Complete SELECT clause, e.g.: `SELECT jsonb_build_object(...) as data`
    ///
    /// # Example
    ///
    /// ```rust
    /// use fraiseql_db::projection_generator::PostgresProjectionGenerator;
    ///
    /// let generator = PostgresProjectionGenerator::new();
    /// let fields = vec!["id".to_string(), "name".to_string()];
    /// let sql = generator.generate_select_clause("t", &fields).unwrap();
    /// assert!(sql.contains("SELECT"));
    /// ```
    ///
    /// # Errors
    ///
    /// Propagates any error from [`Self::generate_projection_sql`].
    pub fn generate_select_clause(&self, table_alias: &str, fields: &[String]) -> Result<String> {
        let projection = self.generate_projection_sql(fields)?;
        Ok(format!(
            "SELECT {} as \"{}\" FROM \"{}\" ",
            projection, self.jsonb_column, table_alias
        ))
    }

    /// Escape a value for use as a SQL *string literal* (inside single quotes).
    ///
    /// Doubles any embedded single-quote (`'` → `''`) to prevent SQL injection
    /// when the field name is embedded as a string literal key, e.g. in
    /// `jsonb_build_object('key', ...)` or `data->>'key'`.
    fn escape_sql_string(s: &str) -> String {
        s.replace('\'', "''")
    }

    /// Escape a SQL identifier using PostgreSQL double-quote quoting.
    ///
    /// Double-quote delimiters prevent identifier injection: any `"` within
    /// the identifier is doubled (`""`), and the whole name is wrapped in `"`.
    /// Use this when the name appears in an *identifier* position (column name,
    /// table alias) rather than as a string literal.
    #[allow(dead_code)] // Reason: available for callers embedding names as SQL identifiers
    fn escape_identifier(field: &str) -> String {
        format!("\"{}\"", field.replace('"', "\"\""))
    }
}

impl Default for PostgresProjectionGenerator {
    fn default() -> Self {
        Self::new()
    }
}

/// MySQL SQL projection generator.
///
/// MySQL uses `JSON_OBJECT()` for field projection, similar to PostgreSQL's `jsonb_build_object()`.
/// Generates efficient SQL that projects only requested JSON fields.
///
/// # Example
///
/// ```
/// use fraiseql_db::projection_generator::MySqlProjectionGenerator;
///
/// let generator = MySqlProjectionGenerator::new();
/// let fields = vec!["id".to_string(), "name".to_string()];
/// let sql = generator.generate_projection_sql(&fields).unwrap();
/// assert!(sql.contains("JSON_OBJECT"));
/// ```
pub struct MySqlProjectionGenerator {
    json_column: String,
}

impl MySqlProjectionGenerator {
    /// Create new MySQL projection generator with default JSON column name.
    ///
    /// Default JSON column: "data"
    #[must_use]
    pub fn new() -> Self {
        Self::with_column("data")
    }

    /// Create projection generator with custom JSON column name.
    ///
    /// # Arguments
    ///
    /// * `json_column` - Name of the JSON column in the database table
    #[must_use]
    pub fn with_column(json_column: &str) -> Self {
        Self {
            json_column: json_column.to_string(),
        }
    }

    /// Generate MySQL projection SQL for specified fields.
    ///
    /// Generates a `JSON_OBJECT()` call that selects only the requested fields
    /// from the JSON column.
    ///
    /// # Arguments
    ///
    /// * `fields` - JSON field names to project
    ///
    /// # Returns
    ///
    /// SQL fragment that can be used in a SELECT clause
    ///
    /// # Errors
    ///
    /// Returns `FraiseQLError::Validation` if any field name cannot be safely projected.
    pub fn generate_projection_sql(&self, fields: &[String]) -> Result<String> {
        if fields.is_empty() {
            return Ok(format!("`{}`", self.json_column));
        }

        // Validate all field names before generating any SQL.
        for field in fields {
            validate_field_name(field)?;
        }

        let field_pairs: Vec<String> = fields
            .iter()
            .map(|field| {
                // Response key used as SQL string literal key — escape ' → ''.
                let safe_field = Self::escape_sql_string(field);
                // JSON key uses the original schema field name (snake_case).
                let json_key = to_snake_case(field);
                format!("'{}', JSON_EXTRACT(`{}`, '$.{}')", safe_field, self.json_column, json_key)
            })
            .collect();

        Ok(format!("JSON_OBJECT({})", field_pairs.join(",")))
    }

    /// Escape a value for use as a SQL *string literal* (inside single quotes).
    fn escape_sql_string(s: &str) -> String {
        s.replace('\'', "''")
    }

    /// Escape a SQL identifier using MySQL backtick quoting.
    ///
    /// Use this when the name appears in an *identifier* position (column name,
    /// table alias), not as a string literal.
    #[allow(dead_code)] // Reason: available for callers embedding names as SQL identifiers
    fn escape_identifier(field: &str) -> String {
        format!("`{}`", field.replace('`', "``"))
    }
}

impl Default for MySqlProjectionGenerator {
    fn default() -> Self {
        Self::new()
    }
}

/// SQLite SQL projection generator.
///
/// SQLite's JSON support is more limited than PostgreSQL and MySQL.
/// Uses `json_object()` with `json_extract()` to project fields.
///
/// # Example
///
/// ```
/// use fraiseql_db::projection_generator::SqliteProjectionGenerator;
///
/// let generator = SqliteProjectionGenerator::new();
/// let fields = vec!["id".to_string(), "name".to_string()];
/// let sql = generator.generate_projection_sql(&fields).unwrap();
/// assert!(sql.contains("json_object"));
/// ```
pub struct SqliteProjectionGenerator {
    json_column: String,
}

impl SqliteProjectionGenerator {
    /// Create new SQLite projection generator with default JSON column name.
    ///
    /// Default JSON column: "data"
    #[must_use]
    pub fn new() -> Self {
        Self::with_column("data")
    }

    /// Create projection generator with custom JSON column name.
    ///
    /// # Arguments
    ///
    /// * `json_column` - Name of the JSON column in the database table
    #[must_use]
    pub fn with_column(json_column: &str) -> Self {
        Self {
            json_column: json_column.to_string(),
        }
    }

    /// Generate SQLite projection SQL for specified fields.
    ///
    /// Generates a `json_object()` call that selects only the requested fields.
    ///
    /// # Arguments
    ///
    /// * `fields` - JSON field names to project
    ///
    /// # Returns
    ///
    /// SQL fragment that can be used in a SELECT clause
    ///
    /// # Errors
    ///
    /// Returns `FraiseQLError::Validation` if any field name cannot be safely projected.
    pub fn generate_projection_sql(&self, fields: &[String]) -> Result<String> {
        if fields.is_empty() {
            return Ok(format!("\"{}\"", self.json_column));
        }

        // Validate all field names before generating any SQL.
        for field in fields {
            validate_field_name(field)?;
        }

        let field_pairs: Vec<String> = fields
            .iter()
            .map(|field| {
                // Response key used as SQL string literal key — escape ' → ''.
                let safe_field = Self::escape_sql_string(field);
                // JSON key uses the original schema field name (snake_case).
                let json_key = to_snake_case(field);
                format!(
                    "'{}', json_extract(\"{}\", '$.{}')",
                    safe_field, self.json_column, json_key
                )
            })
            .collect();

        Ok(format!("json_object({})", field_pairs.join(",")))
    }

    /// Escape a value for use as a SQL *string literal* (inside single quotes).
    fn escape_sql_string(s: &str) -> String {
        s.replace('\'', "''")
    }

    /// Escape a SQL identifier using SQLite double-quote quoting.
    ///
    /// Double-quote delimiters prevent identifier injection: any `"` within
    /// the identifier is doubled (`""`), and the whole name is wrapped in `"`.
    /// Use this when the name appears in an *identifier* position (column name,
    /// table alias), not as a string literal.
    #[allow(dead_code)] // Reason: available for callers that embed field names as identifiers
    fn escape_identifier(field: &str) -> String {
        format!("\"{}\"", field.replace('"', "\"\""))
    }
}

impl Default for SqliteProjectionGenerator {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests;
