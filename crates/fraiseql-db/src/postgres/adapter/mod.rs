//! PostgreSQL database adapter implementation.

mod database;
mod query_stats;
mod relay;

#[cfg(test)]
mod tests;

#[cfg(all(test, feature = "test-postgres"))]
mod integration_tests;

use std::{fmt::Write, time::Duration};

use deadpool_postgres::{Config, ManagerConfig, Pool, RecyclingMethod, Runtime};
use fraiseql_error::{FraiseQLError, Result};
use tokio_postgres::{NoTls, Row};

use super::{
    tls::{PostgresConnector, PostgresSslMode, PostgresTlsConfig},
    where_generator::PostgresWhereGenerator,
};
use crate::{
    dialect::PostgresDialect,
    identifier::quote_postgres_identifier,
    order_by::append_order_by,
    traits::DatabaseAdapter,
    types::{
        DatabaseType, JsonbValue, QueryParam,
        sql_hints::{OrderByClause, SqlProjectionHint},
    },
    where_clause::WhereClause,
};

/// Extract the JSONB `data` cell from a result row, failing loud rather than
/// panicking.
///
/// `Row::get` panics on SQL NULL or a non-JSONB column type, so a backing view
/// that projects NULL `data` (e.g. via a LEFT JOIN) or a mistyped `data` column
/// turned a query into a request-path panic — the PostgreSQL adapter was the
/// only backend that aborted here instead of returning an error (audit H34).
/// Both the NULL and the type-mismatch cases now map to
/// [`FraiseQLError::Database`], including a bounded, char-safe slice of the
/// query so an operator can identify the offending view.
fn jsonb_cell<I>(row: &Row, column: I, sql: &str) -> Result<JsonbValue>
where
    I: tokio_postgres::row::RowIndex + std::fmt::Display,
{
    // `.chars().take(..)` is inherently char-boundary-safe (no byte slicing).
    let query_preview = || sql.chars().take(200).collect::<String>();
    match row.try_get::<_, Option<serde_json::Value>>(column) {
        Ok(Some(value)) => Ok(JsonbValue::new(value)),
        Ok(None) => Err(FraiseQLError::Database {
            message:   format!(
                "Query returned a NULL `data` column; the backing view must project a \
                 non-NULL JSONB `data` value (a view yielding NULL `data`, e.g. via a \
                 LEFT JOIN, is unsupported). Query: {}",
                query_preview()
            ),
            sql_state: None,
        }),
        Err(e) => Err(FraiseQLError::Database {
            message:   format!(
                "Failed to read the `data` column as JSONB ({e}); the backing view must \
                 project a JSONB `data` column. Query: {}",
                query_preview()
            ),
            sql_state: None,
        }),
    }
}

/// Default maximum pool size for PostgreSQL connections.
/// Increased from 10 to 25 to prevent pool exhaustion under concurrent
/// nested query load (fixes Issue #41).
const DEFAULT_POOL_SIZE: usize = 25;

/// Maximum retries for connection acquisition with exponential backoff.
const MAX_CONNECTION_RETRIES: u32 = 3;

/// Base delay in milliseconds for connection retry backoff.
const CONNECTION_RETRY_DELAY_MS: u64 = 50;

/// Configuration for connection pool construction and pre-warming.
///
/// Controls the minimum guaranteed connections (pre-warmed at startup),
/// the maximum pool ceiling, and the wait/create timeout for connection
/// acquisition.
///
/// # Example
///
/// ```rust
/// use fraiseql_db::postgres::{PoolPrewarmConfig, PostgresTlsConfig};
///
/// let cfg = PoolPrewarmConfig {
///     min_size:     5,
///     max_size:     20,
///     timeout_secs: Some(30),
///     search_path:  None,
///     // Mandatory: every pool site must state its transport security (#801/#824).
///     tls:          PostgresTlsConfig::default(),
/// };
/// ```
#[derive(Debug, Clone)]
pub struct PoolPrewarmConfig {
    /// Number of connections to establish at pool creation time.
    ///
    /// After the pool is created, `min_size` connections are opened eagerly
    /// so they are ready when the first request arrives. Set to `0` to disable
    /// pre-warming (lazy init — one connection from the startup health check).
    pub min_size: usize,

    /// Maximum number of connections the pool may hold.
    pub max_size: usize,

