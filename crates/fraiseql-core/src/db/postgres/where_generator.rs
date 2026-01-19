//! PostgreSQL WHERE clause SQL generation.

use std::{
    collections::{HashMap, HashSet},
    sync::Arc,
};

use crate::{
    db::where_clause::{WhereClause, WhereOperator},
    error::{FraiseQLError, Result},
};

/// Cache of indexed columns for views.
///
/// This cache stores column names that follow the FraiseQL indexed column naming conventions:
/// - Human-readable: `items__product__category__code` (double underscore separated path)
/// - Entity ID format: `f{entity_id}__{field_name}` (e.g., `f200100__code`)
///
/// When a WHERE clause references a nested path that has a corresponding indexed column,
/// the generator will use the indexed column directly instead of JSONB extraction,
/// enabling the database to use indexes for the query.
///
/// # Example
///
/// ```rust
/// use fraiseql_core::db::postgres::IndexedColumnsCache;
/// use std::collections::{HashMap, HashSet};
/// use std::sync::Arc;
///
/// let mut cache = IndexedColumnsCache::new();
///
/// // Register indexed columns for a view
/// let mut columns = HashSet::new();
/// columns.insert("items__product__category__code".to_string());
/// columns.insert("f200100__code".to_string());
/// cache.insert("v_order_items".to_string(), columns);
///
/// // Later, the generator uses this to optimize WHERE clauses
/// let arc_cache = Arc::new(cache);
/// ```
pub type IndexedColumnsCache = HashMap<String, HashSet<String>>;

/// PostgreSQL WHERE clause generator.
///
/// Converts `WhereClause` AST to PostgreSQL SQL with parameterized queries.
///
/// # Interior Mutability Pattern
///
/// This struct uses `Cell<usize>` for the parameter counter. This is safe because:
///
/// 1. **Single-threaded usage**: Each WHERE generator is created for a single
///    query execution and isn't shared across async tasks.
///
/// 2. **Reset per call**: The counter is reset at the start of `generate()`,
///    ensuring no state leakage between calls.
///
/// 3. **Performance**: Avoids mutex overhead for a simple counter that needs frequent updates.
///
/// # If Shared Across Tasks
///
/// If this generator were Arc-shared across multiple async tasks, replace
/// `Cell<usize>` with `AtomicUsize` to prevent data races:
///
/// ```rust,ignore
/// // Instead of: Cell<usize>
/// // Use: AtomicUsize
///
/// param_counter: std::sync::atomic::AtomicUsize::new(0),
/// // Then use compare_and_swap or fetch_add operations
/// ```
///
/// # Indexed Column Optimization
///
/// When an `IndexedColumnsCache` is provided, the generator checks if nested paths
/// have corresponding indexed columns. If found, it uses the indexed column directly
/// instead of JSONB extraction, enabling index usage.
///
/// For example, with `items__product__category__code` indexed:
/// - Without cache: `data->'items'->'product'->'category'->>'code' = $1`
/// - With cache: `items__product__category__code = $1`
///
/// # Example
///
/// ```rust
/// use fraiseql_core::db::postgres::PostgresWhereGenerator;
/// use fraiseql_core::db::{WhereClause, WhereOperator};
/// use serde_json::json;
///
/// let generator = PostgresWhereGenerator::new();
///
/// let clause = WhereClause::Field {
///     path: vec!["email".to_string()],
///     operator: WhereOperator::Icontains,
///     value: json!("example.com"),
/// };
///
/// let (sql, params) = generator.generate(&clause).expect("Failed to generate SQL");
/// // sql: "data->>'email' ILIKE '%' || $1 || '%'"
/// // params: ["example.com"]
/// ```
pub struct PostgresWhereGenerator {
    /// Parameter counter for generating placeholder names ($1, $2, etc.)
    ///
    /// Uses `Cell<usize>` for interior mutability. Safe because:
    /// - Single-threaded context (not shared across async tasks)
    /// - Reset at start of each `generate()` call
    /// - No concurrent access possible within query execution
    ///
    /// See struct documentation for why this is safe and how to fix if shared.
    param_counter:   std::cell::Cell<usize>,
    /// Optional indexed columns cache for the current view.
    /// When set, the generator will use indexed columns instead of JSONB extraction
    /// for nested paths that have corresponding indexed columns.
    indexed_columns: Option<Arc<HashSet<String>>>,
}

impl PostgresWhereGenerator {
    /// Create new PostgreSQL WHERE generator.
    #[must_use]
    pub fn new() -> Self {
        Self {
            param_counter:   std::cell::Cell::new(0),
            indexed_columns: None,
        }
    }

