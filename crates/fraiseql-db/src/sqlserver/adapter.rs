//! SQL Server database adapter implementation.
use async_trait::async_trait;
use bb8::Pool;
use bb8_tiberius::ConnectionManager;
use fraiseql_error::{FraiseQLError, Result};
use tiberius::Config;
use tracing;

use super::where_generator::SqlServerWhereGenerator;
use crate::{
    dialect::SqlServerDialect,
    identifier::quote_sqlserver_identifier,
    traits::{
        CursorValue, DatabaseAdapter, MutationCapable, RelayDatabaseAdapter, RelayPageResult,
    },
    types::{DatabaseType, JsonbValue, PoolMetrics, sql_hints::OrderByClause},
    where_clause::WhereClause,
};

/// Map an MSSQL server error code to the closest ANSI SQLSTATE string.
///
/// Returns `None` for codes that don't have a clear ANSI SQLSTATE mapping (e.g. MSSQL 701
/// "out of memory"), which is preferable to returning a vendor-namespaced or incorrect code.
///
/// | MSSQL Code | Meaning                         | SQLSTATE  |
/// |------------|---------------------------------|-----------|
/// | 2627       | Unique constraint violation     | 23505     |
/// | 2601       | Duplicate key (unique index)    | 23505     |
/// | 547        | Foreign key violation           | 23503     |
/// | 515        | NOT NULL violation              | 23502     |
/// | 1205       | Deadlock victim                 | 40001     |
/// | 8152       | String or binary data truncation| 22001     |
/// | 701        | Insufficient memory             | (unmapped)|
fn map_mssql_error_code(code: u32) -> Option<String> {
    let sqlstate = match code {
        // Unique constraint / duplicate key → ANSI unique violation
        2627 | 2601 => "23505",
        // NOT NULL violation → ANSI not-null violation
        515 => "23502",
        // Foreign key violation → ANSI FK violation (unchanged)
        547 => "23503",
        // Deadlock → ANSI serialization failure (NOT PostgreSQL-vendor 40P01)
        1205 => "40001",
        // String truncation (unchanged)
        8152 => "22001",
        // 701 (out of memory) has no ANSI equivalent; emit None rather than a PostgreSQL code
        _ => return None,
    };
    Some(sqlstate.to_string())
}

/// Hard upper bound on pool size to prevent connection exhaustion on the database server.
const MAX_POOL_SIZE: u32 = 200;

/// SQL Server database adapter with connection pooling.
///
/// Uses `tiberius` for native TDS protocol support and `bb8` for connection pooling.
///
/// # Example
///
/// ```no_run
/// use fraiseql_db::sqlserver::SqlServerAdapter;
/// use fraiseql_db::{DatabaseAdapter, WhereClause, WhereOperator};
/// use serde_json::json;
///
/// # async fn example() -> Result<(), Box<dyn std::error::Error>> {
/// // Create adapter with connection string
/// let adapter = SqlServerAdapter::new("server=localhost;database=mydb;user=sa;password=Password123").await?;
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
pub struct SqlServerAdapter {
    pool: Pool<ConnectionManager>,
}

impl SqlServerAdapter {
    /// Create new SQL Server adapter with default pool configuration.
    ///
    /// # Arguments
    ///
    /// * `connection_string` - SQL Server connection string (ADO.NET format)
    ///
    /// # Connection String Format
    ///
    /// ```text
    /// server=localhost;database=mydb;user=sa;password=Password123
    /// server=localhost,1433;database=mydb;integratedSecurity=true
    /// ```
    ///
    /// # Errors
    ///
    /// Returns `FraiseQLError::ConnectionPool` if pool creation fails.
    pub async fn new(connection_string: &str) -> Result<Self> {
        Self::with_pool_size(connection_string, 10).await
    }

