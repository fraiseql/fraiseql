//! WHERE clause abstract syntax tree.

use fraiseql_error::{FraiseQLError, Result};
use serde::{Deserialize, Serialize};

use crate::utils::to_snake_case;

/// WHERE clause abstract syntax tree.
///
/// Represents a type-safe WHERE condition that can be compiled to database-specific SQL.
///
/// # Example
///
/// ```rust
/// use fraiseql_db::{WhereClause, WhereOperator};
/// use serde_json::json;
///
/// // Simple condition: email ILIKE '%example.com%'
/// let where_clause = WhereClause::Field {
///     path: vec!["email".to_string()],
///     operator: WhereOperator::Icontains,
///     value: json!("example.com"),
/// };
///
/// // Complex condition: (published = true) AND (views >= 100)
/// let where_clause = WhereClause::And(vec![
///     WhereClause::Field {
///         path: vec!["published".to_string()],
///         operator: WhereOperator::Eq,
///         value: json!(true),
///     },
///     WhereClause::Field {
///         path: vec!["views".to_string()],
///         operator: WhereOperator::Gte,
///         value: json!(100),
///     },
/// ]);
/// ```
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[non_exhaustive]
pub enum WhereClause {
    /// Single field condition.
    Field {
        /// JSONB path (e.g., `["email"]` or `["posts", "title"]`).
        path:     Vec<String>,
        /// Comparison operator.
        operator: WhereOperator,
        /// Value to compare against.
        value:    serde_json::Value,
    },

    /// Logical AND of multiple conditions.
    And(Vec<WhereClause>),

    /// Logical OR of multiple conditions.
    Or(Vec<WhereClause>),

    /// Logical NOT of a condition.
    Not(Box<WhereClause>),

    /// Native column condition — bypasses JSONB extraction.
    ///
    /// Used when a direct query argument maps to a native column on `sql_source`,
    /// detected at compile time. Generates `"column" = $N` (with an optional
    /// PostgreSQL type cast on the parameter, e.g. `$1::uuid`) instead of the
    /// default `data->>'column' = $N`.
    NativeField {
        /// Native column name (e.g., `"id"`).
        column:   String,
        /// PostgreSQL parameter cast suffix (e.g., `"uuid"`, `"int4"`).
        /// Empty string means no cast is applied.
        pg_cast:  String,
        /// Comparison operator.
        operator: WhereOperator,
        /// Value to compare against.
        value:    serde_json::Value,
    },
}

impl WhereClause {
    /// Check if WHERE clause is empty.
    #[must_use]
    pub const fn is_empty(&self) -> bool {
        match self {
            Self::And(clauses) | Self::Or(clauses) => clauses.is_empty(),
            Self::Not(_) | Self::Field { .. } | Self::NativeField { .. } => false,
        }
    }

    /// Collect all native column names referenced in this WHERE clause.
    ///
    /// Used to enrich error messages when a native column does not exist on the
    /// target table — the caller can hint that the column was auto-inferred from
    /// an `ID`/`UUID`-typed argument and suggest adding the column or using
    /// explicit `native_columns` annotation.
    #[must_use]
    pub fn native_column_names(&self) -> Vec<&str> {
        let mut names = Vec::new();
        self.collect_native_column_names(&mut names);
        names
    }

