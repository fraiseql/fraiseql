//! WHERE clause operators
//!
//! Type-safe operator definitions for WHERE clause generation.
//! Supports 25+ operators across 5 categories with both JSONB and direct column sources.

use super::field::{Field, Value};
use super::order_by::FieldSource;

/// WHERE clause operators
///
/// Supports type-safe, audit-friendly WHERE clause construction
/// without raw SQL strings.
///
/// # Categories
///
/// - **Comparison**: Eq, Neq, Gt, Gte, Lt, Lte
/// - **Array**: In, Nin, Contains, ArrayContains, ArrayContainedBy, ArrayOverlaps
/// - **Array Length**: LenEq, LenGt, LenGte, LenLt, LenLte
/// - **String**: Icontains, Startswith, Endswith, Like, Ilike
/// - **Null**: IsNull
/// - **Vector Distance**: L2Distance, CosineDistance, InnerProduct, JaccardDistance
/// - **Full-Text Search**: Matches, PlainQuery, PhraseQuery, WebsearchQuery
/// - **Network**: IsIPv4, IsIPv6, IsPrivate, IsLoopback, InSubnet, ContainsSubnet, ContainsIP, IPRangeOverlap
#[derive(Debug, Clone)]
pub enum WhereOperator {
    // ============ Comparison Operators ============

    /// Equal: `field = value`
    Eq(Field, Value),

    /// Not equal: `field != value` or `field <> value`
    Neq(Field, Value),

    /// Greater than: `field > value`
    Gt(Field, Value),

    /// Greater than or equal: `field >= value`
    Gte(Field, Value),

    /// Less than: `field < value`
    Lt(Field, Value),

    /// Less than or equal: `field <= value`
    Lte(Field, Value),

    // ============ Array Operators ============

    /// Array contains value: `field IN (...)`
    In(Field, Vec<Value>),

    /// Array does not contain value: `field NOT IN (...)`
    Nin(Field, Vec<Value>),

    /// String contains substring: `field LIKE '%substring%'`
    Contains(Field, String),

    /// Array contains element: PostgreSQL array operator `@>`
    /// Generated SQL: `field @> array[value]`
    ArrayContains(Field, Value),

    /// Array is contained by: PostgreSQL array operator `<@`
    /// Generated SQL: `field <@ array[value]`
    ArrayContainedBy(Field, Value),

    /// Arrays overlap: PostgreSQL array operator `&&`
    /// Generated SQL: `field && array[value]`
    ArrayOverlaps(Field, Vec<Value>),

    // ============ Array Length Operators ============

    /// Array length equals: `array_length(field, 1) = value`
    LenEq(Field, usize),

    /// Array length greater than: `array_length(field, 1) > value`
    LenGt(Field, usize),

    /// Array length greater than or equal: `array_length(field, 1) >= value`
    LenGte(Field, usize),

    /// Array length less than: `array_length(field, 1) < value`
    LenLt(Field, usize),

    /// Array length less than or equal: `array_length(field, 1) <= value`
    LenLte(Field, usize),

    // ============ String Operators ============

    /// Case-insensitive contains: `field ILIKE '%substring%'`
    Icontains(Field, String),

    /// Starts with: `field LIKE 'prefix%'`
    Startswith(Field, String),

    /// Ends with: `field LIKE '%suffix'`
    Endswith(Field, String),

    /// LIKE pattern matching: `field LIKE pattern`
    Like(Field, String),

    /// Case-insensitive LIKE: `field ILIKE pattern`
    Ilike(Field, String),

    // ============ Null Operator ============

    /// IS NULL: `field IS NULL` or `field IS NOT NULL`
    ///
    /// When the boolean is true, generates `IS NULL`
    /// When false, generates `IS NOT NULL`
    IsNull(Field, bool),

    // ============ Vector Distance Operators (pgvector) ============

    /// L2 (Euclidean) distance: `l2_distance(field, vector) < threshold`
    ///
    /// Requires pgvector extension.
    L2Distance {
        field: Field,
        vector: Vec<f32>,
        threshold: f32,
    },

    /// Cosine distance: `cosine_distance(field, vector) < threshold`
    ///
    /// Requires pgvector extension.
    CosineDistance {
        field: Field,
        vector: Vec<f32>,
        threshold: f32,
    },

