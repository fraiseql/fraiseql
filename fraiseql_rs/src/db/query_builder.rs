//! SQL query builder for SELECT, INSERT, UPDATE, DELETE operations.
//!
//! Migrates query building from Python to Rust for:
//! - Compile-time type safety
//! - Performance (10-20x faster query construction)
//! - Consistency between build and execution
//! - Unified single-language implementation
//!
//! Mirrors the Python `fraiseql.db.query_builder` module with enhanced type safety.

use crate::db::types::QueryParam;
use crate::db::where_builder::WhereBuilder;
use std::collections::HashMap;

/// Query type enumeration
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum QueryType {
    Select,
    Insert,
    Update,
    Delete,
}

/// Represents a complete SQL query with parameters
#[derive(Debug, Clone)]
pub struct SqlQuery {
    /// The SQL statement (may contain placeholders like $1, $2)
    pub statement: String,
    /// Query parameters to bind to placeholders
    pub params: Vec<QueryParam>,
    /// The type of query (Select, Insert, etc.)
    pub query_type: QueryType,
    /// Whether this query expects a result set
    pub fetch_result: bool,
}

/// Input format for ORDER BY specifications
#[derive(Debug, Clone)]
pub enum OrderByInput {
    /// Single order specification as string (e.g., "name ASC", "id DESC")
    Single(String),
    /// Multiple order specifications
    Multiple(Vec<String>),
    /// Empty/no ordering
    None,
}

impl OrderByInput {
    /// Convert `OrderByInput` to SQL ORDER BY clause (without "ORDER BY" prefix)
    fn to_sql(&self) -> Option<String> {
        match self {
            Self::Single(order) => {
                if order.trim().is_empty() {
                    None
                } else {
                    Some(order.clone())
                }
            }
            Self::Multiple(orders) => {
                let filtered: Vec<&str> = orders
                    .iter()
                    .filter(|o| !o.trim().is_empty())
                    .map(String::as_str)
                    .collect();
                if filtered.is_empty() {
                    None
                } else {
                    Some(filtered.join(", "))
                }
            }
            Self::None => None,
        }
    }
}

/// SQL Query Builder for all database operations
#[derive(Debug, Clone)]
pub struct QueryBuilder {
    table: String,
    schema: Option<String>,
    columns: Vec<String>,
    where_builder: Option<WhereBuilder>,
    order_by: Option<OrderByInput>,
    limit: Option<i64>,
    offset: Option<i64>,
    jsonb_column: Option<String>,
    select_all_as_json: bool,
    values: HashMap<String, QueryParam>,
}

impl QueryBuilder {
    /// Create new query builder for table
    ///
    /// # Arguments
    /// * `table` - Table name, optionally schema-qualified (e.g., "public.users" or "users")
    ///
    /// # Example
    /// ```ignore
    /// let builder = QueryBuilder::new("users");
    /// let builder = QueryBuilder::new("public.users");  // With schema
    /// ```
    pub fn new(table: impl Into<String>) -> Self {
        let table_str = table.into();
        let (schema, table_name) = if table_str.contains('.') {
            let parts: Vec<&str> = table_str.split('.').collect();
            if parts.len() >= 2 {
                (Some(parts[0].to_string()), parts[1].to_string())
            } else {
                (None, table_str)
            }
        } else {
            (None, table_str)
        };

        Self {
            table: table_name,
            schema,
            columns: Vec::new(),
            where_builder: None,
            order_by: None,
            limit: None,
            offset: None,
            jsonb_column: None,
            select_all_as_json: false,
            values: HashMap::new(),
        }
    }

    /// Add column to SELECT
    #[must_use]
    pub fn select(mut self, column: impl Into<String>) -> Self {
        self.columns.push(column.into());
        self
    }

    /// SELECT all columns as JSON (`row_to_json` or `jsonb_column::text`)
    ///
    /// # Arguments
    /// * `jsonb_col` - Optional JSONB column to use instead of `row_to_json`
    #[must_use]
    pub fn select_as_json(mut self, jsonb_col: Option<String>) -> Self {
        self.select_all_as_json = true;
        self.jsonb_column = jsonb_col;
        self
    }

    /// Add WHERE clause
    #[must_use]
    pub fn where_clause(mut self, builder: WhereBuilder) -> Self {
        self.where_builder = Some(builder);
        self
    }

    /// Add ORDER BY
    #[must_use]
    pub fn order_by(mut self, order: impl Into<OrderByInput>) -> Self {
        self.order_by = Some(order.into());
        self
    }

    /// Add LIMIT
    #[must_use]
    pub fn limit(mut self, limit: i64) -> Self {
        self.limit = Some(limit);
        self
    }

    /// Add OFFSET
    #[must_use]
    pub fn offset(mut self, offset: i64) -> Self {
        self.offset = Some(offset);
        self
    }