    /// Optional timeout (in seconds) for connection acquisition and creation.
    ///
    /// Applied to both the `wait` (blocked waiting for an idle connection) and
    /// `create` (time to open a new TCP connection to PostgreSQL) deadpool slots.
    /// When `None`, acquisition can block indefinitely on pool exhaustion.
    pub timeout_secs: Option<u64>,

    /// Schema search path every connection in this pool must resolve unqualified
    /// relations against. `None` leaves the server default in force.
    ///
    /// This is a **pool** property, not a session one: it is lowered into the
    /// PostgreSQL startup `options` parameter, so the server applies it while
    /// establishing each connection — including connections deadpool opens later
    /// to grow the pool, and replacements it creates after a backend dies. See
    /// [`SearchPath`] for why the session-`SET` alternative is unsound.
    pub search_path: Option<SearchPath>,

    /// Transport security for every connection this pool opens.
    ///
    /// Mandatory, and deliberately not `Option`: the pool used to be built with
    /// `NoTls` unconditionally while three separate configuration surfaces claimed
    /// to control database TLS, so each caller must now state what it wants and a
    /// new pool site cannot compile without deciding (#801, #824). Callers with no
    /// opinion pass [`PostgresTlsConfig::default`] (libpq's `prefer`).
    pub tls: PostgresTlsConfig,
}

/// A validated PostgreSQL `search_path`, applied at connection establishment.
///
/// # Why this is a type and not a `SET` statement
///
/// `SET search_path` is *session*-scoped, and a connection pool is N independent
/// sessions. Issuing the `SET` through a pooled query configures whichever single
/// connection was checked out and leaves every other connection — including ones
/// the pool opens later, and every replacement created after a backend
/// disconnects — on the server default. Schema-per-tenant isolation built that way
/// resolves most queries against `public`: silently wrong rows where `public`
/// shadows the relation, an intermittent `relation ... does not exist` where it does
/// not (#809).
///
/// Lowering the path into the startup `options` parameter instead makes it a
/// property of *how connections are made*, so it cannot be missed by a connection
/// and cannot be lost to recycling — `RESET`/`DISCARD ALL` restore the startup
/// value, not the server default.
///
/// # Example
///
/// ```rust
/// use fraiseql_db::postgres::SearchPath;
///
/// let path = SearchPath::new(["tenant_acme", "public"])?;
/// assert_eq!(path.as_str(), "tenant_acme,public");
///
/// // Anything that is not a bare identifier is refused rather than escaped.
/// assert!(SearchPath::new(["public; DROP SCHEMA x"]).is_err());
/// # Ok::<(), fraiseql_error::FraiseQLError>(())
/// ```
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SearchPath(String);

impl SearchPath {
    /// Build a search path from schema names, in resolution order.
    ///
    /// # Errors
    ///
    /// Returns [`FraiseQLError::Validation`] if the list is empty or any entry is
    /// not a bare, unquoted PostgreSQL identifier (`[A-Za-z_][A-Za-z0-9_]*`, at most
    /// 63 characters). Rejecting rather than quoting keeps the emitted startup
    /// option free of characters that could terminate it.
    pub fn new<I, S>(schemas: I) -> Result<Self>
    where
        I: IntoIterator<Item = S>,
        S: AsRef<str>,
    {
        let mut parts: Vec<String> = Vec::new();
        for schema in schemas {
            let name = schema.as_ref();
            let mut chars = name.chars();
            let valid = match chars.next() {
                Some(c) if c.is_ascii_alphabetic() || c == '_' => {
                    chars.all(|c| c.is_ascii_alphanumeric() || c == '_')
                },
                _ => false,
            };
            if !valid || name.len() > MAX_PG_IDENTIFIER_LEN {
                return Err(FraiseQLError::validation(format!(
                    "Schema name '{name}' is not a bare PostgreSQL identifier and cannot be \
                     used in a search path"
                )));
            }
            parts.push(name.to_string());
        }
        if parts.is_empty() {
            return Err(FraiseQLError::validation("A search path needs at least one schema"));
        }
        Ok(Self(parts.join(",")))
    }

