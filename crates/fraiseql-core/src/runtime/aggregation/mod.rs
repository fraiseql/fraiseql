//! Runtime Aggregation SQL Generation Module
//!
//! This module generates database-specific SQL from aggregation execution plans.
//!
//! # Database-Specific SQL
//!
//! ## PostgreSQL
//! ```sql
//! SELECT
//!   data->>'category' AS category,
//!   DATE_TRUNC('day', occurred_at) AS occurred_at_day,
//!   COUNT(*) AS count,
//!   SUM(revenue) AS revenue_sum
//! FROM tf_sales
//! WHERE customer_id = $1
//! GROUP BY data->>'category', DATE_TRUNC('day', occurred_at)
//! HAVING SUM(revenue) > $2
//! ORDER BY revenue_sum DESC
//! LIMIT 10
//! ```
//!
//! ## MySQL
//! ```sql
//! SELECT
//!   JSON_UNQUOTE(JSON_EXTRACT(data, '$.category')) AS category,
//!   DATE_FORMAT(occurred_at, '%Y-%m-%d') AS occurred_at_day,
//!   COUNT(*) AS count,
//!   SUM(revenue) AS revenue_sum
//! FROM tf_sales
//! WHERE customer_id = ?
//! GROUP BY JSON_UNQUOTE(JSON_EXTRACT(data, '$.category')), DATE_FORMAT(occurred_at, '%Y-%m-%d')
//! HAVING SUM(revenue) > ?
//! ORDER BY revenue_sum DESC
//! LIMIT 10
//! ```
//!
//! ## SQLite
//! ```sql
//! SELECT
//!   json_extract(data, '$.category') AS category,
//!   strftime('%Y-%m-%d', occurred_at) AS occurred_at_day,
//!   COUNT(*) AS count,
//!   SUM(revenue) AS revenue_sum
//! FROM tf_sales
//! WHERE customer_id = ?
//! GROUP BY json_extract(data, '$.category'), strftime('%Y-%m-%d', occurred_at)
//! HAVING SUM(revenue) > ?
//! ORDER BY revenue_sum DESC
//! LIMIT 10
//! ```

use crate::{
    compiler::{
        aggregate_types::{AggregateFunction, TemporalBucket},
        aggregation::{
            AggregateExpression, AggregationPlan, GroupByExpression, OrderByClause, OrderDirection,
            ValidatedHavingCondition,
        },
        fact_table::FactTableMetadata,
    },
    db::{
        identifier::{
            quote_mysql_identifier, quote_postgres_identifier, quote_sqlserver_identifier,
        },
        path_escape::{
            escape_mysql_json_path, escape_postgres_jsonb_segment, escape_sqlite_json_path,
            escape_sqlserver_json_path,
        },
        types::DatabaseType,
        where_clause::{WhereClause, WhereOperator},
    },
    error::{FraiseQLError, Result},
    utils::casing::to_snake_case,
};

mod expressions;
mod where_clause;

#[cfg(test)]
mod tests;

/// Aggregate query with bind parameters instead of escaped string literals.
///
/// Produced by [`AggregationSqlGenerator::generate_parameterized`].  Pass `sql`
/// and `params` directly to [`crate::db::DatabaseAdapter::execute_parameterized_aggregate`].
#[derive(Debug, Clone)]
pub struct ParameterizedAggregationSql {
    /// SQL with `$N` (PostgreSQL), `?` (MySQL / SQLite), or `@P1` (SQL Server) placeholders.
    pub sql:    String,
    /// Bind parameters in placeholder order.
    pub params: Vec<serde_json::Value>,
}

/// Aggregation SQL generator
pub struct AggregationSqlGenerator {
    database_type: DatabaseType,
}

impl AggregationSqlGenerator {
    /// Create new SQL generator for specific database
    #[must_use]
    pub const fn new(database_type: DatabaseType) -> Self {
        Self { database_type }
    }