    /// Create new PostgreSQL WHERE generator with indexed columns for a view.
    ///
    /// When indexed columns are provided, the generator will use them instead of
    /// JSONB extraction for nested paths that have corresponding indexed columns.
    ///
    /// # Arguments
    ///
    /// * `indexed_columns` - Set of indexed column names for the current view
    ///
    /// # Example
    ///
    /// ```rust
    /// use fraiseql_core::db::postgres::PostgresWhereGenerator;
    /// use std::collections::HashSet;
    /// use std::sync::Arc;
    ///
    /// let mut columns = HashSet::new();
    /// columns.insert("items__product__category__code".to_string());
    /// let generator = PostgresWhereGenerator::with_indexed_columns(Arc::new(columns));
    /// ```
    #[must_use]
    pub fn with_indexed_columns(indexed_columns: Arc<HashSet<String>>) -> Self {
        Self {
            param_counter:   std::cell::Cell::new(0),
            indexed_columns: Some(indexed_columns),
        }
    }

    /// Generate SQL WHERE clause and parameters.
    ///
    /// # Arguments
    ///
    /// * `clause` - WHERE clause AST
    ///
    /// # Returns
    ///
    /// Returns tuple of (SQL string, parameter values).
    ///
    /// # Errors
    ///
    /// Returns `FraiseQLError::Validation` if clause is invalid.
    pub fn generate(&self, clause: &WhereClause) -> Result<(String, Vec<serde_json::Value>)> {
        self.param_counter.set(0);
        let mut params = Vec::new();
        let sql = self.generate_clause(clause, &mut params)?;
        Ok((sql, params))
    }

    fn generate_clause(
        &self,
        clause: &WhereClause,
        params: &mut Vec<serde_json::Value>,
    ) -> Result<String> {
        match clause {
            WhereClause::Field {
                path,
                operator,
                value,
            } => self.generate_field(path, operator, value, params),
            WhereClause::And(clauses) => {
                if clauses.is_empty() {
                    return Ok("TRUE".to_string());
                }
                let parts: Result<Vec<String>> =
                    clauses.iter().map(|c| self.generate_clause(c, params)).collect();
                Ok(format!("({})", parts?.join(" AND ")))
            },
            WhereClause::Or(clauses) => {
                if clauses.is_empty() {
                    return Ok("FALSE".to_string());
                }
                let parts: Result<Vec<String>> =
                    clauses.iter().map(|c| self.generate_clause(c, params)).collect();
                Ok(format!("({})", parts?.join(" OR ")))
            },
            WhereClause::Not(clause) => {
                let inner = self.generate_clause(clause, params)?;
                Ok(format!("NOT ({inner})"))
            },
        }
    }

