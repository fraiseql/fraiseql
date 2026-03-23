//! SQL Server SQL dialect implementation.

use std::borrow::Cow;

use super::trait_def::{RowViewColumnType, SqlDialect, UnsupportedOperator};

/// SQL Server dialect for [`GenericWhereGenerator`].
///
/// [`GenericWhereGenerator`]: crate::where_generator::GenericWhereGenerator
pub struct SqlServerDialect;

impl SqlDialect for SqlServerDialect {
    fn name(&self) -> &'static str {
        "SQL Server"
    }

    fn quote_identifier(&self, name: &str) -> String {
        format!("[{}]", name.replace(']', "]]"))
    }

    fn json_extract_scalar(&self, column: &str, path: &[String]) -> String {
        let json_path = crate::path_escape::escape_sqlserver_json_path(path);
        format!("JSON_VALUE({column}, '{json_path}')")
    }

    fn placeholder(&self, n: usize) -> String {
        format!("@p{n}")
    }

    fn cast_to_numeric<'a>(&self, expr: &'a str) -> Cow<'a, str> {
        Cow::Owned(format!("CAST({expr} AS FLOAT)"))
    }

    fn like_sql(&self, lhs: &str, rhs: &str) -> String {
        format!("{lhs} LIKE {rhs} COLLATE Latin1_General_CS_AS")
    }

    fn ilike_sql(&self, lhs: &str, rhs: &str) -> String {
        format!("{lhs} LIKE {rhs} COLLATE Latin1_General_CI_AI")
    }

    fn concat_sql(&self, parts: &[&str]) -> String {
        parts.join(" + ")
    }

    fn always_false(&self) -> &'static str {
        "1=0"
    }

    fn always_true(&self) -> &'static str {
        "1=1"
    }

    fn neq_operator(&self) -> &'static str {
        "<>"
    }

    fn json_array_length(&self, expr: &str) -> String {
        format!("(SELECT COUNT(*) FROM OPENJSON({expr}))")
    }

    fn array_contains_sql(&self, lhs: &str, rhs: &str) -> Result<String, UnsupportedOperator> {
        Ok(format!("EXISTS (SELECT 1 FROM OPENJSON({lhs}) WHERE value = {rhs})"))
    }

    fn fts_matches_sql(&self, expr: &str, param: &str) -> Result<String, UnsupportedOperator> {
        Ok(format!("CONTAINS({expr}, {param})"))
    }

    fn fts_plain_query_sql(&self, expr: &str, param: &str) -> Result<String, UnsupportedOperator> {
        Ok(format!("CONTAINS({expr}, {param})"))
    }

    fn fts_phrase_query_sql(&self, expr: &str, param: &str) -> Result<String, UnsupportedOperator> {
        Ok(format!("FREETEXT({expr}, {param})"))
    }

    fn row_view_column_expr(
        &self,
        json_column: &str,
        field_name: &str,
        target_type: &RowViewColumnType,
    ) -> String {
        let ss_type = match target_type {
            RowViewColumnType::Text => "NVARCHAR(MAX)",
            RowViewColumnType::Int32 => "INT",
            RowViewColumnType::Int64 => "BIGINT",
            RowViewColumnType::Float64 => "FLOAT",
            RowViewColumnType::Boolean => "BIT",
            RowViewColumnType::Uuid => "UNIQUEIDENTIFIER",
            RowViewColumnType::Timestamptz => "DATETIMEOFFSET",
            RowViewColumnType::Date => "DATE",
            RowViewColumnType::Json => {
                // SQL Server has no native JSONB — return raw JSON string
                return format!("JSON_QUERY({json_column}, '$.{field_name}')");
            },
        };
        format!("CAST(JSON_VALUE({json_column}, '$.{field_name}') AS {ss_type})")
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
        format!(
            "CREATE OR ALTER VIEW {quoted_view} AS\nSELECT\n{}\nFROM {quoted_table};",
            col_list.join(",\n")
        )
    }
}