    /// Inner product: `inner_product(field, vector) > threshold`
    ///
    /// Requires pgvector extension.
    InnerProduct {
        field: Field,
        vector: Vec<f32>,
        threshold: f32,
    },

    /// Jaccard distance: `jaccard_distance(field, set) < threshold`
    ///
    /// Works with text arrays, measures set overlap.
    JaccardDistance {
        field: Field,
        set: Vec<String>,
        threshold: f32,
    },

    // ============ Full-Text Search Operators ============

    /// Full-text search with language: `field @@ plainto_tsquery(language, query)`
    ///
    /// If language is None, defaults to 'english'
    Matches {
        field: Field,
        query: String,
        language: Option<String>,
    },

    /// Plain text query: `field @@ plainto_tsquery(query)`
    ///
    /// Uses no language specification
    PlainQuery { field: Field, query: String },

    /// Phrase query with language: `field @@ phraseto_tsquery(language, query)`
    ///
    /// If language is None, defaults to 'english'
    PhraseQuery {
        field: Field,
        query: String,
        language: Option<String>,
    },

    /// Web search query with language: `field @@ websearch_to_tsquery(language, query)`
    ///
    /// If language is None, defaults to 'english'
    WebsearchQuery {
        field: Field,
        query: String,
        language: Option<String>,
    },

    // ============ Network/INET Operators ============

    /// Check if IP is IPv4: `family(field) = 4`
    IsIPv4(Field),

    /// Check if IP is IPv6: `family(field) = 6`
    IsIPv6(Field),

    /// Check if IP is private (RFC1918): matches private ranges
    IsPrivate(Field),

    /// Check if IP is loopback: IPv4 127.0.0.0/8 or IPv6 ::1/128
    IsLoopback(Field),

    /// IP is in subnet: `field << subnet`
    ///
    /// The subnet should be in CIDR notation (e.g., "192.168.0.0/24")
    InSubnet { field: Field, subnet: String },

    /// Network contains subnet: `field >> subnet`
    ///
    /// The subnet should be in CIDR notation
    ContainsSubnet { field: Field, subnet: String },

    /// Network/range contains IP: `field >> ip`
    ///
    /// The IP should be a single address (e.g., "192.168.1.1")
    ContainsIP { field: Field, ip: String },

    /// IP ranges overlap: `field && range`
    ///
    /// The range should be in CIDR notation
    IPRangeOverlap { field: Field, range: String },
}