    fn generate_field(
        &self,
        path: &[String],
        operator: &WhereOperator,
        value: &serde_json::Value,
        params: &mut Vec<serde_json::Value>,
    ) -> Result<String> {
        // Build JSONB path accessor
        let field_path = self.build_jsonb_path(path);

        // Generate operator-specific SQL
        match operator {
            // Comparison operators
            WhereOperator::Eq => self.generate_comparison(&field_path, "=", value, params),
            WhereOperator::Neq => self.generate_comparison(&field_path, "!=", value, params),
            WhereOperator::Gt => self.generate_comparison(&field_path, ">", value, params),
            WhereOperator::Gte => self.generate_comparison(&field_path, ">=", value, params),
            WhereOperator::Lt => self.generate_comparison(&field_path, "<", value, params),
            WhereOperator::Lte => self.generate_comparison(&field_path, "<=", value, params),

            // Containment operators
            WhereOperator::In => self.generate_in(&field_path, value, params),
            WhereOperator::Nin => {
                let in_clause = self.generate_in(&field_path, value, params)?;
                Ok(format!("NOT ({in_clause})"))
            },

            // String operators
            WhereOperator::Contains => {
                self.generate_like(&field_path, "LIKE", value, params, true, true)
            },
            WhereOperator::Icontains => {
                self.generate_like(&field_path, "ILIKE", value, params, true, true)
            },
            WhereOperator::Startswith => {
                self.generate_like(&field_path, "LIKE", value, params, false, true)
            },
            WhereOperator::Istartswith => {
                self.generate_like(&field_path, "ILIKE", value, params, false, true)
            },
            WhereOperator::Endswith => {
                self.generate_like(&field_path, "LIKE", value, params, true, false)
            },
            WhereOperator::Iendswith => {
                self.generate_like(&field_path, "ILIKE", value, params, true, false)
            },
            WhereOperator::Like => self.generate_comparison(&field_path, "LIKE", value, params),
            WhereOperator::Ilike => self.generate_comparison(&field_path, "ILIKE", value, params),

            // Null checks
            WhereOperator::IsNull => {
                let is_null = if value.as_bool().unwrap_or(true) {
                    "IS NULL"
                } else {
                    "IS NOT NULL"
                };
                Ok(format!("{field_path} {is_null}"))
            },

            // Array operators
            WhereOperator::ArrayContains => {
                self.generate_jsonb_op(&field_path, "@>", value, params)
            },
            WhereOperator::ArrayContainedBy => {
                self.generate_jsonb_op(&field_path, "<@", value, params)
            },
            WhereOperator::ArrayOverlaps => {
                self.generate_jsonb_op(&field_path, "&&", value, params)
            },
            WhereOperator::LenEq => self.generate_array_length(&field_path, "=", value, params),
            WhereOperator::LenGt => self.generate_array_length(&field_path, ">", value, params),
            WhereOperator::LenLt => self.generate_array_length(&field_path, "<", value, params),
            WhereOperator::LenGte => self.generate_array_length(&field_path, ">=", value, params),
            WhereOperator::LenLte => self.generate_array_length(&field_path, "<=", value, params),
            WhereOperator::LenNeq => self.generate_array_length(&field_path, "!=", value, params),

            // Vector operators (pgvector)
            WhereOperator::CosineDistance => {
                self.generate_vector_distance(&field_path, "<=>", value, params)
            },
            WhereOperator::L2Distance => {
                self.generate_vector_distance(&field_path, "<->", value, params)
            },
            WhereOperator::L1Distance => {
                self.generate_vector_distance(&field_path, "<+>", value, params)
            },
            WhereOperator::HammingDistance => {
                self.generate_vector_distance(&field_path, "<~>", value, params)
            },
            WhereOperator::InnerProduct => {
                self.generate_vector_distance(&field_path, "<#>", value, params)
            },
            WhereOperator::JaccardDistance => {
                self.generate_jaccard_distance(&field_path, value, params)
            },

            // Full-text search
            WhereOperator::Matches => self.generate_fts(&field_path, "@@", value, params),
            WhereOperator::PlainQuery => {
                self.generate_fts_func(&field_path, "plainto_tsquery", value, params)
            },
            WhereOperator::PhraseQuery => {
                self.generate_fts_func(&field_path, "phraseto_tsquery", value, params)
            },
            WhereOperator::WebsearchQuery => {
                self.generate_fts_func(&field_path, "websearch_to_tsquery", value, params)
            },

            // Network operators
            WhereOperator::IsIPv4 => Ok(format!("family({field_path}::inet) = 4")),
            WhereOperator::IsIPv6 => Ok(format!("family({field_path}::inet) = 6")),
            WhereOperator::IsPrivate => Ok(format!(
                "({field_path}::inet << '10.0.0.0/8'::inet OR {field_path}::inet << '172.16.0.0/12'::inet OR {field_path}::inet << '192.168.0.0/16'::inet OR {field_path}::inet << '169.254.0.0/16'::inet)"
            )),
            WhereOperator::IsPublic => Ok(format!(
                "NOT ({field_path}::inet << '10.0.0.0/8'::inet OR {field_path}::inet << '172.16.0.0/12'::inet OR {field_path}::inet << '192.168.0.0/16'::inet OR {field_path}::inet << '169.254.0.0/16'::inet)"
            )),
            WhereOperator::IsLoopback => Ok(format!(
                "(family({field_path}::inet) = 4 AND {field_path}::inet << '127.0.0.0/8'::inet) OR (family({field_path}::inet) = 6 AND {field_path}::inet << '::1/128'::inet)"
            )),
            WhereOperator::InSubnet => self.generate_inet_op(&field_path, "<<", value, params),
            WhereOperator::ContainsSubnet => {
                self.generate_inet_op(&field_path, ">>", value, params)
            },
            WhereOperator::ContainsIP => self.generate_inet_op(&field_path, ">>", value, params),
            WhereOperator::Overlaps => self.generate_inet_op(&field_path, "&&", value, params),

            // JSONB operators
            WhereOperator::StrictlyContains => {
                self.generate_jsonb_op(&field_path, "@>", value, params)
            },

            // LTree operators
            WhereOperator::AncestorOf => {
                self.generate_ltree_op(&field_path, "@>", "ltree", value, params)
            },
            WhereOperator::DescendantOf => {
                self.generate_ltree_op(&field_path, "<@", "ltree", value, params)
            },
            WhereOperator::MatchesLquery => {
                self.generate_ltree_op(&field_path, "~", "lquery", value, params)
            },
            WhereOperator::MatchesLtxtquery => {
                self.generate_ltree_op(&field_path, "@", "ltxtquery", value, params)
            },
            WhereOperator::MatchesAnyLquery => {
                self.generate_ltree_array_op(&field_path, value, params)
            },
            WhereOperator::DepthEq => self.generate_ltree_depth(&field_path, "=", value, params),
            WhereOperator::DepthNeq => self.generate_ltree_depth(&field_path, "!=", value, params),
            WhereOperator::DepthGt => self.generate_ltree_depth(&field_path, ">", value, params),
            WhereOperator::DepthGte => self.generate_ltree_depth(&field_path, ">=", value, params),
            WhereOperator::DepthLt => self.generate_ltree_depth(&field_path, "<", value, params),
            WhereOperator::DepthLte => self.generate_ltree_depth(&field_path, "<=", value, params),
            WhereOperator::Lca => self.generate_ltree_lca(&field_path, value, params),
        }
    }

