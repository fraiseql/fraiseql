//! Field and value type definitions for operators
//!
//! Provides type-safe representations of database fields and values
//! to prevent SQL injection and improve API ergonomics.

use std::fmt;

/// Represents a field reference in a WHERE clause or ORDER BY
///
/// Supports both JSONB payload fields and direct database columns,
/// with automatic type casting and proper SQL generation.
///
/// # Examples
///
/// ```ignore
/// // JSONB field: (data->>'name')
/// Field::JsonbField("name".to_string())
///
/// // Direct column: created_at
/// Field::DirectColumn("created_at".to_string())
///
/// // Nested JSONB: (data->'user'->>'name')
/// Field::JsonbPath(vec!["user".to_string(), "name".to_string()])
/// ```
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Field {
    /// A field extracted from the JSONB `data` column with text extraction (->>)
    ///
    /// The value is extracted as text and wrapped in parentheses.
    ///
    /// Generated SQL: `(data->>'field_name')`
    JsonbField(String),

    /// A direct database column (not from JSONB)
    ///
    /// Uses the native type stored in the database.
    ///
    /// Generated SQL: `column_name`
    DirectColumn(String),

    /// A nested path within the JSONB `data` column
    ///
    /// The path is traversed left-to-right, with intermediate steps using `->` (JSON navigation)
    /// and the final step using `->>` (text extraction).
    ///
    /// All extracted values are text and wrapped in parentheses.
    ///
    /// Generated SQL: `(data->'path[0]'->...->>'path[n]')`
    JsonbPath(Vec<String>),
}

impl Field {
    /// Validate field name to prevent SQL injection
    ///
    /// Allows: alphanumeric, underscore
    /// Disallows: quotes, brackets, dashes, special characters
    pub fn validate(&self) -> Result<(), String> {
        let name = match self {
            Field::JsonbField(n) => n,
            Field::DirectColumn(n) => n,
            Field::JsonbPath(path) => {
                for segment in path {
                    if !is_valid_field_name(segment) {
                        return Err(format!("Invalid field name in path: {}", segment));
                    }
                }
                return Ok(());
            }
        };

        if !is_valid_field_name(name) {
            return Err(format!("Invalid field name: {}", name));
        }

        Ok(())
    }

    /// Generate SQL for this field
    pub fn to_sql(&self) -> String {
        match self {
            Field::JsonbField(name) => format!("(data->'{}')", name),
            Field::DirectColumn(name) => name.clone(),
            Field::JsonbPath(path) => {
                if path.is_empty() {
                    return "data".to_string();
                }

                let mut sql = String::from("(data");
                for (i, segment) in path.iter().enumerate() {
                    if i == path.len() - 1 {
                        // Last segment: use ->> for text extraction
                        sql.push_str(&format!("->>'{}\'", segment));
                    } else {
                        // Intermediate segments: use -> for JSON objects
                        sql.push_str(&format!("->'{}\'", segment));
                    }
                }
                sql.push(')');
                sql
            }
        }
    }
}

impl fmt::Display for Field {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Field::JsonbField(name) => write!(f, "data->'{}'", name),
            Field::DirectColumn(name) => write!(f, "{}", name),
            Field::JsonbPath(path) => {
                write!(f, "data")?;
                for (i, segment) in path.iter().enumerate() {
                    if i == path.len() - 1 {
                        write!(f, "->>{}", segment)?;
                    } else {
                        write!(f, "->{}", segment)?;
                    }
                }
                Ok(())
            }
        }
    }
}

/// Represents a value to bind in a WHERE clause
///
/// # Examples
///
/// ```ignore
/// Value::String("John".to_string())
/// Value::Number(42.0)
/// Value::Bool(true)
/// Value::Null
/// Value::Array(vec![Value::String("a".to_string()), Value::String("b".to_string())])
/// ```
#[derive(Debug, Clone)]
pub enum Value {
    /// String value
    String(String),

    /// Numeric value (f64 can represent i64, u64, f32 with precision)
    Number(f64),

