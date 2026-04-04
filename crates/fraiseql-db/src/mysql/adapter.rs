//! MySQL database adapter implementation.

use std::fmt::Write;

use async_trait::async_trait;
use fraiseql_error::{FraiseQLError, Result};
use sqlx::{
    Column, Row, TypeInfo,
    mysql::{MySqlPool, MySqlPoolOptions, MySqlRow},
};

use super::where_generator::MySqlWhereGenerator;
use crate::{
    dialect::MySqlDialect,
    identifier::quote_mysql_identifier,
    traits::{
        CursorValue, DatabaseAdapter, RelayDatabaseAdapter, RelayPageResult, SupportsMutations,
    },
    types::{
        DatabaseType, JsonbValue, PoolMetrics,
        sql_hints::{OrderByClause, OrderDirection},
    },
    where_clause::WhereClause,
};

/// MySQL database adapter with connection pooling.
///
/// Uses `sqlx` for connection pooling and async queries.
///
/// # Example
///
/// ```no_run
/// use fraiseql_db::mysql::MySqlAdapter;
/// use fraiseql_db::{DatabaseAdapter, WhereClause, WhereOperator};
/// use serde_json::json;
///
/// # async fn example() -> Result<(), Box<dyn std::error::Error>> {
/// // Create adapter with connection string
/// let adapter = MySqlAdapter::new("mysql://user:password@localhost/mydb").await?;
///
/// // Execute query
/// let where_clause = WhereClause::Field {
///     path: vec!["email".to_string()],
///     operator: WhereOperator::Icontains,
///     value: json!("example.com"),
/// };
///
/// let results = adapter
///     .execute_where_query("v_user", Some(&where_clause), Some(10), None, None)
///     .await?;
///
/// println!("Found {} users", results.len());
/// # Ok(())
/// # }
/// ```
#[derive(Clone)]
pub struct MySqlAdapter {
    pool: MySqlPool,
}

impl MySqlAdapter {
    /// Create new MySQL adapter with default pool configuration.
    ///
    /// # Arguments
    ///
    /// * `connection_string` - MySQL connection string (e.g., "mysql://user:pass@localhost/mydb")
    ///
    /// # Errors
    ///
    /// Returns `FraiseQLError::ConnectionPool` if pool creation fails.
    pub async fn new(connection_string: &str) -> Result<Self> {
        Self::with_pool_size(connection_string, 10).await
    }

    /// Create new MySQL adapter with custom pool configuration.
    ///
    /// # Arguments
    ///
    /// * `connection_string` - MySQL connection string
    /// * `min_size` - Minimum pool size
    /// * `max_size` - Maximum number of connections in pool
    ///
    /// # Errors
    ///
    /// Returns `FraiseQLError::ConnectionPool` if pool creation fails.
    pub async fn with_pool_config(
        connection_string: &str,
        min_size: u32,
        max_size: u32,
    ) -> Result<Self> {
        let pool = MySqlPoolOptions::new()
            .min_connections(min_size)
            .max_connections(max_size)
            .connect(connection_string)
            .await
            .map_err(|e| FraiseQLError::ConnectionPool {
                message: format!("Failed to create MySQL connection pool: {e}"),
            })?;

        Ok(Self { pool })
    }

    /// Create new MySQL adapter with custom pool size.
    ///
    /// # Arguments
    ///
    /// * `connection_string` - MySQL connection string
    /// * `max_size` - Maximum number of connections in pool
    ///
    /// # Errors
    ///
    /// Returns `FraiseQLError::ConnectionPool` if pool creation fails.
    pub async fn with_pool_size(connection_string: &str, max_size: u32) -> Result<Self> {
        let pool = MySqlPoolOptions::new()
            .max_connections(max_size)
            .connect(connection_string)
            .await
            .map_err(|e| FraiseQLError::ConnectionPool {
                message: format!("Failed to create MySQL connection pool: {e}"),
            })?;

        // Test connection
        sqlx::query("SELECT 1")
            .fetch_one(&pool)
            .await
            .map_err(|e| FraiseQLError::Database {
                message:   format!("Failed to connect to MySQL database: {e}"),
                sql_state: None,
            })?;

        Ok(Self { pool })
    }

    /// Execute raw SQL query and return JSONB rows.
    async fn execute_raw(
        &self,
        sql: &str,
        params: Vec<serde_json::Value>,
    ) -> Result<Vec<JsonbValue>> {
        // Build the query with dynamic parameters
        let mut query = sqlx::query(sql);

        for param in &params {
            query = match param {
                serde_json::Value::String(s) => query.bind(s.clone()),
                serde_json::Value::Number(n) => {
                    if let Some(i) = n.as_i64() {
                        query.bind(i)
                    } else if let Some(f) = n.as_f64() {
                        query.bind(f)
                    } else {
                        query.bind(n.to_string())
                    }
                },
                serde_json::Value::Bool(b) => query.bind(*b),
                serde_json::Value::Null => query.bind(Option::<String>::None),
                serde_json::Value::Array(_) | serde_json::Value::Object(_) => {
                    query.bind(param.to_string())
                },
            };
        }

        let rows: Vec<MySqlRow> = query.fetch_all(&self.pool).await.map_err(|e| {
            let sql_state = if let sqlx::Error::Database(ref db_err) = e {
                db_err.code().and_then(|c| c.parse::<u16>().ok()).and_then(map_mysql_error_code)
            } else {
                None
            };
            FraiseQLError::Database {
                message: format!("MySQL query execution failed: {e}"),
                sql_state,
            }
        })?;

        let results = rows
            .into_iter()
            .map(|row| {
                // MySQL stores JSON in a column, get it directly
                let data: serde_json::Value =
                    row.try_get("data").unwrap_or(serde_json::Value::Null);
                JsonbValue::new(data)
            })
            .collect();

        Ok(results)
    }
}