    fn build_jsonb_path(&self, path: &[String]) -> String {
        // Check if an indexed column exists for this path
        if let Some(indexed_col) = self.find_indexed_column(path) {
            // Use the indexed column directly instead of JSONB extraction
            return format!("\"{indexed_col}\"");
        }

        // Fall back to JSONB extraction
        if path.len() == 1 {
            format!("data->>'{}'{}", path[0], "")
        } else {
            let mut result = "data".to_string();
            for (i, segment) in path.iter().enumerate() {
                if i < path.len() - 1 {
                    result.push_str(&format!("->'{segment}'"));
                } else {
                    result.push_str(&format!("->>'{segment}'"));
                }
            }
            result
        }
    }

    /// Find an indexed column for the given path.
    ///
    /// Checks the indexed columns cache for columns matching the path using both
    /// naming conventions:
    /// 1. Human-readable: `items__product__category__code`
    /// 2. Entity ID format: `f{entity_id}__field_name` (not checked here as it requires entity ID)
    ///
    /// Returns the column name if found, None otherwise.
    fn find_indexed_column(&self, path: &[String]) -> Option<String> {
        let indexed_columns = self.indexed_columns.as_ref()?;

        // Build human-readable column name: join with __
        let human_readable = path.join("__");

        // Check if this column exists in the cache
        if indexed_columns.contains(&human_readable) {
            return Some(human_readable);
        }

        // Note: Entity ID format (f{entity_id}__field) would require entity ID mapping
        // which is not available at this level. The DBA can use human-readable names
        // for most cases. Entity ID format is primarily useful for very long paths
        // that exceed PostgreSQL's 63-character identifier limit.

        None
    }

    fn next_param(&self) -> String {
        let current = self.param_counter.get();
        self.param_counter.set(current + 1);
        format!("${}", current + 1)
    }

    fn generate_comparison(
        &self,
        field_path: &str,
        op: &str,
        value: &serde_json::Value,
        params: &mut Vec<serde_json::Value>,
    ) -> Result<String> {
        let param = self.next_param();
        params.push(value.clone());

        // For numeric comparisons, cast both sides to numeric type
        // Use text format for parameter to avoid wire protocol issues
        if value.is_number()
            && (op == ">" || op == ">=" || op == "<" || op == "<=" || op == "=" || op == "!=")
        {
            Ok(format!("({field_path})::numeric {op} ({param}::text)::numeric"))
        } else if value.is_boolean() && (op == "=" || op == "!=") {
            // For boolean comparisons, cast the JSONB text field to boolean
            Ok(format!("({field_path})::boolean {op} {param}"))
        } else {
            Ok(format!("{field_path} {op} {param}"))
        }
    }

    fn generate_in(
        &self,
        field_path: &str,
        value: &serde_json::Value,
        params: &mut Vec<serde_json::Value>,
    ) -> Result<String> {
        let array = value.as_array().ok_or_else(|| {
            FraiseQLError::validation("IN operator requires array value".to_string())
        })?;

        if array.is_empty() {
            return Ok("FALSE".to_string());
        }

        let placeholders: Vec<String> = array
            .iter()
            .map(|v| {
                let param = self.next_param();
                params.push(v.clone());
                param
            })
            .collect();

        Ok(format!("{field_path} IN ({})", placeholders.join(", ")))
    }

    fn generate_like(
        &self,
        field_path: &str,
        op: &str,
        value: &serde_json::Value,
        params: &mut Vec<serde_json::Value>,
        prefix: bool,
        suffix: bool,
    ) -> Result<String> {
        let param = self.next_param();
        let val_str = value.as_str().ok_or_else(|| {
            FraiseQLError::validation("LIKE operator requires string value".to_string())
        })?;

        let pattern = if prefix && suffix {
            format!("'%' || {param} || '%'")
        } else if prefix {
            format!("'%' || {param}")
        } else if suffix {
            format!("{param} || '%'")
        } else {
            param.clone()
        };

        params.push(serde_json::Value::String(val_str.to_string()));
        Ok(format!("{field_path} {op} {pattern}"))
    }