    fn collect_native_column_names<'a>(&'a self, out: &mut Vec<&'a str>) {
        match self {
            Self::And(clauses) | Self::Or(clauses) => {
                for c in clauses {
                    c.collect_native_column_names(out);
                }
            },
            Self::Not(inner) => inner.collect_native_column_names(out),
            Self::NativeField { column, .. } => out.push(column),
            Self::Field { .. } => {},
        }
    }

    /// Parse a `WhereClause` from a nested GraphQL JSON `where` variable.
    ///
    /// Expected format (nested object with field → operator → value):
    /// ```json
    /// {
    ///   "status": { "eq": "active" },
    ///   "name": { "icontains": "john" },
    ///   "_and": [ { "age": { "gte": 18 } }, { "age": { "lte": 65 } } ],
    ///   "_or": [ { "role": { "eq": "admin" } } ],
    ///   "_not": { "deleted": { "eq": true } }
    /// }
    /// ```
    ///
    /// Each top-level key is either a field name (mapped to `WhereClause::Field`
    /// with operator sub-keys) or a logical combinator (`_and`, `_or`, `_not`).
    /// Multiple top-level keys are combined with AND.
    ///
    /// # Errors
    ///
    /// Returns `FraiseQLError::Validation` if the JSON structure is invalid or
    /// contains unknown operators.
    ///
    /// # Panics
    ///
    /// Cannot panic: the internal `.expect("checked len == 1")` is only reached
    /// after verifying `conditions.len() == 1`.
    pub fn from_graphql_json(value: &serde_json::Value) -> Result<Self> {
        Self::parse_where_object(value, &[])
    }

    /// Recursive WHERE parser that builds multi-segment paths for nested objects.
    ///
    /// When parsing `{ machine: { id: { eq: "..." } } }`:
    /// 1. Key `machine`, value is `{ id: { eq: "..." } }` — not an operator map.
    /// 2. Recurse with path prefix `["machine"]`.
    /// 3. Key `id`, value is `{ eq: "..." }` — this IS an operator map.
    /// 4. Emit `Field { path: ["machine", "id"], operator: Eq, value: "..." }`.
    ///
    /// The multi-segment path is then handled by `GenericWhereGenerator`, which
    /// checks `IndexedColumnsCache` for `machine__id` (native column with index)
    /// and falls back to JSONB extraction (`data->'machine'->>'id'`).
    fn parse_where_object(value: &serde_json::Value, path_prefix: &[String]) -> Result<Self> {
        let Some(obj) = value.as_object() else {
            return Err(FraiseQLError::Validation {
                message: "where clause must be a JSON object".to_string(),
                path:    None,
            });
        };

        let mut conditions = Vec::new();

        for (key, val) in obj {
            match key.as_str() {
                "_and" => {
                    let arr = val.as_array().ok_or_else(|| FraiseQLError::Validation {
                        message: "_and must be an array".to_string(),
                        path:    None,
                    })?;
                    let sub: Result<Vec<Self>> =
                        arr.iter().map(|v| Self::parse_where_object(v, path_prefix)).collect();
                    conditions.push(Self::And(sub?));
                },
                "_or" => {
                    let arr = val.as_array().ok_or_else(|| FraiseQLError::Validation {
                        message: "_or must be an array".to_string(),
                        path:    None,
                    })?;
                    let sub: Result<Vec<Self>> =
                        arr.iter().map(|v| Self::parse_where_object(v, path_prefix)).collect();
                    conditions.push(Self::Or(sub?));
                },
                "_not" => {
                    let sub = Self::parse_where_object(val, path_prefix)?;
                    conditions.push(Self::Not(Box::new(sub)));
                },
                field_name => {
                    let ops = val.as_object().ok_or_else(|| FraiseQLError::Validation {
                        message: format!(
                            "where field '{field_name}' must be an object of {{operator: value}}"
                        ),
                        path:    None,
                    })?;
                    let mut field_path = path_prefix.to_vec();
                    field_path.push(to_snake_case(field_name));

                    for (op_str, op_val) in ops {
                        match WhereOperator::from_str(op_str) {
                            Ok(operator) => {
                                conditions.push(Self::Field {
                                    path: field_path.clone(),
                                    operator,
                                    value: op_val.clone(),
                                });
                            },
                            Err(_) if op_val.is_object() => {
                                // Nested relation/object filter: recurse with extended path.
                                // e.g., { machine: { id: { eq: "..." } } }
                                //   → path_prefix=["machine"], key="id", value={ eq: "..." }
                                let nested_json = serde_json::json!({ op_str: op_val });
                                let nested = Self::parse_where_object(&nested_json, &field_path)?;
                                conditions.push(nested);
                            },
                            Err(e) => return Err(e),
                        }
                    }
                },
            }
        }

        if conditions.len() == 1 {
            // Reason: iterator has exactly one element — length was checked on the line above
            Ok(conditions.into_iter().next().expect("checked len == 1"))
        } else {
            Ok(Self::And(conditions))
        }
    }
}

