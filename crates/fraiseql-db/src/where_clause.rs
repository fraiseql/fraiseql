//! WHERE clause abstract syntax tree.

pub mod field_types;
pub mod operator_table;

use std::{collections::HashMap, sync::Arc};

use fraiseql_error::{FraiseQLError, Result};
use serde::{Deserialize, Serialize};

pub use self::{
    field_types::{
        FieldTypeMap, RelationFieldMaps, SharedFieldTypes, WhereFieldInfo, WhereFieldSchema,
    },
    operator_table::{OperatorCategory, WHERE_OPERATORS, WhereOperatorSpec, operator_spec},
};
use crate::{types::sql_hints::ScalarFieldType, utils::to_snake_case};

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

    /// A subtree whose fields carry their declared scalar types.
    ///
    /// The generator needs the *field's* type to pick a SQL cast; deciding the
    /// cast from the operator instead is what made every non-numeric range
    /// filter a hard error (#798). The annotation is a node of the clause rather
    /// than a parameter of the generator on purpose: a clause travels through
    /// `ProjectionRequest`, the relay cursor path, the wire adapter, federation
    /// and the cache key, and an extra argument on any one of those seams is an
    /// extra place to drop it. Carried by the clause, it cannot be dropped
    /// without deleting the clause.
    ///
    /// Only the *user-supplied* filter is wrapped. RLS and injected-parameter
    /// conditions are composed separately and stay untyped — they compare
    /// tenant identifiers as text.
    Typed {
        /// Declared type per dotted snake_case field path.
        types: SharedFieldTypes,
        /// The annotated subtree.
        inner: Box<WhereClause>,
    },
}