    fn generate_jsonb_op(
        &self,
        field_path: &str,
        op: &str,
        value: &serde_json::Value,
        params: &mut Vec<serde_json::Value>,
    ) -> Result<String> {
        let param = self.next_param();
        params.push(value.clone());
        Ok(format!("{field_path}::jsonb {op} {param}::jsonb"))
    }

    fn generate_array_length(
        &self,
        field_path: &str,
        op: &str,
        value: &serde_json::Value,
        params: &mut Vec<serde_json::Value>,
    ) -> Result<String> {
        let param = self.next_param();
        params.push(value.clone());
        Ok(format!("jsonb_array_length({field_path}::jsonb) {op} {param}"))
    }

    fn generate_vector_distance(
        &self,
        field_path: &str,
        op: &str,
        value: &serde_json::Value,
        params: &mut Vec<serde_json::Value>,
    ) -> Result<String> {
        let param = self.next_param();
        params.push(value.clone());
        Ok(format!("{field_path}::vector {op} {param}::vector"))
    }

    fn generate_fts(
        &self,
        field_path: &str,
        op: &str,
        value: &serde_json::Value,
        params: &mut Vec<serde_json::Value>,
    ) -> Result<String> {
        let param = self.next_param();
        params.push(value.clone());
        Ok(format!("to_tsvector({field_path}) {op} to_tsquery({param})"))
    }

    fn generate_fts_func(
        &self,
        field_path: &str,
        func: &str,
        value: &serde_json::Value,
        params: &mut Vec<serde_json::Value>,
    ) -> Result<String> {
        let param = self.next_param();
        params.push(value.clone());
        Ok(format!("to_tsvector({field_path}) @@ {func}({param})"))
    }

    fn generate_jaccard_distance(
        &self,
        field_path: &str,
        value: &serde_json::Value,
        params: &mut Vec<serde_json::Value>,
    ) -> Result<String> {
        let param = self.next_param();
        params.push(value.clone());
        // Jaccard distance uses text arrays
        Ok(format!("({field_path})::text[] <%> ({param})::text[]"))
    }

    fn generate_inet_op(
        &self,
        field_path: &str,
        op: &str,
        value: &serde_json::Value,
        params: &mut Vec<serde_json::Value>,
    ) -> Result<String> {
        let param = self.next_param();
        params.push(value.clone());
        Ok(format!("{field_path}::inet {op} {param}::inet"))
    }

    fn generate_ltree_op(
        &self,
        field_path: &str,
        op: &str,
        value_type: &str,
        value: &serde_json::Value,
        params: &mut Vec<serde_json::Value>,
    ) -> Result<String> {
        let param = self.next_param();
        params.push(value.clone());
        Ok(format!("{field_path}::ltree {op} {param}::{value_type}"))
    }

    fn generate_ltree_array_op(
        &self,
        field_path: &str,
        value: &serde_json::Value,
        params: &mut Vec<serde_json::Value>,
    ) -> Result<String> {
        let array = value.as_array().ok_or_else(|| {
            FraiseQLError::validation(
                "matches_any_lquery operator requires array value".to_string(),
            )
        })?;

        if array.is_empty() {
            return Ok("FALSE".to_string());
        }

        let placeholders: Vec<String> = array
            .iter()
            .map(|v| {
                let param = self.next_param();
                params.push(v.clone());
                format!("{param}::lquery")
            })
            .collect();

        Ok(format!("{field_path}::ltree ? ARRAY[{}]", placeholders.join(", ")))
    }

    fn generate_ltree_depth(
        &self,
        field_path: &str,
        op: &str,
        value: &serde_json::Value,
        params: &mut Vec<serde_json::Value>,
    ) -> Result<String> {
        let param = self.next_param();
        params.push(value.clone());
        Ok(format!("nlevel({field_path}::ltree) {op} {param}"))
    }

    fn generate_ltree_lca(
        &self,
        field_path: &str,
        value: &serde_json::Value,
        params: &mut Vec<serde_json::Value>,
    ) -> Result<String> {
        let array = value.as_array().ok_or_else(|| {
            FraiseQLError::validation("lca operator requires array value".to_string())
        })?;

        if array.is_empty() {
            return Err(FraiseQLError::validation(
                "lca operator requires at least one path".to_string(),
            ));
        }

        let placeholders: Vec<String> = array
            .iter()
            .map(|v| {
                let param = self.next_param();
                params.push(v.clone());
                format!("{param}::ltree")
            })
            .collect();

        Ok(format!("{field_path}::ltree = lca(ARRAY[{}])", placeholders.join(", ")))
    }
}

impl Default for PostgresWhereGenerator {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use std::{collections::HashSet, sync::Arc};

    use serde_json::json;