/// Escape a JSON value for inline use in a MySQL text-protocol query.
///
/// Strings are wrapped in single quotes with internal `'` doubled (`''`),
/// which is the ANSI SQL escaping method and is safe regardless of the
/// `NO_BACKSLASH_ESCAPES` SQL mode.
fn mysql_escape_json_value(v: &serde_json::Value) -> String {
    match v {
        serde_json::Value::String(s) => format!("'{}'", s.replace('\'', "''")),
        serde_json::Value::Number(n) => n.to_string(),
        serde_json::Value::Bool(b) => if *b { "1" } else { "0" }.to_string(),
        serde_json::Value::Null => "NULL".to_string(),
        serde_json::Value::Array(_) | serde_json::Value::Object(_) => {
            format!("'{}'", v.to_string().replace('\'', "''"))
        },
    }
}

// Reason: DatabaseAdapter is defined with #[async_trait]; all implementations must match
// its transformed method signatures to satisfy the trait contract
// async_trait: dyn-dispatch required; remove when RTN + Send is stable (RFC 3425)
#[async_trait]
impl DatabaseAdapter for MySqlAdapter {
    /// # Errors
    ///
    /// Returns [`FraiseQLError::Database`] if the MySQL query fails or the result
    /// cannot be deserialized as JSONB.
    async fn execute_with_projection(
        &self,
        view: &str,
        projection: Option<&crate::types::SqlProjectionHint>,
        where_clause: Option<&WhereClause>,
        limit: Option<u32>,
        offset: Option<u32>,
        _order_by: Option<&[OrderByClause]>,
    ) -> Result<Vec<JsonbValue>> {
        // If no projection provided, fall back to standard query
        if projection.is_none() {
            return self.execute_where_query(view, where_clause, limit, offset, None).await;
        }

        let Some(projection) = projection else {
            // Reason: unreachable — `is_none()` check above returns early
            unreachable!("projection is Some; None case returned above");
        };

        // Build SQL with MySQL-specific JSON_OBJECT projection
        // The projection_template contains the SELECT clause with JSON_OBJECT calls
        // e.g., "JSON_OBJECT('id', data->'$.id', 'email', data->'$.email')"
        let mut sql = format!(
            "SELECT {} FROM {}",
            projection.projection_template,
            quote_mysql_identifier(view)
        );

        // Add WHERE clause if present
        let params: Vec<serde_json::Value> = if let Some(clause) = where_clause {
            let generator = super::where_generator::MySqlWhereGenerator::new(MySqlDialect);
            let (where_sql, where_params) = generator.generate(clause)?;
            sql.push_str(" WHERE ");
            sql.push_str(&where_sql);
            where_params
        } else {
            Vec::new()
        };

        // Add LIMIT/OFFSET — MySQL requires LIMIT before OFFSET.
        // Reason (expect below): fmt::Write for String is infallible.
        match (limit, offset) {
            (Some(lim), Some(off)) => {
                write!(sql, " LIMIT {lim} OFFSET {off}").expect("write to String");
            },
            (Some(lim), None) => {
                write!(sql, " LIMIT {lim}").expect("write to String");
            },
            (None, Some(off)) => {
                // MySQL requires LIMIT before OFFSET; use max u64 as "unlimited"
                write!(sql, " LIMIT 18446744073709551615 OFFSET {off}").expect("write to String");
            },
            (None, None) => {},
        }

        // Execute the query
        self.execute_raw(&sql, params).await
    }

    /// # Errors
    ///
    /// Returns [`FraiseQLError::Validation`] if WHERE clause generation fails, or
    /// [`FraiseQLError::Database`] if the MySQL query fails.
    async fn execute_where_query(
        &self,
        view: &str,
        where_clause: Option<&WhereClause>,
        limit: Option<u32>,
        offset: Option<u32>,
        _order_by: Option<&[OrderByClause]>,
    ) -> Result<Vec<JsonbValue>> {
        // Build base query
        let mut sql = format!("SELECT data FROM {}", quote_mysql_identifier(view));

        // Add WHERE clause if present
        let mut params: Vec<serde_json::Value> = if let Some(clause) = where_clause {
            let generator = MySqlWhereGenerator::new(MySqlDialect);
            let (where_sql, where_params) = generator.generate(clause)?;
            sql.push_str(" WHERE ");
            sql.push_str(&where_sql);
            where_params
        } else {
            Vec::new()
        };

        // Add LIMIT and OFFSET
        // Note: MySQL requires LIMIT when using OFFSET, so we use a large number for "unlimited"
        match (limit, offset) {
            (Some(lim), Some(off)) => {
                sql.push_str(" LIMIT ? OFFSET ?");
                params.push(serde_json::Value::Number(lim.into()));
                params.push(serde_json::Value::Number(off.into()));
            },
            (Some(lim), None) => {
                sql.push_str(" LIMIT ?");
                params.push(serde_json::Value::Number(lim.into()));
            },
            (None, Some(off)) => {
                // MySQL requires LIMIT with OFFSET; use large number for "unlimited"
                // MySQL's max is 18446744073709551615, but we use a practical large value
                sql.push_str(" LIMIT 18446744073709551615 OFFSET ?");
                params.push(serde_json::Value::Number(off.into()));
            },
            (None, None) => {},
        }

        self.execute_raw(&sql, params).await
    }

