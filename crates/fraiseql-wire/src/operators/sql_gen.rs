//! SQL generation from operators
//!
//! Converts operator enums to PostgreSQL WHERE clause SQL strings.
//! Handles parameter binding, type casting, and operator-specific SQL generation
//! for both JSONB and direct column sources.
//!
//! # Type Casting Strategy
//!
//! JSONB fields extracted with `->>` are always text. When comparing with non-string values,
//! we apply explicit type casting:
//!
//! - String comparisons: No cast needed (text = text)
//! - Numeric comparisons: Cast to integer or float (`text::integer` > $1)
//! - Boolean comparisons: Cast to boolean (`text::boolean` = true)
//! - Array comparisons: No special handling (uses array operators)
//!
//! Direct columns use native types from the database schema.

use super::{Field, Value, WhereOperator};
use crate::Result;
use std::collections::HashMap;

/// Escapes LIKE metacharacters in a literal string.
///
/// Escapes `\`, `%`, and `_` so that user-supplied substrings, prefixes, and
/// suffixes are always treated as literals inside a `LIKE` or `ILIKE` pattern.
/// PostgreSQL uses `\` as the default LIKE escape character.
fn escape_like_literal(s: &str) -> String {
    // Order matters: escape `\` first to avoid double-escaping.
    s.replace('\\', "\\\\")
        .replace('%', "\\%")
        .replace('_', "\\_")
}

/// Infers the PostgreSQL type cast needed for a value
///
/// Returns the type cast suffix (e.g., "`::integer`", "`::text`") if needed
const fn infer_type_cast(value: &Value) -> &'static str {
    match value {
        Value::String(_) => "::text",
        Value::Number(_) => "::numeric", // numeric handles both int and float
        Value::Bool(_) => "::boolean",
        Value::Null => "",          // no cast for NULL
        Value::Array(_) => "",      // arrays handled by operators
        Value::FloatArray(_) => "", // vector operators handle their own casting
        Value::RawSql(_) => "",     // raw SQL is assumed correct
    }
}

