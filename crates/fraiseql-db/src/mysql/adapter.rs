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
    order_by::append_order_by,
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
        // Build the query, binding each value as a parameter (never interpolating
        // values into the SQL text).
        let mut query = sqlx::query(sql);
        for param in &params {
            query = bind_mysql_json_arg(query, param);
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

/// Build a parameterized `CALL` statement: ``CALL `fn`(?, ?, …)`` with one `?`
/// placeholder per argument (empty parentheses for zero arguments).
///
/// The procedure name is identifier-quoted; argument *values* are bound
/// separately via [`bind_mysql_json_arg`] and never interpolated into the SQL
/// text. This is the injection-safe replacement for the former inline-escaping
/// path (which doubled `'` only and left `\` unescaped, allowing a backslash
/// breakout under MySQL's default SQL mode).
fn build_mysql_call_sql(function_name: &str, n_args: usize) -> String {
    let placeholders = vec!["?"; n_args].join(", ");
    format!("CALL {}({placeholders})", quote_mysql_identifier(function_name))
}

/// Bind one JSON value as a MySQL query parameter, mirroring the type coercion
/// used across the adapter: strings/bools/nulls bound directly, numbers as
/// `i64`/`f64` when representable (else their decimal text), and arrays/objects
/// as their JSON text. Values are bound, never spliced into SQL.
fn bind_mysql_json_arg<'q>(
    query: sqlx::query::Query<'q, sqlx::MySql, sqlx::mysql::MySqlArguments>,
    value: &serde_json::Value,
) -> sqlx::query::Query<'q, sqlx::MySql, sqlx::mysql::MySqlArguments> {
    match value {
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
        serde_json::Value::Array(_) | serde_json::Value::Object(_) => query.bind(value.to_string()),
    }
}

/// Decode one MySQL column to JSON, dispatched on its wire type
/// (`type_info().name()`) for deterministic extraction. Generic over the column
/// index so callers can address columns by ordinal (binary-protocol `CALL`
/// result sets) — see [`mysql_call_row_to_map`].
fn mysql_extract<I>(row: &MySqlRow, idx: I, type_name: &str) -> serde_json::Value
where
    I: sqlx::ColumnIndex<MySqlRow>,
{
    match type_name {
        "BOOLEAN" | "BIT" | "TINYINT(1)" => row
            .try_get::<bool, _>(idx)
            .map(|v| serde_json::json!(v))
            .unwrap_or(serde_json::Value::Null),
        "BIGINT UNSIGNED" => row
            .try_get::<u64, _>(idx)
            .map(|v| serde_json::json!(v))
            .unwrap_or(serde_json::Value::Null),
        "BIGINT" | "INT" | "INT UNSIGNED" | "MEDIUMINT" | "MEDIUMINT UNSIGNED" | "SMALLINT"
        | "SMALLINT UNSIGNED" | "TINYINT" | "TINYINT UNSIGNED" => row
            .try_get::<i64, _>(idx)
            .map(|v| serde_json::json!(v))
            .unwrap_or(serde_json::Value::Null),
        "DOUBLE" | "FLOAT" => row
            .try_get::<f64, _>(idx)
            .map(|v| serde_json::json!(v))
            .unwrap_or(serde_json::Value::Null),
        "NEWDECIMAL" | "DECIMAL" => row
            .try_get::<String, _>(idx)
            .map(|v| serde_json::from_str(&v).unwrap_or_else(|_| serde_json::json!(v)))
            .unwrap_or(serde_json::Value::Null),
        "JSON" => row.try_get::<serde_json::Value, _>(idx).unwrap_or(serde_json::Value::Null),
        // VARCHAR, CHAR, TEXT, DATE, DATETIME, TIMESTAMP, BLOB, etc.
        _ => row
            .try_get::<String, _>(idx)
            .map(|v| serde_json::from_str(&v).unwrap_or_else(|_| serde_json::json!(v)))
            .unwrap_or(serde_json::Value::Null),
    }
}

/// Convert a row from a `CALL` run over the binary protocol (`sqlx::query`) to a
/// `HashMap`, addressing columns **by ordinal**: a binary `CALL` result set's
/// columns are not addressable by name (`try_get` by name silently yields NULL).
/// Both the plain function-call path and the change-log outbox path run `CALL`
/// over the binary protocol with bound parameters, so both use this mapper.
fn mysql_call_row_to_map(row: &MySqlRow) -> std::collections::HashMap<String, serde_json::Value> {
    let mut map = std::collections::HashMap::new();
    for column in row.columns() {
        let value = mysql_extract(row, column.ordinal(), column.type_info().name());
        map.insert(column.name().to_string(), value);
    }
    map
}