    fn database_type(&self) -> DatabaseType {
        DatabaseType::MySQL
    }

    async fn health_check(&self) -> Result<()> {
        sqlx::query("SELECT 1").fetch_one(&self.pool).await.map_err(|e| {
            FraiseQLError::Database {
                message:   format!("MySQL health check failed: {e}"),
                sql_state: None,
            }
        })?;

        Ok(())
    }

    #[allow(clippy::cast_possible_truncation)] // Reason: value is bounded; truncation cannot occur in practice
    // Reason: pool sizes are always ≪ u32::MAX in practice
    fn pool_metrics(&self) -> PoolMetrics {
        let size = self.pool.size();
        let idle = self.pool.num_idle();

        PoolMetrics {
            total_connections:  size,
            idle_connections:   idle as u32,
            active_connections: size - idle as u32,
            waiting_requests:   0, // sqlx doesn't expose waiting count
        }
    }

    /// # Security
    ///
    /// `sql` **must** be compiler-generated. Never pass user-supplied strings
    /// directly — doing so would open SQL-injection vulnerabilities.
    async fn execute_raw_query(
        &self,
        sql: &str,
    ) -> Result<Vec<std::collections::HashMap<String, serde_json::Value>>> {
        let rows: Vec<MySqlRow> = sqlx::query(sql).fetch_all(&self.pool).await.map_err(|e| {
            let sql_state = if let sqlx::Error::Database(ref db_err) = e {
                db_err.code().map(|c| c.into_owned())
            } else {
                None
            };
            FraiseQLError::Database {
                message: format!("MySQL query execution failed: {e}"),
                sql_state,
            }
        })?;

        // Convert each row to HashMap<String, Value>.
        //
        // Use `column.type_info().name()` for deterministic extraction instead of
        // trial-and-error probing.  This correctly handles computed columns (COUNT,
        // SUM, window functions) whose wire type is NEWDECIMAL — `try_get::<i64>`
        // fails for NEWDECIMAL, but `try_get::<String>` succeeds and can be parsed
        // into a JSON number.
        let results: Vec<std::collections::HashMap<String, serde_json::Value>> = rows
            .into_iter()
            .map(|row| {
                let mut map = std::collections::HashMap::new();
                for column in row.columns() {
                    let col = column.name().to_string();
                    let type_name = column.type_info().name();
                    let value = match type_name {
                        "BOOLEAN" | "BIT" => row
                            .try_get::<bool, _>(col.as_str())
                            .map(|v| serde_json::json!(v))
                            .unwrap_or(serde_json::Value::Null),
                        "TINYINT(1)" => row
                            .try_get::<bool, _>(col.as_str())
                            .map(|v| serde_json::json!(v))
                            .unwrap_or(serde_json::Value::Null),
                        "BIGINT UNSIGNED" => row
                            .try_get::<u64, _>(col.as_str())
                            .map(|v| serde_json::json!(v))
                            .unwrap_or(serde_json::Value::Null),
                        "BIGINT" | "INT" | "INT UNSIGNED" | "MEDIUMINT" | "MEDIUMINT UNSIGNED"
                        | "SMALLINT" | "SMALLINT UNSIGNED" | "TINYINT" | "TINYINT UNSIGNED" => row
                            .try_get::<i64, _>(col.as_str())
                            .map(|v| serde_json::json!(v))
                            .unwrap_or(serde_json::Value::Null),
                        "DOUBLE" | "FLOAT" => row
                            .try_get::<f64, _>(col.as_str())
                            .map(|v| serde_json::json!(v))
                            .unwrap_or(serde_json::Value::Null),
                        "NEWDECIMAL" | "DECIMAL" => row
                            .try_get::<String, _>(col.as_str())
                            .map(|v| {
                                serde_json::from_str(&v).unwrap_or_else(|_| serde_json::json!(v))
                            })
                            .unwrap_or(serde_json::Value::Null),
                        "JSON" => row
                            .try_get::<serde_json::Value, _>(col.as_str())
                            .unwrap_or(serde_json::Value::Null),
                        // VARCHAR, CHAR, TEXT, DATE, DATETIME, TIMESTAMP, BLOB, etc.
                        _ => row
                            .try_get::<String, _>(col.as_str())
                            .map(|v| {
                                serde_json::from_str(&v).unwrap_or_else(|_| serde_json::json!(v))
                            })
                            .unwrap_or(serde_json::Value::Null),
                    };
                    map.insert(col, value);
                }
                map
            })
            .collect();

        Ok(results)
    }