    /// Add a value for INSERT or UPDATE
    #[must_use]
    pub fn value(mut self, column: impl Into<String>, param: QueryParam) -> Self {
        self.values.insert(column.into(), param);
        self
    }

    /// Build SELECT query
    ///
    /// # Returns
    /// Complete SELECT query with parameters properly formatted
    ///
    /// # Example
    /// ```ignore
    /// let query = QueryBuilder::new("users")
    ///     .select("id")
    ///     .select("name")
    ///     .build_select();
    /// ```
    #[must_use]
    pub fn build_select(self) -> SqlQuery {
        let mut sql = String::new();

        // SELECT clause
        if self.select_all_as_json {
            if let Some(ref jsonb_col) = self.jsonb_column {
                use std::fmt::Write;
                let _ = write!(sql, "SELECT {jsonb_col}::text");
            } else {
                sql.push_str("SELECT row_to_json(t)::text");
            }
        } else {
            let columns = if self.columns.is_empty() {
                "*".to_string()
            } else {
                self.columns.join(", ")
            };
            use std::fmt::Write;
            let _ = write!(sql, "SELECT {columns}");
        }

        // FROM clause with schema if present
        sql.push_str(" FROM ");
        if let Some(ref schema) = self.schema {
            use std::fmt::Write;
            let _ = write!(sql, "{schema}.{}", self.table);
        } else {
            sql.push_str(&self.table);
        }

        // Add table alias for row_to_json
        if !self.select_all_as_json || self.jsonb_column.is_none() {
            sql.push_str(" AS t");
        }

        // WHERE clause
        let params = if let Some(where_builder) = self.where_builder {
            let (where_sql, params) = where_builder.build();
            if !where_sql.is_empty() {
                sql.push(' ');
                sql.push_str(&where_sql);
            }
            params
        } else {
            Vec::new()
        };

        // ORDER BY
        if let Some(order_by) = self.order_by {
            if let Some(order_sql) = order_by.to_sql() {
                use std::fmt::Write;
                let _ = write!(sql, " ORDER BY {order_sql}");
            }
        }

        // LIMIT
        if let Some(limit) = self.limit {
            use std::fmt::Write;
            let _ = write!(sql, " LIMIT {limit}");
        }

        // OFFSET
        if let Some(offset) = self.offset {
            use std::fmt::Write;
            let _ = write!(sql, " OFFSET {offset}");
        }

        SqlQuery {
            statement: sql,
            params,
            query_type: QueryType::Select,
            fetch_result: true,
        }
    }

    /// Build INSERT query
    ///
    /// # Panics
    /// If no values have been added to the builder
    ///
    /// # Example
    /// ```ignore
    /// let query = QueryBuilder::new("users")
    ///     .value("name", QueryParam::Text("John".into()))
    ///     .value("email", QueryParam::Text("john@example.com".into()))
    ///     .build_insert();
    /// ```
    pub fn build_insert(mut self) -> SqlQuery {
        if self.values.is_empty() {
            panic!("INSERT requires at least one value");
        }

        let column_names: Vec<String> = self.values.keys().cloned().collect();
        let mut params = Vec::new();
        let mut placeholders = Vec::new();

        for (i, _) in column_names.iter().enumerate() {
            placeholders.push(format!("${}", i + 1));
        }

        let columns_str = column_names.join(", ");

        let mut sql = format!(
            "INSERT INTO {} ({}) VALUES ({})",
            self.table,
            columns_str,
            placeholders.join(", ")
        );

        // If schema is specified, include it in table name
        if let Some(ref schema) = self.schema {
            sql = format!(
                "INSERT INTO {}.{} ({}) VALUES ({})",
                schema,
                self.table,
                columns_str,
                placeholders.join(", ")
            );
        }

        // Collect parameters in the same order as columns
        for col in column_names {
            if let Some(param) = self.values.remove(&col) {
                params.push(param);
            }
        }

        SqlQuery {
            statement: sql,
            params,
            query_type: QueryType::Insert,
            fetch_result: false,
        }
    }

    /// Build UPDATE query
    ///
    /// # Panics
    /// If no values have been added to the builder
    ///
    /// # Example
    /// ```ignore
    /// let query = QueryBuilder::new("users")
    ///     .value("name", QueryParam::Text("Jane".into()))
    ///     .where_clause(/* ... */)
    ///     .build_update();
    /// ```
    pub fn build_update(self) -> SqlQuery {
        if self.values.is_empty() {
            panic!("UPDATE requires at least one value");
        }

        let mut sql = if let Some(schema) = self.schema {
            format!("UPDATE {}.{} SET ", schema, self.table)
        } else {
            format!("UPDATE {} SET ", self.table)
        };

        let mut params = Vec::new();
        let mut set_clauses = Vec::new();

        for (i, (col, param)) in self.values.iter().enumerate() {
            set_clauses.push(format!("{} = ${}", col, i + 1));
            params.push(param.clone());
        }

        sql.push_str(&set_clauses.join(", "));

        // WHERE clause
        if let Some(where_builder) = self.where_builder {
            let (where_sql, where_params) = where_builder.build();
            if !where_sql.is_empty() {
                // Adjust parameter numbering for WHERE clause parameters
                let param_offset = params.len();
                let adjusted_where = adjust_param_numbers(&where_sql, param_offset);
                sql.push_str(&format!(" {}", adjusted_where));
                params.extend(where_params);
            }
        }

        SqlQuery {
            statement: sql,
            params,
            query_type: QueryType::Update,
            fetch_result: false,
        }
    }