/// INSERT one `tb_entity_change_log` outbox row from a parsed `mutation_response`
/// `row`, on the transaction's connection `conn` (the portable, multi-DB
/// counterpart of PostgreSQL's in-txn CTE). The changed-entity columns are read
/// from the procedure's own row; `object_type` falls back to the threaded GraphQL
/// return type when the row omits `entity_type`. `seq` fires from the table
/// default; `duration_ms`/`started_at` stay NULL (no request-scoped DB clock on
/// MySQL); `commit_time` is the app-clock write time.
async fn mysql_write_outbox_row(
    conn: &mut sqlx::MySqlConnection,
    row: &std::collections::HashMap<String, serde_json::Value>,
    changelog: &crate::traits::ChangeLogWrite<'_>,
) -> Result<()> {
    let insert_sql =
        crate::changelog::build_changelog_insert_sql("tb_entity_change_log", DatabaseType::MySQL);
    // Bind in CHANGELOG_PORTABLE_INSERT_COLUMNS order: object_type,
    // modification_type, object_id, object_data, updated_fields, cascade, tenant_id,
    // trace_id, schema_version, trace_context, actor_type, acting_for, commit_time.
    let object_type = row
        .get("entity_type")
        .and_then(serde_json::Value::as_str)
        .map_or_else(|| changelog.object_type.to_string(), str::to_string);
    let object_id = row.get("entity_id").and_then(serde_json::Value::as_str).map(str::to_string);
    let commit_time = chrono::Utc::now().format("%Y-%m-%d %H:%M:%S%.6f").to_string();

    sqlx::query(&insert_sql)
        .bind(object_type)
        .bind(changelog.modification_type.to_string())
        .bind(object_id)
        .bind(crate::changelog::json_column_text(row.get("entity")))
        .bind(crate::changelog::json_column_text(row.get("updated_fields")))
        .bind(crate::changelog::json_column_text(row.get("cascade")))
        .bind(changelog.tenant_id.map(|t| t.to_string()))
        .bind(changelog.trace_id.map(str::to_string))
        .bind(changelog.schema_version.map(str::to_string))
        .bind(changelog.trace_context.map(str::to_string))
        .bind(changelog.actor_type.map(str::to_string))
        .bind(changelog.acting_for.map(|u| u.to_string()))
        .bind(commit_time)
        .execute(conn)
        .await
        .map_err(|e| FraiseQLError::Database {
            message:   format!("MySQL change-log outbox INSERT failed: {e}"),
            sql_state: mysql_sql_state(&e),
        })?;
    Ok(())
}

