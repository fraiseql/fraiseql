//! SQLite database adapter with query and mutation support.
//!
//! This adapter supports query execution (`execute_where_query`, `execute_raw_query`)
//! and **direct-SQL mutations** (INSERT/UPDATE/DELETE with RETURNING).
//!
//! Unlike PostgreSQL/MySQL/SQL Server, SQLite has no stored procedures. Mutations
//! are executed as direct SQL statements rather than function calls. The adapter
//! implements [`MutationCapable`](crate::MutationCapable) and uses
//! [`MutationStrategy::DirectSql`](crate::MutationStrategy::DirectSql).
//!
//! # Requirements
//!
//! - SQLite 3.35.0+ (2021-03-12) for `RETURNING` clause support.
//!
//! # Limitations
//!
//! - `MutationOperation::Custom` mutations are not supported (no stored procedures).
//! - No server-side validation logic beyond constraint checking.
//! - Each mutation is a single atomic statement (no multi-statement transactions).
//!
//! # When to use SQLite
//!
//! - Local development with full read/write cycles
//! - Unit testing queries and mutations without a real database
//! - Schema exploration

use async_trait::async_trait;
use fraiseql_error::{FraiseQLError, Result};
use sqlx::{
    Column, Row,
    sqlite::{SqlitePool, SqlitePoolOptions, SqliteRow},
};

use super::where_generator::SqliteWhereGenerator;
use crate::{
    dialect::SqliteDialect,
    identifier::quote_sqlite_identifier,
    traits::{
        DatabaseAdapter, DirectMutationContext, DirectMutationOp, MutationCapable, MutationStrategy,
    },
    types::{DatabaseType, JsonbValue, PoolMetrics},
    where_clause::WhereClause,
};

/// SQLite database adapter with connection pooling.
///
/// Uses `sqlx` for connection pooling and async queries.
/// Ideal for local development and testing.
///
/// # Example
///
/// ```no_run
/// use fraiseql_core::db::sqlite::SqliteAdapter;
/// use fraiseql_core::db::{DatabaseAdapter, WhereClause, WhereOperator};
/// use serde_json::json;
///
/// # async fn example() -> Result<(), Box<dyn std::error::Error>> {
/// // Create adapter with file path
/// let adapter = SqliteAdapter::new("sqlite:./test.db").await?;
///
/// // Or use in-memory database
/// let adapter = SqliteAdapter::new("sqlite::memory:").await?;
///
/// // Execute query
/// let where_clause = WhereClause::Field {
///     path: vec!["email".to_string()],
///     operator: WhereOperator::Icontains,
///     value: json!("example.com"),
/// };
///
/// let results = adapter
///     .execute_where_query("v_user", Some(&where_clause), Some(10), None)
///     .await?;
///
/// println!("Found {} users", results.len());
/// # Ok(())
/// # }
/// ```
#[derive(Clone)]
pub struct SqliteAdapter {
    pool: SqlitePool,
}

impl SqliteAdapter {
    /// Create new SQLite adapter with default pool configuration.
    ///
    /// # Arguments
    ///
    /// * `connection_string` - SQLite connection string (e.g., "sqlite:./mydb.db" or
    ///   "sqlite::memory:")
    ///
    /// # Errors
    ///
    /// Returns `FraiseQLError::ConnectionPool` if pool creation fails.
    pub async fn new(connection_string: &str) -> Result<Self> {
        Self::with_pool_size(connection_string, 5).await
    }

    /// Create new SQLite adapter with custom pool configuration.
    ///
    /// # Arguments
    ///
    /// * `connection_string` - SQLite connection string
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
        let pool = SqlitePoolOptions::new()
            .min_connections(min_size)
            .max_connections(max_size)
            .connect(connection_string)
            .await
            .map_err(|e| FraiseQLError::ConnectionPool {
                message: format!("Failed to create SQLite connection pool: {e}"),
            })?;

        Ok(Self { pool })
    }

    /// Create new SQLite adapter with custom pool size.
    ///
    /// # Arguments
    ///
    /// * `connection_string` - SQLite connection string
    /// * `max_size` - Maximum number of connections in pool
    ///
    /// # Errors
    ///
    /// Returns `FraiseQLError::ConnectionPool` if pool creation fails.
    pub async fn with_pool_size(connection_string: &str, max_size: u32) -> Result<Self> {
        let pool = SqlitePoolOptions::new()
            .max_connections(max_size)
            .connect(connection_string)
            .await
            .map_err(|e| FraiseQLError::ConnectionPool {
                message: format!("Failed to create SQLite connection pool: {e}"),
            })?;

        // Test connection
        sqlx::query("SELECT 1")
            .fetch_one(&pool)
            .await
            .map_err(|e| FraiseQLError::Database {
                message:   format!("Failed to connect to SQLite database: {e}"),
                sql_state: None,
            })?;

        Ok(Self { pool })
    }

    /// Create an in-memory SQLite adapter (useful for testing).
    ///
    /// # Errors
    ///
    /// Returns `FraiseQLError::ConnectionPool` if pool creation fails.
    pub async fn in_memory() -> Result<Self> {
        Self::new("sqlite::memory:").await
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

        let rows: Vec<SqliteRow> =
            query.fetch_all(&self.pool).await.map_err(|e| FraiseQLError::Database {
                message:   format!("SQLite query execution failed: {e}"),
                sql_state: None,
            })?;

        let results = rows
            .into_iter()
            .map(|row| {
                // SQLite stores JSON as TEXT, parse it
                let data_str: String = row.try_get("data").unwrap_or_default();
                let data: serde_json::Value =
                    serde_json::from_str(&data_str).unwrap_or(serde_json::Value::Null);
                JsonbValue::new(data)
            })
            .collect();

        Ok(results)
    }
}

