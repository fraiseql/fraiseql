//! WHERE clause abstract syntax tree.

use serde::{Deserialize, Serialize};

use crate::error::{FraiseQLError, Result};

/// WHERE clause abstract syntax tree.
///
/// Represents a type-safe WHERE condition that can be compiled to database-specific SQL.
///
/// # Example
///
/// ```rust
/// use fraiseql_core::db::{WhereClause, WhereOperator};
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
pub enum WhereClause {
    /// Single field condition.
    Field {
        /// JSONB path (e.g., ["email"] or ["posts", "title"]).
        path: Vec<String>,
        /// Comparison operator.
        operator: WhereOperator,
        /// Value to compare against.
        value: serde_json::Value,
    },

    /// Logical AND of multiple conditions.
    And(Vec<WhereClause>),

    /// Logical OR of multiple conditions.
    Or(Vec<WhereClause>),

    /// Logical NOT of a condition.
    Not(Box<WhereClause>),
}

impl WhereClause {
    /// Check if WHERE clause is empty.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        match self {
            Self::And(clauses) | Self::Or(clauses) => clauses.is_empty(),
            Self::Not(_) | Self::Field { .. } => false,
        }
    }
}

/// WHERE operators (FraiseQL v1 compatibility).
///
/// All operators from v1 are supported for backwards compatibility.
/// No underscore prefix (e.g., `eq`, `icontains`, not `_eq`, `_icontains`).
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
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
    /// Is private IP (RFC1918 ranges).
    IsPrivate,
    /// Is public IP (not private).
    IsPublic,
    /// Is loopback address (127.0.0.0/8 or ::1).
    IsLoopback,
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
}

impl WhereOperator {
    /// Parse operator from string (GraphQL input).
    ///
    /// # Errors
    ///
    /// Returns `FraiseQLError::Validation` if operator name is unknown.
    pub fn from_str(s: &str) -> Result<Self> {
        match s {
            "eq" => Ok(Self::Eq),
            "neq" => Ok(Self::Neq),
            "gt" => Ok(Self::Gt),
            "gte" => Ok(Self::Gte),
            "lt" => Ok(Self::Lt),
            "lte" => Ok(Self::Lte),
            "in" => Ok(Self::In),
            "nin" => Ok(Self::Nin),
            "contains" => Ok(Self::Contains),
            "icontains" => Ok(Self::Icontains),
            "startswith" => Ok(Self::Startswith),
            "istartswith" => Ok(Self::Istartswith),
            "endswith" => Ok(Self::Endswith),
            "iendswith" => Ok(Self::Iendswith),
            "like" => Ok(Self::Like),
            "ilike" => Ok(Self::Ilike),
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
            "is_public" => Ok(Self::IsPublic),
            "is_loopback" => Ok(Self::IsLoopback),
            "in_subnet" => Ok(Self::InSubnet),
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
            Self::Icontains | Self::Istartswith | Self::Iendswith | Self::Ilike
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
/// use fraiseql_core::db::{HavingClause, WhereOperator};
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
pub enum HavingClause {
    /// Aggregate field condition (e.g., count_gt, revenue_sum_gte).
    Aggregate {
        /// Aggregate name: "count" or "field_function" (e.g., "revenue_sum").
        aggregate: String,
        /// Comparison operator.
        operator: WhereOperator,
        /// Value to compare against.
        value: serde_json::Value,
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
    pub fn is_empty(&self) -> bool {
        match self {
            Self::And(clauses) | Self::Or(clauses) => clauses.is_empty(),
            Self::Not(_) | Self::Aggregate { .. } => false,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn test_where_operator_from_str() {
        assert_eq!(WhereOperator::from_str("eq").unwrap(), WhereOperator::Eq);
        assert_eq!(WhereOperator::from_str("icontains").unwrap(), WhereOperator::Icontains);
        assert_eq!(WhereOperator::from_str("gte").unwrap(), WhereOperator::Gte);
        assert!(WhereOperator::from_str("unknown").is_err());
    }

    #[test]
    fn test_where_operator_expects_array() {
        assert!(WhereOperator::In.expects_array());
        assert!(WhereOperator::Nin.expects_array());
        assert!(!WhereOperator::Eq.expects_array());
    }

    #[test]
    fn test_where_operator_is_case_insensitive() {
        assert!(WhereOperator::Icontains.is_case_insensitive());
        assert!(WhereOperator::Ilike.is_case_insensitive());
        assert!(!WhereOperator::Contains.is_case_insensitive());
    }

    #[test]
    fn test_where_clause_simple() {
        let clause = WhereClause::Field {
            path: vec!["email".to_string()],
            operator: WhereOperator::Eq,
            value: json!("test@example.com"),
        };

        assert!(!clause.is_empty());
    }

    #[test]
    fn test_where_clause_and() {
        let clause = WhereClause::And(vec![
            WhereClause::Field {
                path: vec!["published".to_string()],
                operator: WhereOperator::Eq,
                value: json!(true),
            },
            WhereClause::Field {
                path: vec!["views".to_string()],
                operator: WhereOperator::Gte,
                value: json!(100),
            },
        ]);

        assert!(!clause.is_empty());
    }

    #[test]
    fn test_where_clause_empty() {
        let clause = WhereClause::And(vec![]);
        assert!(clause.is_empty());
    }
}