/// Maximum nesting depth for recursive WHERE field parsing.
/// WHERE operators (FraiseQL v1 compatibility).
///
/// All standard operators are supported.
/// No underscore prefix (e.g., `eq`, `icontains`, not `_eq`, `_icontains`).
///
/// Note: ExtendedOperator variants may contain f64 values which don't implement Eq,
/// so WhereOperator derives PartialEq only (not Eq).
///
/// This enum is marked `#[non_exhaustive]` so that new operators (e.g., `Between`,
/// `Similar`) can be added in future minor versions without breaking downstream
/// exhaustive `match` expressions.
#[non_exhaustive]
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum WhereOperator {
    // ========================================================================
    // Comparison Operators
    // ========================================================================
    /// Equal (=).
    Eq,
    /// Not equal (!=).
    Neq,
    /// Greater than (>).
    Gt,
    /// Greater than or equal (>=).
    Gte,
    /// Less than (<).
    Lt,
    /// Less than or equal (<=).
    Lte,

    // ========================================================================
    // Containment Operators
    // ========================================================================
    /// In list (IN).
    In,
    /// Not in list (NOT IN).
    Nin,

    // ========================================================================
    // String Operators
    // ========================================================================
    /// Contains substring (LIKE '%value%').
    Contains,
    /// Contains substring (case-insensitive) (ILIKE '%value%').
    Icontains,
    /// Starts with (LIKE 'value%').
    Startswith,
    /// Starts with (case-insensitive) (ILIKE 'value%').
    Istartswith,
    /// Ends with (LIKE '%value').
    Endswith,
    /// Ends with (case-insensitive) (ILIKE '%value').
    Iendswith,
    /// Pattern matching (LIKE).
    Like,
    /// Pattern matching (case-insensitive) (ILIKE).
    Ilike,
    /// Negated pattern matching (NOT LIKE).
    Nlike,
    /// Negated pattern matching (case-insensitive) (NOT ILIKE).
    Nilike,
    /// POSIX regex match (~).
    Regex,
    /// POSIX regex match (case-insensitive) (~*).
    Iregex,
    /// Negated POSIX regex match (!~).
    Nregex,
    /// Negated POSIX regex match (case-insensitive) (!~*).
    Niregex,

    // ========================================================================
    // Null Checks
    // ========================================================================
    /// Is null (IS NULL or IS NOT NULL).
    IsNull,

    // ========================================================================
    // Array Operators
    // ========================================================================
    /// Array contains (@>).
    ArrayContains,
    /// Array contained by (<@).
    ArrayContainedBy,
    /// Array overlaps (&&).
    ArrayOverlaps,
    /// Array length equal.
    LenEq,
    /// Array length greater than.
    LenGt,
    /// Array length less than.
    LenLt,
    /// Array length greater than or equal.
    LenGte,
    /// Array length less than or equal.
    LenLte,
    /// Array length not equal.
    LenNeq,

    // ========================================================================
    // Vector Operators (pgvector)
    // ========================================================================
    /// Cosine distance (<=>).
    CosineDistance,
    /// L2 (Euclidean) distance (<->).
    L2Distance,
    /// L1 (Manhattan) distance (<+>).
    L1Distance,
    /// Hamming distance (<~>).
    HammingDistance,
    /// Inner product (<#>). Higher values = more similar.
    InnerProduct,
    /// Jaccard distance for set similarity.
    JaccardDistance,

    // ========================================================================
    // Full-Text Search
    // ========================================================================
    /// Full-text search (@@).
    Matches,
    /// Plain text query (plainto_tsquery).
    PlainQuery,
    /// Phrase query (phraseto_tsquery).
    PhraseQuery,
    /// Web search query (websearch_to_tsquery).
    WebsearchQuery,

    // ========================================================================
    // Network Operators (INET/CIDR)
    // ========================================================================
    /// Is IPv4.
    IsIPv4,
    /// Is IPv6.
    IsIPv6,
    /// Is private IP (RFC1918 ranges). Value controls negation (false = public).
    IsPrivate,
    /// Is loopback address (127.0.0.0/8 or ::1). Value controls negation.
    IsLoopback,
    /// Is multicast (224.0.0.0/4 or ff00::/8). Value controls negation.
    IsMulticast,
    /// Is link-local (169.254.0.0/16 or fe80::/10). Value controls negation.
    IsLinkLocal,
    /// In subnet (<<) - IP is contained within subnet.
    InSubnet,
    /// Contains subnet (>>) - subnet contains another subnet.
    ContainsSubnet,
    /// Contains IP (>>) - subnet contains an IP address.
    ContainsIP,
    /// Overlaps (&&) - subnets overlap.
    Overlaps,

    // ========================================================================
    // JSONB Operators
    // ========================================================================
    /// Strictly contains (@>).
    StrictlyContains,

    // ========================================================================
    // LTree Operators (Hierarchical)
    // ========================================================================
    /// Ancestor of (@>).
    AncestorOf,
    /// Descendant of (<@).
    DescendantOf,
    /// Matches lquery (~).
    MatchesLquery,
    /// Matches ltxtquery (@) - Boolean query syntax.
    MatchesLtxtquery,
    /// Matches any lquery (?).
    MatchesAnyLquery,
    /// Depth equal (nlevel() =).
    DepthEq,
    /// Depth not equal (nlevel() !=).
    DepthNeq,
    /// Depth greater than (nlevel() >).
    DepthGt,
    /// Depth greater than or equal (nlevel() >=).
    DepthGte,
    /// Depth less than (nlevel() <).
    DepthLt,
    /// Depth less than or equal (nlevel() <=).
    DepthLte,
    /// Lowest common ancestor (lca()).
    Lca,

    // ========================================================================
    // Extended Operators (Rich Type Filters)
    // ========================================================================
    /// Extended operator for rich scalar types (Email, VIN, CountryCode, etc.)
    /// These operators are specialized filters enabled via feature flags.
    /// See `fraiseql_core::filters::ExtendedOperator` for available operators.
    #[serde(skip)]
    Extended(crate::filters::ExtendedOperator),
}