    async fn execute_parameterized_aggregate(
        &self,
        sql: &str,
        params: &[serde_json::Value],
    ) -> Result<Vec<std::collections::HashMap<String, serde_json::Value>>> {
        let mut query = sqlx::query(sql);
        for param in params {
            query = match param {
                serde_json::Value::String(s) => query.bind(s.clone()),
                serde_json::Value::Number(n) => {
                    if let Some(i) = n.as_i64() {
                        query.bind(i)
                    } else if let Some(f) = n.as_f64() {
                        query.bind(f)
                    } else {
                        query.bind(n.to_string())
                    }
                },
                serde_json::Value::Bool(b) => query.bind(*b),
                serde_json::Value::Null => query.bind(Option::<String>::None),
                serde_json::Value::Array(_) | serde_json::Value::Object(_) => {
                    query.bind(param.to_string())
                },
            };
        }

        let rows: Vec<MySqlRow> = query.fetch_all(&self.pool).await.map_err(|e| {
            let sql_state = if let sqlx::Error::Database(ref db_err) = e {
                db_err.code().map(|c| c.into_owned())
            } else {
                None
            };
            FraiseQLError::Database {
                message: format!("MySQL parameterized aggregate query failed: {e}"),
                sql_state,
            }
        })?;

        let results = rows
            .into_iter()
            .map(|row| {
                let mut map = std::collections::HashMap::new();
                for column in row.columns() {
                    let col = column.name().to_string();
                    let type_name = column.type_info().name();
                    let value = match type_name {
                        "BOOLEAN" | "BIT" | "TINYINT(1)" => row
                            .try_get::<bool, _>(col.as_str())
                            .map(|v| serde_json::json!(v))
                            .unwrap_or(serde_json::Value::Null),
                        "BIGINT UNSIGNED" => row
                            .try_get::<u64, _>(col.as_str())
                            .map(|v| serde_json::json!(v))
                            .unwrap_or(serde_json::Value::Null),
                        "BIGINT" | "INT" | "INT UNSIGNED" | "MEDIUMINT" | "MEDIUMINT UNSIGNED"
                        | "SMALLINT" | "SMALLINT UNSIGNED" | "TINYINT" | "TINYINT UNSIGNED" => row
                            .try_get::<i64, _>(col.as_str())
                            .map(|v| serde_json::json!(v))
                            .unwrap_or(serde_json::Value::Null),
                        "DOUBLE" | "FLOAT" => row
                            .try_get::<f64, _>(col.as_str())
                            .map(|v| serde_json::json!(v))
                            .unwrap_or(serde_json::Value::Null),
                        "NEWDECIMAL" | "DECIMAL" => row
                            .try_get::<String, _>(col.as_str())
                            .map(|v| {
                                serde_json::from_str(&v).unwrap_or_else(|_| serde_json::json!(v))
                            })
                            .unwrap_or(serde_json::Value::Null),
                        "JSON" => row
                            .try_get::<serde_json::Value, _>(col.as_str())
                            .unwrap_or(serde_json::Value::Null),
                        _ => row
                            .try_get::<String, _>(col.as_str())
                            .map(|v| {
                                serde_json::from_str(&v).unwrap_or_else(|_| serde_json::json!(v))
                            })
                            .unwrap_or(serde_json::Value::Null),
                    };
                    map.insert(col, value);
                }
                map
            })
            .collect();

        Ok(results)
    }

    async fn execute_function_call(
        &self,
        function_name: &str,
        args: &[serde_json::Value],
    ) -> Result<Vec<std::collections::HashMap<String, serde_json::Value>>> {
        // MySQL's binary protocol (prepared statements) does not reliably return
        // row data from stored procedures that use CALL.  Use text protocol
        // (`raw_sql`) with inline-escaped parameters instead.
        let escaped: Vec<String> = args.iter().map(mysql_escape_json_value).collect();
        let call_sql =
            format!("CALL {}({})", quote_mysql_identifier(function_name), escaped.join(", "));
        let rows: Vec<MySqlRow> =
            sqlx::raw_sql(&call_sql).fetch_all(&self.pool).await.map_err(|e| {
                let sql_state = if let sqlx::Error::Database(ref db_err) = e {
                    db_err.code().map(|c| c.into_owned())
                } else {
                    None
                };
                FraiseQLError::Database {
                    message: format!("MySQL stored procedure call failed ({function_name}): {e}"),
                    sql_state,
                }
            })?;

        let results = rows
            .into_iter()
            .map(|row| {
                let mut map = std::collections::HashMap::new();
                for column in row.columns() {
                    let col = column.name().to_string();
                    let type_name = column.type_info().name();
                    let value = match type_name {
                        "BOOLEAN" | "BIT" | "TINYINT(1)" => row
                            .try_get::<bool, _>(col.as_str())
                            .map(|v| serde_json::json!(v))
                            .unwrap_or(serde_json::Value::Null),
                        "BIGINT UNSIGNED" => row
                            .try_get::<u64, _>(col.as_str())
                            .map(|v| serde_json::json!(v))
                            .unwrap_or(serde_json::Value::Null),
                        "BIGINT" | "INT" | "INT UNSIGNED" | "MEDIUMINT" | "MEDIUMINT UNSIGNED"
                        | "SMALLINT" | "SMALLINT UNSIGNED" | "TINYINT" | "TINYINT UNSIGNED" => row
                            .try_get::<i64, _>(col.as_str())
                            .map(|v| serde_json::json!(v))
                            .unwrap_or(serde_json::Value::Null),
                        "DOUBLE" | "FLOAT" => row
                            .try_get::<f64, _>(col.as_str())
                            .map(|v| serde_json::json!(v))
                            .unwrap_or(serde_json::Value::Null),
                        "NEWDECIMAL" | "DECIMAL" => row
                            .try_get::<String, _>(col.as_str())
                            .map(|v| {
                                serde_json::from_str(&v).unwrap_or_else(|_| serde_json::json!(v))
                            })
                            .unwrap_or(serde_json::Value::Null),
                        "JSON" => row
                            .try_get::<serde_json::Value, _>(col.as_str())
                            .unwrap_or(serde_json::Value::Null),
                        // VARCHAR, CHAR, TEXT, DATE, DATETIME, TIMESTAMP, BLOB, etc.
                        _ => row
                            .try_get::<String, _>(col.as_str())
                            .map(|v| {
                                serde_json::from_str(&v).unwrap_or_else(|_| serde_json::json!(v))
                            })
                            .unwrap_or(serde_json::Value::Null),
                    };
                    map.insert(col, value);
                }
                map
            })
            .collect();

        Ok(results)
    }

