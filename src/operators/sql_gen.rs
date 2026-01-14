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
//! - Numeric comparisons: Cast to integer or float (text::integer > $1)
//! - Boolean comparisons: Cast to boolean (text::boolean = true)
//! - Array comparisons: No special handling (uses array operators)
//!
//! Direct columns use native types from the database schema.

use crate::Result;
use super::{Field, Value, WhereOperator};
use std::collections::HashMap;

/// Infers the PostgreSQL type cast needed for a value
///
/// Returns the type cast suffix (e.g., "::integer", "::text") if needed
fn infer_type_cast(value: &Value) -> &'static str {
    match value {
        Value::String(_) => "::text",
        Value::Number(_) => "::numeric", // numeric handles both int and float
        Value::Bool(_) => "::boolean",
        Value::Null => "",                 // no cast for NULL
        Value::Array(_) => "",             // arrays handled by operators
        Value::FloatArray(_) => "",        // vector operators handle their own casting
        Value::RawSql(_) => "",            // raw SQL is assumed correct
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
/// ```ignore
/// let mut param_index = 0;
/// let mut params = HashMap::new();
/// let op = WhereOperator::Eq(Field::JsonbField("name".to_string()), Value::String("John".to_string()));
/// let sql = generate_where_operator_sql(&op, &mut param_index, &mut params)?;
/// assert_eq!(sql, "(data->'name') = $1");
/// assert_eq!(params[&1], Value::String("John".to_string()));
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
            Ok(format!("{} NOT IN ({})", field_sql, placeholders.join(", ")))
        }

        WhereOperator::Contains(field, substring) => {
            let field_sql = field.to_sql();
            let param_num = *param_index + 1;
            *param_index += 1;
            params.insert(param_num, Value::String(substring.clone()));
            Ok(format!("{} LIKE '%' || ${}::text || '%'", field_sql, param_num))
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
            Ok(format!("{} && ARRAY[{}]", field_sql, placeholders.join(", ")))
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
            params.insert(param_num, Value::String(substring.clone()));
            Ok(format!("{} ILIKE '%' || ${}::text || '%'", field_sql, param_num))
        }

        WhereOperator::Startswith(field, prefix) => {
            let field_sql = field.to_sql();
            let param_num = *param_index + 1;
            *param_index += 1;
            params.insert(param_num, Value::String(format!("{}%", prefix)));
            Ok(format!("{} LIKE ${}", field_sql, param_num))
        }

        WhereOperator::Endswith(field, suffix) => {
            let field_sql = field.to_sql();
            let param_num = *param_index + 1;
            *param_index += 1;
            params.insert(param_num, Value::String(format!("%{}", suffix)));
            Ok(format!("{} LIKE ${}", field_sql, param_num))
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
            Ok(format!("{} @@ plainto_tsquery(${})::tsvector", field_sql, param_num))
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
    }
}

#[cfg(test)]
mod tests {
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
}
