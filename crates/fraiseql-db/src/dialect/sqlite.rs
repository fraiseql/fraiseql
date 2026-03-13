//! SQLite SQL dialect implementation.

use std::borrow::Cow;

use super::trait_def::{SqlDialect, UnsupportedOperator};

/// SQLite dialect for [`GenericWhereGenerator`].
///
/// [`GenericWhereGenerator`]: crate::where_generator::GenericWhereGenerator
pub struct SqliteDialect;

impl SqlDialect for SqliteDialect {
    fn name(&self) -> &'static str {
        "SQLite"
    }

    fn quote_identifier(&self, name: &str) -> String {
        format!("\"{}\"", name.replace('"', "\"\""))
    }

    fn json_extract_scalar(&self, column: &str, path: &[String]) -> String {
        let json_path = crate::path_escape::escape_sqlite_json_path(path);
        format!("json_extract({column}, '{json_path}')")
    }

    fn placeholder(&self, _n: usize) -> String {
        "?".to_string()
    }

    fn cast_to_numeric<'a>(&self, expr: &'a str) -> Cow<'a, str> {
        Cow::Owned(format!("CAST({expr} AS REAL)"))
    }

    fn always_false(&self) -> &'static str {
        "1=0"
    }

    fn always_true(&self) -> &'static str {
        "1=1"
    }

    fn json_array_length(&self, expr: &str) -> String {
        format!("json_array_length({expr})")
    }

    fn array_contains_sql(&self, lhs: &str, rhs: &str) -> Result<String, UnsupportedOperator> {
        // SQLite has no native @>; use EXISTS + json_each()
        Ok(format!(
            "EXISTS (SELECT 1 FROM json_each({lhs}) WHERE value = json({rhs}))"
        ))
    }
}
