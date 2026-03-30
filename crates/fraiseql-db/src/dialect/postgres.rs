//! PostgreSQL SQL dialect implementation.

use std::borrow::Cow;
use std::fmt::Write;

use super::trait_def::{RowViewColumnType, SqlDialect, UnsupportedOperator};

/// PostgreSQL dialect for [`GenericWhereGenerator`].
///
/// [`GenericWhereGenerator`]: crate::where_generator::GenericWhereGenerator
pub struct PostgresDialect;

impl SqlDialect for PostgresDialect {
    fn name(&self) -> &'static str {
        "PostgreSQL"
    }

    fn quote_identifier(&self, name: &str) -> String {
        format!("\"{}\"", name.replace('"', "\"\""))
    }

    fn json_extract_scalar(&self, column: &str, path: &[String]) -> String {
        use crate::path_escape::{escape_postgres_jsonb_path, escape_postgres_jsonb_segment};

        if path.len() == 1 {
            let escaped = escape_postgres_jsonb_segment(&path[0]);
            format!("{column}->>'{escaped}'")
        } else {
            let escaped_path = escape_postgres_jsonb_path(path);
            let mut result = column.to_owned();
            for (i, segment) in escaped_path.iter().enumerate() {
                if i < escaped_path.len() - 1 {
                    write!(result, "->'{segment}'").expect("write to String");
                } else {
                    write!(result, "->>'{segment}'").expect("write to String");
                }
            }
            result
        }
    }

    fn placeholder(&self, n: usize) -> String {
        format!("${n}")
    }

    fn cast_to_numeric<'a>(&self, expr: &'a str) -> Cow<'a, str> {
        Cow::Owned(format!("({expr})::numeric"))
    }

    fn cast_to_boolean<'a>(&self, expr: &'a str) -> Cow<'a, str> {
        Cow::Owned(format!("({expr})::boolean"))
    }

    fn cast_param_numeric<'a>(&self, placeholder: &'a str) -> Cow<'a, str> {
        Cow::Owned(format!("({placeholder}::text)::numeric"))
    }

    fn ilike_sql(&self, lhs: &str, rhs: &str) -> String {
        format!("{lhs} ILIKE {rhs}")
    }

    fn json_array_length(&self, expr: &str) -> String {
        format!("jsonb_array_length({expr}::jsonb)")
    }

    fn array_contains_sql(&self, lhs: &str, rhs: &str) -> Result<String, UnsupportedOperator> {
        Ok(format!("{lhs}::jsonb @> {rhs}::jsonb"))
    }

    fn array_contained_by_sql(&self, lhs: &str, rhs: &str) -> Result<String, UnsupportedOperator> {
        Ok(format!("{lhs}::jsonb <@ {rhs}::jsonb"))
    }

    fn array_overlaps_sql(&self, lhs: &str, rhs: &str) -> Result<String, UnsupportedOperator> {
        Ok(format!("{lhs}::jsonb && {rhs}::jsonb"))
    }

    fn fts_matches_sql(&self, expr: &str, param: &str) -> Result<String, UnsupportedOperator> {
        Ok(format!("to_tsvector({expr}) @@ to_tsquery({param})"))
    }

    fn fts_plain_query_sql(&self, expr: &str, param: &str) -> Result<String, UnsupportedOperator> {
        Ok(format!("to_tsvector({expr}) @@ plainto_tsquery({param})"))
    }

    fn fts_phrase_query_sql(&self, expr: &str, param: &str) -> Result<String, UnsupportedOperator> {
        Ok(format!("to_tsvector({expr}) @@ phraseto_tsquery({param})"))
    }

    fn fts_websearch_query_sql(
        &self,
        expr: &str,
        param: &str,
    ) -> Result<String, UnsupportedOperator> {
        Ok(format!("to_tsvector({expr}) @@ websearch_to_tsquery({param})"))
    }

    fn regex_sql(
        &self,
        lhs: &str,
        rhs: &str,
        case_insensitive: bool,
        negate: bool,
    ) -> Result<String, UnsupportedOperator> {
        let op = match (case_insensitive, negate) {
            (false, false) => "~",
            (true, false) => "~*",
            (false, true) => "!~",
            (true, true) => "!~*",
        };
        Ok(format!("{lhs} {op} {rhs}"))
    }

    // ── PostgreSQL-only operators ──────────────────────────────────────────────

    fn vector_distance_sql(
        &self,
        pg_op: &str,
        lhs: &str,
        rhs: &str,
    ) -> Result<String, UnsupportedOperator> {
        Ok(format!("{lhs}::vector {pg_op} {rhs}::vector"))
    }

    fn jaccard_distance_sql(&self, lhs: &str, rhs: &str) -> Result<String, UnsupportedOperator> {
        Ok(format!("({lhs})::text[] <%> ({rhs})::text[]"))
    }

    fn inet_check_sql(&self, lhs: &str, check_name: &str) -> Result<String, UnsupportedOperator> {
        match check_name {
            "IsIPv4" => Ok(format!("family({lhs}::inet) = 4")),
            "IsIPv6" => Ok(format!("family({lhs}::inet) = 6")),
            "IsPrivate" => Ok(format!(
                "({lhs}::inet << '10.0.0.0/8'::inet OR {lhs}::inet << '172.16.0.0/12'::inet OR {lhs}::inet << '192.168.0.0/16'::inet OR {lhs}::inet << '169.254.0.0/16'::inet)"
            )),
            "IsPublic" => Ok(format!(
                "NOT ({lhs}::inet << '10.0.0.0/8'::inet OR {lhs}::inet << '172.16.0.0/12'::inet OR {lhs}::inet << '192.168.0.0/16'::inet OR {lhs}::inet << '169.254.0.0/16'::inet)"
            )),
            "IsLoopback" => Ok(format!(
                "(family({lhs}::inet) = 4 AND {lhs}::inet << '127.0.0.0/8'::inet) OR (family({lhs}::inet) = 6 AND {lhs}::inet << '::1/128'::inet)"
            )),
            _ => Err(UnsupportedOperator {
                dialect:  self.name(),
                operator: "InetCheck",
            }),
        }
    }

    fn inet_binary_sql(
        &self,
        pg_op: &str,
        lhs: &str,
        rhs: &str,
    ) -> Result<String, UnsupportedOperator> {
        Ok(format!("{lhs}::inet {pg_op} {rhs}::inet"))
    }

    fn ltree_binary_sql(
        &self,
        pg_op: &str,
        lhs: &str,
        rhs: &str,
        rhs_type: &str,
    ) -> Result<String, UnsupportedOperator> {
        Ok(format!("{lhs}::ltree {pg_op} {rhs}::{rhs_type}"))
    }

    fn ltree_any_lquery_sql(
        &self,
        lhs: &str,
        placeholders: &[String],
    ) -> Result<String, UnsupportedOperator> {
        Ok(format!("{lhs}::ltree ? ARRAY[{}]", placeholders.join(", ")))
    }

    fn ltree_depth_sql(
        &self,
        op: &str,
        lhs: &str,
        rhs: &str,
    ) -> Result<String, UnsupportedOperator> {
        Ok(format!("nlevel({lhs}::ltree) {op} {rhs}"))
    }

    fn ltree_lca_sql(
        &self,
        lhs: &str,
        placeholders: &[String],
    ) -> Result<String, UnsupportedOperator> {
        Ok(format!("{lhs}::ltree = lca(ARRAY[{}])", placeholders.join(", ")))
    }

    fn row_view_column_expr(
        &self,
        json_column: &str,
        field_name: &str,
        col_type: &RowViewColumnType,
    ) -> String {
        let pg_type = match col_type {
            RowViewColumnType::Text => "text",
            RowViewColumnType::Int32 => "int",
            RowViewColumnType::Int64 => "bigint",
            RowViewColumnType::Float64 => "double precision",
            RowViewColumnType::Boolean => "boolean",
            RowViewColumnType::Uuid => "uuid",
            RowViewColumnType::Timestamptz => "timestamptz",
            RowViewColumnType::Json => "jsonb",
        };
        format!("({json_column}->>'{field_name}')::{pg_type}")
    }

    fn generate_extended_sql(
        &self,
        operator: &crate::filters::ExtendedOperator,
        field_sql: &str,
        params: &mut Vec<serde_json::Value>,
    ) -> fraiseql_error::Result<String> {
        use fraiseql_error::FraiseQLError;

        use crate::filters::ExtendedOperator;
        match operator {
            ExtendedOperator::EmailDomainEq(domain) => {
                params.push(serde_json::Value::String(domain.clone()));
                let idx = params.len();
                Ok(format!("SPLIT_PART({field_sql}, '@', 2) = ${idx}"))
            },
            ExtendedOperator::EmailDomainIn(domains) => {
                let placeholders: Vec<_> = domains
                    .iter()
                    .map(|d| {
                        params.push(serde_json::Value::String(d.clone()));
                        format!("${}", params.len())
                    })
                    .collect();
                Ok(format!("SPLIT_PART({field_sql}, '@', 2) IN ({})", placeholders.join(", ")))
            },
            ExtendedOperator::EmailDomainEndswith(suffix) => {
                let escaped = crate::where_generator::generic::escape_like_literal(suffix);
                params.push(serde_json::Value::String(escaped));
                let idx = params.len();
                Ok(format!("SPLIT_PART({field_sql}, '@', 2) LIKE '%' || ${idx}"))
            },
            ExtendedOperator::EmailLocalPartStartswith(prefix) => {
                let escaped = crate::where_generator::generic::escape_like_literal(prefix);
                params.push(serde_json::Value::String(escaped));
                let idx = params.len();
                Ok(format!("SPLIT_PART({field_sql}, '@', 1) LIKE ${idx} || '%'"))
            },
            ExtendedOperator::VinWmiEq(wmi) => {
                params.push(serde_json::Value::String(wmi.clone()));
                let idx = params.len();
                Ok(format!("SUBSTRING({field_sql} FROM 1 FOR 3) = ${idx}"))
            },
            ExtendedOperator::IbanCountryEq(country) => {
                params.push(serde_json::Value::String(country.clone()));
                let idx = params.len();
                Ok(format!("SUBSTRING({field_sql} FROM 1 FOR 2) = ${idx}"))
            },
            _ => Err(FraiseQLError::validation(format!(
                "Extended operator not yet implemented for PostgreSQL: {operator}"
            ))),
        }
    }
}