    /// Create new SQL Server adapter with custom pool configuration.
    ///
    /// # Arguments
    ///
    /// * `connection_string` - SQL Server connection string
    /// * `min_size` - Minimum pool size (bb8 doesn't support this; connections are created
    ///   on-demand)
    /// * `max_size` - Maximum number of connections in pool
    ///
    /// # Note on min_size
    ///
    /// The bb8 connection pool used by this adapter creates connections on-demand
    /// rather than pre-creating a minimum pool size. The `min_size` parameter is accepted
    /// for API compatibility with other database adapters but does not affect behavior.
    /// If you need a minimum number of pre-created connections, consider:
    /// 1. Calling `warmup_connections()` after creating the pool
    /// 2. Increasing `max_size` to ensure sufficient capacity
    /// 3. Using a different connection pool strategy for your use case
    ///
    /// # Errors
    ///
    /// Returns `FraiseQLError::ConnectionPool` if pool creation fails.
    pub async fn with_pool_config(
        connection_string: &str,
        min_size: u32,
        max_size: u32,
    ) -> Result<Self> {
        if min_size > 0 {
            tracing::warn!(
                min_size,
                "SQL Server adapter does not support min_size parameter - connections are created \
                 on-demand. Consider warmup_connections() if pre-allocation is needed."
            );
        }
        Self::with_pool_size(connection_string, max_size).await
    }

    /// Create new SQL Server adapter with custom pool size.
    ///
    /// # Arguments
    ///
    /// * `connection_string` - SQL Server connection string
    /// * `max_size` - Maximum number of connections in pool
    ///
    /// # Errors
    ///
    /// Returns `FraiseQLError::ConnectionPool` if pool creation fails.
    pub async fn with_pool_size(connection_string: &str, max_size: u32) -> Result<Self> {
        if max_size == 0 || max_size > MAX_POOL_SIZE {
            return Err(FraiseQLError::Validation {
                message: format!("Pool size must be between 1 and {MAX_POOL_SIZE}, got {max_size}"),
                path:    None,
            });
        }

        // Parse connection string into tiberius Config
        let config = Config::from_ado_string(connection_string).map_err(|e| {
            FraiseQLError::ConnectionPool {
                message: format!("Invalid SQL Server connection string: {e}"),
            }
        })?;

        let manager = ConnectionManager::new(config);

        let pool = Pool::builder().max_size(max_size).build(manager).await.map_err(|e| {
            FraiseQLError::ConnectionPool {
                message: format!("Failed to create SQL Server connection pool: {e}"),
            }
        })?;

        // Test connection
        {
            let mut conn = pool.get().await.map_err(|e| FraiseQLError::ConnectionPool {
                message: format!("Failed to acquire connection: {e}"),
            })?;

            conn.simple_query("SELECT 1").await.map_err(|e| FraiseQLError::Database {
                message:   format!("Failed to connect to SQL Server database: {e}"),
                sql_state: None,
            })?;
        }

        Ok(Self { pool })
    }

    /// Execute raw SQL query and return JSONB rows.
    async fn execute_raw(
        &self,
        sql: &str,
        params: Vec<serde_json::Value>,
    ) -> Result<Vec<JsonbValue>> {
        let mut conn = self.pool.get().await.map_err(|e| FraiseQLError::ConnectionPool {
            message: format!("Failed to acquire connection: {e}"),
        })?;

        // Build and execute query
        // Note: tiberius doesn't support dynamic parameter binding like sqlx
        // We need to use simple_query for dynamic SQL or build the query differently
        let rows = if params.is_empty() {
            let result = conn.simple_query(sql).await.map_err(|e| FraiseQLError::Database {
                message:   format!("SQL Server query execution failed: {e}"),
                sql_state: e.code().and_then(map_mssql_error_code),
            })?;
            result.into_first_result().await.map_err(|e| FraiseQLError::Database {
                message:   format!("Failed to get result set: {e}"),
                sql_state: e.code().and_then(map_mssql_error_code),
            })?
        } else {
            // For parameterized queries, use typed parameter binding.
            let string_params = serialise_complex_params(&params);
            let mut query = tiberius::Query::new(sql);
            bind_json_params(&mut query, &params, &string_params)?;

            let result = query.query(&mut *conn).await.map_err(|e| FraiseQLError::Database {
                message:   format!("SQL Server query execution failed: {e}"),
                sql_state: e.code().and_then(map_mssql_error_code),
            })?;
            result.into_first_result().await.map_err(|e| FraiseQLError::Database {
                message:   format!("Failed to get result set: {e}"),
                sql_state: e.code().and_then(map_mssql_error_code),
            })?
        };

        // Process result set
        let mut results = Vec::new();

        for row in rows {
            // Try to get 'data' column as string and parse as JSON
            if let Some(data_str) = row.try_get::<&str, _>("data").ok().flatten() {
                let data: serde_json::Value =
                    serde_json::from_str(data_str).unwrap_or(serde_json::Value::Null);
                results.push(JsonbValue::new(data));
            } else {
                results.push(JsonbValue::new(serde_json::Value::Null));
            }
        }

        Ok(results)
    }
}