    /// Generate JSONB extraction SQL with per-database path escaping.
    ///
    /// Each database uses a different string literal syntax for JSON paths.
    /// Single quotes or other metacharacters in `path` could otherwise break
    /// out of the string literal and inject arbitrary SQL. The per-database
    /// escape functions from `fraiseql_db::path_escape` are applied here as
    /// a second line of defence after schema allowlist validation in the planner.
    pub(super) fn jsonb_extract_sql(&self, jsonb_column: &str, path: &str) -> String {
        match self.database_type {
            DatabaseType::PostgreSQL => {
                let escaped = escape_postgres_jsonb_segment(path);
                format!("{}->>'{}' ", jsonb_column, escaped)
            },
            DatabaseType::MySQL => {
                // escape_mysql_json_path returns "$.escaped_segment"
                let escaped = escape_mysql_json_path(&[path.to_owned()]);
                format!("JSON_UNQUOTE(JSON_EXTRACT({}, '{}'))", jsonb_column, escaped)
            },
            DatabaseType::SQLite => {
                // escape_sqlite_json_path returns "$.escaped_segment"
                let escaped = escape_sqlite_json_path(&[path.to_owned()]);
                format!("json_extract({}, '{}')", jsonb_column, escaped)
            },
            DatabaseType::SQLServer => {
                // escape_sqlserver_json_path returns "$.escaped_segment"
                let escaped = escape_sqlserver_json_path(&[path.to_owned()]);
                format!("JSON_VALUE({}, '{}')", jsonb_column, escaped)
            },
        }
    }