    /// Boolean value
    Bool(bool),

    /// NULL
    Null,

    /// Array of values (for IN operators)
    Array(Vec<Value>),

    /// Vector of floats (for pgvector distance operators)
    FloatArray(Vec<f32>),

    /// Raw SQL expression (use with caution!)
    ///
    /// This should only be used for trusted SQL fragments,
    /// never for user input.
    RawSql(String),
}

impl Value {
    /// Check if value is NULL
    pub fn is_null(&self) -> bool {
        matches!(self, Value::Null)
    }

    /// Convert value to SQL literal
    ///
    /// For parameterized queries, prefer using parameter placeholders ($1, $2, etc.)
    /// This is primarily for documentation and debugging.
    pub fn to_sql_literal(&self) -> String {
        match self {
            Value::String(s) => format!("'{}'", s.replace('\'', "''")),
            Value::Number(n) => n.to_string(),
            Value::Bool(b) => b.to_string(),
            Value::Null => "NULL".to_string(),
            Value::Array(arr) => {
                let items: Vec<String> = arr.iter().map(|v| v.to_sql_literal()).collect();
                format!("ARRAY[{}]", items.join(", "))
            }
            Value::FloatArray(arr) => {
                let items: Vec<String> = arr.iter().map(|f| f.to_string()).collect();
                format!("[{}]", items.join(", "))
            }
            Value::RawSql(sql) => sql.clone(),
        }
    }
}

impl fmt::Display for Value {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.to_sql_literal())
    }
}

/// Check if a field name is valid (alphanumeric + underscore)
fn is_valid_field_name(name: &str) -> bool {
    if name.is_empty() {
        return false;
    }

    // First character must be alphabetic or underscore
    let first = name.chars().next().unwrap();
    if !first.is_alphabetic() && first != '_' {
        return false;
    }

    // Remaining characters must be alphanumeric or underscore
    name.chars().all(|c| c.is_alphanumeric() || c == '_')
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_valid_field_names() {
        assert!(is_valid_field_name("name"));
        assert!(is_valid_field_name("_private"));
        assert!(is_valid_field_name("field_123"));
        assert!(is_valid_field_name("a"));
    }

    #[test]
    fn test_invalid_field_names() {
        assert!(!is_valid_field_name(""));
        assert!(!is_valid_field_name("123field")); // starts with digit
        assert!(!is_valid_field_name("field-name")); // contains dash
        assert!(!is_valid_field_name("field.name")); // contains dot
        assert!(!is_valid_field_name("field'name")); // contains quote
    }

    #[test]
    fn test_field_validation() {
        assert!(Field::JsonbField("name".to_string()).validate().is_ok());
        assert!(Field::JsonbField("name-invalid".to_string()).validate().is_err());
        assert!(Field::JsonbPath(vec!["user".to_string(), "name".to_string()])
            .validate()
            .is_ok());
    }

    #[test]
    fn test_field_to_sql_jsonb() {
        let field = Field::JsonbField("name".to_string());
        assert_eq!(field.to_sql(), "(data->'name')");
    }

    #[test]
    fn test_field_to_sql_direct() {
        let field = Field::DirectColumn("created_at".to_string());
        assert_eq!(field.to_sql(), "created_at");
    }

    #[test]
    fn test_field_to_sql_path() {
        let field = Field::JsonbPath(vec!["user".to_string(), "name".to_string()]);
        assert_eq!(field.to_sql(), "(data->'user'->>'name')");
    }

    #[test]
    fn test_value_to_sql_literal() {
        assert_eq!(Value::String("test".to_string()).to_sql_literal(), "'test'");
        assert_eq!(Value::Number(42.0).to_sql_literal(), "42");
        assert_eq!(Value::Bool(true).to_sql_literal(), "true");
        assert_eq!(Value::Null.to_sql_literal(), "NULL");
    }

    #[test]
    fn test_value_string_escaping() {
        let val = Value::String("O'Brien".to_string());
        assert_eq!(val.to_sql_literal(), "'O''Brien'");
    }
}