// Reason: DatabaseAdapter is defined with #[async_trait]; all implementations must match
// its transformed method signatures to satisfy the trait contract
// async_trait: dyn-dispatch required; remove when RTN + Send is stable (RFC 3425)
#[async_trait]
impl DatabaseAdapter for SqlServerAdapter {
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

        let projection = projection.expect("projection is Some; None was returned above");

        // Build SQL with SQL Server-specific JSON projection
        // The projection_template contains the SELECT clause with JSON functions
        // SQL Server uses square brackets for identifiers and TOP for LIMIT
        // TOP and OFFSET...FETCH are mutually exclusive in T-SQL, so only use
        // TOP when there is no OFFSET (otherwise OFFSET...FETCH handles both).
        let mut sql = if let Some(lim) = limit {
            if offset.is_some() {
                // With OFFSET, we use OFFSET...FETCH instead of TOP
                format!(
                    "SELECT {} FROM {}",
                    projection.projection_template,
                    quote_sqlserver_identifier(view)
                )
            } else {
                format!(
                    "SELECT TOP {} {} FROM {}",
                    lim,
                    projection.projection_template,
                    quote_sqlserver_identifier(view)
                )
            }
        } else {
            format!(
                "SELECT {} FROM {}",
                projection.projection_template,
                quote_sqlserver_identifier(view)
            )
        };

        // Add WHERE clause if present
        let params: Vec<serde_json::Value> = if let Some(clause) = where_clause {
            let generator = super::where_generator::SqlServerWhereGenerator::new(SqlServerDialect);
            let (where_sql, where_params) = generator.generate(clause)?;
            sql.push_str(" WHERE ");
            sql.push_str(&where_sql);
            where_params
        } else {
            Vec::new()
        };

        // Add OFFSET/FETCH for pagination (SQL Server requires ORDER BY for OFFSET)
        if let Some(off) = offset {
            // SQL Server requires ORDER BY for OFFSET/FETCH NEXT syntax
            sql.push_str(&format!(" ORDER BY (SELECT NULL) OFFSET {off} ROWS"));
            if let Some(lim) = limit {
                sql.push_str(&format!(" FETCH NEXT {lim} ROWS ONLY"));
            }
        }