/// Extract the MySQL `SQLSTATE` from a sqlx error, if any.
fn mysql_sql_state(e: &sqlx::Error) -> Option<String> {
    match e {
        sqlx::Error::Database(db_err) => db_err.code().map(|c| c.into_owned()),
        _ => None,
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
        order_by: Option<&[OrderByClause]>,
    ) -> Result<Vec<JsonbValue>> {
        // If no projection provided, fall back to standard query
        if projection.is_none() {
            return self.execute_where_query(view, where_clause, limit, offset, order_by).await;
        }

        let projection = projection.expect("projection is Some; None case returned above");

        // Build SQL with MySQL-specific JSON_OBJECT projection
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

        // ORDER BY must come before LIMIT/OFFSET.
        append_order_by(&mut sql, order_by, DatabaseType::MySQL)?;

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
        order_by: Option<&[OrderByClause]>,
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

        // ORDER BY must come before LIMIT/OFFSET.
        append_order_by(&mut sql, order_by, DatabaseType::MySQL)?;

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
        // Bind arguments as parameters on a prepared `CALL` rather than splicing
        // them into the SQL text — the injection-safe path. The CALL runs over the
        // binary protocol (`sqlx::query`), whose result set is addressable only by
        // ordinal, hence `mysql_call_row_to_map`.
        let call_sql = build_mysql_call_sql(function_name, args.len());
        let mut query = sqlx::query(&call_sql);
        for arg in args {
            query = bind_mysql_json_arg(query, arg);
        }
        let rows: Vec<MySqlRow> =
            query.fetch_all(&self.pool).await.map_err(|e| FraiseQLError::Database {
                message:   format!("MySQL stored procedure call failed ({function_name}): {e}"),
                sql_state: mysql_sql_state(&e),
            })?;

        let results = rows.iter().map(mysql_call_row_to_map).collect();

        Ok(results)
    }

    async fn execute_function_call_with_changelog(
        &self,
        function_name: &str,
        args: &[serde_json::Value],
        session_vars: &[(&str, &str)],
        changelog: Option<&crate::traits::ChangeLogWrite<'_>>,
    ) -> Result<Vec<std::collections::HashMap<String, serde_json::Value>>> {
        // No outbox write requested => identical to the session-affine default,
        // which for MySQL drops session_vars and delegates to execute_function_call.
        let Some(changelog) = changelog else {
            return self
                .execute_function_call_with_session(function_name, args, session_vars)
                .await;
        };

        // MySQL cannot reference a CALL result set in a following `INSERT ... SELECT`
        // (the PostgreSQL CTE path), so run the CALL in a transaction, parse its
        // `mutation_response` row in Rust, and INSERT the outbox row on the same
        // connection before commit — atomic with the mutation (the Change Spine
        // transactional outbox). The `Transaction` auto-rolls-back on drop, so an
        // early `?` (a raised procedure, a failed INSERT) leaves neither the mutation
        // nor the log row. `session_vars` are dropped — MySQL has no transaction-local
        // GUCs, so `duration_ms`/`started_at` are NULL on this path.
        //
        // The CALL runs over the BINARY protocol (`sqlx::query` with bound `?`
        // parameters), forming a `Send` future over a `&mut MySqlConnection` that the
        // connection-affine transaction requires. Its result columns are addressable
        // only by ordinal — hence `mysql_call_row_to_map`. Arguments are bound, never
        // spliced into the SQL text.
        let call_sql = build_mysql_call_sql(function_name, args.len());

        let mut tx = self.pool.begin().await.map_err(|e| FraiseQLError::Database {
            message:   format!("Failed to start MySQL change-log outbox transaction: {e}"),
            sql_state: None,
        })?;

        let mut call_query = sqlx::query(&call_sql);
        for arg in args {
            call_query = bind_mysql_json_arg(call_query, arg);
        }
        let rows: Vec<MySqlRow> =
            call_query.fetch_all(&mut *tx).await.map_err(|e| FraiseQLError::Database {
                message:   format!("MySQL stored procedure call failed ({function_name}): {e}"),
                sql_state: mysql_sql_state(&e),
            })?;
        let results: Vec<std::collections::HashMap<String, serde_json::Value>> =
            rows.iter().map(mysql_call_row_to_map).collect();

        // Write the outbox row only for an effective change (succeeded AND state_changed).
        if let Some(first) = results.first() {
            if crate::changelog::value_is_truthy(first.get("succeeded"))
                && crate::changelog::value_is_truthy(first.get("state_changed"))
            {
                mysql_write_outbox_row(tx.as_mut(), first, changelog).await?;
            }
        }

        tx.commit().await.map_err(|e| FraiseQLError::Database {
            message:   format!("Failed to commit MySQL change-log outbox transaction: {e}"),
            sql_state: None,
        })?;

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

    async fn query_stats(&self, limit: u32) -> Result<Vec<crate::types::QueryStatEntry>> {
        // Check if performance_schema is available
        let probe: std::result::Result<MySqlRow, _> = sqlx::query(
            "SELECT 1 FROM performance_schema.events_statements_summary_by_digest LIMIT 0",
        )
        .fetch_one(&self.pool)
        .await;
        if probe.is_err() {
            return Ok(vec![]);
        }

        let rows: Vec<MySqlRow> = sqlx::query(
            "SELECT \
                 DIGEST AS query_id, \
                 DIGEST_TEXT AS query_text, \
                 COUNT_STAR AS calls, \
                 SUM_TIMER_WAIT / 1000000000 AS total_exec_time_ms, \
                 AVG_TIMER_WAIT / 1000000000 AS mean_exec_time_ms, \
                 MIN_TIMER_WAIT / 1000000000 AS min_exec_time_ms, \
                 MAX_TIMER_WAIT / 1000000000 AS max_exec_time_ms, \
                 SUM_ROWS_SENT AS rows_returned, \
                 SUM_ROWS_EXAMINED, \
                 SUM_NO_INDEX_USED, \
                 SUM_NO_GOOD_INDEX_USED \
             FROM performance_schema.events_statements_summary_by_digest \
             WHERE DIGEST IS NOT NULL \
             ORDER BY SUM_TIMER_WAIT DESC \
             LIMIT ?",
        )
        .bind(limit)
        .fetch_all(&self.pool)
        .await
        .map_err(|e| FraiseQLError::Database {
            message:   format!("Failed to query performance_schema: {e}"),
            sql_state: None,
        })?;

        rows.iter()
            .map(|row| {
                let rows_examined: i64 = row.try_get("SUM_ROWS_EXAMINED").unwrap_or(0);
                let no_index: i64 = row.try_get("SUM_NO_INDEX_USED").unwrap_or(0);
                let no_good_index: i64 = row.try_get("SUM_NO_GOOD_INDEX_USED").unwrap_or(0);

                Ok(crate::types::QueryStatEntry {
                    query_id:           row.try_get::<String, _>("query_id").unwrap_or_default(),
                    query_text:         row.try_get::<String, _>("query_text").unwrap_or_default(),
                    calls:              row.try_get::<i64, _>("calls").unwrap_or(0).unsigned_abs(),
                    total_exec_time_ms: row.try_get("total_exec_time_ms").unwrap_or(0.0),
                    mean_exec_time_ms:  row.try_get("mean_exec_time_ms").unwrap_or(0.0),
                    min_exec_time_ms:   row.try_get("min_exec_time_ms").unwrap_or(0.0),
                    max_exec_time_ms:   row.try_get("max_exec_time_ms").unwrap_or(0.0),
                    rows_returned:      row
                        .try_get::<i64, _>("rows_returned")
                        .unwrap_or(0)
                        .unsigned_abs(),
                    cache_hit_ratio:    None,
                    database_specific:  serde_json::json!({
                        "sum_rows_examined": rows_examined,
                        "sum_no_index_used": no_index,
                        "sum_no_good_index_used": no_good_index,
                    }),
                })
            })
            .collect()
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

        Ok(RelayPageResult::new(rows, total_count))
    }
}

#[cfg(test)]
mod tests;