    async fn explain_query(
        &self,
        sql: &str,
        _params: &[serde_json::Value],
    ) -> Result<serde_json::Value> {
        use sqlx::Row as _;

        // Defense-in-depth: compiler-generated SQL should never contain a
        // semicolon, but guard against it to prevent statement injection.
        if sql.contains(';') {
            return Err(FraiseQLError::Validation {
                message: "EXPLAIN SQL must be a single statement".into(),
                path:    None,
            });
        }
        let explain_sql = format!("EXPLAIN FORMAT=JSON {sql}");
        let row: sqlx::mysql::MySqlRow = sqlx::query(&explain_sql)
            .fetch_one(&self.pool)
            .await
            .map_err(|e| FraiseQLError::Database {
                message:   format!("MySQL EXPLAIN failed: {e}"),
                sql_state: None,
            })?;

        let raw: String = row.try_get(0).map_err(|e| FraiseQLError::Database {
            message:   format!("Failed to read MySQL EXPLAIN output: {e}"),
            sql_state: None,
        })?;

        serde_json::from_str(&raw).map_err(|e| FraiseQLError::Database {
            message:   format!("Failed to parse MySQL EXPLAIN JSON: {e}"),
            sql_state: None,
        })
    }
}

/// Map MySQL error numbers to SQLSTATE strings for uniform error reporting.
///
/// MySQL error numbers are numeric codes from the MySQL error reference.
/// This mapping covers the most common integrity and transaction errors.
///
/// # Arguments
///
/// * `code` - MySQL error number (e.g., 1062 for duplicate entry)
///
/// # Returns
///
/// Returns a SQLSTATE string if a mapping exists, or `None` for unmapped codes.
pub(super) fn map_mysql_error_code(code: u16) -> Option<String> {
    let sqlstate = match code {
        // 1062: Duplicate entry for key (unique constraint violation)
        // 1169: Unique constraint violation (alternate code)
        1062 | 1169 => "23505",
        // 1048: Column cannot be null (NOT NULL violation)
        1048 => "23502",
        // 1451: Cannot delete or update a parent row (FK parent violation)
        // 1452: Cannot add or update a child row (FK child violation)
        1451 | 1452 => "23503",
        // 1205: Lock wait timeout exceeded — treat as serialization failure
        1205 => "40001",
        // 1213: Deadlock found when trying to get lock
        1213 => "40001",
        _ => return None,
    };
    Some(sqlstate.to_string())
}

// ── MySQL relay helpers ────────────────────────────────────────────────────

/// Build the `ORDER BY` clause for a relay page query.
///
/// Custom `order_by` columns come first (using MySQL JSON path syntax), then the
/// cursor column is appended as a stable tiebreaker.  The sort direction is
/// flipped for backward queries (inner subquery) and restored by the outer
/// `ORDER BY _relay_cursor ASC` wrapper.
fn build_mysql_relay_order_sql(
    quoted_col: &str,
    order_by: Option<&[OrderByClause]>,
    forward: bool,
) -> String {
    let mut parts: Vec<String> = Vec::new();

    if let Some(clauses) = order_by {
        for c in clauses {
            let dir = match (c.direction, forward) {
                (OrderDirection::Asc, true) | (OrderDirection::Desc, false) => "ASC",
                (OrderDirection::Desc, true) | (OrderDirection::Asc, false) => "DESC",
            };
            // JSON_UNQUOTE(JSON_EXTRACT(data, '$.field')) — field names are validated
            // GraphQL identifiers, which cannot contain ' or other SQL-special chars.
            let escaped = c.field.replace('\'', "''");
            parts.push(format!("JSON_UNQUOTE(JSON_EXTRACT(data, '$.{escaped}')) {dir}"));
        }
    }

    let cursor_dir = if forward { "ASC" } else { "DESC" };
    parts.push(format!("{quoted_col} {cursor_dir}"));
    format!(" ORDER BY {}", parts.join(", "))
}

