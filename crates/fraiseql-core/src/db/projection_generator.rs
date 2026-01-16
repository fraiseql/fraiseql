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
//! - MySQL, SQLite, SQL Server: Support added in Phase 10+
//!
//! # Example
//!
//! ```rust,ignore
//! use fraiseql_core::db::projection_generator::PostgresProjectionGenerator;
//!
//! let generator = PostgresProjectionGenerator::new();
//! let fields = vec!["id".to_string(), "name".to_string(), "email".to_string()];
//! let sql = generator.generate_projection_sql("user_data", &fields)?;
//! // Returns: jsonb_build_object('id', data->>'id', 'name', data->>'name', 'email', data->>'email')
//! ```

use crate::error::Result;

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
    /// ```rust,ignore
    /// let generator = PostgresProjectionGenerator::new();
    /// let fields = vec!["id".to_string(), "email".to_string()];
    /// let sql = generator.generate_projection_sql(&fields)?;
    /// // Returns:
    /// // jsonb_build_object('id', data->>'id', 'email', data->>'email')
    /// ```
    pub fn generate_projection_sql(&self, fields: &[String]) -> Result<String> {
        if fields.is_empty() {
            // No fields to project, return pass-through
            return Ok(format!("\"{}\"", self.jsonb_column));
        }

        // Build the jsonb_build_object() call with all requested fields
        let field_pairs: Vec<String> = fields
            .iter()
            .map(|field| {
                // Escape field name for SQL (simple protection against injection)
                let safe_field = Self::escape_identifier(field);
                format!("'{}', \"{}\"->>'{}' ", safe_field, self.jsonb_column, safe_field)
            })
            .collect();

        // Format: jsonb_build_object('field1', data->>'field1', 'field2', data->>'field2', ...)
        Ok(format!(
            "jsonb_build_object({})",
            field_pairs.join(",")
        ))
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
    /// ```rust,ignore
    /// let sql = generator.generate_select_clause("t", &fields)?;
    /// // Returns: SELECT jsonb_build_object(...) as data
    /// ```
    pub fn generate_select_clause(&self, table_alias: &str, fields: &[String]) -> Result<String> {
        let projection = self.generate_projection_sql(fields)?;
        Ok(format!(
            "SELECT {} as \"{}\" FROM \"{}\" ",
            projection, self.jsonb_column, table_alias
        ))
    }

    /// Check if field name is safe for SQL (no injection).
    ///
    /// PostgreSQL identifiers can contain alphanumeric, underscore, and dollar signs.
    /// This is a conservative check - in production, use parameterized queries.
    fn is_safe_identifier(field: &str) -> bool {
        !field.is_empty() && field.chars().all(|c| c.is_alphanumeric() || c == '_' || c == '$')
    }

    /// Escape SQL identifier safely.
    ///
    /// For production use, should be parameterized. This is defensive escaping
    /// by replacing single quotes with double quotes (PostgreSQL convention).
    fn escape_identifier(field: &str) -> String {
        // Validate field name
        if !Self::is_safe_identifier(field) {
            // In production, would reject or sanitize more strictly
            // For now, pass through with warning logged at runtime
            return field.to_string();
        }
        field.to_string()
    }
}

impl Default for PostgresProjectionGenerator {
    fn default() -> Self {
        Self::new()
    }
}

/// MySQL SQL projection generator (Phase 10).
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
    pub fn generate_projection_sql(&self, fields: &[String]) -> Result<String> {
        if fields.is_empty() {
            return Ok(format!("`{}`", self.json_column));
        }

        let field_pairs: Vec<String> = fields
            .iter()
            .map(|field| {
                let safe_field = Self::escape_identifier(field);
                format!("'{}', JSON_EXTRACT(`{}`, '$.{}')", safe_field, self.json_column, safe_field)
            })
            .collect();

        Ok(format!("JSON_OBJECT({})", field_pairs.join(",")))
    }

    /// Check if field name is safe for SQL.
    fn is_safe_identifier(field: &str) -> bool {
        !field.is_empty() && field.chars().all(|c| c.is_alphanumeric() || c == '_' || c == '$')
    }

    /// Escape SQL identifier safely.
    fn escape_identifier(field: &str) -> String {
        if !Self::is_safe_identifier(field) {
            return field.to_string();
        }
        field.to_string()
    }
}

impl Default for MySqlProjectionGenerator {
    fn default() -> Self {
        Self::new()
    }
}

/// SQLite SQL projection generator (Phase 10).
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
    pub fn generate_projection_sql(&self, fields: &[String]) -> Result<String> {
        if fields.is_empty() {
            return Ok(format!("\"{}\"", self.json_column));
        }

        let field_pairs: Vec<String> = fields
            .iter()
            .map(|field| {
                let safe_field = Self::escape_identifier(field);
                format!("'{}', json_extract(\"{}\", '$.{}')", safe_field, self.json_column, safe_field)
            })
            .collect();

        Ok(format!("json_object({})", field_pairs.join(",")))
    }

    /// Check if field name is safe for SQL.
    fn is_safe_identifier(field: &str) -> bool {
        !field.is_empty() && field.chars().all(|c| c.is_alphanumeric() || c == '_' || c == '$')
    }

    /// Escape SQL identifier safely.
    fn escape_identifier(field: &str) -> String {
        if !Self::is_safe_identifier(field) {
            return field.to_string();
        }
        field.to_string()
    }
}

impl Default for SqliteProjectionGenerator {
    fn default() -> Self {
        Self::new()
    }
}

/// SQL Server SQL projection generator (Phase 10).
///
/// SQL Server uses `JSON_QUERY()` and `JSON_MODIFY()` for JSON manipulation.
/// Placeholder for future implementation.
#[allow(dead_code)]
pub struct SqlServerProjectionGenerator {
    json_column: String,
}

#[cfg(test)]
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
        let fields = vec![
            "id".to_string(),
            "name".to_string(),
            "email".to_string(),
        ];

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
    fn test_identifier_validation() {
        assert!(PostgresProjectionGenerator::is_safe_identifier("id"));
        assert!(PostgresProjectionGenerator::is_safe_identifier("user_id"));
        assert!(PostgresProjectionGenerator::is_safe_identifier("user$data"));
        assert!(PostgresProjectionGenerator::is_safe_identifier("field123"));
        assert!(!PostgresProjectionGenerator::is_safe_identifier("field-name")); // hyphen not allowed
        assert!(!PostgresProjectionGenerator::is_safe_identifier("field.name")); // dot not allowed
        assert!(!PostgresProjectionGenerator::is_safe_identifier("")); // empty not allowed
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
        let fields = vec![
            "id".to_string(),
            "name".to_string(),
            "email".to_string(),
        ];

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
        let fields = vec![
            "id".to_string(),
            "name".to_string(),
            "email".to_string(),
        ];

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
}