impl WhereOperator {
    /// Get a human-readable name for this operator
    pub fn name(&self) -> &'static str {
        match self {
            WhereOperator::Eq(_, _) => "Eq",
            WhereOperator::Neq(_, _) => "Neq",
            WhereOperator::Gt(_, _) => "Gt",
            WhereOperator::Gte(_, _) => "Gte",
            WhereOperator::Lt(_, _) => "Lt",
            WhereOperator::Lte(_, _) => "Lte",
            WhereOperator::In(_, _) => "In",
            WhereOperator::Nin(_, _) => "Nin",
            WhereOperator::Contains(_, _) => "Contains",
            WhereOperator::ArrayContains(_, _) => "ArrayContains",
            WhereOperator::ArrayContainedBy(_, _) => "ArrayContainedBy",
            WhereOperator::ArrayOverlaps(_, _) => "ArrayOverlaps",
            WhereOperator::LenEq(_, _) => "LenEq",
            WhereOperator::LenGt(_, _) => "LenGt",
            WhereOperator::LenGte(_, _) => "LenGte",
            WhereOperator::LenLt(_, _) => "LenLt",
            WhereOperator::LenLte(_, _) => "LenLte",
            WhereOperator::Icontains(_, _) => "Icontains",
            WhereOperator::Startswith(_, _) => "Startswith",
            WhereOperator::Endswith(_, _) => "Endswith",
            WhereOperator::Like(_, _) => "Like",
            WhereOperator::Ilike(_, _) => "Ilike",
            WhereOperator::IsNull(_, _) => "IsNull",
            WhereOperator::L2Distance { .. } => "L2Distance",
            WhereOperator::CosineDistance { .. } => "CosineDistance",
            WhereOperator::InnerProduct { .. } => "InnerProduct",
            WhereOperator::JaccardDistance { .. } => "JaccardDistance",
            WhereOperator::Matches { .. } => "Matches",
            WhereOperator::PlainQuery { .. } => "PlainQuery",
            WhereOperator::PhraseQuery { .. } => "PhraseQuery",
            WhereOperator::WebsearchQuery { .. } => "WebsearchQuery",
            WhereOperator::IsIPv4(_) => "IsIPv4",
            WhereOperator::IsIPv6(_) => "IsIPv6",
            WhereOperator::IsPrivate(_) => "IsPrivate",
            WhereOperator::IsLoopback(_) => "IsLoopback",
            WhereOperator::InSubnet { .. } => "InSubnet",
            WhereOperator::ContainsSubnet { .. } => "ContainsSubnet",
            WhereOperator::ContainsIP { .. } => "ContainsIP",
            WhereOperator::IPRangeOverlap { .. } => "IPRangeOverlap",
        }
    }

    /// Validate operator for basic correctness
    pub fn validate(&self) -> Result<(), String> {
        match self {
            WhereOperator::Eq(f, _)
            | WhereOperator::Neq(f, _)
            | WhereOperator::Gt(f, _)
            | WhereOperator::Gte(f, _)
            | WhereOperator::Lt(f, _)
            | WhereOperator::Lte(f, _)
            | WhereOperator::In(f, _)
            | WhereOperator::Nin(f, _)
            | WhereOperator::Contains(f, _)
            | WhereOperator::ArrayContains(f, _)
            | WhereOperator::ArrayContainedBy(f, _)
            | WhereOperator::ArrayOverlaps(f, _)
            | WhereOperator::LenEq(f, _)
            | WhereOperator::LenGt(f, _)
            | WhereOperator::LenGte(f, _)
            | WhereOperator::LenLt(f, _)
            | WhereOperator::LenLte(f, _)
            | WhereOperator::Icontains(f, _)
            | WhereOperator::Startswith(f, _)
            | WhereOperator::Endswith(f, _)
            | WhereOperator::Like(f, _)
            | WhereOperator::Ilike(f, _)
            | WhereOperator::IsNull(f, _) => f.validate(),

            WhereOperator::L2Distance { field, .. }
            | WhereOperator::CosineDistance { field, .. }
            | WhereOperator::InnerProduct { field, .. }
            | WhereOperator::JaccardDistance { field, .. }
            | WhereOperator::Matches { field, .. }
            | WhereOperator::PlainQuery { field, .. }
            | WhereOperator::PhraseQuery { field, .. }
            | WhereOperator::WebsearchQuery { field, .. }
            | WhereOperator::IsIPv4(field)
            | WhereOperator::IsIPv6(field)
            | WhereOperator::IsPrivate(field)
            | WhereOperator::IsLoopback(field)
            | WhereOperator::InSubnet { field, .. }
            | WhereOperator::ContainsSubnet { field, .. }
            | WhereOperator::ContainsIP { field, .. }
            | WhereOperator::IPRangeOverlap { field, .. } => field.validate(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_operator_names() {
        let op = WhereOperator::Eq(Field::JsonbField("id".to_string()), Value::Number(1.0));
        assert_eq!(op.name(), "Eq");

        let op = WhereOperator::LenGt(Field::JsonbField("tags".to_string()), 5);
        assert_eq!(op.name(), "LenGt");
    }

    #[test]
    fn test_operator_validation() {
        let op = WhereOperator::Eq(Field::JsonbField("name".to_string()), Value::String("John".to_string()));
        assert!(op.validate().is_ok());

        let op = WhereOperator::Eq(Field::JsonbField("bad-name".to_string()), Value::String("John".to_string()));
        assert!(op.validate().is_err());
    }

    #[test]
    fn test_vector_operator_creation() {
        let op = WhereOperator::L2Distance {
            field: Field::JsonbField("embedding".to_string()),
            vector: vec![0.1, 0.2, 0.3],
            threshold: 0.5,
        };
        assert_eq!(op.name(), "L2Distance");
    }

    #[test]
    fn test_network_operator_creation() {
        let op = WhereOperator::InSubnet {
            field: Field::JsonbField("ip".to_string()),
            subnet: "192.168.0.0/24".to_string(),
        };
        assert_eq!(op.name(), "InSubnet");
    }
}
