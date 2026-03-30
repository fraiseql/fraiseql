//! MySQL SQL dialect implementation.

use std::borrow::Cow;

use super::trait_def::{RowViewColumnType, SqlDialect, UnsupportedOperator};

/// MySQL dialect for [`GenericWhereGenerator`].
///
/// [`GenericWhereGenerator`]: crate::where_generator::GenericWhereGenerator
pub struct MySqlDialect;

impl SqlDialect for MySqlDialect {
    fn name(&self) -> &'static str {
        "MySQL"
    }

    fn quote_identifier(&self, name: &str) -> String {
        format!("`{}`", name.replace('`', "``"))
    }

    fn json_extract_scalar(&self, column: &str, path: &[String]) -> String {
        let json_path = crate::path_escape::escape_mysql_json_path(path);
        format!("JSON_UNQUOTE(JSON_EXTRACT({column}, '{json_path}'))")
    }

    fn placeholder(&self, _n: usize) -> String {
        "?".to_string()
    }

    fn cast_to_numeric<'a>(&self, expr: &'a str) -> Cow<'a, str> {
        Cow::Owned(format!("CAST({expr} AS DECIMAL)"))
    }

    fn ilike_sql(&self, lhs: &str, rhs: &str) -> String {
        // MySQL LIKE is case-insensitive by default with utf8mb4_unicode_ci;
        // use LOWER() to be explicit and portable.
        format!("LOWER({lhs}) LIKE LOWER({rhs})")
    }

    fn concat_sql(&self, parts: &[&str]) -> String {
        format!("CONCAT({})", parts.join(", "))
    }

    fn json_array_length(&self, expr: &str) -> String {
        format!("JSON_LENGTH({expr})")
    }

    fn array_contains_sql(&self, lhs: &str, rhs: &str) -> Result<String, UnsupportedOperator> {
        Ok(format!("JSON_CONTAINS({lhs}, {rhs})"))
    }

    fn array_overlaps_sql(&self, lhs: &str, rhs: &str) -> Result<String, UnsupportedOperator> {
        Ok(format!("JSON_OVERLAPS({lhs}, {rhs})"))
    }

    fn row_view_column_expr(
        &self,
        json_column: &str,
        field_name: &str,
        col_type: &RowViewColumnType,
    ) -> String {
        let mysql_type = match col_type {
            RowViewColumnType::Text | RowViewColumnType::Uuid => "CHAR",
            RowViewColumnType::Int32 => "SIGNED",
            RowViewColumnType::Int64 => "SIGNED",
            RowViewColumnType::Float64 => "DOUBLE",
            RowViewColumnType::Boolean => "UNSIGNED",
            RowViewColumnType::Timestamptz => "DATETIME",
            RowViewColumnType::Json => "JSON",
        };
        format!("CAST(JSON_UNQUOTE(JSON_EXTRACT({json_column}, '$.{field_name}')) AS {mysql_type})")
    }

    // MySQL FTS: all variants map to MATCH/AGAINST
    fn fts_matches_sql(&self, expr: &str, param: &str) -> Result<String, UnsupportedOperator> {
        Ok(format!("MATCH({expr}) AGAINST({param} IN NATURAL LANGUAGE MODE)"))
    }

    fn fts_plain_query_sql(&self, expr: &str, param: &str) -> Result<String, UnsupportedOperator> {
        Ok(format!("MATCH({expr}) AGAINST({param} IN BOOLEAN MODE)"))
    }

    fn fts_phrase_query_sql(&self, expr: &str, param: &str) -> Result<String, UnsupportedOperator> {
        Ok(format!("MATCH({expr}) AGAINST({param} IN NATURAL LANGUAGE MODE)"))
    }

    // WebsearchQuery unsupported → default Err from trait

    fn regex_sql(
        &self,
        lhs: &str,
        rhs: &str,
        _case_insensitive: bool,
        negate: bool,
    ) -> Result<String, UnsupportedOperator> {
        // MySQL REGEXP is case-insensitive by default with utf8mb4; both
        // case-sensitive and case-insensitive variants use the same operator.
        if negate {
            Ok(format!("{lhs} NOT REGEXP {rhs}"))
        } else {
            Ok(format!("{lhs} REGEXP {rhs}"))
        }
    }
}