// Reason: DatabaseAdapter is defined with #[async_trait]; all implementations must match
// its transformed method signatures to satisfy the trait contract
// async_trait: dyn-dispatch required; remove when RTN + Send is stable (RFC 3425)
#[async_trait]
impl DatabaseAdapter for SqliteAdapter {
    async fn execute_with_projection(
        &self,
        view: &str,
        projection: Option<&crate::types::SqlProjectionHint>,
        where_clause: Option<&WhereClause>,
        limit: Option<u32>,
    ) -> Result<Vec<JsonbValue>> {
        // If no projection provided, fall back to standard query
        if projection.is_none() {
            return self.execute_where_query(view, where_clause, limit, None).await;
        }

        let projection = projection.expect("projection is Some; None was returned above");

        // Build SQL with SQLite-specific json_object projection
        // The projection_template contains the SELECT clause with json_object() calls
        // SQLite uses double quotes for identifiers, not backticks
        // e.g., "json_object('id', data->'$.id', 'email', data->'$.email')"
        let mut sql = format!(
            "SELECT {} FROM {}",
            projection.projection_template,
            quote_sqlite_identifier(view)
        );

        // Add WHERE clause if present
        let params: Vec<serde_json::Value> = if let Some(clause) = where_clause {
            let generator = super::where_generator::SqliteWhereGenerator::new(SqliteDialect);
            let (where_sql, where_params) = generator.generate(clause)?;
            sql.push_str(" WHERE ");
            sql.push_str(&where_sql);
            where_params
        } else {
            Vec::new()
        };

        // Add LIMIT if present (SQLite uses LIMIT before OFFSET)
        if let Some(lim) = limit {
            sql.push_str(&format!(" LIMIT {lim}"));
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
    ) -> Result<Vec<JsonbValue>> {
        // Build base query - SQLite uses double quotes for identifiers
        // SAFETY: view is schema-derived (from CompiledSchema, validated at compile time),
        // not user input. Additionally passed through quote_sqlite_identifier().
        let mut sql = format!("SELECT data FROM {}", quote_sqlite_identifier(view));

        // Add WHERE clause if present
        let mut params: Vec<serde_json::Value> = if let Some(clause) = where_clause {
            let generator = SqliteWhereGenerator::new(SqliteDialect);
            let (where_sql, where_params) = generator.generate(clause)?;
            sql.push_str(" WHERE ");
            sql.push_str(&where_sql);
            where_params
        } else {
            Vec::new()
        };

        // Add LIMIT and OFFSET
        // Note: SQLite requires LIMIT when using OFFSET, so we use LIMIT -1 for "unlimited"
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
                // SQLite requires LIMIT with OFFSET; use -1 for unlimited
                sql.push_str(" LIMIT -1 OFFSET ?");
                params.push(serde_json::Value::Number(off.into()));
            },
            (None, None) => {},
        }