    use super::*;

    #[test]
    fn test_simple_equality() {
        let gen = PostgresWhereGenerator::new();
        let clause = WhereClause::Field {
            path:     vec!["email".to_string()],
            operator: WhereOperator::Eq,
            value:    json!("test@example.com"),
        };

        let (sql, params) = gen.generate(&clause).unwrap();
        assert_eq!(sql, "data->>'email' = $1");
        assert_eq!(params, vec![json!("test@example.com")]);
    }

    #[test]
    fn test_icontains() {
        let gen = PostgresWhereGenerator::new();
        let clause = WhereClause::Field {
            path:     vec!["email".to_string()],
            operator: WhereOperator::Icontains,
            value:    json!("example.com"),
        };

        let (sql, params) = gen.generate(&clause).unwrap();
        assert_eq!(sql, "data->>'email' ILIKE '%' || $1 || '%'");
        assert_eq!(params, vec![json!("example.com")]);
    }

    #[test]
    fn test_nested_path() {
        let gen = PostgresWhereGenerator::new();
        let clause = WhereClause::Field {
            path:     vec!["address".to_string(), "city".to_string()],
            operator: WhereOperator::Eq,
            value:    json!("Paris"),
        };

        let (sql, params) = gen.generate(&clause).unwrap();
        assert_eq!(sql, "data->'address'->>'city' = $1");
        assert_eq!(params, vec![json!("Paris")]);
    }

    #[test]
    fn test_and_clause() {
        let gen = PostgresWhereGenerator::new();
        let clause = WhereClause::And(vec![
            WhereClause::Field {
                path:     vec!["age".to_string()],
                operator: WhereOperator::Gte,
                value:    json!(18),
            },
            WhereClause::Field {
                path:     vec!["active".to_string()],
                operator: WhereOperator::Eq,
                value:    json!(true),
            },
        ]);

        let (sql, params) = gen.generate(&clause).unwrap();
        // Numeric comparisons cast to ::numeric, boolean comparisons cast to ::boolean
        assert_eq!(
            sql,
            "((data->>'age')::numeric >= ($1::text)::numeric AND (data->>'active')::boolean = $2)"
        );
        assert_eq!(params, vec![json!(18), json!(true)]);
    }

    #[test]
    fn test_or_clause() {
        let gen = PostgresWhereGenerator::new();
        let clause = WhereClause::Or(vec![
            WhereClause::Field {
                path:     vec!["role".to_string()],
                operator: WhereOperator::Eq,
                value:    json!("admin"),
            },
            WhereClause::Field {
                path:     vec!["role".to_string()],
                operator: WhereOperator::Eq,
                value:    json!("moderator"),
            },
        ]);

        let (sql, params) = gen.generate(&clause).unwrap();
        assert_eq!(sql, "(data->>'role' = $1 OR data->>'role' = $2)");
        assert_eq!(params, vec![json!("admin"), json!("moderator")]);
    }

    #[test]
    fn test_not_clause() {
        let gen = PostgresWhereGenerator::new();
        let clause = WhereClause::Not(Box::new(WhereClause::Field {
            path:     vec!["deleted".to_string()],
            operator: WhereOperator::Eq,
            value:    json!(true),
        }));

        let (sql, params) = gen.generate(&clause).unwrap();
        // Boolean comparisons now cast to ::boolean
        assert_eq!(sql, "NOT ((data->>'deleted')::boolean = $1)");
        assert_eq!(params, vec![json!(true)]);
    }

    #[test]
    fn test_in_operator() {
        let gen = PostgresWhereGenerator::new();
        let clause = WhereClause::Field {
            path:     vec!["status".to_string()],
            operator: WhereOperator::In,
            value:    json!(["active", "pending"]),
        };

        let (sql, params) = gen.generate(&clause).unwrap();
        assert_eq!(sql, "data->>'status' IN ($1, $2)");
        assert_eq!(params, vec![json!("active"), json!("pending")]);
    }

    #[test]
    fn test_is_null() {
        let gen = PostgresWhereGenerator::new();
        let clause = WhereClause::Field {
            path:     vec!["deleted_at".to_string()],
            operator: WhereOperator::IsNull,
            value:    json!(true),
        };

        let (sql, _params) = gen.generate(&clause).unwrap();
        assert_eq!(sql, "data->>'deleted_at' IS NULL");
    }

    #[test]
    fn test_array_contains() {
        let gen = PostgresWhereGenerator::new();
        let clause = WhereClause::Field {
            path:     vec!["tags".to_string()],
            operator: WhereOperator::ArrayContains,
            value:    json!(["rust"]),
        };

        let (sql, params) = gen.generate(&clause).unwrap();
        assert_eq!(sql, "data->>'tags'::jsonb @> $1::jsonb");
        assert_eq!(params, vec![json!(["rust"])]);
    }