    /// The comma-separated path, as it is written into `-c search_path=…`.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl std::fmt::Display for SearchPath {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

/// Maximum length of a PostgreSQL identifier.
const MAX_PG_IDENTIFIER_LEN: usize = 63;

/// Compose the PostgreSQL startup `options` string for a pool.
///
/// Appends `-c search_path=…` to whatever `options` the operator already put in the
/// connection string, rather than replacing it: deadpool's struct-level `options`
/// field overrides the URL-parsed value outright, so building this by assignment
/// alone would silently discard an operator's own `-c` settings.
fn compose_startup_options(connection_string: &str, search_path: &SearchPath) -> String {
    let existing = connection_string
        .parse::<tokio_postgres::Config>()
        .ok()
        .and_then(|c| c.get_options().map(str::to_owned))
        .unwrap_or_default();
    let existing = existing.trim();
    if existing.is_empty() {
        format!("-c search_path={search_path}")
    } else {
        format!("{existing} -c search_path={search_path}")
    }
}

/// Build a `deadpool-postgres` pool with an optional wait/create timeout.
///
/// # Errors
///
/// Returns `FraiseQLError::ConnectionPool` if pool creation fails (e.g., unparseable URL).
fn build_pool(
    connection_string: &str,
    max_size: usize,
    timeout_secs: Option<u64>,
    search_path: Option<&SearchPath>,
    tls: &PostgresTlsConfig,
) -> Result<Pool> {
    let mut cfg = Config::new();
    cfg.url = Some(connection_string.to_string());
    cfg.manager = Some(ManagerConfig {
        recycling_method: RecyclingMethod::Fast,
    });

    // Schema isolation is a property of connection *establishment*, so it rides the
    // startup packet and applies to every connection this pool ever opens — see
    // `SearchPath` for why a pooled `SET` is unsound (#809).
    if let Some(path) = search_path {
        cfg.options = Some(compose_startup_options(connection_string, path));
    }

    let mut pool_cfg = deadpool_postgres::PoolConfig::new(max_size);
    if let Some(secs) = timeout_secs {
        let t = Duration::from_secs(secs);
        pool_cfg.timeouts.wait = Some(t);
        pool_cfg.timeouts.create = Some(t);
        // `recycle` intentionally stays None — fast recycle, not user-configurable.
    }
    cfg.pool = Some(pool_cfg);

    // Transport security is a property of the connection, so it is decided here, at
    // the one place connections are made, rather than by a log line elsewhere (#801).
    //
    // `ssl_mode` is applied by deadpool *after* it parses `cfg.url`, so an explicit
    // setting overrides a `?sslmode=` in the URL while leaving the URL authoritative
    // when the caller expressed no preference. Both routes now reach a connector that
    // can actually negotiate TLS; previously the hard-coded `NoTls` made the driver's
    // own sslmode handling unreachable no matter which surface set it.
    // Only when the operator actually chose one: deadpool applies `ssl_mode` after
    // parsing the URL, so setting it unconditionally would override an explicit
    // `?sslmode=require` with this struct's default and silently downgrade it. The
    // URL form is the one surface that already refused a plaintext server, so it
    // must keep outranking a default nobody wrote.
    cfg.ssl_mode = tls.mode.map(|mode| match mode {
        PostgresSslMode::Disable => deadpool_postgres::SslMode::Disable,
        PostgresSslMode::Prefer => deadpool_postgres::SslMode::Prefer,
        PostgresSslMode::Require | PostgresSslMode::VerifyFull => {
            deadpool_postgres::SslMode::Require
        },
    });

    let pool = match tls.connector()? {
        PostgresConnector::Plaintext => cfg.create_pool(Some(Runtime::Tokio1), NoTls),
        PostgresConnector::Tls(connector) => cfg.create_pool(Some(Runtime::Tokio1), *connector),
    };

    pool.map_err(|e| FraiseQLError::ConnectionPool {
        message: format!("Failed to create connection pool: {e}"),
    })
}

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
/// use fraiseql_db::postgres::PostgresAdapter;
/// use fraiseql_db::{DatabaseAdapter, WhereClause, WhereOperator};
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
///     .execute_where_query("v_user", Some(&where_clause), Some(10), None, None)
///     .await?;
///
/// println!("Found {} users", results.len());
/// # Ok(())
/// # }
/// ```
#[derive(Clone)]
pub struct PostgresAdapter {
    pub(super) pool:         Pool,
    /// Whether mutation timing injection is enabled.
    mutation_timing_enabled: bool,
    /// The PostgreSQL session variable name for timing.
    timing_variable_name:    String,
}

impl std::fmt::Debug for PostgresAdapter {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PostgresAdapter")
            .field("mutation_timing_enabled", &self.mutation_timing_enabled)
            .field("timing_variable_name", &self.timing_variable_name)
            .field("pool", &"<Pool>")
            .finish()
    }
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
    /// # use fraiseql_db::postgres::PostgresAdapter;
    /// # async fn example() -> Result<(), Box<dyn std::error::Error>> {
    /// let adapter = PostgresAdapter::new("postgresql://localhost/mydb").await?;
    /// # Ok(())
    /// # }
    /// ```
    pub async fn new(connection_string: &str) -> Result<Self> {
        Self::with_pool_config(
            connection_string,
            PoolPrewarmConfig {
                min_size:     0,
                max_size:     DEFAULT_POOL_SIZE,
                timeout_secs: None,
                search_path:  None,
                // libpq's default: negotiate TLS when the server offers it. Callers
                // that need a guarantee go through `with_pool_config`.
                tls:          PostgresTlsConfig::default(),
            },
        )
        .await
    }

