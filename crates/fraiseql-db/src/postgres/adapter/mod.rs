//! PostgreSQL database adapter implementation.

mod database;
mod relay;

#[cfg(test)]
mod tests;

#[cfg(all(test, feature = "test-postgres"))]
mod integration_tests;

use deadpool_postgres::{Config, ManagerConfig, Pool, RecyclingMethod, Runtime};
use tokio_postgres::{NoTls, Row};

use super::where_generator::PostgresWhereGenerator;
use fraiseql_error::{FraiseQLError, Result};

use crate::{
    identifier::quote_postgres_identifier,
    traits::DatabaseAdapter,
    types::{JsonbValue, QueryParam},
    types::sql_hints::SqlProjectionHint,
    where_clause::WhereClause,
};

/// Default maximum pool size for PostgreSQL connections.
/// Increased from 10 to 25 to prevent pool exhaustion under concurrent
/// nested query load (fixes Issue #41).
const DEFAULT_POOL_SIZE: usize = 25;

/// Maximum retries for connection acquisition with exponential backoff.
const MAX_CONNECTION_RETRIES: u32 = 3;

/// Base delay in milliseconds for connection retry backoff.
const CONNECTION_RETRY_DELAY_MS: u64 = 50;

/// Escape a JSONB key for use in a PostgreSQL string literal (`data->>'key'`).
///
/// PostgreSQL string literals use single-quote doubling for escaping (`'` → `''`).
/// This function is defense-in-depth: `OrderByClause` already rejects field names
/// that are not valid GraphQL identifiers (which cannot contain `'`), but this
/// escaping ensures correctness for any future caller that bypasses that validation.
pub(super) fn escape_jsonb_key(key: &str) -> String {
    key.replace('\'', "''")
}


/// PostgreSQL database adapter with connection pooling.
///
/// Uses `deadpool-postgres` for connection pooling and `tokio-postgres` for async queries.
///
/// # Example
///
/// ```rust,no_run
/// use fraiseql_core::db::postgres::PostgresAdapter;
/// use fraiseql_core::db::{DatabaseAdapter, WhereClause, WhereOperator};
/// use serde_json::json;
///
/// # async fn example() -> Result<(), Box<dyn std::error::Error>> {
/// // Create adapter with connection string
/// let adapter = PostgresAdapter::new("postgresql://localhost/mydb").await?;
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
pub struct PostgresAdapter {
    pub(super) pool: Pool,
}

impl PostgresAdapter {
    /// Create new PostgreSQL adapter with default pool configuration.
    ///
    /// # Arguments
    ///
    /// * `connection_string` - PostgreSQL connection string (e.g., "postgresql://localhost/mydb")
    ///
    /// # Errors
    ///
    /// Returns `FraiseQLError::ConnectionPool` if pool creation fails.
    ///
    /// # Example
    ///
    /// ```rust,no_run
    /// # use fraiseql_core::db::postgres::PostgresAdapter;
    /// # async fn example() -> Result<(), Box<dyn std::error::Error>> {
    /// let adapter = PostgresAdapter::new("postgresql://localhost/mydb").await?;
    /// # Ok(())
    /// # }
    /// ```
    pub async fn new(connection_string: &str) -> Result<Self> {
        Self::with_pool_size(connection_string, DEFAULT_POOL_SIZE).await
    }

    /// Create new PostgreSQL adapter with custom pool configuration.
    ///
    /// # Arguments
    ///
    /// * `connection_string` - PostgreSQL connection string
    /// * `min_size` - Minimum size hint (not enforced by deadpool-postgres)
    /// * `max_size` - Maximum number of connections in pool
    ///
    /// # Errors
    ///
    /// Returns `FraiseQLError::ConnectionPool` if pool creation fails.
    ///
    /// # Note
    ///
    /// `min_size` is accepted for API compatibility but deadpool-postgres uses
    /// lazy initialization with dynamic pool sizing up to `max_size`.
    pub async fn with_pool_config(
        connection_string: &str,
        _min_size: usize,
        max_size: usize,
    ) -> Result<Self> {
        Self::with_pool_size(connection_string, max_size).await
    }

    /// Create new PostgreSQL adapter with custom pool size.
    ///
    /// # Arguments
    ///
    /// * `connection_string` - PostgreSQL connection string
    /// * `max_size` - Maximum number of connections in pool
    ///
    /// # Errors
    ///
    /// Returns `FraiseQLError::ConnectionPool` if pool creation fails.
    pub async fn with_pool_size(connection_string: &str, max_size: usize) -> Result<Self> {
        let mut cfg = Config::new();
        cfg.url = Some(connection_string.to_string());
        cfg.manager = Some(ManagerConfig {
            recycling_method: RecyclingMethod::Fast,
        });
        cfg.pool = Some(deadpool_postgres::PoolConfig::new(max_size));

        let pool = cfg.create_pool(Some(Runtime::Tokio1), NoTls).map_err(|e| {
            FraiseQLError::ConnectionPool {
                message: format!("Failed to create connection pool: {e}"),
            }
        })?;

        // Test connection
        let client = pool.get().await.map_err(|e| FraiseQLError::ConnectionPool {
            message: format!("Failed to acquire connection: {e}"),
        })?;

        client.query("SELECT 1", &[]).await.map_err(|e| FraiseQLError::Database {
            message:   format!("Failed to connect to database: {e}"),
            sql_state: e.code().map(|c| c.code().to_string()),
        })?;

        Ok(Self { pool })
    }

