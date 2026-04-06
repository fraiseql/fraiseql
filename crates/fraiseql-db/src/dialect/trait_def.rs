//! `SqlDialect` trait — dialect-specific SQL rendering primitives.

use std::borrow::Cow;

/// Column type used by row-shaped view (`vr_*`) DDL generation.
///
/// Maps GraphQL scalar types to their SQL equivalents for typed column
/// extraction from JSON/JSONB data columns.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RowViewColumnType {
    /// Text / varchar column.
    Text,
    /// 32-bit integer.
    Int32,
    /// 64-bit integer.
    Int64,
    /// 64-bit floating point.
    Float64,
    /// Boolean.
    Boolean,
    /// UUID.
    Uuid,
    /// Timestamp with timezone.
    Timestamptz,
    /// JSON / JSONB.
    Json,
    /// Date (without time).
    Date,
}

/// Error returned when an operator is not supported by a dialect.
#[derive(Debug)]
pub struct UnsupportedOperator {
    /// Dialect name (e.g., "MySQL").
    pub dialect:  &'static str,
    /// Operator name (e.g., "ArrayContainedBy").
    pub operator: &'static str,
}

impl std::fmt::Display for UnsupportedOperator {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "operator `{}` is not supported by the {} dialect",
            self.operator, self.dialect
        )
    }
}

impl std::error::Error for UnsupportedOperator {}