impl WhereOperator {
    /// Parse operator from string (GraphQL input).
    ///
    /// # Errors
    ///
    /// Returns `FraiseQLError::Validation` if operator name is unknown.
    #[allow(clippy::should_implement_trait)] // Reason: intentionally not implementing `FromStr` because this returns `FraiseQLError`, not `<Self as FromStr>::Err`.
    pub fn from_str(s: &str) -> Result<Self> {
        match s {
            "eq" => Ok(Self::Eq),
            "neq" => Ok(Self::Neq),
            "gt" => Ok(Self::Gt),
            "gte" => Ok(Self::Gte),
            "lt" => Ok(Self::Lt),
            "lte" => Ok(Self::Lte),
            "in" => Ok(Self::In),
            "nin" | "notin" => Ok(Self::Nin),
            "contains" => Ok(Self::Contains),
            "icontains" => Ok(Self::Icontains),
            "startswith" => Ok(Self::Startswith),
            "istartswith" => Ok(Self::Istartswith),
            "endswith" => Ok(Self::Endswith),
            "iendswith" => Ok(Self::Iendswith),
            "like" => Ok(Self::Like),
            "ilike" => Ok(Self::Ilike),
            "nlike" => Ok(Self::Nlike),
            "nilike" => Ok(Self::Nilike),
            "regex" => Ok(Self::Regex),
            "iregex" | "imatches" => Ok(Self::Iregex),
            "nregex" | "not_matches" => Ok(Self::Nregex),
            "niregex" => Ok(Self::Niregex),
            "isnull" => Ok(Self::IsNull),
            "array_contains" => Ok(Self::ArrayContains),
            "array_contained_by" => Ok(Self::ArrayContainedBy),
            "array_overlaps" => Ok(Self::ArrayOverlaps),
            "len_eq" => Ok(Self::LenEq),
            "len_gt" => Ok(Self::LenGt),
            "len_lt" => Ok(Self::LenLt),
            "len_gte" => Ok(Self::LenGte),
            "len_lte" => Ok(Self::LenLte),
            "len_neq" => Ok(Self::LenNeq),
            "cosine_distance" => Ok(Self::CosineDistance),
            "l2_distance" => Ok(Self::L2Distance),
            "l1_distance" => Ok(Self::L1Distance),
            "hamming_distance" => Ok(Self::HammingDistance),
            "inner_product" => Ok(Self::InnerProduct),
            "jaccard_distance" => Ok(Self::JaccardDistance),
            "matches" => Ok(Self::Matches),
            "plain_query" => Ok(Self::PlainQuery),
            "phrase_query" => Ok(Self::PhraseQuery),
            "websearch_query" => Ok(Self::WebsearchQuery),
            "is_ipv4" => Ok(Self::IsIPv4),
            "is_ipv6" => Ok(Self::IsIPv6),
            "is_private" => Ok(Self::IsPrivate),
            "is_loopback" => Ok(Self::IsLoopback),
            "is_multicast" => Ok(Self::IsMulticast),
            "is_link_local" => Ok(Self::IsLinkLocal),
            "in_subnet" | "inrange" => Ok(Self::InSubnet),
            "contains_subnet" => Ok(Self::ContainsSubnet),
            "contains_ip" => Ok(Self::ContainsIP),
            "overlaps" => Ok(Self::Overlaps),
            "strictly_contains" => Ok(Self::StrictlyContains),
            "ancestor_of" => Ok(Self::AncestorOf),
            "descendant_of" => Ok(Self::DescendantOf),
            "matches_lquery" => Ok(Self::MatchesLquery),
            "matches_ltxtquery" => Ok(Self::MatchesLtxtquery),
            "matches_any_lquery" => Ok(Self::MatchesAnyLquery),
            "depth_eq" => Ok(Self::DepthEq),
            "depth_neq" => Ok(Self::DepthNeq),
            "depth_gt" => Ok(Self::DepthGt),
            "depth_gte" => Ok(Self::DepthGte),
            "depth_lt" => Ok(Self::DepthLt),
            "depth_lte" => Ok(Self::DepthLte),
            "lca" => Ok(Self::Lca),
            _ => Err(FraiseQLError::validation(format!("Unknown WHERE operator: {s}"))),
        }
    }