    /// Get a reference to the internal connection pool.
    ///
    /// This allows sharing the pool with other components like `PostgresIntrospector`.
    #[must_use]
    pub const fn pool(&self) -> &Pool {
        &self.pool
    }

    /// Execute raw SQL query and return JSONB rows.
    ///
    /// # Errors
    ///
    /// Returns `FraiseQLError::Database` on query execution failure.
    pub(super) async fn execute_raw(
        &self,
        sql: &str,
        params: &[&(dyn tokio_postgres::types::ToSql + Sync)],
    ) -> Result<Vec<JsonbValue>> {
        let client = self.acquire_connection_with_retry().await?;

        let rows: Vec<Row> =
            client.query(sql, params).await.map_err(|e| FraiseQLError::Database {
                message:   format!("Query execution failed: {e}"),
                sql_state: e.code().map(|c| c.code().to_string()),
            })?;

        let results = rows
            .into_iter()
            .map(|row| {
                let data: serde_json::Value = row.get(0);
                JsonbValue::new(data)
            })
            .collect();

        Ok(results)
    }

    /// Acquire a connection from the pool with retry logic.
    ///
    /// Implements exponential backoff retry when the pool is exhausted.
    /// This prevents transient pool exhaustion from causing query failures
    /// under concurrent load (fixes Issue #41).
    ///
    /// # Errors
    ///
    /// Returns `FraiseQLError::ConnectionPool` if all retries are exhausted.
    pub(super) async fn acquire_connection_with_retry(&self) -> Result<deadpool_postgres::Client> {
        let mut last_error = None;

        for attempt in 0..MAX_CONNECTION_RETRIES {
            match self.pool.get().await {
                Ok(client) => {
                    if attempt > 0 {
                        tracing::info!(
                            "Successfully acquired connection after {} retries",
                            attempt
                        );
                    }
                    return Ok(client);
                },
                Err(e) => {
                    last_error = Some(e);
                    if attempt < MAX_CONNECTION_RETRIES - 1 {
                        let delay = CONNECTION_RETRY_DELAY_MS * (u64::from(attempt) + 1);
                        tracing::warn!(
                            "Connection pool exhausted (attempt {}/{}), retrying in {}ms...",
                            attempt + 1,
                            MAX_CONNECTION_RETRIES,
                            delay
                        );
                        tokio::time::sleep(std::time::Duration::from_millis(delay)).await;
                    }
                },
            }
        }

        // All retries exhausted - log detailed pool state
        let pool_metrics = self.pool_metrics();
        tracing::error!(
            "Failed to acquire connection after {} retries. Pool state: available={}, active={}, total={}",
            MAX_CONNECTION_RETRIES,
            pool_metrics.idle_connections,
            pool_metrics.active_connections,
            pool_metrics.total_connections
        );

        Err(FraiseQLError::ConnectionPool {
            message: format!(
                "Failed to acquire connection after {} retries: {}. Pool exhausted (available={}/{}). Consider increasing pool size or reducing concurrent load.",
                MAX_CONNECTION_RETRIES,
                last_error.expect("last_error is set on every retry iteration"),
                pool_metrics.idle_connections,
                pool_metrics.total_connections
            ),
        })
    }