    /// Convert WhereOperator to SQL operator
    pub(super) const fn operator_to_sql(&self, operator: &WhereOperator) -> &'static str {
        match operator {
            WhereOperator::Eq => "=",
            WhereOperator::Neq => "!=",
            WhereOperator::Gt => ">",
            WhereOperator::Gte => ">=",
            WhereOperator::Lt => "<",
            WhereOperator::Lte => "<=",
            WhereOperator::In => "IN",
            WhereOperator::Nin => "NOT IN",
            WhereOperator::Like | WhereOperator::Contains => "LIKE",
            WhereOperator::Ilike | WhereOperator::Icontains => {
                match self.database_type {
                    DatabaseType::PostgreSQL => "ILIKE",
                    _ => "LIKE", // Other databases use LIKE with UPPER/LOWER
                }
            },
            WhereOperator::Startswith => "LIKE",
            WhereOperator::Istartswith => match self.database_type {
                DatabaseType::PostgreSQL => "ILIKE",
                _ => "LIKE",
            },
            WhereOperator::Endswith => "LIKE",
            WhereOperator::Iendswith => match self.database_type {
                DatabaseType::PostgreSQL => "ILIKE",
                _ => "LIKE",
            },
            _ => "=", // Safe default for other operators
        }
    }

    /// Quote a validated field alias/column name using the database-appropriate identifier syntax.
    ///
    /// Field names arrive here after `OrderByClause::validate_field_name` has verified they
    /// match `[_A-Za-z][_0-9A-Za-z]*`, so no delimiter-escaping is required — but quoting
    /// still protects against SQL reserved words (`order`, `count`, `group`, `select`, …)
    /// that would break unquoted ORDER BY clauses.
    pub(super) fn quote_identifier(&self, name: &str) -> String {
        match self.database_type {
            DatabaseType::MySQL => quote_mysql_identifier(name),
            DatabaseType::SQLServer => quote_sqlserver_identifier(name),
            // PostgreSQL and SQLite both use double-quote syntax.
            DatabaseType::PostgreSQL | DatabaseType::SQLite => quote_postgres_identifier(name),
        }
    }

    /// Escape a string value for embedding inside a SQL string literal.
    ///
    /// MySQL treats backslash as an escape character in string literals by default
    /// (unless `NO_BACKSLASH_ESCAPES` sql_mode is set). Backslashes must be doubled
    /// before single-quote escaping to prevent injection via sequences like `\';`.
    ///
    /// Null bytes (`\x00`) are stripped before escaping. PostgreSQL rejects null
    /// bytes in string literals with "invalid byte sequence for encoding", which
    /// would surface as a confusing database error. Stripping them produces
    /// deterministic SQL regardless of the database's null-byte handling.
    pub(super) fn escape_sql_string(&self, s: &str) -> String {
        // Strip null bytes — never valid in SQL string literals.
        let without_nulls: std::borrow::Cow<str> = if s.contains('\0') {
            s.replace('\0', "").into()
        } else {
            s.into()
        };
        if matches!(self.database_type, DatabaseType::MySQL) {
            // Escape backslashes first, then single quotes.
            without_nulls.replace('\\', "\\\\").replace('\'', "''")
        } else {
            // Standard SQL: only double single quotes.
            without_nulls.replace('\'', "''")
        }
    }

    /// Returns the bind-parameter placeholder for position `index` (0-based).
    pub(super) fn placeholder(&self, index: usize) -> String {
        match self.database_type {
            DatabaseType::PostgreSQL => format!("${}", index + 1),
            DatabaseType::SQLServer => format!("@P{}", index + 1),
            _ => "?".to_string(),
        }
    }

    /// If `value` is non-NULL, appends it to `params` and returns the placeholder.
    ///
    /// `NULL` is emitted inline as the literal `NULL`; it cannot be reliably
    /// parameterized across all four database drivers in the same way.
    pub(super) fn emit_value_param(
        &self,
        value: &serde_json::Value,
        params: &mut Vec<serde_json::Value>,
    ) -> String {
        if matches!(value, serde_json::Value::Null) {
            return "NULL".to_string();
        }
        let idx = params.len();
        params.push(value.clone());
        self.placeholder(idx)
    }

    /// Build a LIKE pattern string, escape LIKE metacharacters with `!`, bind it as a
    /// parameter, and return the placeholder.  Returns `(placeholder, needs_escape_clause)`
    /// where `needs_escape_clause` indicates whether `ESCAPE '!'` should be appended to
    /// the SQL fragment.
    pub(super) fn emit_like_pattern_param(
        &self,
        operator: &WhereOperator,
        value: &str,
        params: &mut Vec<serde_json::Value>,
    ) -> (String, bool) {
        // Strip null bytes before binding (same invariant as escape_sql_string).
        let clean: String = if value.contains('\0') {
            value.replace('\0', "")
        } else {
            value.to_string()
        };

        let (pattern, needs_escape) = match operator {
            WhereOperator::Contains | WhereOperator::Icontains => {
                let esc = clean.replace('!', "!!").replace('%', "!%").replace('_', "!_");
                (format!("%{esc}%"), true)
            },
            WhereOperator::Startswith | WhereOperator::Istartswith => {
                let esc = clean.replace('!', "!!").replace('%', "!%").replace('_', "!_");
                (format!("{esc}%"), true)
            },
            WhereOperator::Endswith | WhereOperator::Iendswith => {
                let esc = clean.replace('!', "!!").replace('%', "!%").replace('_', "!_");
                (format!("%{esc}"), true)
            },
            // Like / Ilike: caller controls wildcards — bind as-is.
            _ => (clean, false),
        };

        let ph = self.emit_value_param(&serde_json::Value::String(pattern), params);
        (ph, needs_escape)
    }

    /// Generate a parameterized aggregate SQL query.
    ///
    /// All user-supplied string values in `WHERE` and `HAVING` clauses are emitted as
    /// bind placeholders (`$N` / `?` / `@P1` depending on the database dialect) rather
    /// than being embedded as escaped string literals.  Numeric, boolean, and `NULL`
    /// values are still inlined since they carry no injection risk.
    ///
    /// # Errors
    ///
    /// Returns error if SQL generation fails (unknown aggregate function, etc.).
    pub fn generate_parameterized(
        &self,
        plan: &AggregationPlan,
    ) -> Result<ParameterizedAggregationSql> {
        let mut params: Vec<serde_json::Value> = Vec::new();

        let select_sql =
            self.build_select_clause(&plan.group_by_expressions, &plan.aggregate_expressions)?;
        // SAFETY: table_name is schema-derived (from CompiledSchema, validated at compile
        // time), not user input.
        let from_sql = format!("FROM {}", plan.request.table_name);

        let where_sql = if let Some(ref wc) = plan.request.where_clause {
            self.build_where_clause_parameterized(wc, &plan.metadata, &mut params)?
        } else {
            String::new()
        };

        let group_sql = if !plan.group_by_expressions.is_empty() {
            self.build_group_by_clause(&plan.group_by_expressions)?
        } else {
            String::new()
        };

        let having_sql =
            self.build_having_clause_parameterized(&plan.having_conditions, &mut params)?;

        let order_sql = if !plan.request.order_by.is_empty() {
            self.build_order_by_clause(&plan.request.order_by)?
        } else {
            String::new()
        };

        let mut parts: Vec<&str> = vec![
            &select_sql,
            &from_sql,
            &where_sql,
            &group_sql,
            &having_sql,
            &order_sql,
        ];
        parts.retain(|s| !s.is_empty());

        let mut sql = parts.join("\n");

        if let Some(limit) = plan.request.limit {
            sql.push_str(&format!("\nLIMIT {limit}"));
        }
        if let Some(offset) = plan.request.offset {
            sql.push_str(&format!("\nOFFSET {offset}"));
        }

        Ok(ParameterizedAggregationSql { sql, params })
    }
}