/// Dialect-specific SQL rendering primitives for WHERE clause generation.
///
/// Implement this trait to add a new database backend.  All methods that are
/// identical across dialects have default implementations — override only
/// what your dialect requires.
///
/// # Security contract
///
/// Implementations MUST:
/// - Never interpolate user-supplied values into the returned SQL string. Use [`Self::placeholder`]
///   and append values to the `params` vector instead.
/// - Escape field / column names via the `path_escape` module.
/// - Escape literal SQL identifiers (not values) by doubling the delimiter.
pub trait SqlDialect: Send + Sync + 'static {
    // ── Core primitives (must implement) ──────────────────────────────────────

    /// Dialect name for error messages (e.g., `"PostgreSQL"`, `"MySQL"`).
    fn name(&self) -> &'static str;

    /// Quote a database identifier (table or column name).
    ///
    /// # Examples
    /// - PostgreSQL: `v_user` → `"v_user"`,  `evil"name` → `"evil""name"`
    /// - MySQL:      `v_user` → `` `v_user` ``, `` evil`name `` → `` `evil``name` ``
    /// - SQL Server: `v_user` → `[v_user]`,   `evil]name` → `[evil]]name]`
    fn quote_identifier(&self, name: &str) -> String;

    /// Generate SQL to extract a scalar string from a JSON/JSONB column.
    ///
    /// `column` is the unquoted column name (typically `"data"`).
    /// `path` is the slice of field-name segments (pre-escaped by caller if needed).
    ///
    /// # Examples
    /// - PostgreSQL (1 segment): `data->>'field'`
    /// - MySQL: `JSON_UNQUOTE(JSON_EXTRACT(data, '$.outer.inner'))`
    /// - SQLite: `json_extract(data, '$.outer.inner')`
    /// - SQL Server: `JSON_VALUE(data, '$.outer.inner')`
    fn json_extract_scalar(&self, column: &str, path: &[String]) -> String;

    /// Next parameter placeholder.  Called with the current 1-based index.
    ///
    /// - PostgreSQL: `$1`, `$2`, …
    /// - SQL Server: `@p1`, `@p2`, …
    /// - MySQL / SQLite: `?`
    fn placeholder(&self, n: usize) -> String;

    // ── Numeric / boolean casts (have defaults) ────────────────────────────────

    /// Wrap a JSON-extracted scalar expression so it compares numerically.
    ///
    /// Default: no cast (MySQL / SQLite coerce implicitly).
    fn cast_to_numeric<'a>(&self, expr: &'a str) -> Cow<'a, str> {
        Cow::Borrowed(expr)
    }

    /// Wrap a JSON-extracted scalar expression so it compares as a boolean.
    ///
    /// Default: no cast.
    fn cast_to_boolean<'a>(&self, expr: &'a str) -> Cow<'a, str> {
        Cow::Borrowed(expr)
    }

    /// Wrap a parameter placeholder for numeric comparison.
    ///
    /// PostgreSQL uses `({p}::text)::numeric` to avoid wire-protocol type
    /// mismatch when the driver sends JSON numbers as text.  All other dialects
    /// pass the placeholder through unchanged because their type coercion
    /// handles it transparently.
    ///
    /// Default: no cast (MySQL, SQLite, SQL Server).
    fn cast_param_numeric<'a>(&self, placeholder: &'a str) -> Cow<'a, str> {
        Cow::Borrowed(placeholder)
    }

    /// Wrap a parameter placeholder for a native-column equality condition.
    ///
    /// `native_type` is the PostgreSQL canonical type name stored in
    /// `native_columns` (e.g. `"uuid"`, `"int4"`, `"timestamp"`).
    ///
    /// Each dialect must translate it to the appropriate cast expression:
    /// - PostgreSQL uses `{placeholder}::text::{native_type}` (two-step to avoid
    ///   binary wire-format mismatch; `bool` is exempted because `QueryParam::Bool`
    ///   already encodes correctly in binary).
    /// - MySQL uses `CAST({placeholder} AS {mysql_type})`.
    /// - SQLite uses `CAST({placeholder} AS {sqlite_type})` or the placeholder as-is.
    /// - SQL Server uses `CAST({placeholder} AS {tsql_type})`.
    ///
    /// Default: pass placeholder unchanged (safe no-op fallback).
    fn cast_native_param(&self, placeholder: &str, _native_type: &str) -> String {
        placeholder.to_string()
    }

    // ── LIKE / pattern matching ────────────────────────────────────────────────

    /// SQL fragment for case-sensitive LIKE: `lhs LIKE rhs`.
    fn like_sql(&self, lhs: &str, rhs: &str) -> String {
        format!("{lhs} LIKE {rhs}")
    }

    /// SQL fragment for case-insensitive LIKE.
    ///
    /// Default: `LOWER(lhs) LIKE LOWER(rhs)` (MySQL / SQLite compatible).
    /// PostgreSQL overrides with `lhs ILIKE rhs`.
    /// SQL Server overrides with `lhs LIKE rhs COLLATE Latin1_General_CI_AI`.
    fn ilike_sql(&self, lhs: &str, rhs: &str) -> String {
        format!("LOWER({lhs}) LIKE LOWER({rhs})")
    }

    /// String concatenation operator / function for building LIKE patterns.
    ///
    /// Default: `||` (ANSI SQL — works for PostgreSQL and SQLite).
    /// MySQL overrides with `CONCAT(…)`.
    /// SQL Server overrides with `+`.
    fn concat_sql(&self, parts: &[&str]) -> String {
        parts.join(" || ")
    }

    // ── Empty clause sentinels ─────────────────────────────────────────────────

    /// SQL literal for "always false" (used for empty IN clauses, empty OR).
    ///
    /// Default: `FALSE`. SQLite and SQL Server use `1=0`.
    fn always_false(&self) -> &'static str {
        "FALSE"
    }

    /// SQL literal for "always true" (used for empty AND).
    ///
    /// Default: `TRUE`. SQLite and SQL Server use `1=1`.
    fn always_true(&self) -> &'static str {
        "TRUE"
    }

    // ── Inequality operator ────────────────────────────────────────────────────

    /// SQL inequality operator.  Default `!=`.  SQL Server uses `<>`.
    fn neq_operator(&self) -> &'static str {
        "!="
    }

    // ── Array length function ──────────────────────────────────────────────────

    /// SQL expression for the length of a JSON array stored in `expr`.
    fn json_array_length(&self, expr: &str) -> String;

    // ── Array containment (returns Err if not supported) ──────────────────────

    /// SQL for "array contains this element".
    ///
    /// Default: returns `Err(UnsupportedOperator)`.
    ///
    /// # Errors
    ///
    /// Returns [`UnsupportedOperator`] if this dialect does not support array containment.
    fn array_contains_sql(&self, _lhs: &str, _rhs: &str) -> Result<String, UnsupportedOperator> {
        Err(UnsupportedOperator {
            dialect:  self.name(),
            operator: "ArrayContains",
        })
    }

    /// SQL for "element is contained by array".
    ///
    /// # Errors
    ///
    /// Returns [`UnsupportedOperator`] if this dialect does not support array containment.
    fn array_contained_by_sql(
        &self,
        _lhs: &str,
        _rhs: &str,
    ) -> Result<String, UnsupportedOperator> {
        Err(UnsupportedOperator {
            dialect:  self.name(),
            operator: "ArrayContainedBy",
        })
    }

    /// SQL for "arrays overlap".
    ///
    /// # Errors
    ///
    /// Returns [`UnsupportedOperator`] if this dialect does not support array overlap.
    fn array_overlaps_sql(&self, _lhs: &str, _rhs: &str) -> Result<String, UnsupportedOperator> {
        Err(UnsupportedOperator {
            dialect:  self.name(),
            operator: "ArrayOverlaps",
        })
    }

    // ── Full-text search (returns Err if not supported) ────────────────────────

    /// SQL for `to_tsvector(expr) @@ to_tsquery(param)`.
    ///
    /// # Errors
    ///
    /// Returns [`UnsupportedOperator`] if this dialect does not support full-text search.
    fn fts_matches_sql(&self, _expr: &str, _param: &str) -> Result<String, UnsupportedOperator> {
        Err(UnsupportedOperator {
            dialect:  self.name(),
            operator: "Matches",
        })
    }

    /// SQL for plain-text full-text search.
    ///
    /// # Errors
    ///
    /// Returns [`UnsupportedOperator`] if this dialect does not support plain-text FTS.
    fn fts_plain_query_sql(
        &self,
        _expr: &str,
        _param: &str,
    ) -> Result<String, UnsupportedOperator> {
        Err(UnsupportedOperator {
            dialect:  self.name(),
            operator: "PlainQuery",
        })
    }

    /// SQL for phrase full-text search.
    ///
    /// # Errors
    ///
    /// Returns [`UnsupportedOperator`] if this dialect does not support phrase FTS.
    fn fts_phrase_query_sql(
        &self,
        _expr: &str,
        _param: &str,
    ) -> Result<String, UnsupportedOperator> {
        Err(UnsupportedOperator {
            dialect:  self.name(),
            operator: "PhraseQuery",
        })
    }

    /// SQL for web-search full-text search.
    ///
    /// # Errors
    ///
    /// Returns [`UnsupportedOperator`] if this dialect does not support web-search FTS.
    fn fts_websearch_query_sql(
        &self,
        _expr: &str,
        _param: &str,
    ) -> Result<String, UnsupportedOperator> {
        Err(UnsupportedOperator {
            dialect:  self.name(),
            operator: "WebsearchQuery",
        })
    }

    // ── Regex (returns Err if not supported) ───────────────────────────────────

    /// SQL for POSIX-style regex match.
    ///
    /// Default: returns `Err(UnsupportedOperator)`.
    /// PostgreSQL overrides with `~`, `~*`, `!~`, `!~*`.
    /// MySQL overrides with `REGEXP` / `NOT REGEXP`.
    ///
    /// # Errors
    ///
    /// Returns [`UnsupportedOperator`] if this dialect does not support regex matching.
    fn regex_sql(
        &self,
        _lhs: &str,
        _rhs: &str,
        _case_insensitive: bool,
        _negate: bool,
    ) -> Result<String, UnsupportedOperator> {
        Err(UnsupportedOperator {
            dialect:  self.name(),
            operator: "Regex",
        })
    }

    // ── PostgreSQL-only operators (Vector, Network, LTree) ────────────────────
    // These methods have default `Err` implementations; only `PostgresDialect`
    // overrides them.  Callers push parameter values before calling these methods
    // and pass the already-generated placeholder strings.

    /// Generate SQL for a pgvector distance operator.
    ///
    /// `pg_op` is one of `<=>`, `<->`, `<+>`, `<~>`, `<#>`.
    /// `lhs` / `rhs` are the field expression and the placeholder string.
    ///
    /// # Errors
    ///
    /// Returns [`UnsupportedOperator`] if this dialect does not support vector distance.
    fn vector_distance_sql(
        &self,
        _pg_op: &str,
        _lhs: &str,
        _rhs: &str,
    ) -> Result<String, UnsupportedOperator> {
        Err(UnsupportedOperator {
            dialect:  self.name(),
            operator: "VectorDistance",
        })
    }

    /// Generate SQL for Jaccard distance (`::text[] <%> ::text[]`).
    ///
    /// # Errors
    ///
    /// Returns [`UnsupportedOperator`] if this dialect does not support Jaccard distance.
    fn jaccard_distance_sql(&self, _lhs: &str, _rhs: &str) -> Result<String, UnsupportedOperator> {
        Err(UnsupportedOperator {
            dialect:  self.name(),
            operator: "JaccardDistance",
        })
    }

    /// Generate SQL for an INET unary check (IsIPv4, IsIPv6, IsPrivate, IsPublic, IsLoopback).
    ///
    /// `check_name` identifies the operator (passed to `UnsupportedOperator` on failure).
    ///
    /// # Errors
    ///
    /// Returns [`UnsupportedOperator`] if this dialect does not support INET checks.
    fn inet_check_sql(&self, _lhs: &str, _check_name: &str) -> Result<String, UnsupportedOperator> {
        Err(UnsupportedOperator {
            dialect:  self.name(),
            operator: "InetCheck",
        })
    }

    /// Generate SQL for an INET binary operation (InSubnet, ContainsSubnet, ContainsIP, Overlaps).
    ///
    /// `pg_op` is one of `<<`, `>>`, `&&`.
    ///
    /// # Errors
    ///
    /// Returns [`UnsupportedOperator`] if this dialect does not support INET binary operations.
    fn inet_binary_sql(
        &self,
        _pg_op: &str,
        _lhs: &str,
        _rhs: &str,
    ) -> Result<String, UnsupportedOperator> {
        Err(UnsupportedOperator {
            dialect:  self.name(),
            operator: "InetBinaryOp",
        })
    }

    /// Generate SQL for an LTree binary operator.
    ///
    /// `pg_op` is one of `@>`, `<@`, `~`, `@`.
    /// `rhs_type` is the cast type for `rhs` (e.g., `"ltree"`, `"lquery"`, `"ltxtquery"`).
    ///
    /// # Errors
    ///
    /// Returns [`UnsupportedOperator`] if this dialect does not support LTree operations.
    fn ltree_binary_sql(
        &self,
        _pg_op: &str,
        _lhs: &str,
        _rhs: &str,
        _rhs_type: &str,
    ) -> Result<String, UnsupportedOperator> {
        Err(UnsupportedOperator {
            dialect:  self.name(),
            operator: "LTreeBinaryOp",
        })
    }

    /// Generate SQL for `ltree ? ARRAY[...]` (MatchesAnyLquery).
    ///
    /// `placeholders` contains pre-formatted placeholder strings
    /// (e.g., `["$1::lquery", "$2::lquery"]`).
    ///
    /// # Errors
    ///
    /// Returns [`UnsupportedOperator`] if this dialect does not support LTree lquery arrays.
    fn ltree_any_lquery_sql(
        &self,
        _lhs: &str,
        _placeholders: &[String],
    ) -> Result<String, UnsupportedOperator> {
        Err(UnsupportedOperator {
            dialect:  self.name(),
            operator: "MatchesAnyLquery",
        })
    }

    /// Generate SQL for `nlevel(ltree) OP param` (depth comparison operators).
    ///
    /// # Errors
    ///
    /// Returns [`UnsupportedOperator`] if this dialect does not support LTree depth comparisons.
    fn ltree_depth_sql(
        &self,
        _op: &str,
        _lhs: &str,
        _rhs: &str,
    ) -> Result<String, UnsupportedOperator> {
        Err(UnsupportedOperator {
            dialect:  self.name(),
            operator: "LTreeDepth",
        })
    }

    /// Generate SQL for `ltree = lca(ARRAY[...])` (Lca operator).
    ///
    /// `placeholders` contains pre-formatted placeholder strings
    /// (e.g., `["$1::ltree", "$2::ltree"]`).
    ///
    /// # Errors
    ///
    /// Returns [`UnsupportedOperator`] if this dialect does not support LTree LCA.
    fn ltree_lca_sql(
        &self,
        _lhs: &str,
        _placeholders: &[String],
    ) -> Result<String, UnsupportedOperator> {
        Err(UnsupportedOperator {
            dialect:  self.name(),
            operator: "Lca",
        })
    }

    // ── Row-view DDL helpers (for gRPC transport) ──────────────────────────────

    /// Generate a SQL expression that extracts a scalar field from a JSON column
    /// and casts it to the given [`RowViewColumnType`].
    ///
    /// `json_column` is the unquoted column name (e.g., `"data"`).
    /// `field_name` is the JSON key to extract.
    /// `col_type` selects the target SQL type.
    ///
    /// Default: panics — each dialect must override.
    fn row_view_column_expr(
        &self,
        json_column: &str,
        field_name: &str,
        col_type: &RowViewColumnType,
    ) -> String {
        let _ = (json_column, field_name, col_type);
        panic!("{} dialect has not implemented row_view_column_expr", self.name())
    }

    /// Generate the full DDL statement(s) to create a row-shaped view.
    ///
    /// `view_name` is the unquoted view name (e.g., `"vr_user"`).
    /// `source_table` is the unquoted source table (e.g., `"tb_user"`).
    /// `columns` are `(alias, expression)` pairs from [`Self::row_view_column_expr`].
    ///
    /// Default: `CREATE OR REPLACE VIEW "view" AS SELECT ... FROM "table"`.
    fn create_row_view_ddl(
        &self,
        view_name: &str,
        source_table: &str,
        columns: &[(String, String)],
    ) -> String {
        let quoted_view = self.quote_identifier(view_name);
        let quoted_table = self.quote_identifier(source_table);
        let col_list: Vec<String> = columns
            .iter()
            .map(|(alias, expr)| format!("{expr} AS {}", self.quote_identifier(alias)))
            .collect();
        format!(
            "CREATE OR REPLACE VIEW {quoted_view} AS\nSELECT\n  {}\nFROM {quoted_table};",
            col_list.join(",\n  ")
        )
    }

    // ── Extended operators (Email, VIN, IBAN, …) ───────────────────────────────

    /// Generate SQL for an extended rich-type operator.
    ///
    /// Default: returns a validation error (operator not supported).
    /// Each dialect overrides this to provide dialect-specific SQL functions
    /// (e.g. `SPLIT_PART` for PostgreSQL, `SUBSTRING_INDEX` for MySQL).
    ///
    /// # Errors
    ///
    /// Returns `FraiseQLError::Validation` if the operator is not supported
    /// by this dialect or the parameters are invalid.
    fn generate_extended_sql(
        &self,
        operator: &crate::filters::ExtendedOperator,
        _field_sql: &str,
        _params: &mut Vec<serde_json::Value>,
    ) -> fraiseql_error::Result<String> {
        Err(fraiseql_error::FraiseQLError::validation(format!(
            "Extended operator {operator} is not supported by the {} dialect",
            self.name()
        )))
    }
}