    // ============ LTree Operator Tests ============

    #[test]
    fn test_ltree_ancestor_of() {
        let gen = PostgresWhereGenerator::new();
        let clause = WhereClause::Field {
            path:     vec!["path".to_string()],
            operator: WhereOperator::AncestorOf,
            value:    json!("Top.Sciences.Astronomy"),
        };

        let (sql, params) = gen.generate(&clause).unwrap();
        assert_eq!(sql, "data->>'path'::ltree @> $1::ltree");
        assert_eq!(params, vec![json!("Top.Sciences.Astronomy")]);
    }

    #[test]
    fn test_ltree_descendant_of() {
        let gen = PostgresWhereGenerator::new();
        let clause = WhereClause::Field {
            path:     vec!["path".to_string()],
            operator: WhereOperator::DescendantOf,
            value:    json!("Top.Sciences"),
        };

        let (sql, params) = gen.generate(&clause).unwrap();
        assert_eq!(sql, "data->>'path'::ltree <@ $1::ltree");
        assert_eq!(params, vec![json!("Top.Sciences")]);
    }

    #[test]
    fn test_ltree_matches_lquery() {
        let gen = PostgresWhereGenerator::new();
        let clause = WhereClause::Field {
            path:     vec!["path".to_string()],
            operator: WhereOperator::MatchesLquery,
            value:    json!("Top.*.Ast*"),
        };

        let (sql, params) = gen.generate(&clause).unwrap();
        assert_eq!(sql, "data->>'path'::ltree ~ $1::lquery");
        assert_eq!(params, vec![json!("Top.*.Ast*")]);
    }

    #[test]
    fn test_ltree_matches_ltxtquery() {
        let gen = PostgresWhereGenerator::new();
        let clause = WhereClause::Field {
            path:     vec!["path".to_string()],
            operator: WhereOperator::MatchesLtxtquery,
            value:    json!("Science & !Deprecated"),
        };

        let (sql, params) = gen.generate(&clause).unwrap();
        assert_eq!(sql, "data->>'path'::ltree @ $1::ltxtquery");
        assert_eq!(params, vec![json!("Science & !Deprecated")]);
    }

    #[test]
    fn test_ltree_matches_any_lquery() {
        let gen = PostgresWhereGenerator::new();
        let clause = WhereClause::Field {
            path:     vec!["path".to_string()],
            operator: WhereOperator::MatchesAnyLquery,
            value:    json!(["Top.*", "Other.*"]),
        };

        let (sql, params) = gen.generate(&clause).unwrap();
        assert_eq!(sql, "data->>'path'::ltree ? ARRAY[$1::lquery, $2::lquery]");
        assert_eq!(params, vec![json!("Top.*"), json!("Other.*")]);
    }

    #[test]
    fn test_ltree_depth_eq() {
        let gen = PostgresWhereGenerator::new();
        let clause = WhereClause::Field {
            path:     vec!["path".to_string()],
            operator: WhereOperator::DepthEq,
            value:    json!(3),
        };

        let (sql, params) = gen.generate(&clause).unwrap();
        assert_eq!(sql, "nlevel(data->>'path'::ltree) = $1");
        assert_eq!(params, vec![json!(3)]);
    }

    #[test]
    fn test_ltree_depth_gt() {
        let gen = PostgresWhereGenerator::new();
        let clause = WhereClause::Field {
            path:     vec!["path".to_string()],
            operator: WhereOperator::DepthGt,
            value:    json!(2),
        };

        let (sql, params) = gen.generate(&clause).unwrap();
        assert_eq!(sql, "nlevel(data->>'path'::ltree) > $1");
        assert_eq!(params, vec![json!(2)]);
    }

    #[test]
    fn test_ltree_lca() {
        let gen = PostgresWhereGenerator::new();
        let clause = WhereClause::Field {
            path:     vec!["path".to_string()],
            operator: WhereOperator::Lca,
            value:    json!(["Org.Engineering.Backend", "Org.Engineering.Frontend"]),
        };

        let (sql, params) = gen.generate(&clause).unwrap();
        assert_eq!(sql, "data->>'path'::ltree = lca(ARRAY[$1::ltree, $2::ltree])");
        assert_eq!(
            params,
            vec![
                json!("Org.Engineering.Backend"),
                json!("Org.Engineering.Frontend")
            ]
        );
    }

    // ============ Indexed Column Optimization Tests ============