    /// Create new PostgreSQL adapter with pre-warming and timeout configuration.
    ///
    /// Constructs the pool, runs a startup health check, then eagerly opens
    /// `cfg.min_size` connections so they are ready when the first request arrives.
    ///
    /// # Arguments
    ///
    /// * `connection_string` - PostgreSQL connection string
    /// * `cfg` - Pool pre-warming and timeout configuration
    ///
    /// # Errors
    ///
    /// Returns `FraiseQLError::ConnectionPool` if pool creation or the startup
    /// health check fails.
    pub async fn with_pool_config(connection_string: &str, cfg: PoolPrewarmConfig) -> Result<Self> {
        let pool = build_pool(
            connection_string,
            cfg.max_size,
            cfg.timeout_secs,
            cfg.search_path.as_ref(),
            &cfg.tls,
        )?;

        // Startup health check — establishes the first connection.
        let client = pool.get().await.map_err(|e| FraiseQLError::ConnectionPool {
            message: format!("Failed to acquire connection: {e}"),
        })?;

        client.query("SELECT 1", &[]).await.map_err(|e| FraiseQLError::Database {
            message:   format!("Failed to connect to database: {e}"),
            sql_state: e.code().map(|c| c.code().to_string()),
        })?;

        // Drop client back to the pool before pre-warming so that the health-check
        // connection counts as idle slot #1.
        drop(client);

        let adapter = Self {
            pool,
            mutation_timing_enabled: false,
            timing_variable_name: "fraiseql.started_at".to_string(),
        };

        // Pre-warm: open `min_size - 1` additional connections (one already exists).
        let warm_target = cfg.min_size.min(cfg.max_size).saturating_sub(1);
        if warm_target > 0 {
            adapter.prewarm(warm_target).await;
        }

        Ok(adapter)
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
        Self::with_pool_config(
            connection_string,
            PoolPrewarmConfig {
                min_size: 0,
                max_size,
                timeout_secs: None,
                search_path: None,
                tls: PostgresTlsConfig::default(),
            },
        )
        .await
    }

    /// Pre-warm the pool by opening `count` additional connections.
    ///
    /// Pre-warming is best-effort: failures from individual connections are logged
    /// but do not prevent startup. A 10-second outer timeout ensures the server
    /// never blocks indefinitely on a slow or unreachable PostgreSQL instance.
    async fn prewarm(&self, count: usize) {
        use futures::future::join_all;
        use tokio::time::timeout;

        let handles: Vec<_> = (0..count)
            .map(|_| {
                let pool = self.pool.clone();
                tokio::spawn(async move { pool.get().await })
            })
            .collect();

        let result = timeout(Duration::from_secs(10), join_all(handles)).await;

        let (succeeded, failed) = match result {
            Ok(outcomes) => {
                let s = outcomes
                    .iter()
                    .filter(|r| r.as_ref().map(|inner| inner.is_ok()).unwrap_or(false))
                    .count();
                (s, count - s)
            },
            Err(_elapsed) => {
                tracing::warn!(
                    target_connections = count,
                    "Pool pre-warm timed out after 10s; server will continue with partial pre-warm"
                );
                (0, count)
            },
        };

        if failed > 0 {
            tracing::warn!(
                succeeded,
                failed,
                "Pool pre-warm: some connections could not be established"
            );
        } else {
            tracing::info!(
                idle_connections = succeeded + 1,
                "PostgreSQL pool pre-warmed successfully"
            );
        }
    }

