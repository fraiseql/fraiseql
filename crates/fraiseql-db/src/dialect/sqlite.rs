//! SQLite SQL dialect implementation.

use std::borrow::Cow;

use super::trait_def::{RowViewColumnType, SqlDialect, UnsupportedOperator};

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
        Ok(format!("EXISTS (SELECT 1 FROM json_each({lhs}) WHERE value = json({rhs}))"))
    }

    fn row_view_column_expr(
        &self,
        json_column: &str,
        field_name: &str,
        target_type: &RowViewColumnType,
    ) -> String {
        let sqlite_type = match target_type {
            RowViewColumnType::Text
            | RowViewColumnType::Uuid
            | RowViewColumnType::Timestamptz
            | RowViewColumnType::Date => "TEXT",
            RowViewColumnType::Int32 | RowViewColumnType::Int64 | RowViewColumnType::Boolean => {
                "INTEGER"
            },
            RowViewColumnType::Float64 => "REAL",
            RowViewColumnType::Json => {
                return format!("json_extract({json_column}, '$.{field_name}')");
            },
        };
        format!("CAST(json_extract({json_column}, '$.{field_name}') AS {sqlite_type})")
    }

    fn create_row_view_ddl(
        &self,
        view_name: &str,
        source_table: &str,
        columns: &[(String, String)],
    ) -> String {
        let quoted_view = self.quote_identifier(view_name);
        let quoted_table = self.quote_identifier(source_table);
        let col_list: Vec<String> = columns
            .iter()
            .map(|(alias, expr)| format!("  {expr} AS {}", self.quote_identifier(alias)))
            .collect();
        // SQLite has no CREATE OR REPLACE VIEW — use DROP + CREATE
        format!(
            "DROP VIEW IF EXISTS {quoted_view};\nCREATE VIEW {quoted_view} AS\nSELECT\n{}\nFROM {quoted_table};",
            col_list.join(",\n")
        )
    }
}
