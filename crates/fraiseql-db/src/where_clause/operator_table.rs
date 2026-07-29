//! The one table of WHERE operator names.
//!
//! Before #828 there were two: `WhereOperator::match_exact` decided what the
//! executor could *run*, and `fraiseql_core::utils::operators::OPERATOR_REGISTRY`
//! decided what the REST transport would *accept*. They disagreed on 27 names,
//! so `?status[ne]=archived` passed validation and then failed in the WHERE
//! parser with `Unknown WHERE operator: ne` — and the 400's "Available
//! operators" list recommended two dozen more names with the same behaviour.
//!
//! Both are now generated from [`WHERE_OPERATORS`] by the `where_operators!`
//! macro below, so a name that is advertised is a name that runs, by
//! construction. Adding a variant without giving it a name is a compile error;
//! adding a name here advertises it everywhere at once.

use super::WhereOperator;

/// Broad family of an operator. Drives documentation grouping and the REST
/// surface's error messages; it is not consulted during SQL generation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum OperatorCategory {
    /// Basic comparison: `=`, `!=`, `>`, `<`, `>=`, `<=`.
    Comparison,
    /// String operations: LIKE, ILIKE, regex.
    String,
    /// NULL checks: IS NULL, IS NOT NULL.
    Null,
    /// Array/list containment: `@>`, `<@`, `&&`.
    Array,
    /// pgvector distance operators.
    Vector,
    /// `PostgreSQL` full-text search.
    Fulltext,
    /// JSONB containment: `@>`, `<@`.
    Containment,
    /// Network/IP operators.
    Network,
    /// Ltree (hierarchical) operators.
    Ltree,
}

/// Everything known about one WHERE operator.
#[derive(Debug, Clone, Copy)]
pub struct WhereOperatorSpec {
    /// Canonical GraphQL name (e.g. `"eq"`).
    pub name:           &'static str,
    /// Accepted alternative spellings (e.g. `"ne"` for `"neq"`).
    pub aliases:        &'static [&'static str],
    /// SQL operator or function template, for documentation.
    pub sql_op:         &'static str,
    /// Family this operator belongs to.
    pub category:       OperatorCategory,
    /// Whether the operand must be a list.
    pub requires_array: bool,
    /// Whether the operator works on the JSONB value rather than its text form.
    pub jsonb_operator: bool,
}

impl WhereOperatorSpec {
    /// Canonical name and aliases, in that order.
    pub fn all_names(&self) -> impl Iterator<Item = &'static str> + '_ {
        std::iter::once(self.name).chain(self.aliases.iter().copied())
    }
}

/// Generate [`WHERE_OPERATORS`] and `WhereOperator::match_exact` from one list.
macro_rules! where_operators {
    ($(
        $variant:ident, $name:literal, [$($alias:literal),*], $sql:literal,
        $category:ident, $array:literal, $jsonb:literal;
    )*) => {
        /// Every WHERE operator the executor can run, with the names it answers to.
        pub const WHERE_OPERATORS: &[WhereOperatorSpec] = &[
            $(WhereOperatorSpec {
                name:           $name,
                aliases:        &[$($alias),*],
                sql_op:         $sql,
                category:       OperatorCategory::$category,
                requires_array: $array,
                jsonb_operator: $jsonb,
            }),*
        ];

        impl WhereOperator {
            /// Match an operator name exactly against the known set.
            pub(super) fn match_exact(s: &str) -> Option<Self> {
                match s {
                    $($name $(| $alias)* => Some(Self::$variant),)*
                    _ => None,
                }
            }
        }
    };
}