/// Combine cursor and user WHERE conditions into a single `WHERE` clause fragment.
fn build_mysql_relay_where(cursor_sql: Option<&str>, user_sql: Option<&str>) -> String {
    match (cursor_sql, user_sql) {
        (None, None) => String::new(),
        (Some(c), None) => format!(" WHERE {c}"),
        (None, Some(u)) => format!(" WHERE ({u})"),
        (Some(c), Some(u)) => format!(" WHERE {c} AND ({u})"),
    }
}

impl MySqlAdapter {
    /// Execute a parameterized SQL query and return the first column of the first row as `i64`.
    ///
    /// Used internally for `COUNT(*)` queries in relay pagination.
    async fn execute_count_query(&self, sql: &str, params: Vec<serde_json::Value>) -> Result<u64> {
        let mut query = sqlx::query(sql);

        for param in &params {
            query = match param {
                serde_json::Value::String(s) => query.bind(s.clone()),
                serde_json::Value::Number(n) => {
                    if let Some(i) = n.as_i64() {
                        query.bind(i)
                    } else if let Some(f) = n.as_f64() {
                        query.bind(f)
                    } else {
                        query.bind(n.to_string())
                    }
                },
                serde_json::Value::Bool(b) => query.bind(*b),
                serde_json::Value::Null => query.bind(Option::<String>::None),
                serde_json::Value::Array(_) | serde_json::Value::Object(_) => {
                    query.bind(param.to_string())
                },
            };
        }

        let row: MySqlRow =
            query.fetch_one(&self.pool).await.map_err(|e| FraiseQLError::Database {
                message:   format!("MySQL COUNT query failed: {e}"),
                sql_state: None,
            })?;

        // COUNT(*) returns BIGINT UNSIGNED in MySQL; try i64 first (covers most real counts).
        let cnt: u64 = if let Ok(v) = row.try_get::<i64, _>(0) {
            v.cast_unsigned()
        } else {
            row.try_get::<u64, _>(0).unwrap_or_default()
        };

        Ok(cnt)
    }
}

impl SupportsMutations for MySqlAdapter {}

// ── RelayDatabaseAdapter ───────────────────────────────────────────────────

impl RelayDatabaseAdapter for MySqlAdapter {
    /// Execute keyset (cursor-based) pagination against a JSONB view.
    ///
    /// # MySQL specifics
    ///
    /// - Identifiers are quoted with backticks.
    /// - Parameters use positional `?` placeholders (not numbered like `$1`).
    /// - JSON field access in ORDER BY uses `JSON_UNQUOTE(JSON_EXTRACT(data, '$.field'))`.
    /// - UUID cursors are compared as `CHAR(36)` strings — no explicit cast needed.
    /// - Backward pagination uses an inner DESC subquery re-sorted ASC by the outer query.
    ///
    /// # `totalCount` semantics
    ///
    /// When `include_total_count` is `true`, a separate `SELECT COUNT(*) FROM {view}
    /// WHERE {user_filter}` is issued **without** the cursor condition. This implements
    /// the Relay spec requirement that `totalCount` reflects the full connection size,
    /// not the filtered page.
    async fn execute_relay_page(
        &self,
        view: &str,
        cursor_column: &str,
        after: Option<CursorValue>,
        before: Option<CursorValue>,
        limit: u32,
        forward: bool,
        where_clause: Option<&WhereClause>,
        order_by: Option<&[OrderByClause]>,
        include_total_count: bool,
    ) -> Result<RelayPageResult> {
        let quoted_view = quote_mysql_identifier(view);
        let quoted_col = quote_mysql_identifier(cursor_column);

        // ── Cursor condition ───────────────────────────────────────────────
        let active_cursor = if forward { after } else { before };
        let (cursor_where_sql, cursor_param): (Option<String>, Option<serde_json::Value>) =
            match active_cursor {
                None => (None, None),
                Some(CursorValue::Int64(pk)) => {
                    let op = if forward { ">" } else { "<" };
                    (
                        Some(format!("{quoted_col} {op} ?")),
                        Some(serde_json::Value::Number(pk.into())),
                    )
                },
                Some(CursorValue::Uuid(uuid)) => {
                    // MySQL UUIDs stored as CHAR(36); string comparison works for canonical form.
                    let op = if forward { ">" } else { "<" };
                    (Some(format!("{quoted_col} {op} ?")), Some(serde_json::Value::String(uuid)))
                },
            };

        // ── User WHERE clause ──────────────────────────────────────────────
        let (user_where_sql, user_where_params): (Option<String>, Vec<serde_json::Value>) =
            if let Some(clause) = where_clause {
                let generator = MySqlWhereGenerator::new(MySqlDialect);
                let (sql, params) = generator.generate(clause)?;
                (Some(sql), params)
            } else {
                (None, Vec::new())
            };

        // ── ORDER BY ───────────────────────────────────────────────────────
        let order_sql = build_mysql_relay_order_sql(&quoted_col, order_by, forward);

        // ── Combined page WHERE ────────────────────────────────────────────
        let page_where_sql =
            build_mysql_relay_where(cursor_where_sql.as_deref(), user_where_sql.as_deref());

        // ── Page params: [cursor?, ...user_where_params, limit] ────────────
        let mut page_params: Vec<serde_json::Value> = Vec::new();
        if let Some(cp) = cursor_param {
            page_params.push(cp);
        }
        page_params.extend(user_where_params.iter().cloned());
        page_params.push(serde_json::Value::Number(limit.into()));

        // ── Page SQL ───────────────────────────────────────────────────────
        //
        // Backward pagination: inner DESC query + outer re-sorts ASC so the
        // caller always receives rows in ascending cursor order.
        let page_sql = if forward {
            format!("SELECT data FROM {quoted_view}{page_where_sql}{order_sql} LIMIT ?")
        } else {
            let inner = format!(
                "SELECT data, {quoted_col} AS _relay_cursor \
                 FROM {quoted_view}{page_where_sql}{order_sql} LIMIT ?"
            );
            format!("SELECT data FROM ({inner}) _relay_page ORDER BY _relay_cursor ASC")
        };

        let rows = self.execute_raw(&page_sql, page_params).await?;

        // ── Count query (cursor-independent per Relay spec) ────────────────
        let total_count = if include_total_count {
            let (count_sql, count_params) = if let Some(u_sql) = &user_where_sql {
                (
                    format!("SELECT COUNT(*) FROM {quoted_view} WHERE ({u_sql})"),
                    user_where_params.clone(),
                )
            } else {
                (format!("SELECT COUNT(*) FROM {quoted_view}"), vec![])
            };
            Some(self.execute_count_query(&count_sql, count_params).await?)
        } else {
            None
        };

        Ok(RelayPageResult { rows, total_count })
    }
}