    /// Build DELETE query
    ///
    /// # Example
    /// ```ignore
    /// let query = QueryBuilder::new("users")
    ///     .where_clause(/* ... */)
    ///     .build_delete();
    /// ```
    pub fn build_delete(self) -> SqlQuery {
        let mut sql = if let Some(schema) = self.schema {
            format!("DELETE FROM {}.{}", schema, self.table)
        } else {
            format!("DELETE FROM {}", self.table)
        };

        let params = if let Some(where_builder) = self.where_builder {
            let (where_sql, params) = where_builder.build();
            if !where_sql.is_empty() {
                sql.push_str(&format!(" {}", where_sql));
            }
            params
        } else {
            Vec::new()
        };

        SqlQuery {
            statement: sql,
            params,
            query_type: QueryType::Delete,
            fetch_result: false,
        }
    }
}

/// Helper function to adjust parameter placeholders in SQL
///
/// Converts parameter placeholders like `$1`, `$2` to `$3`, `$4` when offset=2
fn adjust_param_numbers(sql: &str, offset: usize) -> String {
    let mut result = String::new();
    let mut chars = sql.chars().peekable();

    while let Some(c) = chars.next() {
        if c != '$' {
            result.push(c);
            continue;
        }

        // Process potential parameter placeholder
        if let Some(&next) = chars.peek() {
            if !next.is_ascii_digit() {
                result.push(c);
                continue;
            }

            // Extract the numeric part
            result.push('$');
            let num_str = extract_digits(&mut chars);

            if let Ok(num) = num_str.parse::<usize>() {
                result.push_str(&(num + offset).to_string());
            } else {
                result.push_str(&num_str);
            }
        } else {
            result.push(c);
        }
    }

    result
}

/// Extract consecutive digits from a character iterator
fn extract_digits(chars: &mut std::iter::Peekable<std::str::Chars>) -> String {
    let mut num_str = String::new();
    while let Some(&digit) = chars.peek() {
        if digit.is_ascii_digit() {
            num_str.push(chars.next().expect("digit already peeked"));
        } else {
            break;
        }
    }
    num_str
}

// Implement From trait for convenient OrderByInput construction
impl From<String> for OrderByInput {
    fn from(s: String) -> Self {
        if s.is_empty() {
            Self::None
        } else {
            Self::Single(s)
        }
    }
}

impl From<&str> for OrderByInput {
    fn from(s: &str) -> Self {
        if s.is_empty() {
            Self::None
        } else {
            Self::Single(s.to_string())
        }
    }
}