impl WhereClause {
    /// Check if WHERE clause is empty.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        match self {
            Self::And(clauses) | Self::Or(clauses) => clauses.is_empty(),
            Self::Typed { inner, .. } => inner.is_empty(),
            Self::Not(_) | Self::Field { .. } | Self::NativeField { .. } => false,
        }
    }

    /// Annotate this clause with the declared scalar types of its fields.
    #[must_use]
    pub fn typed(self, types: SharedFieldTypes) -> Self {
        Self::Typed {
            types,
            inner: Box::new(self),
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
            Self::Not(inner) | Self::Typed { inner, .. } => inner.collect_native_column_names(out),
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
    /// `types` carries the declared scalar type of each filterable field, taken
    /// from the compiled schema. It is a required argument rather than an
    /// optional enrichment step because the cast a filter needs is a property of
    /// the field: a caller that has a schema and forgets to consult it produces
    /// SQL that errors on dates and silently under-matches on numbers. Callers
    /// that genuinely have no schema pass [`SharedFieldTypes::default`] and get
    /// value-shape inference.
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
    pub fn from_graphql_json(value: &serde_json::Value, schema: &WhereFieldSchema) -> Result<Self> {
        // Casts discovered while descending, keyed by the dotted path actually
        // walked (#1157). `WhereFieldSchema::casts()` covers the entry type only;
        // a nested path such as `customer.signed_up_at` has no entry there, and
        // the generator would otherwise infer the type from the shape of the
        // supplied JSON value — right for a number, wrong for a string that
        // denotes an instant, a date or a UUID.
        //
        // Collected during the descent rather than enumerated up front because a
        // relation cycle (`Order → Customer → Order`) makes the set of dotted
        // paths unbounded. The query's own depth bounds this one.
        let mut discovered = Vec::new();
        let parsed =
            Self::parse_where_object(value, &[], schema, schema.root_level(), &mut discovered)?;

        let types = schema.casts();
        if types.is_empty() && discovered.is_empty() {
            return Ok(parsed);
        }
        Ok(parsed.typed(Arc::new(types.as_ref().clone().with_paths(discovered))))
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
    ///
    /// `level` is the set of keys legal *at this depth* — the entry type's at the
    /// top, and the relation target's below it. `None` means the schema cannot
    /// adjudicate here, and every key passes (#939).
    ///
    /// `discovered` accumulates `(dotted path, cast)` for every comparison the
    /// descent reaches, which is how a nested path gets a declared type without
    /// enumerating the (possibly cyclic) closure of paths up front (#1157).
    fn parse_where_object(
        value: &serde_json::Value,
        path_prefix: &[String],
        schema: &WhereFieldSchema,
        level: Option<&HashMap<String, WhereFieldInfo>>,
        discovered: &mut Vec<(String, ScalarFieldType)>,
    ) -> Result<Self> {
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
                    let sub: Result<Vec<Self>> = arr
                        .iter()
                        .map(|v| {
                            Self::parse_where_object(v, path_prefix, schema, level, discovered)
                        })
                        .collect();
                    conditions.push(Self::And(sub?));
                },
                "_or" => {
                    let arr = val.as_array().ok_or_else(|| FraiseQLError::Validation {
                        message: "_or must be an array".to_string(),
                        path:    None,
                    })?;
                    let sub: Result<Vec<Self>> = arr
                        .iter()
                        .map(|v| {
                            Self::parse_where_object(v, path_prefix, schema, level, discovered)
                        })
                        .collect();
                    conditions.push(Self::Or(sub?));
                },
                "_not" => {
                    let sub =
                        Self::parse_where_object(val, path_prefix, schema, level, discovered)?;
                    conditions.push(Self::Not(Box::new(sub)));
                },
                field_name => {
                    // Security boundary (#833): the name will be interpolated
                    // into SQL text (JSON path literals, `data->>'…'`), so it
                    // must be a plain GraphQL identifier — same rule orderBy
                    // enforces. Rejecting here covers every dialect and every
                    // downstream consumer.
                    crate::utils::validate_graphql_identifier(field_name, "where")?;

                    // Every level the schema can name is adjudicated. A nested
                    // path such as `{machine: {id: …}}` resolves `id` against
                    // *machine's* type, which the compiled schema now names —
                    // and which the published `MachineWhereInput` already
                    // promises. A level the schema cannot name passes, because
                    // rejecting there would be guessing (#939).
                    // The lookup normalises to the storage key, because that is
                    // what `FieldTypeMap` is keyed by and what the path below
                    // lowers to. The *acceptance* does not: a key is legal only
                    // under the spelling the schema declares, which is the one
                    // the published `{Entity}WhereInput` carries. Accepting the
                    // storage spelling as well would mean honouring keys the
                    // published input type does not declare — the defect class
                    // this release closes, and the asymmetry `orderBy` never had.
                    //
                    // The rule is "equals `declared_name`", not "is camelCase":
                    // a schema that declares `ip_address` keeps working under
                    // that spelling, because that is what it publishes.
                    let snake = to_snake_case(field_name);
                    if let Some(level) = level {
                        match level.get(&snake) {
                            Some(info) if info.declared_name == field_name => {},
                            Some(info) => {
                                return Err(undeclared_where_spelling(
                                    field_name,
                                    &info.declared_name,
                                ));
                            },
                            None => return Err(unknown_where_field(field_name, level)),
                        }
                    }

                    let ops = val.as_object().ok_or_else(|| FraiseQLError::Validation {
                        message: format!(
                            "where field '{field_name}' must be an object of {{operator: value}}"
                        ),
                        path:    None,
                    })?;
                    let mut field_path = path_prefix.to_vec();
                    field_path.push(snake.clone());

                    // A field the schema calls a relation carries a whole nested
                    // predicate, not an operator map — which is exactly what the
                    // published `{Target}WhereInput` on it says. Descending
                    // wholesale is what makes `_and`/`_or`/`_not` work inside it:
                    // the key-at-a-time path below only recognises a combinator
                    // when its value is an object, and `_and`/`_or` take arrays,
                    // so `{machine: {_or: […]}}` failed with
                    // "Unknown WHERE operator: _or" while the type advertised it.
                    if let Some(info) = level.and_then(|l| l.get(&snake)) {
                        if info.is_relation {
                            let nested_level = info
                                .relation_type
                                .as_deref()
                                .and_then(|target| schema.level_of(target));
                            conditions.push(Self::parse_where_object(
                                val,
                                &field_path,
                                schema,
                                nested_level,
                                discovered,
                            )?);
                            continue;
                        }
                    }

                    // #1157: a comparison below the entry type has no entry in
                    // `WhereFieldSchema::casts()`, so record the one this level
                    // declares against the path actually walked. Top-level paths
                    // are already covered and are skipped rather than re-asserted,
                    // so this can never contradict the entry type's own map.
                    if !path_prefix.is_empty() {
                        if let Some(cast) = level.and_then(|l| l.get(&snake)).and_then(|i| i.cast) {
                            discovered.push((field_path.join("."), cast));
                        }
                    }

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
                                //
                                // …but only when the field is actually a
                                // relation. On a *scalar* field this arm was
                                // reinterpreting an unknown operator as a
                                // relation filter, so
                                // `{"reference": {"notAnOperator": {"eq": "x"}}}`
                                // built the path `reference.notAnOperator`,
                                // matched nothing, and returned `[]` with no
                                // error — while the same bogus operator with a
                                // *scalar* value was correctly refused. A nested
                                // relation filter on a scalar field is never
                                // legitimate.
                                if level
                                    .and_then(|l| l.get(&snake))
                                    .is_some_and(|info| !info.is_relation)
                                {
                                    return Err(FraiseQLError::Validation {
                                        message: format!(
                                            "Unknown WHERE operator '{op_str}' on field \
                                             '{field_name}'. '{field_name}' is a scalar field, so \
                                             it cannot take a nested filter."
                                        ),
                                        path:    None,
                                    });
                                }
                                let nested_json = serde_json::json!({ op_str: op_val });
                                // The level below is the relation target's own
                                // keys. `None` when the field is a relation the
                                // caller could not resolve — then, as before,
                                // the nested level is not adjudicated.
                                let nested_level = level
                                    .and_then(|l| l.get(&snake))
                                    .and_then(|info| info.relation_type.as_deref())
                                    .and_then(|target| schema.level_of(target));
                                let nested = Self::parse_where_object(
                                    &nested_json,
                                    &field_path,
                                    schema,
                                    nested_level,
                                    discovered,
                                )?;
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

/// The error for a `where` key that names a real field by a spelling the schema
/// does not declare — in practice the `snake_case` storage key, which this engine
/// accepted alongside the declared name until 2.15.0.
///
/// Named exactly rather than hinted: the client wrote a key that *resolves*, so
/// "did you mean" fuzzy matching would be answering a question we already know
/// the answer to. This is also the whole migration instruction for the change,
/// so it is worth stating outright.
fn undeclared_where_spelling(field_name: &str, declared: &str) -> FraiseQLError {
    FraiseQLError::Validation {
        message: format!(
            "Unknown field '{field_name}' in where clause. Use '{declared}', the name this \
             schema declares — the underlying storage spelling is not part of the published \
             filter input."
        ),
        path:    Some(format!("where.{field_name}")),
    }
}

/// The error for a `where` key the type does not declare.
///
/// Mirrors the shape of the unknown-operator error already in this file, plus a
/// "did you mean" hint: a `where` key is most often a rename or a
/// camelCase/snake_case slip, and both are one edit from the name that works.
fn unknown_where_field(field_name: &str, level: &HashMap<String, WhereFieldInfo>) -> FraiseQLError {
    let candidates: Vec<&str> = level.values().map(|i| i.declared_name.as_str()).collect();
    let subject = format!("Unknown field '{field_name}' in where clause.");
    let message = match crate::utils::suggest_similar(field_name, &candidates).as_slice() {
        [s] => format!("{subject} Did you mean '{s}'?"),
        [a, b] => format!("{subject} Did you mean '{a}' or '{b}'?"),
        [a, b, c, ..] => format!("{subject} Did you mean '{a}', '{b}', or '{c}'?"),
        _ => subject,
    };
    FraiseQLError::Validation {
        message,
        path: Some(format!("where.{field_name}")),
    }
}

/// Maximum nesting depth for recursive WHERE field parsing.
/// WHERE operators (FraiseQL v1 compatibility).
///
/// All standard operators are supported.
/// No underscore prefix (e.g., `eq`, `icontains`, not `_eq`, `_icontains`).
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
    /// Is null. A `false` operand inverts it to IS NOT NULL.
    IsNull,
    /// Is not null. A `false` operand inverts it to IS NULL.
    ///
    /// The inverse spelling exists because REST advertises `is_not_null` as a
    /// bracket operator; expressing it as `is_null: false` requires the client
    /// to send a boolean through a URL query string, which is exactly where
    /// #828's coercion inverted the meaning.
    IsNotNull,

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
    /// Is documentation range (RFC 5737/3849). Value controls negation.
    IsDocumentation,
    /// Is carrier-grade NAT (100.64.0.0/10, RFC 6598). Value controls negation.
    IsCarrierGrade,
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
    // LTree ID-Based Operators (resolve path from entity UUID)
    // ========================================================================
    /// Descendant of entity by ID: `path <@ (SELECT path FROM t WHERE id = $1)`.
    DescendantOfId,
    /// Ancestor of entity by ID: `path @> (SELECT path FROM t WHERE id = $1)`.
    AncestorOfId,
}

impl WhereOperator {
    /// Parse operator from string (GraphQL input).
    ///
    /// # Errors
    ///
    /// Returns `FraiseQLError::Validation` if operator name is unknown.
    #[allow(clippy::should_implement_trait)] // Reason: intentionally not implementing `FromStr` because this returns `FraiseQLError`, not `<Self as FromStr>::Err`.
    pub fn from_str(s: &str) -> Result<Self> {
        if let Some(op) = Self::match_exact(s) {
            return Ok(op);
        }

        // If the name has no underscores and contains uppercase letters, it may
        // be a camelCase form of a registered snake_case operator. Convert and
        // retry. This avoids allocation when the first match succeeds.
        if !s.contains('_') && s.chars().any(char::is_uppercase) {
            let snake = crate::utils::to_snake_case(s);
            if let Some(op) = Self::match_exact(&snake) {
                return Ok(op);
            }
        }

        Err(FraiseQLError::validation(format!("Unknown WHERE operator: {s}")))
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
