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
pub(crate) fn escape_like_literal(s: &str) -> String {
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

/// Generates a CIDR containment check for network classification operators.
///
/// Produces SQL that tests whether a field (cast to `inet`) is strictly contained
/// within one or more CIDR ranges using the `<<` operator.
///
/// When `negate` is true, the entire expression is wrapped in `NOT (...)`.
///
/// # Examples
///
/// ```text
/// // negate=false, single range:
/// (field::inet << '100.64.0.0/10'::inet)
///
/// // negate=false, multiple ranges:
/// (field::inet << '224.0.0.0/4'::inet OR field::inet << 'ff00::/8'::inet)
///
/// // negate=true:
/// NOT (field::inet << '224.0.0.0/4'::inet OR field::inet << 'ff00::/8'::inet)
/// ```
pub(crate) fn cidr_containment_check(field_sql: &str, ranges: &[&str], negate: bool) -> String {
    let conditions: Vec<String> = ranges
        .iter()
        .map(|r| format!("{field_sql}::inet << '{r}'::inet"))
        .collect();
    let inner = format!("({})", conditions.join(" OR "));
    if negate {
        format!("NOT {inner}")
    } else {
        inner
    }
}

/// CIDR ranges for RFC1918 private addresses plus IPv6 unique-local.
const PRIVATE_RANGES: &[&str] = &[
    "10.0.0.0/8",
    "172.16.0.0/12",
    "192.168.0.0/16",
    "fc00::/7",
];

/// CIDR ranges for loopback addresses.
const LOOPBACK_RANGES: &[&str] = &["127.0.0.0/8", "::1/128"];

/// CIDR ranges for multicast addresses (RFC 3171, RFC 4291).
const MULTICAST_RANGES: &[&str] = &["224.0.0.0/4", "ff00::/8"];

/// CIDR ranges for link-local addresses (RFC 3927, RFC 4291).
const LINK_LOCAL_RANGES: &[&str] = &["169.254.0.0/16", "fe80::/10"];

/// CIDR ranges for documentation addresses (RFC 5737, RFC 3849).
const DOCUMENTATION_RANGES: &[&str] = &[
    "192.0.2.0/24",
    "198.51.100.0/24",
    "203.0.113.0/24",
    "2001:db8::/32",
];

/// CIDR ranges for carrier-grade NAT (RFC 6598, IPv4 only).
const CARRIER_GRADE_RANGES: &[&str] = &["100.64.0.0/10"];

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
/// # Errors
///
/// Returns `WireError::InvalidSchema` if the operator fails validation (e.g., invalid
/// field names or unsupported value types for the given operator).
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
    operator
        .validate()
        .map_err(crate::WireError::InvalidSchema)?;

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

        WhereOperator::IsPrivate { field, value } => {
            let field_sql = field.to_sql();
            Ok(cidr_containment_check(&field_sql, PRIVATE_RANGES, !value))
        }

        WhereOperator::IsLoopback { field, value } => {
            let field_sql = field.to_sql();
            Ok(cidr_containment_check(&field_sql, LOOPBACK_RANGES, !value))
        }

        WhereOperator::IsMulticast { field, value } => {
            let field_sql = field.to_sql();
            Ok(cidr_containment_check(&field_sql, MULTICAST_RANGES, !value))
        }

        WhereOperator::IsLinkLocal { field, value } => {
            let field_sql = field.to_sql();
            Ok(cidr_containment_check(&field_sql, LINK_LOCAL_RANGES, !value))
        }

        WhereOperator::IsDocumentation { field, value } => {
            let field_sql = field.to_sql();
            Ok(cidr_containment_check(
                &field_sql,
                DOCUMENTATION_RANGES,
                !value,
            ))
        }

        WhereOperator::IsCarrierGrade { field, value } => {
            let field_sql = field.to_sql();
            Ok(cidr_containment_check(
                &field_sql,
                CARRIER_GRADE_RANGES,
                !value,
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

        // ============ LTree ID-Based Operators ============
        // SQL generation requires HierarchyContext (table, path_column).
        // These operators are handled by the GenericWhereGenerator in fraiseql-db,
        // not the wire-level SQL generator. Return an error if reached here.
        WhereOperator::DescendantOfId { .. } | WhereOperator::AncestorOfId { .. } => {
            Err(crate::WireError::InvalidSchema(
                "ID-based ltree operators require HierarchyContext; use GenericWhereGenerator"
                    .to_string(),
            ))
        }
    }
}

#[cfg(test)]
mod tests;