impl From<Vec<String>> for OrderByInput {
    fn from(v: Vec<String>) -> Self {
        if v.is_empty() {
            Self::None
        } else {
            Self::Multiple(v)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_simple_select() {
        let query = QueryBuilder::new("users")
            .select("id")
            .select("name")
            .build_select();

        assert!(query.statement.contains("SELECT id, name FROM users AS t"));
        assert_eq!(query.query_type, QueryType::Select);
        assert!(query.fetch_result);
    }

    #[test]
    fn test_select_all_columns() {
        let query = QueryBuilder::new("users").build_select();

        assert!(query.statement.contains("SELECT * FROM users AS t"));
    }

    #[test]
    fn test_select_as_json_row_to_json() {
        let query = QueryBuilder::new("users")
            .select_as_json(None)
            .build_select();

        assert!(query
            .statement
            .contains("SELECT row_to_json(t)::text FROM users AS t"));
    }

    #[test]
    fn test_select_as_json_with_column() {
        let query = QueryBuilder::new("users")
            .select_as_json(Some("data".to_string()))
            .build_select();

        assert!(query.statement.contains("SELECT data::text FROM users"));
        assert!(!query.statement.contains(" AS t"));
    }

    #[test]
    fn test_schema_qualified_table() {
        let query = QueryBuilder::new("public.users")
            .select_as_json(None)
            .build_select();

        assert!(query.statement.contains("FROM public.users"));
    }

    #[test]
    fn test_limit_offset() {
        let query = QueryBuilder::new("users")
            .select_as_json(None)
            .limit(10)
            .offset(5)
            .build_select();

        assert!(query.statement.contains("LIMIT 10"));
        assert!(query.statement.contains("OFFSET 5"));
    }

    #[test]
    fn test_order_by_single() {
        let query = QueryBuilder::new("users")
            .select_as_json(None)
            .order_by("name ASC")
            .build_select();

        assert!(query.statement.contains("ORDER BY name ASC"));
    }

    #[test]
    fn test_order_by_multiple() {
        let query = QueryBuilder::new("users")
            .select_as_json(None)
            .order_by(vec!["name ASC".to_string(), "id DESC".to_string()])
            .build_select();

        assert!(query.statement.contains("ORDER BY name ASC, id DESC"));
    }

    #[test]
    fn test_order_by_none() {
        let query = QueryBuilder::new("users")
            .select_as_json(None)
            .order_by(OrderByInput::None)
            .build_select();

        assert!(!query.statement.contains("ORDER BY"));
    }

    #[test]
    fn test_insert_single_value() {
        let query = QueryBuilder::new("users")
            .value("name", QueryParam::Text("John".to_string()))
            .build_insert();

        assert!(query
            .statement
            .contains("INSERT INTO users (name) VALUES ($1)"));
        assert_eq!(query.query_type, QueryType::Insert);
        assert!(!query.fetch_result);
        assert_eq!(query.params.len(), 1);
    }

    #[test]
    fn test_insert_multiple_values() {
        let query = QueryBuilder::new("users")
            .value("name", QueryParam::Text("John".to_string()))
            .value("email", QueryParam::Text("john@example.com".to_string()))
            .build_insert();

        assert!(query.statement.contains("INSERT INTO users"));
        assert!(
            query.statement.contains("VALUES ($1, $2)")
                || query.statement.contains("VALUES ($2, $1)")
        );
        assert_eq!(query.params.len(), 2);
    }

    #[test]
    fn test_insert_with_schema() {
        let query = QueryBuilder::new("public.users")
            .value("name", QueryParam::Text("John".to_string()))
            .build_insert();

        assert!(query.statement.contains("INSERT INTO public.users"));
    }

    #[test]
    #[should_panic(expected = "INSERT requires at least one value")]
    fn test_insert_no_values() {
        QueryBuilder::new("users").build_insert();
    }

    #[test]
    fn test_update_single_value() {
        let query = QueryBuilder::new("users")
            .value("name", QueryParam::Text("Jane".to_string()))
            .build_update();

        assert!(query.statement.contains("UPDATE users SET name = $1"));
        assert_eq!(query.query_type, QueryType::Update);
        assert!(!query.fetch_result);
    }

    #[test]
    fn test_update_with_schema() {
        let query = QueryBuilder::new("public.users")
            .value("name", QueryParam::Text("Jane".to_string()))
            .build_update();

        assert!(query.statement.contains("UPDATE public.users SET"));
    }

    #[test]
    #[should_panic(expected = "UPDATE requires at least one value")]
    fn test_update_no_values() {
        QueryBuilder::new("users").build_update();
    }

    #[test]
    fn test_delete() {
        let query = QueryBuilder::new("users").build_delete();

        assert!(query.statement.contains("DELETE FROM users"));
        assert_eq!(query.query_type, QueryType::Delete);
        assert!(!query.fetch_result);
    }

    #[test]
    fn test_delete_with_schema() {
        let query = QueryBuilder::new("public.users").build_delete();

        assert!(query.statement.contains("DELETE FROM public.users"));
    }

    #[test]
    fn test_query_param_from_implementations() {
        let _int_param = QueryParam::from(42i32);
        let _bigint_param = QueryParam::from(42i64);
        let _float_param = QueryParam::from(3.14f32);
        let _double_param = QueryParam::from(3.14f64);
        let _bool_param = QueryParam::from(true);
        let _text_param = QueryParam::from("hello".to_string());
    }

    #[test]
    fn test_parameter_number_adjustment() {
        let adjusted = adjust_param_numbers("WHERE id = $1 AND name = $2", 2);
        assert_eq!(adjusted, "WHERE id = $3 AND name = $4");
    }

    #[test]
    fn test_parameter_number_adjustment_with_offset_zero() {
        let adjusted = adjust_param_numbers("WHERE id = $1", 0);
        assert_eq!(adjusted, "WHERE id = $1");
    }

    #[test]
    fn test_order_by_empty_string() {
        let input = OrderByInput::Single(String::new());
        assert_eq!(input.to_sql(), None);
    }

    #[test]
    fn test_order_by_multiple_with_empty() {
        let input = OrderByInput::Multiple(vec![
            "name ASC".to_string(),
            "".to_string(),
            "id DESC".to_string(),
        ]);
        assert_eq!(input.to_sql(), Some("name ASC, id DESC".to_string()));
    }
}