    /// Execute query with SQL field projection optimization.
    ///
    /// Uses the provided `SqlProjectionHint` to generate optimized SQL that projects
    /// only the requested fields from the JSONB column, reducing network payload and
    /// JSON deserialization overhead.
    ///
    /// # Arguments
    ///
    /// * `view` - View/table name to query
    /// * `projection` - Optional SQL projection hint with field list
    /// * `where_clause` - Optional WHERE clause for filtering
    /// * `limit` - Optional row limit
    ///
    /// # Returns
    ///
    /// Vector of projected JSONB rows with only the requested fields
    ///
    /// # Errors
    ///
    /// Returns `FraiseQLError::Database` on query execution failure.
    ///
    /// # Example
    ///
    /// ```no_run
    /// // Requires: running PostgreSQL database.
    /// use fraiseql_db::postgres::PostgresAdapter;
    /// use fraiseql_db::types::SqlProjectionHint;
    ///
    /// # async fn example(adapter: &PostgresAdapter) -> Result<(), Box<dyn std::error::Error>> {
    /// let projection = SqlProjectionHint {
    ///     database: "postgresql".to_string(),
    ///     projection_template: "jsonb_build_object('id', data->>'id')".to_string(),
    ///     estimated_reduction_percent: 75,
    /// };
    ///
    /// let results = adapter
    ///     .execute_with_projection("v_user", Some(&projection), None, Some(10))
    ///     .await?;
    /// # Ok(())
    /// # }
    /// ```
    pub async fn execute_with_projection(
        &self,
        view: &str,
        projection: Option<&SqlProjectionHint>,
        where_clause: Option<&WhereClause>,
        limit: Option<u32>,
    ) -> Result<Vec<JsonbValue>> {
        // If no projection, fall back to standard query
        if projection.is_none() {
            return self.execute_where_query(view, where_clause, limit, None).await;
        }

        let projection = projection.expect("projection is Some; None was returned above");

        // Build SQL with projection
        // The projection_template is expected to be the SELECT clause with projection SQL
        // e.g., "jsonb_build_object('id', data->>'id', 'email', data->>'email')"
        let mut sql = format!(
            "SELECT {} FROM {}",
            projection.projection_template,
            quote_postgres_identifier(view)
        );

        // Add WHERE clause if present
        if let Some(clause) = where_clause {
            let generator = PostgresWhereGenerator::new();
            let (where_sql, where_params) = generator.generate(clause)?;
            sql.push_str(" WHERE ");
            sql.push_str(&where_sql);

            // Add parameterized LIMIT
            let mut params = where_params;
            let mut param_count = params.len();

            if let Some(lim) = limit {
                param_count += 1;
                sql.push_str(&format!(" LIMIT ${param_count}"));
                params.push(serde_json::Value::Number(lim.into()));
            }

            // Convert JSON values to QueryParam (preserves types)
            let typed_params: Vec<QueryParam> = params.into_iter().map(QueryParam::from).collect();

            tracing::debug!("SQL with projection = {}", sql);
            tracing::debug!("typed_params = {:?}", typed_params);

            // Create references to QueryParam for ToSql
            let param_refs: Vec<&(dyn tokio_postgres::types::ToSql + Sync)> = typed_params
                .iter()
                .map(|p| p as &(dyn tokio_postgres::types::ToSql + Sync))
                .collect();

            self.execute_raw(&sql, &param_refs).await
        } else {
            // No WHERE clause
            let mut params: Vec<serde_json::Value> = vec![];
            let mut param_count = 0;

            if let Some(lim) = limit {
                param_count += 1;
                sql.push_str(&format!(" LIMIT ${param_count}"));
                params.push(serde_json::Value::Number(lim.into()));
            }

            // Convert JSON values to QueryParam (preserves types)
            let typed_params: Vec<QueryParam> = params.into_iter().map(QueryParam::from).collect();

            // Create references to QueryParam for ToSql
            let param_refs: Vec<&(dyn tokio_postgres::types::ToSql + Sync)> = typed_params
                .iter()
                .map(|p| p as &(dyn tokio_postgres::types::ToSql + Sync))
                .collect();

            self.execute_raw(&sql, &param_refs).await
        }
    }
}

/// Build a parameterized `SELECT data FROM {view}` SQL string.
///
/// Shared by [`PostgresAdapter::execute_where_query`] and
/// [`PostgresAdapter::explain_where_query`] so that SQL construction
/// logic is never duplicated.
///
/// # Returns
///
/// `(sql, typed_params)` — the SQL string and the bound parameter values.
///
/// # Errors
///
/// Returns `FraiseQLError` if WHERE clause generation fails.
pub(super) fn build_where_select_sql(
    view: &str,
    where_clause: Option<&WhereClause>,
    limit: Option<u32>,
    offset: Option<u32>,
) -> Result<(String, Vec<QueryParam>)> {
    // Build base query
    let mut sql = format!("SELECT data FROM {}", quote_postgres_identifier(view));

    // Collect WHERE clause params (if any)
    let mut typed_params: Vec<QueryParam> = if let Some(clause) = where_clause {
        let generator = PostgresWhereGenerator::new();
        let (where_sql, where_params) = generator.generate(clause)?;
        sql.push_str(" WHERE ");
        sql.push_str(&where_sql);

        // Convert WHERE clause JSON values to QueryParam
        where_params.into_iter().map(QueryParam::from).collect()
    } else {
        Vec::new()
    };
    let mut param_count = typed_params.len();

    // Add LIMIT as BigInt (PostgreSQL requires integer type for LIMIT)
    if let Some(lim) = limit {
        param_count += 1;
        sql.push_str(&format!(" LIMIT ${param_count}"));
        typed_params.push(QueryParam::BigInt(i64::from(lim)));
    }

    // Add OFFSET as BigInt (PostgreSQL requires integer type for OFFSET)
    if let Some(off) = offset {
        param_count += 1;
        sql.push_str(&format!(" OFFSET ${param_count}"));
        typed_params.push(QueryParam::BigInt(i64::from(off)));
    }

    Ok((sql, typed_params))
}