#[cfg(test)]
mod unit_tests {
    use super::*;
    use crate::{identifier::quote_mysql_identifier, types::DatabaseType};

    // Unit tests for MySQL adapter internals.
    // These tests do NOT require a live MySQL connection.
    // Integration tests in the `tests` module below cover actual query execution.

    // ========================================================================
    // DatabaseType Invariant
    // ========================================================================

    #[test]
    fn mysql_database_type_as_str() {
        assert_eq!(DatabaseType::MySQL.as_str(), "mysql");
        assert_eq!(DatabaseType::MySQL.to_string(), "mysql");
    }

    #[test]
    fn mysql_database_type_differs_from_others() {
        assert_ne!(DatabaseType::MySQL, DatabaseType::PostgreSQL);
        assert_ne!(DatabaseType::MySQL, DatabaseType::SQLite);
        assert_ne!(DatabaseType::MySQL, DatabaseType::SQLServer);
    }

    // ========================================================================
    // MySQL Error Code Mapping
    // ========================================================================

    #[test]
    fn mysql_error_1062_maps_to_unique_violation() {
        assert_eq!(map_mysql_error_code(1062), Some("23505".to_string()));
    }

    #[test]
    fn mysql_error_1169_also_maps_to_unique_violation() {
        assert_eq!(map_mysql_error_code(1169), Some("23505".to_string()));
    }

    #[test]
    fn mysql_error_1048_maps_to_not_null_violation() {
        assert_eq!(map_mysql_error_code(1048), Some("23502".to_string()));
    }

    #[test]
    fn mysql_error_1451_maps_to_foreign_key_violation() {
        assert_eq!(map_mysql_error_code(1451), Some("23503".to_string()));
    }

    #[test]
    fn mysql_error_1452_also_maps_to_foreign_key_violation() {
        assert_eq!(map_mysql_error_code(1452), Some("23503".to_string()));
    }

    #[test]
    fn mysql_error_1205_maps_to_lock_timeout() {
        assert_eq!(map_mysql_error_code(1205), Some("40001".to_string()));
    }

    #[test]
    fn mysql_error_1213_maps_to_deadlock() {
        assert_eq!(map_mysql_error_code(1213), Some("40001".to_string()));
    }

    #[test]
    fn unknown_mysql_error_code_returns_none() {
        assert_eq!(map_mysql_error_code(9999), None);
        assert_eq!(map_mysql_error_code(0), None);
        assert_eq!(map_mysql_error_code(1064), None);
    }

    // ========================================================================
    // Relay Helper Functions
    // ========================================================================

    #[test]
    fn relay_where_both_none_returns_empty() {
        assert_eq!(build_mysql_relay_where(None, None), "");
    }

    #[test]
    fn relay_where_cursor_only() {
        assert_eq!(build_mysql_relay_where(Some("`id` > ?"), None), " WHERE `id` > ?");
    }

    #[test]
    fn relay_where_user_only_wraps_in_parens() {
        assert_eq!(build_mysql_relay_where(None, Some("active = ?")), " WHERE (active = ?)");
    }

    #[test]
    fn relay_where_both_combines_with_and() {
        assert_eq!(
            build_mysql_relay_where(Some("`id` > ?"), Some("active = ?")),
            " WHERE `id` > ? AND (active = ?)"
        );
    }

    #[test]
    fn relay_order_sql_forward_no_custom_order() {
        let quoted_col = quote_mysql_identifier("id");
        let result = build_mysql_relay_order_sql(&quoted_col, None, true);
        assert_eq!(result, " ORDER BY `id` ASC");
    }

