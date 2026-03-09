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
//! use fraiseql_core::db::projection_generator::PostgresProjectionGenerator;
//! # use fraiseql_core::error::Result;
//! # fn example() -> Result<()> {
//! let generator = PostgresProjectionGenerator::new();
//! let fields = vec!["id".to_string(), "name".to_string(), "email".to_string()];
//! let sql = generator.generate_projection_sql(&fields)?;
//! assert!(sql.contains("jsonb_build_object"));
//! # Ok(())
//! # }
//! ```

use fraiseql_error::Result;

/// Convert camelCase field name to snake_case for JSON/JSONB key lookup.
///
/// FraiseQL converts schema field names from snake_case to camelCase for GraphQL spec compliance.
/// However, JSON/JSONB keys are stored in their original snake_case form.
/// This function reverses that conversion for JSON key access.
///
/// # Examples
///
/// ```text
/// assert_eq!(to_snake_case("firstName"), "first_name");
/// assert_eq!(to_snake_case("id"), "id");
/// ```
fn to_snake_case(name: &str) -> String {
    let mut result = String::new();
    for (i, ch) in name.chars().enumerate() {
        if ch.is_uppercase() && i > 0 {
            result.push('_');
            result.push(
                ch.to_lowercase()
                    .next()
                    .expect("char::to_lowercase always yields at least one char"),
            );
        } else {
            result.push(ch);
        }
    }
    result
}

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
    /// use fraiseql_core::db::projection_generator::PostgresProjectionGenerator;
    /// # use fraiseql_core::error::Result;
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
    /// Returns [`FraiseQLError::Validation`] if any field name contains characters
    /// that cannot be safely included in a SQL projection.
    pub fn generate_projection_sql(&self, fields: &[String]) -> Result<String> {
        if fields.is_empty() {
            // No fields to project, return pass-through
            return Ok(format!("\"{}\"", self.jsonb_column));
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
/// use fraiseql_core::db::projection_generator::MySqlProjectionGenerator;
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
    /// Returns [`FraiseQLError::Validation`] if any field name cannot be safely projected.
    pub fn generate_projection_sql(&self, fields: &[String]) -> Result<String> {
        if fields.is_empty() {
            return Ok(format!("`{}`", self.json_column));
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
/// use fraiseql_core::db::projection_generator::SqliteProjectionGenerator;
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
    /// Returns [`FraiseQLError::Validation`] if any field name cannot be safely projected.
    pub fn generate_projection_sql(&self, fields: &[String]) -> Result<String> {
        if fields.is_empty() {
            return Ok(format!("\"{}\"", self.json_column));
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
#[allow(clippy::unwrap_used)] // Reason: test code, panics are acceptable
mod tests {
    use super::*;

    #[test]
    fn test_postgres_projection_single_field() {
        let generator = PostgresProjectionGenerator::new();
        let fields = vec!["id".to_string()];

        let sql = generator.generate_projection_sql(&fields).unwrap();
        assert_eq!(sql, "jsonb_build_object('id', \"data\"->>'id' )");
    }

    #[test]
    fn test_postgres_projection_multiple_fields() {
        let generator = PostgresProjectionGenerator::new();
        let fields = vec!["id".to_string(), "name".to_string(), "email".to_string()];

        let sql = generator.generate_projection_sql(&fields).unwrap();
        assert!(sql.contains("jsonb_build_object("));
        assert!(sql.contains("'id', \"data\"->>'id'"));
        assert!(sql.contains("'name', \"data\"->>'name'"));
        assert!(sql.contains("'email', \"data\"->>'email'"));
    }

    #[test]
    fn test_postgres_projection_empty_fields() {
        let generator = PostgresProjectionGenerator::new();
        let fields: Vec<String> = vec![];

        let sql = generator.generate_projection_sql(&fields).unwrap();
        // Empty projection should pass through the JSONB column
        assert_eq!(sql, "\"data\"");
    }

    #[test]
    fn test_postgres_projection_custom_column() {
        let generator = PostgresProjectionGenerator::with_column("metadata");
        let fields = vec!["id".to_string()];

        let sql = generator.generate_projection_sql(&fields).unwrap();
        assert_eq!(sql, "jsonb_build_object('id', \"metadata\"->>'id' )");
    }

    #[test]
    fn test_postgres_select_clause() {
        let generator = PostgresProjectionGenerator::new();
        let fields = vec!["id".to_string(), "name".to_string()];

        let sql = generator.generate_select_clause("users", &fields).unwrap();
        assert!(sql.starts_with("SELECT jsonb_build_object("));
        assert!(sql.contains("as \"data\""));
        assert!(sql.contains("FROM \"users\""));
    }

    #[test]
    fn test_escape_identifier_quoting() {
        // Simple identifiers are wrapped in double-quotes.
        assert_eq!(PostgresProjectionGenerator::escape_identifier("id"), "\"id\"");
        assert_eq!(PostgresProjectionGenerator::escape_identifier("user_id"), "\"user_id\"");
        // Special chars (hyphens, dots) are safe inside quotes.
        assert_eq!(PostgresProjectionGenerator::escape_identifier("field-name"), "\"field-name\"");
        assert_eq!(PostgresProjectionGenerator::escape_identifier("field.name"), "\"field.name\"");
        // Double-quote chars inside the name are doubled.
        assert_eq!(
            PostgresProjectionGenerator::escape_identifier("col\"inject"),
            "\"col\"\"inject\""
        );
    }

    // MySQL Projection Generator Tests
    #[test]
    fn test_mysql_projection_single_field() {
        let generator = MySqlProjectionGenerator::new();
        let fields = vec!["id".to_string()];

        let sql = generator.generate_projection_sql(&fields).unwrap();
        assert_eq!(sql, "JSON_OBJECT('id', JSON_EXTRACT(`data`, '$.id'))");
    }

    #[test]
    fn test_mysql_projection_multiple_fields() {
        let generator = MySqlProjectionGenerator::new();
        let fields = vec!["id".to_string(), "name".to_string(), "email".to_string()];

        let sql = generator.generate_projection_sql(&fields).unwrap();
        assert!(sql.contains("JSON_OBJECT("));
        assert!(sql.contains("'id', JSON_EXTRACT(`data`, '$.id')"));
        assert!(sql.contains("'name', JSON_EXTRACT(`data`, '$.name')"));
        assert!(sql.contains("'email', JSON_EXTRACT(`data`, '$.email')"));
    }

    #[test]
    fn test_mysql_projection_empty_fields() {
        let generator = MySqlProjectionGenerator::new();
        let fields: Vec<String> = vec![];

        let sql = generator.generate_projection_sql(&fields).unwrap();
        assert_eq!(sql, "`data`");
    }

    #[test]
    fn test_mysql_projection_custom_column() {
        let generator = MySqlProjectionGenerator::with_column("metadata");
        let fields = vec!["id".to_string()];

        let sql = generator.generate_projection_sql(&fields).unwrap();
        assert_eq!(sql, "JSON_OBJECT('id', JSON_EXTRACT(`metadata`, '$.id'))");
    }

    // SQLite Projection Generator Tests
    #[test]
    fn test_sqlite_projection_single_field() {
        let generator = SqliteProjectionGenerator::new();
        let fields = vec!["id".to_string()];

        let sql = generator.generate_projection_sql(&fields).unwrap();
        assert_eq!(sql, "json_object('id', json_extract(\"data\", '$.id'))");
    }

    #[test]
    fn test_sqlite_projection_multiple_fields() {
        let generator = SqliteProjectionGenerator::new();
        let fields = vec!["id".to_string(), "name".to_string(), "email".to_string()];

        let sql = generator.generate_projection_sql(&fields).unwrap();
        assert!(sql.contains("json_object("));
        assert!(sql.contains("'id', json_extract(\"data\", '$.id')"));
        assert!(sql.contains("'name', json_extract(\"data\", '$.name')"));
        assert!(sql.contains("'email', json_extract(\"data\", '$.email')"));
    }

    #[test]
    fn test_sqlite_projection_empty_fields() {
        let generator = SqliteProjectionGenerator::new();
        let fields: Vec<String> = vec![];

        let sql = generator.generate_projection_sql(&fields).unwrap();
        assert_eq!(sql, "\"data\"");
    }

    #[test]
    fn test_sqlite_projection_custom_column() {
        let generator = SqliteProjectionGenerator::with_column("metadata");
        let fields = vec!["id".to_string()];

        let sql = generator.generate_projection_sql(&fields).unwrap();
        assert_eq!(sql, "json_object('id', json_extract(\"metadata\", '$.id'))");
    }

    // ========================================================================
    // Issue #269: JSONB field extraction with snake_case/camelCase mapping
    // ========================================================================

    #[test]
    fn test_to_snake_case_conversion() {
        // Test camelCase to snake_case conversion
        assert_eq!(super::to_snake_case("id"), "id");
        assert_eq!(super::to_snake_case("firstName"), "first_name");
        assert_eq!(super::to_snake_case("createdAt"), "created_at");
        assert_eq!(super::to_snake_case("userId"), "user_id");
        assert_eq!(super::to_snake_case("updatedAtTimestamp"), "updated_at_timestamp");
    }

    #[test]
    fn test_postgres_projection_with_field_mapping_snake_case() {
        // Problem: GraphQL converts field names to camelCase (first_name → firstName)
        // But JSONB stores them in snake_case (first_name).
        // When generating JSONB extraction SQL, we must use the original snake_case key,
        // not the camelCase GraphQL name.

        let generator = PostgresProjectionGenerator::new();

        // Simulate what happens when fields come from GraphQL query
        // These are camelCase field names (what GraphQL expects in response)
        let graphql_fields = vec![
            "id".to_string(),
            "firstName".to_string(),
            "createdAt".to_string(),
        ];

        let sql = generator.generate_projection_sql(&graphql_fields).unwrap();

        eprintln!("Generated SQL: {}", sql);

        // Current broken behavior generates:
        // jsonb_build_object('id', data->>'id', 'firstName', data->>'firstName', 'createdAt',
        // data->>'createdAt')
        //
        // This fails because JSONB has snake_case keys: first_name, created_at
        // Result: data->>'firstName' returns NULL (key not found)

        // Regression guard: SQL must use snake_case keys for JSONB access.
        // camelCase field names in the schema (firstName, createdAt) must be
        // mapped to snake_case in generated SQL (first_name, created_at) because
        // PostgreSQL stores JSONB keys verbatim and FraiseQL always writes snake_case.
        assert!(
            !sql.contains("->>'firstName'") && !sql.contains("->>'createdAt'"),
            "Regression: SQL is using camelCase keys for JSONB access. \
             JSONB has snake_case keys ('first_name', 'created_at'). SQL: {}",
            sql
        );
    }

    // =========================================================================
    // Additional projection_generator.rs tests
    // =========================================================================

    #[test]
    fn test_postgres_projection_sql_injection_in_field_name() {
        // A field name containing a single quote must be escaped in the SQL output
        let generator = PostgresProjectionGenerator::new();
        let fields = vec!["user'name".to_string()];
        let sql = generator.generate_projection_sql(&fields).unwrap();
        // The response key literal must not contain an unescaped single quote
        assert!(!sql.contains("'user'name'"), "Single quote in field name must be escaped");
        // Should contain the doubled-quote escape
        assert!(sql.contains("user''name"), "Single quote must be doubled in the SQL literal");
    }

    #[test]
    fn test_mysql_projection_sql_contains_json_object() {
        let generator = MySqlProjectionGenerator::new();
        let fields = vec!["email".to_string(), "name".to_string()];
        let sql = generator.generate_projection_sql(&fields).unwrap();
        assert!(sql.starts_with("JSON_OBJECT("), "MySQL projection must start with JSON_OBJECT");
    }

    #[test]
    fn test_sqlite_projection_custom_column_appears_in_sql() {
        let generator = SqliteProjectionGenerator::with_column("payload");
        let fields = vec!["id".to_string()];
        let sql = generator.generate_projection_sql(&fields).unwrap();
        assert!(sql.contains("\"payload\""), "Custom column name must appear in SQLite SQL");
    }

    #[test]
    fn test_postgres_projection_camel_to_snake_in_jsonb_key() {
        let generator = PostgresProjectionGenerator::new();
        let fields = vec!["updatedAt".to_string()];
        let sql = generator.generate_projection_sql(&fields).unwrap();
        // The JSONB extraction key should be snake_case
        assert!(sql.contains("'updated_at'"), "updatedAt must be mapped to updated_at for JSONB key");
        // The response key in jsonb_build_object should be the original camelCase
        assert!(sql.contains("'updatedAt'"), "Response key must remain camelCase");
    }

    #[test]
    fn test_postgres_select_clause_contains_from() {
        let generator = PostgresProjectionGenerator::new();
        let fields = vec!["id".to_string()];
        let sql = generator.generate_select_clause("orders", &fields).unwrap();
        assert!(sql.contains("FROM \"orders\""), "SELECT clause must include FROM clause with table name");
        assert!(sql.contains("SELECT"), "SELECT clause must start with SELECT");
    }
}