    /// Check if operator requires array value.
    #[must_use]
    pub const fn expects_array(&self) -> bool {
        matches!(self, Self::In | Self::Nin)
    }

    /// Check if operator is case-insensitive.
    #[must_use]
    pub const fn is_case_insensitive(&self) -> bool {
        matches!(
            self,
            Self::Icontains
                | Self::Istartswith
                | Self::Iendswith
                | Self::Ilike
                | Self::Nilike
                | Self::Iregex
                | Self::Niregex
        )
    }

    /// Check if operator works with strings.
    #[must_use]
    pub const fn is_string_operator(&self) -> bool {
        matches!(
            self,
            Self::Contains
                | Self::Icontains
                | Self::Startswith
                | Self::Istartswith
                | Self::Endswith
                | Self::Iendswith
                | Self::Like
                | Self::Ilike
                | Self::Nlike
                | Self::Nilike
                | Self::Regex
                | Self::Iregex
                | Self::Nregex
                | Self::Niregex
        )
    }
}

/// HAVING clause abstract syntax tree.
///
/// HAVING filters aggregated results after GROUP BY, while WHERE filters rows before aggregation.
///
/// # Example
///
/// ```rust
/// use fraiseql_db::{HavingClause, WhereOperator};
/// use serde_json::json;
///
/// // Simple condition: COUNT(*) > 10
/// let having_clause = HavingClause::Aggregate {
///     aggregate: "count".to_string(),
///     operator: WhereOperator::Gt,
///     value: json!(10),
/// };
///
/// // Complex condition: (COUNT(*) > 10) AND (SUM(revenue) >= 1000)
/// let having_clause = HavingClause::And(vec![
///     HavingClause::Aggregate {
///         aggregate: "count".to_string(),
///         operator: WhereOperator::Gt,
///         value: json!(10),
///     },
///     HavingClause::Aggregate {
///         aggregate: "revenue_sum".to_string(),
///         operator: WhereOperator::Gte,
///         value: json!(1000),
///     },
/// ]);
/// ```
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[non_exhaustive]
pub enum HavingClause {
    /// Aggregate field condition (e.g., count_gt, revenue_sum_gte).
    Aggregate {
        /// Aggregate name: "count" or "field_function" (e.g., "revenue_sum").
        aggregate: String,
        /// Comparison operator.
        operator:  WhereOperator,
        /// Value to compare against.
        value:     serde_json::Value,
    },

    /// Logical AND of multiple conditions.
    And(Vec<HavingClause>),

    /// Logical OR of multiple conditions.
    Or(Vec<HavingClause>),

    /// Logical NOT of a condition.
    Not(Box<HavingClause>),
}

impl HavingClause {
    /// Check if HAVING clause is empty.
    #[must_use]
    pub const fn is_empty(&self) -> bool {
        match self {
            Self::And(clauses) | Self::Or(clauses) => clauses.is_empty(),
            Self::Not(_) | Self::Aggregate { .. } => false,
        }
    }
}

#[cfg(test)]
mod tests;