    /// Get a reference to the internal connection pool.
    ///
    /// This allows sharing the pool with other components like `PostgresIntrospector`.
    #[must_use]
    pub const fn pool(&self) -> &Pool {
        &self.pool
    }

    /// Enable mutation timing injection.
    ///
    /// When enabled, `execute_function_call` wraps each mutation in a transaction
    /// and sets a session variable to `clock_timestamp()::text` before execution,
    /// allowing SQL functions to compute their own duration.
    ///
    /// # Arguments
    ///
    /// * `variable_name` - The PostgreSQL session variable name (e.g., `"fraiseql.started_at"`)
    #[must_use]
    pub fn with_mutation_timing(mut self, variable_name: &str) -> Self {
        self.mutation_timing_enabled = true;
        self.timing_variable_name = variable_name.to_string();
        self
    }

    /// Returns whether mutation timing injection is enabled.
    #[must_use]
    pub const fn mutation_timing_enabled(&self) -> bool {
        self.mutation_timing_enabled
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
            .map(|row| jsonb_cell(&row, 0, sql))
            .collect::<Result<Vec<_>>>()?;

        Ok(results)
    }

    /// Like [`execute_raw`](Self::execute_raw) but applies transaction-local
    /// session variables on the same connection / transaction that runs the
    /// query.
    ///
    /// `set_config(..., true)` and the `SELECT` share one transaction, so
    /// PostgreSQL RLS policies backed by `current_setting()` see the configured
    /// values (fixes #329).
    ///
    /// # Errors
    ///
    /// Returns `FraiseQLError::Database` on transaction, `set_config`, query, or
    /// commit failure.
    pub(super) async fn execute_raw_with_session(
        &self,
        sql: &str,
        params: &[&(dyn tokio_postgres::types::ToSql + Sync)],
        session_vars: &[(&str, &str)],
    ) -> Result<Vec<JsonbValue>> {
        let mut client = self.acquire_connection_with_retry().await?;
        let txn =
            client.build_transaction().start().await.map_err(|e| FraiseQLError::Database {
                message:   format!("Failed to start session-var transaction: {e}"),
                sql_state: e.code().map(|c| c.code().to_string()),
            })?;

        database::apply_session_vars(&txn, session_vars).await?;

        let rows: Vec<Row> = txn.query(sql, params).await.map_err(|e| FraiseQLError::Database {
            message:   format!("Query execution failed: {e}"),
            sql_state: e.code().map(|c| c.code().to_string()),
        })?;

        txn.commit().await.map_err(|e| FraiseQLError::Database {
            message:   format!("Failed to commit session-var transaction: {e}"),
            sql_state: e.code().map(|c| c.code().to_string()),
        })?;

        rows.into_iter().map(|row| jsonb_cell(&row, 0, sql)).collect()
    }