        self.execute_raw(&sql, params).await
    }

    fn database_type(&self) -> DatabaseType {
        DatabaseType::SQLite
    }

    fn supports_mutations(&self) -> bool {
        true
    }

    fn mutation_strategy(&self) -> MutationStrategy {
        MutationStrategy::DirectSql
    }

    async fn execute_direct_mutation(
        &self,
        ctx: &DirectMutationContext<'_>,
    ) -> Result<Vec<std::collections::HashMap<String, serde_json::Value>>> {
        let (sql, bind_values) = build_direct_mutation_sql(ctx)?;

        // Bind parameters and execute
        let mut query = sqlx::query(&sql);
        for value in bind_values {
            query = match value {
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
                    query.bind(value.to_string())
                },
            };
        }

        let row_opt = query
            .fetch_optional(&self.pool)
            .await
            .map_err(|e| FraiseQLError::Database {
                message:   format!("SQLite direct mutation failed: {e}"),
                sql_state: None,
            })?;

        let Some(row) = row_opt else {
            // DELETE/UPDATE of non-existent row
            return Err(FraiseQLError::Validation {
                message: format!(
                    "Mutation on table '{}' affected no rows (entity not found)",
                    ctx.table
                ),
                path: None,
            });
        };

        // Convert the RETURNING * row into a JSON object for the `entity` field
        let entity = sqlite_row_to_json(&row);

        // Determine status and entity_id based on operation
        let status = match ctx.operation {
            DirectMutationOp::Insert => "new",
            DirectMutationOp::Update => "updated",
            DirectMutationOp::Delete => "deleted",
        };

        // For UPDATE/DELETE, extract the PK value as entity_id
        let entity_id: serde_json::Value = match ctx.operation {
            DirectMutationOp::Update | DirectMutationOp::Delete => {
                // First value is the PK
                match &ctx.values[0] {
                    serde_json::Value::Number(n) => serde_json::json!(n.to_string()),
                    v => serde_json::json!(v.to_string().trim_matches('"')),
                }
            },
            DirectMutationOp::Insert => serde_json::Value::Null,
        };

        let mut result = std::collections::HashMap::new();
        result.insert("status".into(), serde_json::json!(status));
        result.insert("message".into(), serde_json::Value::Null);
        result.insert("entity".into(), entity);
        result.insert("entity_type".into(), serde_json::json!(ctx.return_type));
        result.insert("entity_id".into(), entity_id);
        result.insert("cascade".into(), serde_json::Value::Null);
        result.insert("metadata".into(), serde_json::Value::Null);
        Ok(vec![result])
    }

    async fn health_check(&self) -> Result<()> {
        sqlx::query("SELECT 1").fetch_one(&self.pool).await.map_err(|e| {
            FraiseQLError::Database {
                message:   format!("SQLite health check failed: {e}"),
                sql_state: None,
            }
        })?;

        Ok(())
    }

    #[allow(clippy::cast_possible_truncation)] // Reason: pool sizes are always far below u32::MAX in practice
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
        let rows: Vec<SqliteRow> =
            sqlx::query(sql)
                .fetch_all(&self.pool)
                .await
                .map_err(|e| FraiseQLError::Database {
                    message:   format!("SQLite query execution failed: {e}"),
                    sql_state: None,
                })?;

        // Convert each row to HashMap<String, Value>
        let results: Vec<std::collections::HashMap<String, serde_json::Value>> = rows
            .into_iter()
            .map(|row| {
                let mut map = std::collections::HashMap::new();

                // Iterate over all columns in the row
                for column in row.columns() {
                    let column_name = column.name().to_string();

                    // Try to extract value based on SQLite type
                    let value: serde_json::Value =
                        if let Ok(v) = row.try_get::<i32, _>(column_name.as_str()) {
                            serde_json::json!(v)
                        } else if let Ok(v) = row.try_get::<i64, _>(column_name.as_str()) {
                            serde_json::json!(v)
                        } else if let Ok(v) = row.try_get::<f64, _>(column_name.as_str()) {
                            serde_json::json!(v)
                        } else if let Ok(v) = row.try_get::<String, _>(column_name.as_str()) {
                            // Try to parse as JSON first
                            if let Ok(json_val) = serde_json::from_str::<serde_json::Value>(&v) {
                                json_val
                            } else {
                                serde_json::json!(v)
                            }
                        } else if let Ok(v) = row.try_get::<bool, _>(column_name.as_str()) {
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

        let rows: Vec<SqliteRow> =
            query.fetch_all(&self.pool).await.map_err(|e| FraiseQLError::Database {
                message:   format!("SQLite parameterized aggregate query failed: {e}"),
                sql_state: None,
            })?;

        let results = rows
            .into_iter()
            .map(|row| {
                let mut map = std::collections::HashMap::new();
                for column in row.columns() {
                    let column_name = column.name().to_string();
                    let value: serde_json::Value =
                        if let Ok(v) = row.try_get::<i32, _>(column_name.as_str()) {
                            serde_json::json!(v)
                        } else if let Ok(v) = row.try_get::<i64, _>(column_name.as_str()) {
                            serde_json::json!(v)
                        } else if let Ok(v) = row.try_get::<f64, _>(column_name.as_str()) {
                            serde_json::json!(v)
                        } else if let Ok(v) = row.try_get::<String, _>(column_name.as_str()) {
                            if let Ok(json_val) = serde_json::from_str::<serde_json::Value>(&v) {
                                json_val
                            } else {
                                serde_json::json!(v)
                            }
                        } else if let Ok(v) = row.try_get::<bool, _>(column_name.as_str()) {
                            serde_json::json!(v)
                        } else {
                            serde_json::Value::Null
                        };
                    map.insert(column_name, value);
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
        // SAFETY: sql is compiler-generated from schema-derived sources, not user input.
        // Defense-in-depth: semicolons are rejected above.
        let explain_sql = format!("EXPLAIN QUERY PLAN {sql}");
        let rows: Vec<sqlx::sqlite::SqliteRow> = sqlx::query(&explain_sql)
            .fetch_all(&self.pool)
            .await
            .map_err(|e| FraiseQLError::Database {
                message:   format!("SQLite EXPLAIN failed: {e}"),
                sql_state: None,
            })?;

        let steps: Vec<serde_json::Value> = rows
            .iter()
            .map(|row| {
                let detail: String = row.try_get("detail").unwrap_or_default();
                serde_json::json!({ "detail": detail })
            })
            .collect();

        Ok(serde_json::json!(steps))
    }
}

impl MutationCapable for SqliteAdapter {}

/// Convert a SQLite row (from `RETURNING *`) into a JSON object.
///
/// Uses type-sniffing (i32 → i64 → f64 → String → bool → Null) to convert
/// each column value. String values that look like JSON are parsed.
fn sqlite_row_to_json(row: &SqliteRow) -> serde_json::Value {
    use sqlx::{Column as _, Row as _, TypeInfo as _, ValueRef as _};

    let mut obj = serde_json::Map::new();
    for column in row.columns() {
        let name = column.name().to_string();

        // Check for NULL first — sqlx type-sniffing can coerce NULL to default values
        let is_null = row
            .try_get_raw(name.as_str())
            .is_ok_and(|v| v.is_null() || v.type_info().name() == "NULL");

        let value: serde_json::Value = if is_null {
            serde_json::Value::Null
        } else if let Ok(v) = row.try_get::<i32, _>(name.as_str()) {
            serde_json::json!(v)
        } else if let Ok(v) = row.try_get::<i64, _>(name.as_str()) {
            serde_json::json!(v)
        } else if let Ok(v) = row.try_get::<f64, _>(name.as_str()) {
            serde_json::json!(v)
        } else if let Ok(v) = row.try_get::<String, _>(name.as_str()) {
            if let Ok(json_val) = serde_json::from_str::<serde_json::Value>(&v) {
                json_val
            } else {
                serde_json::json!(v)
            }
        } else if let Ok(v) = row.try_get::<bool, _>(name.as_str()) {
            serde_json::json!(v)
        } else {
            serde_json::Value::Null
        };
        obj.insert(name, value);
    }
    serde_json::Value::Object(obj)
}

/// Build the SQL statement and ordered bind values for a direct mutation.
///
/// # Errors
///
/// Returns `FraiseQLError::Unsupported` if the operation/column combination is invalid.
fn build_direct_mutation_sql<'a>(
    ctx: &'a DirectMutationContext<'_>,
) -> Result<(String, Vec<&'a serde_json::Value>)> {
    // Number of client args
    let n_client = ctx.columns.len();
    let n_inject = ctx.inject_columns.len();

    match ctx.operation {
        DirectMutationOp::Insert => {
            // INSERT INTO "table" ("col1", "col2", "inject1") VALUES (?, ?, ?) RETURNING *
            // All client + inject columns, all values in order
            let all_columns: Vec<String> = ctx
                .columns
                .iter()
                .chain(ctx.inject_columns.iter())
                .map(|c| quote_sqlite_identifier(c))
                .collect();
            let placeholders: Vec<&str> = vec!["?"; n_client + n_inject];
            let sql = format!(
                "INSERT INTO {} ({}) VALUES ({}) RETURNING *",
                quote_sqlite_identifier(ctx.table),
                all_columns.join(", "),
                placeholders.join(", ")
            );
            // Bind all values in order (client args + inject args)
            let bind_values: Vec<&serde_json::Value> =
                ctx.values.iter().collect();
            Ok((sql, bind_values))
        },
        DirectMutationOp::Update => {
            // UPDATE "table" SET "col2" = ?, "inject1" = ? WHERE "pk_col" = ? RETURNING *
            // First client column is PK (for WHERE), rest are SET columns
            if ctx.columns.is_empty() {
                return Err(FraiseQLError::Validation {
                    message: "UPDATE mutation requires at least one argument (primary key)"
                        .into(),
                    path: None,
                });
            }
            let pk_col = quote_sqlite_identifier(&ctx.columns[0]);

            // SET columns: client columns after PK + inject columns
            let set_columns: Vec<String> = ctx.columns[1..]
                .iter()
                .chain(ctx.inject_columns.iter())
                .map(|c| format!("{} = ?", quote_sqlite_identifier(c)))
                .collect();

            if set_columns.is_empty() {
                return Err(FraiseQLError::Validation {
                    message: "UPDATE mutation requires at least one column to update".into(),
                    path: None,
                });
            }

            let sql = format!(
                "UPDATE {} SET {} WHERE {} = ? RETURNING *",
                quote_sqlite_identifier(ctx.table),
                set_columns.join(", "),
                pk_col
            );

            // Bind order: SET values (client[1..] + inject), then PK value (client[0])
            let mut bind_values: Vec<&serde_json::Value> = Vec::with_capacity(ctx.values.len());
            // Client args after PK (indices 1..n_client)
            for v in &ctx.values[1..n_client] {
                bind_values.push(v);
            }
            // Inject args (indices n_client..)
            for v in &ctx.values[n_client..] {
                bind_values.push(v);
            }
            // PK value last (for WHERE clause)
            bind_values.push(&ctx.values[0]);
            Ok((sql, bind_values))
        },
        DirectMutationOp::Delete => {
            // DELETE FROM "table" WHERE "pk_col" = ? [AND "inject_col" = ?] RETURNING *
            if ctx.columns.is_empty() {
                return Err(FraiseQLError::Validation {
                    message: "DELETE mutation requires at least one argument (primary key)"
                        .into(),
                    path: None,
                });
            }
            let pk_col = quote_sqlite_identifier(&ctx.columns[0]);

            let mut where_parts = vec![format!("{pk_col} = ?")];
            for ic in ctx.inject_columns {
                where_parts.push(format!("{} = ?", quote_sqlite_identifier(ic)));
            }

            let sql = format!(
                "DELETE FROM {} WHERE {} RETURNING *",
                quote_sqlite_identifier(ctx.table),
                where_parts.join(" AND ")
            );

            // Bind order: PK value, then inject values
            let mut bind_values: Vec<&serde_json::Value> = Vec::with_capacity(1 + n_inject);
            bind_values.push(&ctx.values[0]);
            for v in &ctx.values[n_client..] {
                bind_values.push(v);
            }
            Ok((sql, bind_values))
        },
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)] // Reason: test code, panics are acceptable
mod tests {
    use serde_json::json;
    use sqlx::Executor as _;

    use super::*;

    /// Create an in-memory adapter and seed a `v_user` table with N rows.
    async fn setup_user_table(n: usize) -> SqliteAdapter {
        let adapter = SqliteAdapter::in_memory().await.expect("Failed to create SQLite adapter");
        adapter
            .pool
            .execute("CREATE TABLE \"v_user\" (id INTEGER PRIMARY KEY, data TEXT)")
            .await
            .expect("Failed to create v_user");
        for i in 1..=n {
            let row = format!(
                r#"INSERT INTO "v_user" (data) VALUES ('{{"id":{i},"name":"user{i}","age":{age},"active":{active},"score":{score},"deleted_at":null}}')"#,
                age = 20 + i,
                active = if i % 2 == 0 { "true" } else { "false" },
                score = i * 10,
            );
            adapter.pool.execute(row.as_str()).await.expect("Failed to insert row");
        }
        adapter
    }

    #[tokio::test]
    async fn test_in_memory_adapter_creation() {
        let adapter = SqliteAdapter::in_memory().await.expect("Failed to create SQLite adapter");

        let metrics = adapter.pool_metrics();
        assert!(metrics.total_connections > 0);
        assert_eq!(adapter.database_type(), DatabaseType::SQLite);
    }

    #[tokio::test]
    async fn test_health_check() {
        let adapter = SqliteAdapter::in_memory().await.expect("Failed to create SQLite adapter");

        adapter.health_check().await.expect("Health check failed");
    }

    #[tokio::test]
    async fn test_raw_query() {
        let adapter = SqliteAdapter::in_memory().await.expect("Failed to create SQLite adapter");

        // Create a test table
        sqlx::query("CREATE TABLE test_table (id INTEGER PRIMARY KEY, data TEXT)")
            .execute(&adapter.pool)
            .await
            .expect("Failed to create table");

        // Insert test data
        sqlx::query("INSERT INTO test_table (data) VALUES ('{\"name\": \"test\"}')")
            .execute(&adapter.pool)
            .await
            .expect("Failed to insert data");

        // Query the data
        let results = adapter
            .execute_raw_query("SELECT * FROM test_table")
            .await
            .expect("Failed to execute query");

        assert_eq!(results.len(), 1);
        assert!(results[0].contains_key("id"));
        assert!(results[0].contains_key("data"));
    }

    #[tokio::test]
    async fn test_parameterized_limit_only() {
        let adapter = SqliteAdapter::in_memory().await.expect("Failed to create SQLite adapter");

        // Create test table
        sqlx::query("CREATE TABLE \"v_user\" (id INTEGER PRIMARY KEY, data TEXT)")
            .execute(&adapter.pool)
            .await
            .expect("Failed to create table");

        // Insert test data
        for i in 1..=5 {
            sqlx::query(&format!(
                "INSERT INTO \"v_user\" (data) VALUES ('{{\"id\": {}, \"name\": \"user{}\"}}') ",
                i, i
            ))
            .execute(&adapter.pool)
            .await
            .expect("Failed to insert data");
        }

        let results = adapter
            .execute_where_query("v_user", None, Some(2), None)
            .await
            .expect("Failed to execute query");

        assert_eq!(results.len(), 2);
    }

    #[tokio::test]
    async fn test_parameterized_offset_only() {
        let adapter = SqliteAdapter::in_memory().await.expect("Failed to create SQLite adapter");

        // Create test table
        sqlx::query("CREATE TABLE \"v_user\" (id INTEGER PRIMARY KEY, data TEXT)")
            .execute(&adapter.pool)
            .await
            .expect("Failed to create table");

        // Insert test data
        for i in 1..=5 {
            sqlx::query(&format!(
                "INSERT INTO \"v_user\" (data) VALUES ('{{\"id\": {}, \"name\": \"user{}\"}}') ",
                i, i
            ))
            .execute(&adapter.pool)
            .await
            .expect("Failed to insert data");
        }

        let results = adapter
            .execute_where_query("v_user", None, None, Some(2))
            .await
            .expect("Failed to execute query");

        assert_eq!(results.len(), 3);
    }

    #[tokio::test]
    async fn test_parameterized_limit_and_offset() {
        let adapter = SqliteAdapter::in_memory().await.expect("Failed to create SQLite adapter");

        // Create test table
        sqlx::query("CREATE TABLE \"v_user\" (id INTEGER PRIMARY KEY, data TEXT)")
            .execute(&adapter.pool)
            .await
            .expect("Failed to create table");

        // Insert test data
        for i in 1..=5 {
            sqlx::query(&format!(
                "INSERT INTO \"v_user\" (data) VALUES ('{{\"id\": {}, \"name\": \"user{}\"}}') ",
                i, i
            ))
            .execute(&adapter.pool)
            .await
            .expect("Failed to insert data");
        }

        let results = adapter
            .execute_where_query("v_user", None, Some(2), Some(1))
            .await
            .expect("Failed to execute query");

        assert_eq!(results.len(), 2);
    }

    #[tokio::test]
    async fn test_function_call_returns_unsupported_error() {
        // SQLite uses DirectSql strategy, not FunctionCall. Calling execute_function_call
        // directly still returns the default Unsupported error because SQLite doesn't
        // override it — mutations go through execute_direct_mutation instead.
        let adapter = SqliteAdapter::in_memory().await.expect("Failed to create SQLite adapter");

        let err = adapter
            .execute_function_call("fn_create_user", &[json!("alice")])
            .await
            .expect_err("Expected Unsupported error");

        assert!(
            matches!(err, FraiseQLError::Unsupported { .. }),
            "Expected Unsupported error, got: {err:?}"
        );
        assert!(
            err.to_string().contains("fn_create_user"),
            "Error message should name the function"
        );
    }

    #[tokio::test]
    async fn test_supports_mutations() {
        let adapter = SqliteAdapter::in_memory().await.unwrap();
        assert!(adapter.supports_mutations());
        assert_eq!(adapter.mutation_strategy(), MutationStrategy::DirectSql);
    }

    // ── Direct mutation tests ────────────────────────────────────────────────

    /// Create an in-memory adapter with a `users` table for mutation testing.
    async fn setup_mutation_table() -> SqliteAdapter {
        let adapter = SqliteAdapter::in_memory().await.expect("Failed to create SQLite adapter");
        adapter
            .pool
            .execute(
                "CREATE TABLE \"users\" (\
                    pk_user INTEGER PRIMARY KEY AUTOINCREMENT, \
                    name TEXT NOT NULL, \
                    email TEXT NOT NULL UNIQUE, \
                    tenant_id TEXT\
                )",
            )
            .await
            .expect("Failed to create users table");
        adapter
    }

    #[tokio::test]
    async fn test_direct_mutation_insert() {
        let adapter = setup_mutation_table().await;
        let columns = vec!["name".to_string(), "email".to_string()];
        let values = vec![json!("Alice"), json!("alice@example.com")];

        let ctx = DirectMutationContext {
            operation:      DirectMutationOp::Insert,
            table:          "users",
            columns:        &columns,
            values:         &values,
            inject_columns: &[],
            return_type:    "User",
        };

        let rows = adapter.execute_direct_mutation(&ctx).await.unwrap();
        assert_eq!(rows.len(), 1);
        let row = &rows[0];
        assert_eq!(row["status"], json!("new"));
        assert_eq!(row["entity_type"], json!("User"));
        assert!(row["entity_id"].is_null());
        let entity = &row["entity"];
        assert_eq!(entity["name"], "Alice");
        assert_eq!(entity["email"], "alice@example.com");
        assert_eq!(entity["pk_user"], 1);
    }

    #[tokio::test]
    async fn test_direct_mutation_insert_with_inject_params() {
        let adapter = setup_mutation_table().await;
        let columns = vec!["name".to_string(), "email".to_string()];
        let inject_columns = vec!["tenant_id".to_string()];
        let values = vec![json!("Bob"), json!("bob@example.com"), json!("tenant-42")];

        let ctx = DirectMutationContext {
            operation:      DirectMutationOp::Insert,
            table:          "users",
            columns:        &columns,
            values:         &values,
            inject_columns: &inject_columns,
            return_type:    "User",
        };

        let rows = adapter.execute_direct_mutation(&ctx).await.unwrap();
        let entity = &rows[0]["entity"];
        assert_eq!(entity["tenant_id"], "tenant-42");
    }

    #[tokio::test]
    async fn test_direct_mutation_update() {
        let adapter = setup_mutation_table().await;
        // Seed a row
        adapter
            .pool
            .execute("INSERT INTO \"users\" (name, email) VALUES ('Alice', 'alice@example.com')")
            .await
            .unwrap();

        let columns = vec!["pk_user".to_string(), "name".to_string()];
        let values = vec![json!(1), json!("Alice Updated")];

        let ctx = DirectMutationContext {
            operation:      DirectMutationOp::Update,
            table:          "users",
            columns:        &columns,
            values:         &values,
            inject_columns: &[],
            return_type:    "User",
        };

        let rows = adapter.execute_direct_mutation(&ctx).await.unwrap();
        let row = &rows[0];
        assert_eq!(row["status"], json!("updated"));
        assert_eq!(row["entity_id"], json!("1"));
        assert_eq!(row["entity"]["name"], "Alice Updated");
        assert_eq!(row["entity"]["email"], "alice@example.com");
    }

    #[tokio::test]
    async fn test_direct_mutation_delete() {
        let adapter = setup_mutation_table().await;
        // Seed a row
        adapter
            .pool
            .execute("INSERT INTO \"users\" (name, email) VALUES ('Alice', 'alice@example.com')")
            .await
            .unwrap();

        let columns = vec!["pk_user".to_string()];
        let values = vec![json!(1)];

        let ctx = DirectMutationContext {
            operation:      DirectMutationOp::Delete,
            table:          "users",
            columns:        &columns,
            values:         &values,
            inject_columns: &[],
            return_type:    "User",
        };

        let rows = adapter.execute_direct_mutation(&ctx).await.unwrap();
        let row = &rows[0];
        assert_eq!(row["status"], json!("deleted"));
        assert_eq!(row["entity_id"], json!("1"));
        assert_eq!(row["entity"]["name"], "Alice");

        // Verify row is actually gone
        let remaining = adapter.execute_raw_query("SELECT * FROM \"users\"").await.unwrap();
        assert!(remaining.is_empty());
    }

    #[tokio::test]
    async fn test_direct_mutation_delete_nonexistent_row() {
        let adapter = setup_mutation_table().await;
        let columns = vec!["pk_user".to_string()];
        let values = vec![json!(999)];

        let ctx = DirectMutationContext {
            operation:      DirectMutationOp::Delete,
            table:          "users",
            columns:        &columns,
            values:         &values,
            inject_columns: &[],
            return_type:    "User",
        };

        let err = adapter
            .execute_direct_mutation(&ctx)
            .await
            .expect_err("Expected error for nonexistent row");
        assert!(matches!(err, FraiseQLError::Validation { .. }));
        assert!(err.to_string().contains("no rows"));
    }

    #[tokio::test]
    async fn test_direct_mutation_constraint_violation() {
        let adapter = setup_mutation_table().await;
        adapter
            .pool
            .execute("INSERT INTO \"users\" (name, email) VALUES ('Alice', 'alice@example.com')")
            .await
            .unwrap();

        // Insert duplicate email (UNIQUE constraint)
        let columns = vec!["name".to_string(), "email".to_string()];
        let values = vec![json!("Bob"), json!("alice@example.com")];

        let ctx = DirectMutationContext {
            operation:      DirectMutationOp::Insert,
            table:          "users",
            columns:        &columns,
            values:         &values,
            inject_columns: &[],
            return_type:    "User",
        };

        let err = adapter
            .execute_direct_mutation(&ctx)
            .await
            .expect_err("Expected constraint violation");
        assert!(matches!(err, FraiseQLError::Database { .. }));
    }

    #[tokio::test]
    async fn test_direct_mutation_null_handling() {
        let adapter = setup_mutation_table().await;
        adapter
            .pool
            .execute("INSERT INTO \"users\" (name, email) VALUES ('Alice', 'alice@example.com')")
            .await
            .unwrap();

        // Update tenant_id to null
        let columns = vec!["pk_user".to_string(), "tenant_id".to_string()];
        let values = vec![json!(1), serde_json::Value::Null];

        let ctx = DirectMutationContext {
            operation:      DirectMutationOp::Update,
            table:          "users",
            columns:        &columns,
            values:         &values,
            inject_columns: &[],
            return_type:    "User",
        };

        let rows = adapter.execute_direct_mutation(&ctx).await.unwrap();
        assert!(rows[0]["entity"]["tenant_id"].is_null());
    }

    // ── WHERE operator matrix ─────────────────────────────────────────────────

    #[tokio::test]
    async fn test_where_eq_operator() {
        let adapter = setup_user_table(5).await;
        let clause = WhereClause::Field {
            path:     vec!["name".to_string()],
            operator: crate::where_clause::WhereOperator::Eq,
            value:    json!("user3"),
        };
        let results =
            adapter.execute_where_query("v_user", Some(&clause), None, None).await.unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].as_value()["name"], "user3");
    }

    #[tokio::test]
    async fn test_where_neq_operator() {
        let adapter = setup_user_table(3).await;
        let clause = WhereClause::Field {
            path:     vec!["name".to_string()],
            operator: crate::where_clause::WhereOperator::Neq,
            value:    json!("user1"),
        };
        let results =
            adapter.execute_where_query("v_user", Some(&clause), None, None).await.unwrap();
        assert_eq!(results.len(), 2);
    }

    #[tokio::test]
    async fn test_where_gt_operator() {
        let adapter = setup_user_table(5).await;
        // age = 20+i, so age > 23 → users 4 and 5
        let clause = WhereClause::Field {
            path:     vec!["age".to_string()],
            operator: crate::where_clause::WhereOperator::Gt,
            value:    json!(23),
        };
        let results =
            adapter.execute_where_query("v_user", Some(&clause), None, None).await.unwrap();
        assert_eq!(results.len(), 2);
    }

    #[tokio::test]
    async fn test_where_gte_operator() {
        let adapter = setup_user_table(5).await;
        // age >= 23 → users 3, 4, 5
        let clause = WhereClause::Field {
            path:     vec!["age".to_string()],
            operator: crate::where_clause::WhereOperator::Gte,
            value:    json!(23),
        };
        let results =
            adapter.execute_where_query("v_user", Some(&clause), None, None).await.unwrap();
        assert_eq!(results.len(), 3);
    }

    #[tokio::test]
    async fn test_where_lt_operator() {
        let adapter = setup_user_table(5).await;
        // age < 23 → users 1 and 2
        let clause = WhereClause::Field {
            path:     vec!["age".to_string()],
            operator: crate::where_clause::WhereOperator::Lt,
            value:    json!(23),
        };
        let results =
            adapter.execute_where_query("v_user", Some(&clause), None, None).await.unwrap();
        assert_eq!(results.len(), 2);
    }

    #[tokio::test]
    async fn test_where_lte_operator() {
        let adapter = setup_user_table(5).await;
        // age <= 23 → users 1, 2, 3
        let clause = WhereClause::Field {
            path:     vec!["age".to_string()],
            operator: crate::where_clause::WhereOperator::Lte,
            value:    json!(23),
        };
        let results =
            adapter.execute_where_query("v_user", Some(&clause), None, None).await.unwrap();
        assert_eq!(results.len(), 3);
    }

    #[tokio::test]
    async fn test_where_in_operator() {
        let adapter = setup_user_table(5).await;
        let clause = WhereClause::Field {
            path:     vec!["name".to_string()],
            operator: crate::where_clause::WhereOperator::In,
            value:    json!(["user1", "user3", "user5"]),
        };
        let results =
            adapter.execute_where_query("v_user", Some(&clause), None, None).await.unwrap();
        assert_eq!(results.len(), 3);
    }

    #[tokio::test]
    async fn test_where_not_in_operator() {
        let adapter = setup_user_table(5).await;
        let clause = WhereClause::Field {
            path:     vec!["name".to_string()],
            operator: crate::where_clause::WhereOperator::Nin,
            value:    json!(["user1", "user2"]),
        };
        let results =
            adapter.execute_where_query("v_user", Some(&clause), None, None).await.unwrap();
        assert_eq!(results.len(), 3);
    }

    #[tokio::test]
    async fn test_where_like_operator() {
        let adapter = setup_user_table(5).await;
        // name LIKE 'user%' matches all 5
        let clause = WhereClause::Field {
            path:     vec!["name".to_string()],
            operator: crate::where_clause::WhereOperator::Like,
            value:    json!("user%"),
        };
        let results =
            adapter.execute_where_query("v_user", Some(&clause), None, None).await.unwrap();
        assert_eq!(results.len(), 5);
    }

    #[tokio::test]
    async fn test_where_is_null_operator() {
        let adapter = setup_user_table(3).await;
        // deleted_at is null for all rows (seeded as null)
        let clause = WhereClause::Field {
            path:     vec!["deleted_at".to_string()],
            operator: crate::where_clause::WhereOperator::IsNull,
            value:    json!(true),
        };
        let results =
            adapter.execute_where_query("v_user", Some(&clause), None, None).await.unwrap();
        assert_eq!(results.len(), 3);
    }

    #[tokio::test]
    async fn test_where_is_not_null_operator() {
        let adapter = setup_user_table(3).await;
        // deleted_at is null → IS NOT NULL returns 0 rows
        let clause = WhereClause::Field {
            path:     vec!["deleted_at".to_string()],
            operator: crate::where_clause::WhereOperator::IsNull,
            value:    json!(false),
        };
        let results =
            adapter.execute_where_query("v_user", Some(&clause), None, None).await.unwrap();
        assert_eq!(results.len(), 0);
    }

    #[tokio::test]
    async fn test_where_multiple_conditions_and() {
        let adapter = setup_user_table(5).await;
        // name = "user2" AND age = 22
        let clause = WhereClause::And(vec![
            WhereClause::Field {
                path:     vec!["name".to_string()],
                operator: crate::where_clause::WhereOperator::Eq,
                value:    json!("user2"),
            },
            WhereClause::Field {
                path:     vec!["age".to_string()],
                operator: crate::where_clause::WhereOperator::Eq,
                value:    json!(22),
            },
        ]);
        let results =
            adapter.execute_where_query("v_user", Some(&clause), None, None).await.unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].as_value()["name"], "user2");
    }

    #[tokio::test]
    async fn test_where_multiple_conditions_or() {
        let adapter = setup_user_table(5).await;
        // name = "user1" OR name = "user5"
        let clause = WhereClause::Or(vec![
            WhereClause::Field {
                path:     vec!["name".to_string()],
                operator: crate::where_clause::WhereOperator::Eq,
                value:    json!("user1"),
            },
            WhereClause::Field {
                path:     vec!["name".to_string()],
                operator: crate::where_clause::WhereOperator::Eq,
                value:    json!("user5"),
            },
        ]);
        let results =
            adapter.execute_where_query("v_user", Some(&clause), None, None).await.unwrap();
        assert_eq!(results.len(), 2);
    }

    // ── Error paths ───────────────────────────────────────────────────────────

    #[tokio::test]
    async fn test_empty_result_set() {
        let adapter = setup_user_table(3).await;
        let clause = WhereClause::Field {
            path:     vec!["name".to_string()],
            operator: crate::where_clause::WhereOperator::Eq,
            value:    json!("nonexistent"),
        };
        let results =
            adapter.execute_where_query("v_user", Some(&clause), None, None).await.unwrap();
        assert!(results.is_empty());
    }

    #[tokio::test]
    async fn test_invalid_raw_query_returns_error() {
        let adapter = SqliteAdapter::in_memory().await.unwrap();
        let err = adapter
            .execute_raw_query("SELECT * FROM nonexistent_table_xyz")
            .await
            .expect_err("Expected database error");
        assert!(matches!(err, FraiseQLError::Database { .. }));
    }

    // ── Pool metrics ──────────────────────────────────────────────────────────

    #[tokio::test]
    async fn test_pool_metrics_when_idle() {
        let adapter = SqliteAdapter::in_memory().await.unwrap();
        let metrics = adapter.pool_metrics();
        // Idle connections should be ≤ total
        assert!(metrics.idle_connections <= metrics.total_connections);
        assert_eq!(metrics.waiting_requests, 0);
    }

    // ── explain_query ─────────────────────────────────────────────────────────

    #[tokio::test]
    async fn test_explain_query_returns_plan() {
        let adapter = setup_user_table(3).await;
        let result = adapter
            .explain_query("SELECT data FROM \"v_user\"", &[])
            .await
            .expect("explain_query should succeed");
        // EXPLAIN QUERY PLAN returns at least one step
        assert!(result.as_array().is_some_and(|a| !a.is_empty()));
    }

    // ── Projection ────────────────────────────────────────────────────────────

    #[tokio::test]
    async fn test_projection_filters_fields() {
        use crate::types::SqlProjectionHint;

        let adapter = setup_user_table(3).await;
        let projection = SqlProjectionHint {
            database:                    crate::DatabaseType::SQLite,
            projection_template:
                "json_object('name', json_extract(data, '$.name')) AS data".to_string(),
            estimated_reduction_percent: 50,
        };
        let results = adapter
            .execute_with_projection("v_user", Some(&projection), None, None)
            .await
            .expect("execute_with_projection should succeed");
        assert_eq!(results.len(), 3);
        // Only 'name' key is present; 'age' should be absent
        for row in &results {
            assert!(row.as_value().get("name").is_some());
            assert!(row.as_value().get("age").is_none());
        }
    }
}