/// Generates SQL from a WHERE operator with parameter binding support
///
/// # Parameters
///
/// - `operator`: The WHERE operator to generate SQL for
/// - `param_index`: Mutable reference to parameter counter (for $1, $2, etc.)
/// - `params`: Mutable map to accumulate parameter values (for later binding)
///
/// # Returns
///
/// SQL string with parameter placeholders ($1, $2, etc.)
///
/// # Examples
///
/// ```no_run
/// // Requires: fraiseql_wire::operators re-exports; Value has no PartialEq so assert_eq on params omitted.
/// use std::collections::HashMap;
/// use fraiseql_wire::operators::{Field, Value, WhereOperator, generate_where_operator_sql};
/// let mut param_index = 0;
/// let mut params = HashMap::new();
/// let op = WhereOperator::Eq(Field::JsonbField("name".to_string()), Value::String("John".to_string()));
/// let sql = generate_where_operator_sql(&op, &mut param_index, &mut params).unwrap();
/// assert_eq!(sql, "(data->'name')::text = $1");
/// ```
pub fn generate_where_operator_sql(
    operator: &WhereOperator,
    param_index: &mut usize,
    params: &mut HashMap<usize, Value>,
) -> Result<String> {
    operator.validate().map_err(crate::Error::InvalidSchema)?;

    match operator {
        // ============ Comparison Operators ============
        // These operators work on both JSONB and direct columns.
        // For JSONB text extraction, we apply type casting for proper comparison.
        WhereOperator::Eq(field, value) => {
            let field_sql = field.to_sql();
            if value.is_null() {
                Ok(format!("{} IS NULL", field_sql))
            } else {
                let param_num = *param_index + 1;
                *param_index += 1;
                params.insert(param_num, value.clone());
                // JSONB fields need type cast for non-string comparisons
                let cast = match field {
                    Field::JsonbField(_) | Field::JsonbPath(_) => infer_type_cast(value),
                    Field::DirectColumn(_) => "", // direct columns use native types
                };
                Ok(format!("{}{} = ${}", field_sql, cast, param_num))
            }
        }

        WhereOperator::Neq(field, value) => {
            let field_sql = field.to_sql();
            if value.is_null() {
                Ok(format!("{} IS NOT NULL", field_sql))
            } else {
                let param_num = *param_index + 1;
                *param_index += 1;
                params.insert(param_num, value.clone());
                let cast = match field {
                    Field::JsonbField(_) | Field::JsonbPath(_) => infer_type_cast(value),
                    Field::DirectColumn(_) => "",
                };
                Ok(format!("{}{} != ${}", field_sql, cast, param_num))
            }
        }

        WhereOperator::Gt(field, value) => {
            let field_sql = field.to_sql();
            let param_num = *param_index + 1;
            *param_index += 1;
            params.insert(param_num, value.clone());
            let cast = match field {
                Field::JsonbField(_) | Field::JsonbPath(_) => infer_type_cast(value),
                Field::DirectColumn(_) => "",
            };
            Ok(format!("{}{} > ${}", field_sql, cast, param_num))
        }

        WhereOperator::Gte(field, value) => {
            let field_sql = field.to_sql();
            let param_num = *param_index + 1;
            *param_index += 1;
            params.insert(param_num, value.clone());
            let cast = match field {
                Field::JsonbField(_) | Field::JsonbPath(_) => infer_type_cast(value),
                Field::DirectColumn(_) => "",
            };
            Ok(format!("{}{} >= ${}", field_sql, cast, param_num))
        }

        WhereOperator::Lt(field, value) => {
            let field_sql = field.to_sql();
            let param_num = *param_index + 1;
            *param_index += 1;
            params.insert(param_num, value.clone());
            let cast = match field {
                Field::JsonbField(_) | Field::JsonbPath(_) => infer_type_cast(value),
                Field::DirectColumn(_) => "",
            };
            Ok(format!("{}{} < ${}", field_sql, cast, param_num))
        }

        WhereOperator::Lte(field, value) => {
            let field_sql = field.to_sql();
            let param_num = *param_index + 1;
            *param_index += 1;
            params.insert(param_num, value.clone());
            let cast = match field {
                Field::JsonbField(_) | Field::JsonbPath(_) => infer_type_cast(value),
                Field::DirectColumn(_) => "",
            };
            Ok(format!("{}{} <= ${}", field_sql, cast, param_num))
        }

        // ============ Array Operators ============
        WhereOperator::In(field, values) => {
            // Empty IN () is a syntax error in all databases; semantically equivalent to FALSE.
            if values.is_empty() {
                return Ok("FALSE".to_string());
            }
            let field_sql = field.to_sql();
            let placeholders: Vec<String> = values
                .iter()
                .map(|v| {
                    let param_num = *param_index + 1;
                    *param_index += 1;
                    params.insert(param_num, v.clone());
                    format!("${}", param_num)
                })
                .collect();
            Ok(format!("{} IN ({})", field_sql, placeholders.join(", ")))
        }

        WhereOperator::Nin(field, values) => {
            // Empty NOT IN () is a syntax error in all databases; semantically equivalent to TRUE.
            if values.is_empty() {
                return Ok("TRUE".to_string());
            }
            let field_sql = field.to_sql();
            let placeholders: Vec<String> = values
                .iter()
                .map(|v| {
                    let param_num = *param_index + 1;
                    *param_index += 1;
                    params.insert(param_num, v.clone());
                    format!("${}", param_num)
                })
                .collect();
            Ok(format!(
                "{} NOT IN ({})",
                field_sql,
                placeholders.join(", ")
            ))
        }

        WhereOperator::Contains(field, substring) => {
            let field_sql = field.to_sql();
            let param_num = *param_index + 1;
            *param_index += 1;
            params.insert(param_num, Value::String(escape_like_literal(substring)));
            Ok(format!(
                "{} LIKE '%' || ${}::text || '%'",
                field_sql, param_num
            ))
        }

        WhereOperator::ArrayContains(field, value) => {
            let field_sql = field.to_sql();
            let param_num = *param_index + 1;
            *param_index += 1;
            params.insert(param_num, value.clone());
            Ok(format!("{} @> ARRAY[${}]", field_sql, param_num))
        }

        WhereOperator::ArrayContainedBy(field, value) => {
            let field_sql = field.to_sql();
            let param_num = *param_index + 1;
            *param_index += 1;
            params.insert(param_num, value.clone());
            Ok(format!("{} <@ ARRAY[${}]", field_sql, param_num))
        }

        WhereOperator::ArrayOverlaps(field, values) => {
            let field_sql = field.to_sql();
            let placeholders: Vec<String> = values
                .iter()
                .map(|v| {
                    let param_num = *param_index + 1;
                    *param_index += 1;
                    params.insert(param_num, v.clone());
                    format!("${}", param_num)
                })
                .collect();
            Ok(format!(
                "{} && ARRAY[{}]",
                field_sql,
                placeholders.join(", ")
            ))
        }

        // ============ Array Length Operators ============
        WhereOperator::LenEq(field, len) => {
            let field_sql = field.to_sql();
            Ok(format!("array_length({}, 1) = {}", field_sql, len))
        }

        WhereOperator::LenGt(field, len) => {
            let field_sql = field.to_sql();
            Ok(format!("array_length({}, 1) > {}", field_sql, len))
        }

        WhereOperator::LenGte(field, len) => {
            let field_sql = field.to_sql();
            Ok(format!("array_length({}, 1) >= {}", field_sql, len))
        }

        WhereOperator::LenLt(field, len) => {
            let field_sql = field.to_sql();
            Ok(format!("array_length({}, 1) < {}", field_sql, len))
        }

        WhereOperator::LenLte(field, len) => {
            let field_sql = field.to_sql();
            Ok(format!("array_length({}, 1) <= {}", field_sql, len))
        }

        // ============ String Operators ============
        WhereOperator::Icontains(field, substring) => {
            let field_sql = field.to_sql();
            let param_num = *param_index + 1;
            *param_index += 1;
            params.insert(param_num, Value::String(escape_like_literal(substring)));
            Ok(format!(
                "{} ILIKE '%' || ${}::text || '%'",
                field_sql, param_num
            ))
        }

        WhereOperator::Startswith(field, prefix) => {
            let field_sql = field.to_sql();
            let param_num = *param_index + 1;
            *param_index += 1;
            params.insert(
                param_num,
                Value::String(format!("{}%", escape_like_literal(prefix))),
            );
            Ok(format!("{} LIKE ${}", field_sql, param_num))
        }

        WhereOperator::Istartswith(field, prefix) => {
            let field_sql = field.to_sql();
            let param_num = *param_index + 1;
            *param_index += 1;
            params.insert(
                param_num,
                Value::String(format!("{}%", escape_like_literal(prefix))),
            );
            Ok(format!("{} ILIKE ${}", field_sql, param_num))
        }

        WhereOperator::Endswith(field, suffix) => {
            let field_sql = field.to_sql();
            let param_num = *param_index + 1;
            *param_index += 1;
            params.insert(
                param_num,
                Value::String(format!("%{}", escape_like_literal(suffix))),
            );
            Ok(format!("{} LIKE ${}", field_sql, param_num))
        }

        WhereOperator::Iendswith(field, suffix) => {
            let field_sql = field.to_sql();
            let param_num = *param_index + 1;
            *param_index += 1;
            params.insert(
                param_num,
                Value::String(format!("%{}", escape_like_literal(suffix))),
            );
            Ok(format!("{} ILIKE ${}", field_sql, param_num))
        }

        WhereOperator::Like(field, pattern) => {
            let field_sql = field.to_sql();
            let param_num = *param_index + 1;
            *param_index += 1;
            params.insert(param_num, Value::String(pattern.clone()));
            Ok(format!("{} LIKE ${}", field_sql, param_num))
        }

        WhereOperator::Ilike(field, pattern) => {
            let field_sql = field.to_sql();
            let param_num = *param_index + 1;
            *param_index += 1;
            params.insert(param_num, Value::String(pattern.clone()));
            Ok(format!("{} ILIKE ${}", field_sql, param_num))
        }

        // ============ Null Operator ============
        WhereOperator::IsNull(field, is_null) => {
            let field_sql = field.to_sql();
            if *is_null {
                Ok(format!("{} IS NULL", field_sql))
            } else {
                Ok(format!("{} IS NOT NULL", field_sql))
            }
        }

        // ============ Vector Distance Operators (pgvector) ============
        WhereOperator::L2Distance {
            field,
            vector,
            threshold,
        } => {
            let field_sql = field.to_sql();
            let param_num = *param_index + 1;
            *param_index += 1;
            params.insert(param_num, Value::FloatArray(vector.clone()));
            Ok(format!(
                "l2_distance({}::vector, ${}::vector) < {}",
                field_sql, param_num, threshold
            ))
        }

        WhereOperator::CosineDistance {
            field,
            vector,
            threshold,
        } => {
            let field_sql = field.to_sql();
            let param_num = *param_index + 1;
            *param_index += 1;
            params.insert(param_num, Value::FloatArray(vector.clone()));
            Ok(format!(
                "cosine_distance({}::vector, ${}::vector) < {}",
                field_sql, param_num, threshold
            ))
        }

        WhereOperator::InnerProduct {
            field,
            vector,
            threshold,
        } => {
            let field_sql = field.to_sql();
            let param_num = *param_index + 1;
            *param_index += 1;
            params.insert(param_num, Value::FloatArray(vector.clone()));
            Ok(format!(
                "inner_product({}::vector, ${}::vector) > {}",
                field_sql, param_num, threshold
            ))
        }

        WhereOperator::L1Distance {
            field,
            vector,
            threshold,
        } => {
            let field_sql = field.to_sql();
            let param_num = *param_index + 1;
            *param_index += 1;
            params.insert(param_num, Value::FloatArray(vector.clone()));
            Ok(format!(
                "l1_distance({}::vector, ${}::vector) < {}",
                field_sql, param_num, threshold
            ))
        }

        WhereOperator::HammingDistance {
            field,
            vector,
            threshold,
        } => {
            let field_sql = field.to_sql();
            let param_num = *param_index + 1;
            *param_index += 1;
            params.insert(param_num, Value::FloatArray(vector.clone()));
            Ok(format!(
                "hamming_distance({}::bit, ${}::bit) < {}",
                field_sql, param_num, threshold
            ))
        }

        WhereOperator::JaccardDistance {
            field,
            set,
            threshold,
        } => {
            let field_sql = field.to_sql();
            let param_num = *param_index + 1;
            *param_index += 1;
            let value_array: Vec<Value> = set.iter().map(|s| Value::String(s.clone())).collect();
            params.insert(param_num, Value::Array(value_array));
            Ok(format!(
                "jaccard_distance({}::text[], ${}::text[]) < {}",
                field_sql, param_num, threshold
            ))
        }

        // ============ Full-Text Search Operators ============
        WhereOperator::Matches {
            field,
            query,
            language,
        } => {
            let field_sql = field.to_sql();
            let param_num = *param_index + 1;
            *param_index += 1;
            params.insert(param_num, Value::String(query.clone()));
            let lang = language.as_deref().unwrap_or("english");
            Ok(format!(
                "{} @@ plainto_tsquery('{}', ${})",
                field_sql, lang, param_num
            ))
        }

        WhereOperator::PlainQuery { field, query } => {
            let field_sql = field.to_sql();
            let param_num = *param_index + 1;
            *param_index += 1;
            params.insert(param_num, Value::String(query.clone()));
            Ok(format!(
                "{} @@ plainto_tsquery(${})::tsvector",
                field_sql, param_num
            ))
        }

        WhereOperator::PhraseQuery {
            field,
            query,
            language,
        } => {
            let field_sql = field.to_sql();
            let param_num = *param_index + 1;
            *param_index += 1;
            params.insert(param_num, Value::String(query.clone()));
            let lang = language.as_deref().unwrap_or("english");
            Ok(format!(
                "{} @@ phraseto_tsquery('{}', ${})",
                field_sql, lang, param_num
            ))
        }

        WhereOperator::WebsearchQuery {
            field,
            query,
            language,
        } => {
            let field_sql = field.to_sql();
            let param_num = *param_index + 1;
            *param_index += 1;
            params.insert(param_num, Value::String(query.clone()));
            let lang = language.as_deref().unwrap_or("english");
            Ok(format!(
                "{} @@ websearch_to_tsquery('{}', ${})",
                field_sql, lang, param_num
            ))
        }

        // ============ Network/INET Operators ============
        WhereOperator::IsIPv4(field) => {
            let field_sql = field.to_sql();
            Ok(format!("family({}::inet) = 4", field_sql))
        }

        WhereOperator::IsIPv6(field) => {
            let field_sql = field.to_sql();
            Ok(format!("family({}::inet) = 6", field_sql))
        }

        WhereOperator::IsPrivate(field) => {
            let field_sql = field.to_sql();
            // RFC1918 private ranges + link-local
            Ok(format!(
                "({}::inet << '10.0.0.0/8'::inet OR {}::inet << '172.16.0.0/12'::inet OR {}::inet << '192.168.0.0/16'::inet OR {}::inet << '169.254.0.0/16'::inet)",
                field_sql, field_sql, field_sql, field_sql
            ))
        }

        WhereOperator::IsPublic(field) => {
            let field_sql = field.to_sql();
            // NOT private (opposite of IsPrivate)
            Ok(format!(
                "NOT ({}::inet << '10.0.0.0/8'::inet OR {}::inet << '172.16.0.0/12'::inet OR {}::inet << '192.168.0.0/16'::inet OR {}::inet << '169.254.0.0/16'::inet)",
                field_sql, field_sql, field_sql, field_sql
            ))
        }

        WhereOperator::IsLoopback(field) => {
            let field_sql = field.to_sql();
            Ok(format!(
                "(family({}::inet) = 4 AND {}::inet << '127.0.0.0/8'::inet) OR (family({}::inet) = 6 AND {}::inet << '::1/128'::inet)",
                field_sql, field_sql, field_sql, field_sql
            ))
        }

        WhereOperator::InSubnet { field, subnet } => {
            let field_sql = field.to_sql();
            let param_num = *param_index + 1;
            *param_index += 1;
            params.insert(param_num, Value::String(subnet.clone()));
            Ok(format!("{}::inet << ${}::inet", field_sql, param_num))
        }

        WhereOperator::ContainsSubnet { field, subnet } => {
            let field_sql = field.to_sql();
            let param_num = *param_index + 1;
            *param_index += 1;
            params.insert(param_num, Value::String(subnet.clone()));
            Ok(format!("{}::inet >> ${}::inet", field_sql, param_num))
        }

        WhereOperator::ContainsIP { field, ip } => {
            let field_sql = field.to_sql();
            let param_num = *param_index + 1;
            *param_index += 1;
            params.insert(param_num, Value::String(ip.clone()));
            Ok(format!("{}::inet >> ${}::inet", field_sql, param_num))
        }

        WhereOperator::IPRangeOverlap { field, range } => {
            let field_sql = field.to_sql();
            let param_num = *param_index + 1;
            *param_index += 1;
            params.insert(param_num, Value::String(range.clone()));
            Ok(format!("{}::inet && ${}::inet", field_sql, param_num))
        }

        // ============ JSONB Operators ============
        WhereOperator::StrictlyContains(field, value) => {
            let field_sql = field.to_sql();
            let param_num = *param_index + 1;
            *param_index += 1;
            params.insert(param_num, value.clone());
            Ok(format!("{}::jsonb @> ${}::jsonb", field_sql, param_num))
        }

        // ============ LTree Operators ============
        WhereOperator::AncestorOf { field, path } => {
            let field_sql = field.to_sql();
            let param_num = *param_index + 1;
            *param_index += 1;
            params.insert(param_num, Value::String(path.clone()));
            Ok(format!("{}::ltree @> ${}::ltree", field_sql, param_num))
        }

        WhereOperator::DescendantOf { field, path } => {
            let field_sql = field.to_sql();
            let param_num = *param_index + 1;
            *param_index += 1;
            params.insert(param_num, Value::String(path.clone()));
            Ok(format!("{}::ltree <@ ${}::ltree", field_sql, param_num))
        }

        WhereOperator::MatchesLquery { field, pattern } => {
            let field_sql = field.to_sql();
            let param_num = *param_index + 1;
            *param_index += 1;
            params.insert(param_num, Value::String(pattern.clone()));
            Ok(format!("{}::ltree ~ ${}::lquery", field_sql, param_num))
        }

        WhereOperator::MatchesLtxtquery { field, query } => {
            let field_sql = field.to_sql();
            let param_num = *param_index + 1;
            *param_index += 1;
            params.insert(param_num, Value::String(query.clone()));
            Ok(format!("{}::ltree @ ${}::ltxtquery", field_sql, param_num))
        }

        WhereOperator::MatchesAnyLquery { field, patterns } => {
            let field_sql = field.to_sql();
            let placeholders: Vec<String> = patterns
                .iter()
                .map(|p| {
                    let param_num = *param_index + 1;
                    *param_index += 1;
                    params.insert(param_num, Value::String(p.clone()));
                    format!("${}::lquery", param_num)
                })
                .collect();
            Ok(format!(
                "{}::ltree ? ARRAY[{}]",
                field_sql,
                placeholders.join(", ")
            ))
        }

        // ============ LTree Depth Operators ============
        WhereOperator::DepthEq { field, depth } => {
            let field_sql = field.to_sql();
            Ok(format!("nlevel({}::ltree) = {}", field_sql, depth))
        }

        WhereOperator::DepthNeq { field, depth } => {
            let field_sql = field.to_sql();
            Ok(format!("nlevel({}::ltree) != {}", field_sql, depth))
        }

        WhereOperator::DepthGt { field, depth } => {
            let field_sql = field.to_sql();
            Ok(format!("nlevel({}::ltree) > {}", field_sql, depth))
        }

        WhereOperator::DepthGte { field, depth } => {
            let field_sql = field.to_sql();
            Ok(format!("nlevel({}::ltree) >= {}", field_sql, depth))
        }

        WhereOperator::DepthLt { field, depth } => {
            let field_sql = field.to_sql();
            Ok(format!("nlevel({}::ltree) < {}", field_sql, depth))
        }

        WhereOperator::DepthLte { field, depth } => {
            let field_sql = field.to_sql();
            Ok(format!("nlevel({}::ltree) <= {}", field_sql, depth))
        }

        // ============ LTree LCA Operator ============
        WhereOperator::Lca { field, paths } => {
            let field_sql = field.to_sql();
            let placeholders: Vec<String> = paths
                .iter()
                .map(|p| {
                    let param_num = *param_index + 1;
                    *param_index += 1;
                    params.insert(param_num, Value::String(p.clone()));
                    format!("${}::ltree", param_num)
                })
                .collect();
            Ok(format!(
                "{}::ltree = lca(ARRAY[{}])",
                field_sql,
                placeholders.join(", ")
            ))
        }
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)] // Reason: test code, panics are acceptable
    use super::*;

    #[test]
    fn test_eq_operator_jsonb_string() {
        let mut param_index = 0;
        let mut params = HashMap::new();
        let op = WhereOperator::Eq(
            Field::JsonbField("name".to_string()),
            Value::String("John".to_string()),
        );
        let sql = generate_where_operator_sql(&op, &mut param_index, &mut params).unwrap();
        // JSONB string fields get ::text cast for proper text comparison
        assert_eq!(sql, "(data->'name')::text = $1");
        assert_eq!(param_index, 1);
    }

    #[test]
    fn test_eq_operator_direct_column() {
        let mut param_index = 0;
        let mut params = HashMap::new();
        let op = WhereOperator::Eq(
            Field::DirectColumn("status".to_string()),
            Value::String("active".to_string()),
        );
        let sql = generate_where_operator_sql(&op, &mut param_index, &mut params).unwrap();
        // Direct columns don't need casting (use native types)
        assert_eq!(sql, "status = $1");
        assert_eq!(param_index, 1);
    }

    #[test]
    fn test_len_eq_operator() {
        let mut param_index = 0;
        let mut params = HashMap::new();
        let op = WhereOperator::LenEq(Field::JsonbField("tags".to_string()), 5);
        let sql = generate_where_operator_sql(&op, &mut param_index, &mut params).unwrap();
        assert_eq!(sql, "array_length((data->'tags'), 1) = 5");
        assert_eq!(param_index, 0); // No parameters for length operators
    }

    #[test]
    fn test_is_ipv4_operator() {
        let mut param_index = 0;
        let mut params = HashMap::new();
        let op = WhereOperator::IsIPv4(Field::JsonbField("ip".to_string()));
        let sql = generate_where_operator_sql(&op, &mut param_index, &mut params).unwrap();
        assert_eq!(sql, "family((data->'ip')::inet) = 4");
    }

    #[test]
    fn test_l2_distance_operator() {
        let mut param_index = 0;
        let mut params = HashMap::new();
        let op = WhereOperator::L2Distance {
            field: Field::JsonbField("embedding".to_string()),
            vector: vec![0.1, 0.2, 0.3],
            threshold: 0.5,
        };
        let sql = generate_where_operator_sql(&op, &mut param_index, &mut params).unwrap();
        assert_eq!(
            sql,
            "l2_distance((data->'embedding')::vector, $1::vector) < 0.5"
        );
        assert_eq!(param_index, 1);
    }

    #[test]
    fn test_in_operator() {
        let mut param_index = 0;
        let mut params = HashMap::new();
        let op = WhereOperator::In(
            Field::JsonbField("status".to_string()),
            vec![
                Value::String("active".to_string()),
                Value::String("pending".to_string()),
            ],
        );
        let sql = generate_where_operator_sql(&op, &mut param_index, &mut params).unwrap();
        assert_eq!(sql, "(data->'status') IN ($1, $2)");
        assert_eq!(param_index, 2);
    }

    #[test]
    fn test_in_empty_list_returns_false() {
        let mut param_index = 0;
        let mut params = HashMap::new();
        let op = WhereOperator::In(Field::DirectColumn("status".to_string()), vec![]);
        let sql = generate_where_operator_sql(&op, &mut param_index, &mut params).unwrap();
        assert_eq!(sql, "FALSE");
        assert_eq!(param_index, 0, "no parameters consumed for empty IN");
    }

    #[test]
    fn test_nin_empty_list_returns_true() {
        let mut param_index = 0;
        let mut params = HashMap::new();
        let op = WhereOperator::Nin(Field::DirectColumn("status".to_string()), vec![]);
        let sql = generate_where_operator_sql(&op, &mut param_index, &mut params).unwrap();
        assert_eq!(sql, "TRUE");
        assert_eq!(param_index, 0, "no parameters consumed for empty NOT IN");
    }

    // Helper: extract the inner string from Value::String via Debug, panics otherwise.
    fn value_as_str(v: &Value) -> &str {
        match v {
            Value::String(s) => s.as_str(),
            other => panic!("expected Value::String, got {other:?}"),
        }
    }

    #[test]
    fn test_contains_escapes_percent() {
        let mut param_index = 0;
        let mut params = HashMap::new();
        let op =
            WhereOperator::Contains(Field::DirectColumn("note".to_string()), "50%".to_string());
        generate_where_operator_sql(&op, &mut param_index, &mut params).unwrap();
        assert_eq!(value_as_str(&params[&1]), "50\\%");
    }

    #[test]
    fn test_contains_escapes_underscore() {
        let mut param_index = 0;
        let mut params = HashMap::new();
        let op =
            WhereOperator::Contains(Field::DirectColumn("code".to_string()), "A_B".to_string());
        generate_where_operator_sql(&op, &mut param_index, &mut params).unwrap();
        assert_eq!(value_as_str(&params[&1]), "A\\_B");
    }

    #[test]
    fn test_startswith_escapes_wildcard_in_prefix() {
        let mut param_index = 0;
        let mut params = HashMap::new();
        let op =
            WhereOperator::Startswith(Field::DirectColumn("name".to_string()), "C%D".to_string());
        generate_where_operator_sql(&op, &mut param_index, &mut params).unwrap();
        // prefix escaped, trailing % appended for LIKE
        assert_eq!(value_as_str(&params[&1]), "C\\%D%");
    }

    #[test]
    fn test_endswith_escapes_wildcard_in_suffix() {
        let mut param_index = 0;
        let mut params = HashMap::new();
        let op = WhereOperator::Endswith(
            Field::DirectColumn("name".to_string()),
            "_suffix".to_string(),
        );
        generate_where_operator_sql(&op, &mut param_index, &mut params).unwrap();
        // suffix escaped, leading % prepended for LIKE
        assert_eq!(value_as_str(&params[&1]), "%\\_suffix");
    }

    #[test]
    fn test_escape_like_literal_backslash() {
        assert_eq!(escape_like_literal("a\\b"), "a\\\\b");
        assert_eq!(escape_like_literal("a%b"), "a\\%b");
        assert_eq!(escape_like_literal("a_b"), "a\\_b");
        // Combined: order matters — backslash must be escaped first
        assert_eq!(escape_like_literal("100%_\\n"), "100\\%\\_\\\\n");
    }

    // ============ LTree Operator Tests ============

    #[test]
    fn test_ltree_ancestor_of() {
        let mut param_index = 0;
        let mut params = HashMap::new();
        let op = WhereOperator::AncestorOf {
            field: Field::DirectColumn("path".to_string()),
            path: "Top.Sciences.Astronomy".to_string(),
        };
        let sql = generate_where_operator_sql(&op, &mut param_index, &mut params).unwrap();
        assert_eq!(sql, "path::ltree @> $1::ltree");
        assert_eq!(param_index, 1);
    }

    #[test]
    fn test_ltree_descendant_of() {
        let mut param_index = 0;
        let mut params = HashMap::new();
        let op = WhereOperator::DescendantOf {
            field: Field::DirectColumn("path".to_string()),
            path: "Top.Sciences".to_string(),
        };
        let sql = generate_where_operator_sql(&op, &mut param_index, &mut params).unwrap();
        assert_eq!(sql, "path::ltree <@ $1::ltree");
        assert_eq!(param_index, 1);
    }

    #[test]
    fn test_ltree_matches_lquery() {
        let mut param_index = 0;
        let mut params = HashMap::new();
        let op = WhereOperator::MatchesLquery {
            field: Field::DirectColumn("path".to_string()),
            pattern: "Top.*.Ast*".to_string(),
        };
        let sql = generate_where_operator_sql(&op, &mut param_index, &mut params).unwrap();
        assert_eq!(sql, "path::ltree ~ $1::lquery");
        assert_eq!(param_index, 1);
    }

    #[test]
    fn test_ltree_matches_ltxtquery() {
        let mut param_index = 0;
        let mut params = HashMap::new();
        let op = WhereOperator::MatchesLtxtquery {
            field: Field::DirectColumn("path".to_string()),
            query: "Science & !Deprecated".to_string(),
        };
        let sql = generate_where_operator_sql(&op, &mut param_index, &mut params).unwrap();
        assert_eq!(sql, "path::ltree @ $1::ltxtquery");
        assert_eq!(param_index, 1);
    }

    #[test]
    fn test_ltree_matches_any_lquery() {
        let mut param_index = 0;
        let mut params = HashMap::new();
        let op = WhereOperator::MatchesAnyLquery {
            field: Field::DirectColumn("path".to_string()),
            patterns: vec!["Top.*".to_string(), "Other.*".to_string()],
        };
        let sql = generate_where_operator_sql(&op, &mut param_index, &mut params).unwrap();
        assert_eq!(sql, "path::ltree ? ARRAY[$1::lquery, $2::lquery]");
        assert_eq!(param_index, 2);
    }

    #[test]
    fn test_ltree_depth_eq() {
        let mut param_index = 0;
        let mut params = HashMap::new();
        let op = WhereOperator::DepthEq {
            field: Field::DirectColumn("path".to_string()),
            depth: 3,
        };
        let sql = generate_where_operator_sql(&op, &mut param_index, &mut params).unwrap();
        assert_eq!(sql, "nlevel(path::ltree) = 3");
        assert_eq!(param_index, 0); // Depth is inlined, not parameterized
    }

    #[test]
    fn test_ltree_depth_gt() {
        let mut param_index = 0;
        let mut params = HashMap::new();
        let op = WhereOperator::DepthGt {
            field: Field::DirectColumn("path".to_string()),
            depth: 2,
        };
        let sql = generate_where_operator_sql(&op, &mut param_index, &mut params).unwrap();
        assert_eq!(sql, "nlevel(path::ltree) > 2");
        assert_eq!(param_index, 0);
    }

    #[test]
    fn test_ltree_depth_lte() {
        let mut param_index = 0;
        let mut params = HashMap::new();
        let op = WhereOperator::DepthLte {
            field: Field::DirectColumn("path".to_string()),
            depth: 5,
        };
        let sql = generate_where_operator_sql(&op, &mut param_index, &mut params).unwrap();
        assert_eq!(sql, "nlevel(path::ltree) <= 5");
        assert_eq!(param_index, 0);
    }

    #[test]
    fn test_ltree_lca() {
        let mut param_index = 0;
        let mut params = HashMap::new();
        let op = WhereOperator::Lca {
            field: Field::DirectColumn("path".to_string()),
            paths: vec![
                "Org.Engineering.Backend".to_string(),
                "Org.Engineering.Frontend".to_string(),
            ],
        };
        let sql = generate_where_operator_sql(&op, &mut param_index, &mut params).unwrap();
        assert_eq!(sql, "path::ltree = lca(ARRAY[$1::ltree, $2::ltree])");
        assert_eq!(param_index, 2);
    }
}