    /// Acquire a connection from the pool with retry logic.
    ///
    /// - `PoolError::Timeout`: the pool was exhausted for the full configured wait period. This is
    ///   not transient — retrying would only multiply the wait. Fails immediately.
    /// - `PoolError::Backend` / create errors: potentially transient. Retries with exponential
    ///   backoff (up to `MAX_CONNECTION_RETRIES` attempts).
    ///
    /// # Errors
    ///
    /// Returns `FraiseQLError::ConnectionPool` on timeout or when all retries are exhausted.
    pub(super) async fn acquire_connection_with_retry(&self) -> Result<deadpool_postgres::Client> {
        use deadpool_postgres::PoolError;

        let mut last_error = None;

        for attempt in 0..MAX_CONNECTION_RETRIES {
            match self.pool.get().await {
                Ok(client) => {
                    if attempt > 0 {
                        tracing::info!(attempt, "Successfully acquired connection after retries");
                    }
                    return Ok(client);
                },
                // Pool exhausted for the full wait period — not transient, fail immediately.
                Err(PoolError::Timeout(_)) => {
                    let metrics = self.pool_metrics();
                    tracing::error!(
                        available = metrics.idle_connections,
                        active = metrics.active_connections,
                        max = metrics.total_connections,
                        "Connection pool timeout: all connections busy"
                    );
                    return Err(FraiseQLError::ConnectionPool {
                        message: format!(
                            "Connection pool timeout: {}/{} connections busy. \
                             Increase pool_max_size or reduce concurrent load.",
                            metrics.active_connections, metrics.total_connections,
                        ),
                    });
                },
                // Backend/create errors are potentially transient — retry with backoff.
                Err(e) => {
                    last_error = Some(e);
                    if attempt < MAX_CONNECTION_RETRIES - 1 {
                        let delay = CONNECTION_RETRY_DELAY_MS * (u64::from(attempt) + 1);
                        tracing::warn!(
                            attempt = attempt + 1,
                            total = MAX_CONNECTION_RETRIES,
                            delay_ms = delay,
                            "Transient connection error, retrying"
                        );
                        tokio::time::sleep(Duration::from_millis(delay)).await;
                    }
                },
            }
        }

        // All retries for transient errors exhausted.
        let pool_metrics = self.pool_metrics();
        tracing::error!(
            retries = MAX_CONNECTION_RETRIES,
            available = pool_metrics.idle_connections,
            active = pool_metrics.active_connections,
            max = pool_metrics.total_connections,
            "Failed to acquire connection after all retries"
        );

        Err(FraiseQLError::ConnectionPool {
            message: format!(
                "Failed to acquire connection after {} retries: {}. \
                 Pool state: idle={}, active={}, max={}",
                MAX_CONNECTION_RETRIES,
                last_error.expect("last_error is set on every retry iteration"),
                pool_metrics.idle_connections,
                pool_metrics.active_connections,
                pool_metrics.total_connections,
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
    /// # Panics
    ///
    /// Cannot panic in practice: the inner `expect` is guarded by an `is_none()` check
    /// immediately above it.
    ///
    /// # Example
    ///
    /// ```no_run
    /// // Requires: running PostgreSQL database.
    /// use fraiseql_db::postgres::PostgresAdapter;
    /// use fraiseql_db::types::SqlProjectionHint;
    /// use fraiseql_db::DatabaseType;
    ///
    /// # async fn example(adapter: &PostgresAdapter) -> Result<(), Box<dyn std::error::Error>> {
    /// let projection = SqlProjectionHint::new(
    ///     DatabaseType::PostgreSQL,
    ///     "jsonb_build_object('id', data->>'id')".to_string(),
    ///     75,
    /// );
    ///
    /// let results = adapter
    ///     .execute_with_projection("v_user", Some(&projection), None, Some(10), None)
    ///     .await?;
    /// # Ok(())
    /// # }
    /// ```
    /// Implementation of `execute_with_projection` with ORDER BY support.
    ///
    /// Called by both the inherent convenience method and the `DatabaseAdapter`
    /// trait implementation.
    pub(super) async fn execute_with_projection_impl(
        &self,
        view: &str,
        projection: Option<&SqlProjectionHint>,
        where_clause: Option<&WhereClause>,
        limit: Option<u32>,
        offset: Option<u32>,
        order_by: Option<&[OrderByClause]>,
    ) -> Result<Vec<JsonbValue>> {
        // If no projection, fall back to standard query
        if projection.is_none() {
            return self.execute_where_query(view, where_clause, limit, offset, order_by).await;
        }

        let projection = projection.expect("projection is Some; None was returned above");

        let (sql, typed_params) =
            build_projection_select_sql(projection, view, where_clause, limit, offset, order_by)?;

        tracing::debug!("SQL with projection = {}", sql);
        tracing::debug!("typed_params = {:?}", typed_params);

        let param_refs = crate::types::as_sql_param_refs(&typed_params);

        self.execute_raw(&sql, &param_refs).await
    }

    /// Execute query with SQL field projection optimization.
    ///
    /// Convenience wrapper for callers that don't need ORDER BY.
    /// See `execute_with_projection_impl` for details.
    ///
    /// # Errors
    ///
    /// Returns `FraiseQLError::Database` on query execution failure.
    pub async fn execute_with_projection(
        &self,
        view: &str,
        projection: Option<&SqlProjectionHint>,
        where_clause: Option<&WhereClause>,
        limit: Option<u32>,
        offset: Option<u32>,
    ) -> Result<Vec<JsonbValue>> {
        self.execute_with_projection_impl(view, projection, where_clause, limit, offset, None)
            .await
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
    build_where_select_sql_ordered(view, where_clause, limit, offset, None)
}

/// Build a parameterized `SELECT data FROM {view}` SQL string with optional ORDER BY.
///
/// ORDER BY is inserted between the WHERE clause and LIMIT/OFFSET as required by SQL.
///
/// # Returns
///
/// `(sql, typed_params)` — the SQL string and the bound parameter values.
///
/// # Errors
///
/// Returns `FraiseQLError` if WHERE clause generation or field name validation fails.
pub(super) fn build_where_select_sql_ordered(
    view: &str,
    where_clause: Option<&WhereClause>,
    limit: Option<u32>,
    offset: Option<u32>,
    order_by: Option<&[OrderByClause]>,
) -> Result<(String, Vec<QueryParam>)> {
    // Build base query
    let mut sql = format!("SELECT data FROM {}", quote_postgres_identifier(view));

    // Collect WHERE clause params (if any)
    let mut typed_params: Vec<QueryParam> = if let Some(clause) = where_clause {
        let generator = PostgresWhereGenerator::new(PostgresDialect);
        let (where_sql, where_params) = generator.generate(clause)?;
        sql.push_str(" WHERE ");
        sql.push_str(&where_sql);

        // Convert WHERE clause JSON values to QueryParam
        where_params.into_iter().map(QueryParam::from).collect()
    } else {
        Vec::new()
    };
    let mut param_count = typed_params.len();

    // ORDER BY must come before LIMIT/OFFSET in SQL.
    append_order_by(&mut sql, order_by, DatabaseType::PostgreSQL)?;

    // Add LIMIT as BigInt (PostgreSQL requires integer type for LIMIT).
    // Reason (expect below): fmt::Write for String is infallible.
    if let Some(lim) = limit {
        param_count += 1;
        write!(sql, " LIMIT ${param_count}").expect("write to String");
        typed_params.push(QueryParam::BigInt(i64::from(lim)));
    }

    // Add OFFSET as BigInt (PostgreSQL requires integer type for OFFSET)
    if let Some(off) = offset {
        param_count += 1;
        write!(sql, " OFFSET ${param_count}").expect("write to String");
        typed_params.push(QueryParam::BigInt(i64::from(off)));
    }

    Ok((sql, typed_params))
}

/// Build a parameterized projection `SELECT` SQL string.
///
/// Mirrors the SQL produced inline by [`PostgresAdapter::execute_with_projection_impl`],
/// extracted so the connection-affine `*_with_session` path can reuse it
/// without acquiring its own connection.
///
/// # Returns
///
/// `(sql, typed_params)` — the SQL string and the bound parameter values.
///
/// # Errors
///
/// Returns `FraiseQLError` if WHERE clause generation fails.
pub(super) fn build_projection_select_sql(
    projection: &SqlProjectionHint,
    view: &str,
    where_clause: Option<&WhereClause>,
    limit: Option<u32>,
    offset: Option<u32>,
    order_by: Option<&[OrderByClause]>,
) -> Result<(String, Vec<QueryParam>)> {
    // The projection_template is the SELECT clause with projection SQL,
    // e.g. "jsonb_build_object('id', data->>'id', 'email', data->>'email')".
    let mut sql = format!(
        "SELECT {} FROM {}",
        projection.projection_template,
        quote_postgres_identifier(view)
    );

    let mut typed_params: Vec<QueryParam> = if let Some(clause) = where_clause {
        let generator = PostgresWhereGenerator::new(PostgresDialect);
        let (where_sql, where_params) = generator.generate(clause)?;
        sql.push_str(" WHERE ");
        sql.push_str(&where_sql);
        where_params.into_iter().map(QueryParam::from).collect()
    } else {
        Vec::new()
    };
    let mut param_count = typed_params.len();

    // ORDER BY must come before LIMIT/OFFSET in SQL.
    append_order_by(&mut sql, order_by, DatabaseType::PostgreSQL)?;

    // Append LIMIT/OFFSET as BigInt (PostgreSQL requires integer type).
    // Reason (expect below): fmt::Write for String is infallible.
    if let Some(lim) = limit {
        param_count += 1;
        write!(sql, " LIMIT ${param_count}").expect("write to String");
        typed_params.push(QueryParam::BigInt(i64::from(lim)));
    }

    if let Some(off) = offset {
        param_count += 1;
        write!(sql, " OFFSET ${param_count}").expect("write to String");
        typed_params.push(QueryParam::BigInt(i64::from(off)));
    }

    Ok((sql, typed_params))
}