    #[test]
    fn test_indexed_column_simple_path() {
        // When an indexed column exists for a simple path, use it directly
        let mut indexed = HashSet::new();
        indexed.insert("category__code".to_string());
        let gen = PostgresWhereGenerator::with_indexed_columns(Arc::new(indexed));

        let clause = WhereClause::Field {
            path:     vec!["category".to_string(), "code".to_string()],
            operator: WhereOperator::Eq,
            value:    json!("ELEC"),
        };

        let (sql, params) = gen.generate(&clause).unwrap();
        // Uses indexed column instead of JSONB extraction
        assert_eq!(sql, "\"category__code\" = $1");
        assert_eq!(params, vec![json!("ELEC")]);
    }

    #[test]
    fn test_indexed_column_nested_path() {
        // Deep nested path with indexed column
        let mut indexed = HashSet::new();
        indexed.insert("items__product__category__code".to_string());
        let gen = PostgresWhereGenerator::with_indexed_columns(Arc::new(indexed));

        let clause = WhereClause::Field {
            path:     vec![
                "items".to_string(),
                "product".to_string(),
                "category".to_string(),
                "code".to_string(),
            ],
            operator: WhereOperator::Eq,
            value:    json!("ELEC"),
        };

        let (sql, params) = gen.generate(&clause).unwrap();
        // Uses indexed column instead of deep JSONB extraction
        assert_eq!(sql, "\"items__product__category__code\" = $1");
        assert_eq!(params, vec![json!("ELEC")]);
    }

    #[test]
    fn test_indexed_column_fallback_to_jsonb() {
        // Path without indexed column falls back to JSONB
        let mut indexed = HashSet::new();
        indexed.insert("items__product__category__code".to_string());
        let gen = PostgresWhereGenerator::with_indexed_columns(Arc::new(indexed));

        let clause = WhereClause::Field {
            path:     vec![
                "items".to_string(),
                "product".to_string(),
                "name".to_string(),
            ],
            operator: WhereOperator::Eq,
            value:    json!("Widget"),
        };

        let (sql, params) = gen.generate(&clause).unwrap();
        // Falls back to JSONB extraction since no indexed column exists
        assert_eq!(sql, "data->'items'->'product'->>'name' = $1");
        assert_eq!(params, vec![json!("Widget")]);
    }

    #[test]
    fn test_indexed_column_with_like_operator() {
        // Indexed columns work with all operators
        let mut indexed = HashSet::new();
        indexed.insert("category__name".to_string());
        let gen = PostgresWhereGenerator::with_indexed_columns(Arc::new(indexed));

        let clause = WhereClause::Field {
            path:     vec!["category".to_string(), "name".to_string()],
            operator: WhereOperator::Icontains,
            value:    json!("electronics"),
        };

        let (sql, params) = gen.generate(&clause).unwrap();
        // Uses indexed column with ILIKE operator
        assert_eq!(sql, "\"category__name\" ILIKE '%' || $1 || '%'");
        assert_eq!(params, vec![json!("electronics")]);
    }

    #[test]
    fn test_indexed_column_with_numeric_comparison() {
        // Indexed columns with numeric values
        let mut indexed = HashSet::new();
        indexed.insert("order__total".to_string());
        let gen = PostgresWhereGenerator::with_indexed_columns(Arc::new(indexed));

        let clause = WhereClause::Field {
            path:     vec!["order".to_string(), "total".to_string()],
            operator: WhereOperator::Gt,
            value:    json!(100),
        };

        let (sql, params) = gen.generate(&clause).unwrap();
        // Uses indexed column with numeric cast
        assert_eq!(sql, "(\"order__total\")::numeric > ($1::text)::numeric");
        assert_eq!(params, vec![json!(100)]);
    }

    #[test]
    fn test_indexed_column_empty_cache() {
        // Empty cache falls back to JSONB
        let indexed = HashSet::new();
        let gen = PostgresWhereGenerator::with_indexed_columns(Arc::new(indexed));

        let clause = WhereClause::Field {
            path:     vec!["category".to_string(), "code".to_string()],
            operator: WhereOperator::Eq,
            value:    json!("ELEC"),
        };

        let (sql, params) = gen.generate(&clause).unwrap();
        // Falls back to JSONB extraction
        assert_eq!(sql, "data->'category'->>'code' = $1");
        assert_eq!(params, vec![json!("ELEC")]);
    }

    #[test]
    fn test_no_indexed_columns_cache() {
        // No cache provided uses JSONB (default behavior)
        let gen = PostgresWhereGenerator::new();

        let clause = WhereClause::Field {
            path:     vec!["category".to_string(), "code".to_string()],
            operator: WhereOperator::Eq,
            value:    json!("ELEC"),
        };

        let (sql, params) = gen.generate(&clause).unwrap();
        // Uses JSONB extraction
        assert_eq!(sql, "data->'category'->>'code' = $1");
        assert_eq!(params, vec![json!("ELEC")]);
    }
}