    #[test]
    fn relay_order_sql_backward_no_custom_order() {
        let quoted_col = quote_mysql_identifier("id");
        let result = build_mysql_relay_order_sql(&quoted_col, None, false);
        assert_eq!(result, " ORDER BY `id` DESC");
    }

    #[test]
    fn relay_order_sql_forward_with_desc_custom_order() {
        use crate::types::sql_hints::{OrderByClause, OrderDirection};
        let quoted_col = quote_mysql_identifier("id");
        let order_by = vec![OrderByClause {
            field:     "created_at".to_string(),
            direction: OrderDirection::Desc,
        }];
        let result = build_mysql_relay_order_sql(&quoted_col, Some(&order_by), true);
        assert!(result.contains("JSON_UNQUOTE(JSON_EXTRACT(data, '$.created_at')) DESC"));
        assert!(result.ends_with("`id` ASC"));
    }

    #[test]
    fn relay_order_sql_backward_flips_asc_to_desc() {
        use crate::types::sql_hints::{OrderByClause, OrderDirection};
        let quoted_col = quote_mysql_identifier("id");
        let order_by = vec![OrderByClause {
            field:     "created_at".to_string(),
            direction: OrderDirection::Asc,
        }];
        let result = build_mysql_relay_order_sql(&quoted_col, Some(&order_by), false);
        assert!(result.contains("JSON_UNQUOTE(JSON_EXTRACT(data, '$.created_at')) DESC"));
        assert!(result.ends_with("`id` DESC"));
    }

    // ========================================================================
    // MySQL Identifier Quoting
    // ========================================================================

    #[test]
    fn mysql_identifier_wraps_in_backticks() {
        assert_eq!(quote_mysql_identifier("v_user"), "`v_user`");
    }

    #[test]
    fn mysql_identifier_escapes_embedded_backtick() {
        assert_eq!(quote_mysql_identifier("bad`name"), "`bad``name`");
    }

    #[test]
    fn mysql_identifier_schema_qualified_name() {
        assert_eq!(quote_mysql_identifier("mydb.v_user"), "`mydb`.`v_user`");
    }

    // ── EP-6: Connection pool failure paths ───────────────────────────────────

    #[tokio::test]
    async fn test_new_with_malformed_url_returns_connection_pool_error() {
        // sqlx parses the URL immediately; an unparseable string fails before
        // any network I/O occurs and the error is mapped to ConnectionPool.
        let result = MySqlAdapter::new("not-a-mysql-url").await;
        assert!(result.is_err(), "expected error for malformed URL");
        let err = result.err().expect("error confirmed above");
        assert!(
            matches!(err, FraiseQLError::ConnectionPool { .. }),
            "expected ConnectionPool error for malformed URL, got: {err:?}"
        );
    }

    #[tokio::test]
    async fn test_with_pool_size_malformed_url_returns_connection_pool_error() {
        let result = MySqlAdapter::with_pool_size("://bad-url", 1).await;
        assert!(result.is_err(), "expected error for bad URL");
        let err = result.err().expect("error confirmed above");
        assert!(
            matches!(err, FraiseQLError::ConnectionPool { .. }),
            "expected ConnectionPool error for bad URL with custom pool size, got: {err:?}"
        );
    }
}

#[cfg(all(test, feature = "test-mysql"))]
mod tests {
    use super::*;

    // Note: These tests require a running MySQL instance with test data.
    // Run with: cargo test --features test-mysql -p fraiseql-core db::mysql::adapter

    const TEST_DB_URL: &str =
        "mysql://fraiseql_test:fraiseql_test_password@localhost:3307/test_fraiseql";

    #[tokio::test]
    async fn test_adapter_creation() {
        let adapter = MySqlAdapter::new(TEST_DB_URL).await.expect("Failed to create MySQL adapter");

        let metrics = adapter.pool_metrics();
        assert!(metrics.total_connections > 0);
        assert_eq!(adapter.database_type(), DatabaseType::MySQL);
    }

    #[tokio::test]
    async fn test_health_check() {
        let adapter = MySqlAdapter::new(TEST_DB_URL).await.expect("Failed to create MySQL adapter");

        adapter.health_check().await.expect("Health check failed");
    }

    #[tokio::test]
    async fn test_parameterized_limit_only() {
        let adapter = MySqlAdapter::new(TEST_DB_URL).await.expect("Failed to create MySQL adapter");

        let results = adapter
            .execute_where_query("v_user", None, Some(2), None, None)
            .await
            .expect("Failed to execute query");

        assert!(results.len() <= 2);
    }

    #[tokio::test]
    async fn test_parameterized_offset_only() {
        let adapter = MySqlAdapter::new(TEST_DB_URL).await.expect("Failed to create MySQL adapter");

        let results = adapter
            .execute_where_query("v_user", None, None, Some(1), None)
            .await
            .expect("Failed to execute query");

        assert!(!results.is_empty());
    }

    #[tokio::test]
    async fn test_parameterized_limit_and_offset() {
        let adapter = MySqlAdapter::new(TEST_DB_URL).await.expect("Failed to create MySQL adapter");

        let results = adapter
            .execute_where_query("v_user", None, Some(2), Some(1), None)
            .await
            .expect("Failed to execute query");

        assert!(results.len() <= 2);
    }
}