where_operators! {
    // ── Comparison ────────────────────────────────────────────────────────
    Eq,  "eq",  [],     "=",      Comparison, false, false;
    Neq, "neq", ["ne"], "!=",     Comparison, false, false;
    Gt,  "gt",  [],     ">",      Comparison, false, false;
    Gte, "gte", [],     ">=",     Comparison, false, false;
    Lt,  "lt",  [],     "<",      Comparison, false, false;
    Lte, "lte", [],     "<=",     Comparison, false, false;
    In,  "in",  [],     "IN",     Comparison, true,  false;
    Nin, "nin", ["notin"], "NOT IN", Comparison, true, false;

    // ── NULL ──────────────────────────────────────────────────────────────
    IsNull,    "isnull",      ["is_null"],     "IS NULL",     Null, false, false;
    IsNotNull, "is_not_null", ["isnotnull"],   "IS NOT NULL", Null, false, false;

    // ── String ────────────────────────────────────────────────────────────
    Contains,    "contains",    [], "LIKE",     String, false, false;
    Icontains,   "icontains",   [], "ILIKE",    String, false, false;
    Startswith,  "startswith",  [], "LIKE",     String, false, false;
    Istartswith, "istartswith", [], "ILIKE",    String, false, false;
    Endswith,    "endswith",    [], "LIKE",     String, false, false;
    Iendswith,   "iendswith",   [], "ILIKE",    String, false, false;
    Like,        "like",        [], "LIKE",     String, false, false;
    Ilike,       "ilike",       [], "ILIKE",    String, false, false;
    Nlike,       "nlike",       [], "NOT LIKE", String, false, false;
    Nilike,      "nilike",      [], "NOT ILIKE", String, false, false;
    Regex,       "regex",       [], "~",        String, false, false;
    Iregex,      "iregex",      ["imatches"],    "~*",  String, false, false;
    Nregex,      "nregex",      ["not_matches"], "!~",  String, false, false;
    Niregex,     "niregex",     [], "!~*",      String, false, false;

    // ── Array ─────────────────────────────────────────────────────────────
    ArrayContains,     "array_contains",     [], "@>", Array, false, false;
    ArrayContainedBy,  "array_contained_by", ["array_contained_in", "contained_in"], "<@",
                       Array, false, false;
    ArrayOverlaps,     "array_overlaps",     [], "&&", Array, false, false;
    LenEq,             "len_eq",             [], "jsonb_array_length =",  Array, false, true;
    LenNeq,            "len_neq",            [], "jsonb_array_length !=", Array, false, true;
    LenGt,             "len_gt",             [], "jsonb_array_length >",  Array, false, true;
    LenGte,            "len_gte",            [], "jsonb_array_length >=", Array, false, true;
    LenLt,             "len_lt",             [], "jsonb_array_length <",  Array, false, true;
    LenLte,            "len_lte",            [], "jsonb_array_length <=", Array, false, true;

    // ── JSONB containment ─────────────────────────────────────────────────
    StrictlyContains, "strictly_contains", [], "@>", Containment, false, true;

    // ── Vector (pgvector) ─────────────────────────────────────────────────
    CosineDistance,  "cosine_distance",  [], "<=>", Vector, false, false;
    L2Distance,      "l2_distance",      [], "<->", Vector, false, false;
    L1Distance,      "l1_distance",      [], "<+>", Vector, false, false;
    HammingDistance, "hamming_distance", [], "<~>", Vector, false, false;
    InnerProduct,    "inner_product",    [], "<#>", Vector, false, false;
    JaccardDistance, "jaccard_distance", [], "<%>", Vector, false, false;

    // ── Full-text search ──────────────────────────────────────────────────
    Matches,        "matches",         ["search"],               "@@", Fulltext, false, false;
    PlainQuery,     "plain_query",     ["plainto_tsquery"],      "@@", Fulltext, false, false;
    PhraseQuery,    "phrase_query",    ["phraseto_tsquery"],     "@@", Fulltext, false, false;
    WebsearchQuery, "websearch_query", ["websearch_to_tsquery"], "@@", Fulltext, false, false;

    // ── Network (INET/CIDR) ───────────────────────────────────────────────
    IsIPv4,           "is_ipv4",           ["isIPv4"],  "family({}) = 4",   Network, false, false;
    IsIPv6,           "is_ipv6",           ["isIPv6"],  "family({}) = 6",   Network, false, false;
    IsPrivate,        "is_private",        ["isPrivate"],        "CIDR_RANGE_CHECK", Network, false, false;
    IsLoopback,       "is_loopback",       ["isLoopback"],       "CIDR_RANGE_CHECK", Network, false, false;
    IsMulticast,      "is_multicast",      ["isMulticast"],      "CIDR_RANGE_CHECK", Network, false, false;
    IsLinkLocal,      "is_link_local",     ["isLinkLocal"],      "CIDR_RANGE_CHECK", Network, false, false;
    IsDocumentation,  "is_documentation",  ["isDocumentation"],  "CIDR_RANGE_CHECK", Network, false, false;
    IsCarrierGrade,   "is_carrier_grade",  ["isCarrierGrade"],   "CIDR_RANGE_CHECK", Network, false, false;
    InSubnet,         "in_subnet",         ["inrange", "inSubnet"], "<<",   Network, false, false;
    ContainsSubnet,   "contains_subnet",   ["subnet_contains"],  ">>",      Network, false, false;
    ContainsIP,       "contains_ip",       [],                   ">>",      Network, false, false;
    Overlaps,         "overlaps",          ["subnet_overlaps"],  "&&",      Network, false, false;

    // ── Ltree ─────────────────────────────────────────────────────────────
    AncestorOf,       "ancestor_of",        [],               "@>", Ltree, false, false;
    DescendantOf,     "descendant_of",      ["isdescendant"], "<@", Ltree, false, false;
    MatchesLquery,    "matches_lquery",     [], "~", Ltree, false, false;
    MatchesLtxtquery, "matches_ltxtquery",  [], "@", Ltree, false, false;
    MatchesAnyLquery, "matches_any_lquery", [], "?", Ltree, true,  false;
    DepthEq,          "depth_eq",           [], "nlevel({}) =",  Ltree, false, false;
    DepthNeq,         "depth_neq",          [], "nlevel({}) !=", Ltree, false, false;
    DepthGt,          "depth_gt",           [], "nlevel({}) >",  Ltree, false, false;
    DepthGte,         "depth_gte",          [], "nlevel({}) >=", Ltree, false, false;
    DepthLt,          "depth_lt",           [], "nlevel({}) <",  Ltree, false, false;
    DepthLte,         "depth_lte",          [], "nlevel({}) <=", Ltree, false, false;
    Lca,              "lca",                [], "lca()", Ltree, true, false;
    DescendantOfId,   "descendant_of_id",   [], "<@", Ltree, false, false;
    AncestorOfId,     "ancestor_of_id",     [], "@>", Ltree, false, false;
}

/// Look up an operator's specification by any of its accepted names.
#[must_use]
pub fn operator_spec(name: &str) -> Option<&'static WhereOperatorSpec> {
    WHERE_OPERATORS.iter().find(|spec| spec.all_names().any(|n| n == name))
}

#[cfg(test)]
#[path = "operator_table_tests.rs"]
mod operator_table_tests;