        // Execute the query
        self.execute_raw(&sql, params).await
    }

    async fn execute_where_query(
        &self,
        view: &str,
        where_clause: Option<&WhereClause>,
        limit: Option<u32>,
        offset: Option<u32>,
        _order_by: Option<&[OrderByClause]>,
    ) -> Result<Vec<JsonbValue>> {
        // Build base query - SQL Server uses square brackets for identifiers
        // SQL Server uses TOP instead of LIMIT, and OFFSET...FETCH for pagination
        // SAFETY: view is schema-derived (from CompiledSchema, validated at compile time),
        // not user input. Additionally passed through quote_sqlserver_identifier().
        let mut sql = if let Some(lim) = limit {
            if offset.is_some() {
                // With OFFSET, we need ORDER BY for pagination
                format!("SELECT data FROM {}", quote_sqlserver_identifier(view))
            } else {
                format!("SELECT TOP {lim} data FROM {}", quote_sqlserver_identifier(view))
            }
        } else {
            format!("SELECT data FROM {}", quote_sqlserver_identifier(view))
        };

        // Add WHERE clause if present
        let (mut params, mut param_count): (Vec<serde_json::Value>, usize) =
            if let Some(clause) = where_clause {
                let generator = SqlServerWhereGenerator::new(SqlServerDialect);
                let (where_sql, where_params) = generator.generate(clause)?;
                sql.push_str(" WHERE ");
                sql.push_str(&where_sql);
                let len = where_params.len();
                (where_params, len)
            } else {
                (Vec::new(), 0)
            };

        // Handle pagination with OFFSET...FETCH (requires ORDER BY)
        // SQL Server uses @p1, @p2, ... for parameters
        if let Some(off) = offset {
            sql.push_str(" ORDER BY (SELECT NULL)"); // Arbitrary ordering for pagination
            param_count += 1;
            sql.push_str(&format!(" OFFSET @p{param_count} ROWS"));
            params.push(serde_json::Value::Number(off.into()));
            if let Some(lim) = limit {
                param_count += 1;
                sql.push_str(&format!(" FETCH NEXT @p{param_count} ROWS ONLY"));
                params.push(serde_json::Value::Number(lim.into()));
            }
        }

        self.execute_raw(&sql, params).await
    }

    fn database_type(&self) -> DatabaseType {
        DatabaseType::SQLServer
    }

    async fn health_check(&self) -> Result<()> {
        let mut conn = self.pool.get().await.map_err(|e| FraiseQLError::ConnectionPool {
            message: format!("Failed to acquire connection: {e}"),
        })?;

        conn.simple_query("SELECT 1").await.map_err(|e| FraiseQLError::Database {
            message:   format!("SQL Server health check failed: {e}"),
            sql_state: None,
        })?;

        Ok(())
    }

    fn pool_metrics(&self) -> PoolMetrics {
        let state = self.pool.state();

        PoolMetrics {
            total_connections:  state.connections,
            idle_connections:   state.idle_connections,
            active_connections: state.connections - state.idle_connections,
            waiting_requests:   0, // bb8 doesn't expose waiting count directly
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
        let mut conn = self.pool.get().await.map_err(|e| FraiseQLError::ConnectionPool {
            message: format!("Failed to acquire connection: {e}"),
        })?;

        let result = conn.simple_query(sql).await.map_err(|e| FraiseQLError::Database {
            message:   format!("SQL Server query execution failed: {e}"),
            sql_state: e.code().and_then(map_mssql_error_code),
        })?;

        let rows = result.into_first_result().await.map_err(|e| FraiseQLError::Database {
            message:   format!("Failed to get result set: {e}"),
            sql_state: e.code().and_then(map_mssql_error_code),
        })?;

        // Convert each row to HashMap<String, Value>
        let results: Vec<std::collections::HashMap<String, serde_json::Value>> = rows
            .into_iter()
            .map(|row| {
                let mut map = std::collections::HashMap::new();

                // Iterate over all columns in the row
                for (idx, column) in row.columns().iter().enumerate() {
                    let column_name = column.name().to_string();

                    // Try to extract value based on SQL Server type
                    let value: serde_json::Value =
                        if let Some(v) = row.try_get::<i32, _>(idx).ok().flatten() {
                            serde_json::json!(v)
                        } else if let Some(v) = row.try_get::<i64, _>(idx).ok().flatten() {
                            serde_json::json!(v)
                        } else if let Some(v) = row.try_get::<f64, _>(idx).ok().flatten() {
                            serde_json::json!(v)
                        } else if let Some(v) = row.try_get::<&str, _>(idx).ok().flatten() {
                            // Try to parse as JSON first
                            if let Ok(json_val) = serde_json::from_str::<serde_json::Value>(v) {
                                json_val
                            } else {
                                serde_json::json!(v)
                            }
                        } else if let Some(v) = row.try_get::<bool, _>(idx).ok().flatten() {
                            serde_json::json!(v)
                        } else {
                            // Fallback: NULL
                            serde_json::Value::Null
                        };

                    map.insert(column_name, value);
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
        let mut conn = self.pool.get().await.map_err(|e| FraiseQLError::ConnectionPool {
            message: format!("Failed to acquire connection: {e}"),
        })?;

        let string_params = serialise_complex_params(params);
        let mut query = tiberius::Query::new(sql);
        bind_json_params(&mut query, params, &string_params)?;

        let result = query.query(&mut *conn).await.map_err(|e| FraiseQLError::Database {
            message:   format!("SQL Server parameterized aggregate query failed: {e}"),
            sql_state: e.code().and_then(map_mssql_error_code),
        })?;

        let rows = result.into_first_result().await.map_err(|e| FraiseQLError::Database {
            message:   format!("Failed to get aggregate result set: {e}"),
            sql_state: e.code().and_then(map_mssql_error_code),
        })?;

        let results = rows
            .into_iter()
            .map(|row| {
                let mut map = std::collections::HashMap::new();
                for (idx, column) in row.columns().iter().enumerate() {
                    let col = column.name().to_string();
                    let value: serde_json::Value =
                        if let Some(v) = row.try_get::<i32, _>(idx).ok().flatten() {
                            serde_json::json!(v)
                        } else if let Some(v) = row.try_get::<i64, _>(idx).ok().flatten() {
                            serde_json::json!(v)
                        } else if let Some(v) = row.try_get::<f64, _>(idx).ok().flatten() {
                            serde_json::json!(v)
                        } else if let Some(v) = row.try_get::<bool, _>(idx).ok().flatten() {
                            serde_json::json!(v)
                        } else if let Some(s) = row.try_get::<&str, _>(idx).ok().flatten() {
                            serde_json::from_str(s).unwrap_or_else(|_| serde_json::json!(s))
                        } else {
                            serde_json::Value::Null
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
        // Build: SELECT * FROM [schema].[fn_name](@p1, @p2, ...)
        let placeholders: Vec<String> = (1..=args.len()).map(|i| format!("@p{i}")).collect();
        let sql = format!(
            "SELECT * FROM {}({})",
            quote_sqlserver_identifier(function_name),
            placeholders.join(", ")
        );

        let mut conn = self.pool.get().await.map_err(|e| FraiseQLError::ConnectionPool {
            message: format!("Failed to acquire connection: {e}"),
        })?;

        // Bind args; non-primitive types are serialised to JSON strings.
        let string_params = serialise_complex_params(args);
        let mut query = tiberius::Query::new(sql);
        bind_json_params(&mut query, args, &string_params)?;

        let result = query.query(&mut *conn).await.map_err(|e| FraiseQLError::Database {
            message:   format!("SQL Server function call failed ({function_name}): {e}"),
            sql_state: e.code().and_then(map_mssql_error_code),
        })?;

        let rows = result.into_first_result().await.map_err(|e| FraiseQLError::Database {
            message:   format!("Failed to get result set from {function_name}: {e}"),
            sql_state: e.code().and_then(map_mssql_error_code),
        })?;

        let results = rows
            .into_iter()
            .map(|row| {
                let mut map = std::collections::HashMap::new();
                for (idx, column) in row.columns().iter().enumerate() {
                    let col = column.name().to_string();
                    let value: serde_json::Value =
                        if let Some(v) = row.try_get::<i32, _>(idx).ok().flatten() {
                            serde_json::json!(v)
                        } else if let Some(v) = row.try_get::<i64, _>(idx).ok().flatten() {
                            serde_json::json!(v)
                        } else if let Some(v) = row.try_get::<f64, _>(idx).ok().flatten() {
                            serde_json::json!(v)
                        } else if let Some(v) = row.try_get::<bool, _>(idx).ok().flatten() {
                            serde_json::json!(v)
                        } else if let Some(s) = row.try_get::<&str, _>(idx).ok().flatten() {
                            serde_json::from_str(s).unwrap_or_else(|_| serde_json::json!(s))
                        } else {
                            serde_json::Value::Null
                        };
                    map.insert(col, value);
                }
                map
            })
            .collect();

        Ok(results)
    }
}

use super::helpers::{
    bind_json_params, build_relay_backward_outer_order_sql, build_relay_order_sql,
    build_relay_where_sql, is_valid_uuid_format, serialise_complex_params,
};

impl MutationCapable for SqlServerAdapter {}

impl RelayDatabaseAdapter for SqlServerAdapter {
    /// Execute keyset (cursor-based) pagination against a JSONB view.
    ///
    /// Uses `OFFSET 0 ROWS FETCH NEXT n ROWS ONLY` with mandatory `ORDER BY`.
    /// Backward pagination wraps a DESC inner query in an outer ASC sort.
    ///
    /// UUID cursors are compared via `CONVERT(UNIQUEIDENTIFIER, @pN)`.
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
        let quoted_view = quote_sqlserver_identifier(view);
        let quoted_col = quote_sqlserver_identifier(cursor_column);

        // ── Cursor condition ─────────────────────────────────────────────────
        let active_cursor = if forward { after } else { before };
        let (cursor_param, cursor_where_part): (Option<serde_json::Value>, Option<String>) =
            match active_cursor {
                None => (None, None),
                Some(CursorValue::Int64(pk)) => {
                    let op = if forward { ">" } else { "<" };
                    (Some(serde_json::json!(pk)), Some(format!("{quoted_col} {op} @p1")))
                },
                Some(CursorValue::Uuid(uuid)) => {
                    // Validate format before sending to SQL Server; malformed UUIDs produce an
                    // opaque conversion error (MSSQL 8169) rather than a useful validation message.
                    if !is_valid_uuid_format(&uuid) {
                        return Err(FraiseQLError::Validation {
                            message: format!("Invalid UUID cursor value: '{uuid}'"),
                            path:    None,
                        });
                    }
                    let op = if forward { ">" } else { "<" };
                    (
                        Some(serde_json::json!(uuid)),
                        Some(format!("{quoted_col} {op} CONVERT(UNIQUEIDENTIFIER, @p1)")),
                    )
                },
            };
        let cursor_param_count: usize = if cursor_param.is_some() { 1 } else { 0 };

        // ── User WHERE clause ────────────────────────────────────────────────
        let mut user_where_params: Vec<serde_json::Value> = Vec::new();
        let page_user_where_sql: Option<String> = if let Some(clause) = where_clause {
            let generator = SqlServerWhereGenerator::new(SqlServerDialect);
            let (sql, params) = generator.generate_with_param_offset(clause, cursor_param_count)?;
            user_where_params = params;
            Some(sql)
        } else {
            None
        };
        let user_param_count = user_where_params.len();

        // ── ORDER BY and WHERE strings ────────────────────────────────────────
        let order_sql = build_relay_order_sql(&quoted_col, order_by, forward);
        let page_where_sql =
            build_relay_where_sql(cursor_where_part.as_deref(), page_user_where_sql.as_deref());

        // ── LIMIT parameter index ─────────────────────────────────────────────
        let limit_idx = cursor_param_count + user_param_count + 1;

        // ── Page SQL ──────────────────────────────────────────────────────────
        //
        // SQL Server requires ORDER BY for OFFSET…FETCH. The mandatory
        // `OFFSET 0 ROWS` prelude cannot be elided.
        //
        // Backward pagination: the inner query uses flipped sort directions + FETCH so it
        // retrieves the correct N rows before the cursor.  The inner query also projects each
        // custom sort column under an alias (`_relay_sort_0`, …) so the outer re-sort can
        // reference them.  The outer query restores the original (non-flipped) sort order.
        // SAFETY: quoted_view and quoted_col are schema-derived (from CompiledSchema, validated
        // at compile time) and passed through quote_sqlserver_identifier(), not user input.
        let page_sql = if forward {
            format!(
                "SELECT data FROM {quoted_view}{page_where_sql}{order_sql} \
                 OFFSET 0 ROWS FETCH NEXT @p{limit_idx} ROWS ONLY"
            )
        } else {
            // Project each custom sort column under a stable alias for the outer re-sort.
            let sort_aliases: String = order_by.unwrap_or(&[]).iter().enumerate().fold(
                String::new(),
                |mut acc, (i, c)| {
                    use std::fmt::Write as _;
                    let _ = write!(acc, ", JSON_VALUE(data, '$.{}') AS _relay_sort_{i}", c.field);
                    acc
                },
            );

            let inner = format!(
                "SELECT data, {quoted_col} AS _relay_cursor{sort_aliases} \
                 FROM {quoted_view}{page_where_sql}{order_sql} \
                 OFFSET 0 ROWS FETCH NEXT @p{limit_idx} ROWS ONLY"
            );
            let outer_order = build_relay_backward_outer_order_sql(order_by);
            // SAFETY: quoted_view and quoted_col are schema-derived (validated at compile time),
            // not user input. Passed through quote_sqlserver_identifier().
            format!("SELECT data FROM ({inner}) AS _relay_page{outer_order}")
        };

        // ── Page params: [cursor?, user_where..., limit] ──────────────────────
        let mut page_params: Vec<serde_json::Value> = Vec::new();
        if let Some(cp) = cursor_param {
            page_params.push(cp);
        }
        page_params.extend_from_slice(&user_where_params);
        page_params.push(serde_json::json!(limit));

        // ── Execute page query ────────────────────────────────────────────────
        let rows = self.execute_raw(&page_sql, page_params).await?;

        // ── Count query (Relay spec: totalCount ignores cursor position) ──────
        //
        // Re-generates the WHERE clause with offset 0 (no cursor prefix) because
        // the count is a standalone query.
        let total_count = if include_total_count {
            let (count_sql, count_params) = if let Some(clause) = where_clause {
                let generator = SqlServerWhereGenerator::new(SqlServerDialect);
                let (where_sql, params) = generator.generate_with_param_offset(clause, 0)?;
                // SAFETY: quoted_view is schema-derived (validated at compile time), not user
                // input.
                (
                    format!("SELECT COUNT_BIG(*) AS cnt FROM {quoted_view} WHERE ({where_sql})"),
                    params,
                )
            } else {
                (format!("SELECT COUNT_BIG(*) AS cnt FROM {quoted_view}"), vec![])
            };

            let mut conn = self.pool.get().await.map_err(|e| FraiseQLError::ConnectionPool {
                message: format!("Failed to acquire connection for relay count: {e}"),
            })?;

            // Serialise complex types up-front so their string refs live long enough.
            let count_string_params = serialise_complex_params(&count_params);
            let mut count_query = tiberius::Query::new(&count_sql);
            bind_json_params(&mut count_query, &count_params, &count_string_params)?;

            let count_result =
                count_query.query(&mut *conn).await.map_err(|e| FraiseQLError::Database {
                    message:   format!("SQL Server relay count query failed: {e}"),
                    sql_state: e.code().and_then(map_mssql_error_code),
                })?;

            let count_rows =
                count_result.into_first_result().await.map_err(|e| FraiseQLError::Database {
                    message:   format!("Failed to get relay count result set: {e}"),
                    sql_state: e.code().and_then(map_mssql_error_code),
                })?;

            // A missing or empty row must surface as an error rather than silently
            // reporting `totalCount: 0`, which would be a misleading data-loss trap.
            let n: i64 = count_rows
                .first()
                .and_then(|row| row.try_get::<i64, _>(0).ok().flatten())
                .ok_or_else(|| FraiseQLError::Database {
                    message:   format!("Relay count query returned no rows for view '{view}'"),
                    sql_state: None,
                })?;
            let count = u64::try_from(n).map_err(|_| FraiseQLError::Database {
                message:   format!(
                    "Relay count query returned negative value ({n}) for view '{view}'"
                ),
                sql_state: None,
            })?;

            Some(count)
        } else {
            None
        };

        Ok(RelayPageResult { rows, total_count })
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)] // Reason: test code — unwrap is acceptable for assertions
#[path = "adapter_tests.rs"]
mod tests;
