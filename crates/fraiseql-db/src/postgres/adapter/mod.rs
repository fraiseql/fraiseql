//! PostgreSQL database adapter implementation.

mod database;
mod numeric;
mod query_stats;
mod relay;

#[cfg(test)]
mod tests;

#[cfg(all(test, feature = "test-postgres"))]
mod integration_tests;

use std::{
    fmt::Write,
    sync::{
        Arc, Weak,
        atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering},
    },
    time::Duration,
};

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
    postgres::pg_detail,
    traits::DatabaseAdapter,
    types::{
        DatabaseType, JsonbValue, QueryParam, ReadRouting,
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
///     min_size:      5,
///     max_size:      20,
///     timeout_secs:  Some(30),
///     search_path:   None,
///     // Mandatory: every pool site must state its transport security (#801/#824).
///     tls:           PostgresTlsConfig::default(),
///     // Mandatory: every pool site must state its replica topology (#407).
///     read_replicas: None,
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

    /// Read replicas for this pool set, or `None` for a single-primary adapter.
    ///
    /// Like [`tls`](Self::tls), this is a mandatory decision at every pool site:
    /// replica pools are built **from this same config** — the same
    /// [`search_path`](Self::search_path), the same [`tls`](Self::tls), the same
    /// sizing — so tenant isolation and transport security cannot silently differ
    /// between the primary and a replica (#809 generalised: isolation is a
    /// property of how *every* pool's connections are made). See
    /// [`ReadReplicaConfig`] for the routing and consistency rules (#407).
    pub read_replicas: Option<ReadReplicaConfig>,
}

/// Read-replica configuration for a PostgreSQL adapter (#407).
///
/// # Routing model
///
/// FraiseQL's read/write partition is **static**: compiled GraphQL queries execute
/// through the adapter's structurally read-only methods (`execute_where_query*`,
/// `execute_with_projection*`, `execute_parameterized_aggregate*`, `explain_*`,
/// relay pagination), and those route to a replica selected round-robin. The
/// mutation pipeline (`execute_function_call*`) and every mixed-use or
/// administrative surface (`execute_raw_query`, `execute_row_query`, query stats,
/// health checks, schema DDL) always run on the primary — a surface that *can*
/// write is never routed to a replica.
///
/// # Read-your-writes
///
/// Every mutation-pipeline write arms a shared watermark; for
/// [`pin_after_write`](Self::pin_after_write) afterwards, reads route to the
/// primary so replication lag cannot serve a client its own stale write. The
/// window is an operator assertion about worst-case replica lag: writes that
/// bypass the mutation pipeline (raw SQL, out-of-band jobs) do not arm it, and a
/// replica lagging beyond the window can serve stale — but never torn — rows.
///
/// # Failure behaviour
///
/// A replica that is unreachable at boot **refuses to boot** (a configured-but-
/// unusable pool must not downgrade silently). A replica that fails at runtime is
/// skipped for that acquisition — the next replica is tried, then the primary —
/// so replica loss degrades read capacity, never read availability.
/// # Bounded staleness
///
/// [`max_lag`](Self::max_lag) turns the pin window's *assertion* into a
/// *measurement*: a replica is only eligible while its last probed replay lag,
/// aged by how long ago that probe ran, is still inside the budget.
///
/// That sum is a true upper bound rather than an estimate — replay lag grows at
/// most one millisecond per millisecond of wall clock, so however stale the probe
/// is, the lag cannot have grown by more than its age — and it makes the gate
/// self-closing: an unrefreshed probe ages past any budget on its own. A replica
/// whose lag cannot be *measured* at all is never eligible; unknown staleness is
/// not zero staleness.
#[derive(Debug, Clone)]
pub struct ReadReplicaConfig {
    /// Connection URLs of the read replicas, tried round-robin. Must be non-empty.
    pub urls: Vec<String>,

    /// How long after a mutation-pipeline write reads keep routing to the primary.
    ///
    /// Must be at least the worst replication lag the operator is prepared to
    /// tolerate; within it, a client is guaranteed to read its own writes.
    pub pin_after_write: Duration,

    /// Bounded staleness: the most replication lag a replica-served read may
    /// carry. `None` disables lag-based routing entirely — replicas are then
    /// selected round-robin regardless of how far behind they are, which is the
    /// behaviour before this option existed.
    ///
    /// A replica whose lag cannot be measured — never probed, last probe failed,
    /// no transaction replayed yet, or promoted out of recovery by a failover —
    /// is **not** eligible while this is set. Unknown staleness is not "zero
    /// staleness", and the primary is always a correct answer.
    ///
    /// Must be strictly greater than [`health_probe_interval`](Self::health_probe_interval):
    /// eligibility ages the last probe, so a budget no larger than the probe
    /// period would make even a perfectly caught-up replica ineligible for part
    /// of every cycle.
    pub max_lag: Option<Duration>,

    /// How often each replica is probed for recovery state and replay lag.
    ///
    /// Probing is unconditional when replicas are configured: the boot health
    /// check runs once, and a replica promoted by a failover after boot is a
    /// writable server that a `pg_is_in_recovery()` reading taken at startup can
    /// never notice.
    pub health_probe_interval: Duration,
}

/// [`ReadReplicaConfig`] without the URLs: how replica routing *behaves*,
/// separated from *where* the replicas are (#957).
///
/// The split exists because the two halves have different owners once tenant
/// pools carry replicas. A tenant registration names its own replica URLs, the
/// same way it names its own primary connection string — but the pin window, the
/// staleness budget and the probe cadence are the operator's policy, stamped
/// onto every tenant pool by `make_executor_factory`, exactly as `[database_tls]`
/// is (#801). A tenant that could send its own `max_lag` in a registration body
/// would be choosing how stale *its* reads may be, against a server whose
/// operator already decided.
///
/// [`Default`] carries the shipped defaults — a 5 s pin, no staleness budget, a
/// 1 s probe cadence — so they live in one place rather than once per binary.
#[derive(Debug, Clone)]
pub struct ReadReplicaPolicy {
    /// See [`ReadReplicaConfig::pin_after_write`].
    pub pin_after_write:       Duration,
    /// See [`ReadReplicaConfig::max_lag`].
    pub max_lag:               Option<Duration>,
    /// See [`ReadReplicaConfig::health_probe_interval`].
    pub health_probe_interval: Duration,
}

impl Default for ReadReplicaPolicy {
    fn default() -> Self {
        Self {
            pin_after_write:       Duration::from_millis(5000),
            max_lag:               None,
            health_probe_interval: Duration::from_millis(1000),
        }
    }
}

impl ReadReplicaPolicy {
    /// Apply this policy to `urls`, or `None` when there are no replicas.
    ///
    /// Returning `None` for an empty list is what keeps a pool site that has no
    /// replicas from constructing the one shape
    /// [`ReadReplicaConfig`](ReadReplicaConfig) refuses to build.
    #[must_use]
    pub fn with_urls(&self, urls: Vec<String>) -> Option<ReadReplicaConfig> {
        if urls.is_empty() {
            return None;
        }
        Some(ReadReplicaConfig {
            urls,
            pin_after_write: self.pin_after_write,
            max_lag: self.max_lag,
            health_probe_interval: self.health_probe_interval,
        })
    }
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

/// Build and boot-verify the replica pool set (#407).
///
/// Each replica pool inherits the primary's `search_path`, TLS, sizing and
/// timeouts from `cfg`. Every replica is health-checked at boot; an unreachable
/// replica refuses to boot rather than silently shrinking the read fleet, and a
/// reachable server that is not in recovery (not actually a standby) is allowed
/// but loudly logged — dev rigs legitimately stand in a plain database for a
/// replica, but in production that warning means reads may diverge from the
/// primary in both directions.
///
/// # Errors
///
/// Returns [`FraiseQLError::Validation`] when `rc.urls` is empty, and
/// [`FraiseQLError::ConnectionPool`] / [`FraiseQLError::Database`] when a replica
/// pool cannot be created or its boot health check fails.
async fn build_read_replica_set(
    rc: &ReadReplicaConfig,
    cfg: &PoolPrewarmConfig,
) -> Result<ReadReplicaSet> {
    if rc.urls.is_empty() {
        return Err(FraiseQLError::validation(
            "read_replicas was configured with an empty URL list; remove the configuration \
             or list at least one replica",
        ));
    }
    if rc.health_probe_interval.is_zero() {
        return Err(FraiseQLError::validation(
            "read replica health_probe_interval is zero; probing every replica in a tight \
             loop is never intended. Set an interval, or remove the replica configuration.",
        ));
    }
    if rc.max_lag.is_some_and(|max_lag| max_lag <= rc.health_probe_interval) {
        return Err(FraiseQLError::validation(format!(
            "read replica max_lag ({} ms) must be greater than health_probe_interval ({} ms): \
             a replica's eligibility ages its last probe, so a staleness budget no larger than \
             the probe period would drop even a fully caught-up replica out of rotation for \
             part of every cycle. Lower the probe interval or raise the budget.",
            rc.max_lag.unwrap_or_default().as_millis(),
            rc.health_probe_interval.as_millis(),
        )));
    }

    let mut pools = Vec::with_capacity(rc.urls.len());
    let mut health = Vec::with_capacity(rc.urls.len());
    for (index, url) in rc.urls.iter().enumerate() {
        let pool =
            build_pool(url, cfg.max_size, cfg.timeout_secs, cfg.search_path.as_ref(), &cfg.tls)?;

        // Boot health check: a configured replica that cannot serve refuses to
        // boot — the alternative is a read fleet that silently collapsed onto
        // the primary before the first request.
        let client = pool.get().await.map_err(|e| FraiseQLError::ConnectionPool {
            message: format!(
                "Read replica {index} is unreachable at boot ({e}); refusing to start"
            ),
        })?;
        let row = client.query_one("SELECT pg_is_in_recovery()", &[]).await.map_err(|e| {
            FraiseQLError::Database {
                message:   format!(
                    "Read replica {index} failed its boot health check: {}",
                    pg_detail(&e)
                ),
                sql_state: e.code().map(|c| c.code().to_string()),
            }
        })?;
        let in_recovery: bool = row.get(0);
        if !in_recovery {
            tracing::warn!(
                replica = index,
                "Configured read replica is not in recovery (pg_is_in_recovery() = false); it \
                 is a writable server, not a standby. Reads routed to it may diverge from the \
                 primary."
            );
        }
        drop(client);

        pools.push(pool);
        health.push(ReplicaHealth::booted(in_recovery));
    }

    // Reason (fallible u64::try_from + saturation): a pin window beyond u64::MAX
    // milliseconds (~585 million years) saturates to "pin forever", which is the
    // conservative, primary-only side.
    let pin_after_write_ms = u64::try_from(rc.pin_after_write.as_millis()).unwrap_or(u64::MAX);
    // Reason (saturation, opposite direction): a budget that overflows is
    // effectively unbounded, and an unbounded budget is what `None` already
    // means, so saturating to u64::MAX changes nothing an operator could observe.
    let max_lag_ms = rc.max_lag.map(|d| u64::try_from(d.as_millis()).unwrap_or(u64::MAX));

    Ok(ReadReplicaSet {
        pools,
        health,
        next: AtomicUsize::new(0),
        pin_after_write_ms,
        max_lag_ms,
        probe_interval: rc.health_probe_interval,
        last_write_unix_ms: AtomicU64::new(0),
    })
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
    /// Read-replica pool set, or `None` for a single-primary adapter (#407).
    read_replicas:           Option<Arc<ReadReplicaSet>>,
    /// Whether mutation timing injection is enabled.
    mutation_timing_enabled: bool,
    /// The PostgreSQL session variable name for timing.
    timing_variable_name:    String,
}

/// The built replica pools plus the shared read-your-writes watermark.
///
/// Held behind one `Arc` so adapter clones share a single round-robin cursor and
/// a single write watermark — a clone that kept its own watermark would let a
/// write through one clone go unseen by reads through another.
struct ReadReplicaSet {
    pools:              Vec<Pool>,
    /// Per-replica probe results, parallel to [`pools`](Self::pools).
    health:             Vec<ReplicaHealth>,
    /// Round-robin cursor over `pools`.
    next:               AtomicUsize,
    /// [`ReadReplicaConfig::pin_after_write`], in milliseconds.
    pin_after_write_ms: u64,
    /// [`ReadReplicaConfig::max_lag`], in milliseconds; `None` = no lag gating.
    max_lag_ms:         Option<u64>,
    /// [`ReadReplicaConfig::health_probe_interval`].
    probe_interval:     Duration,
    /// Unix-epoch milliseconds of the most recent mutation-pipeline write;
    /// `0` = never written.
    last_write_unix_ms: AtomicU64,
}

/// What the last health probe observed about one replica (#957).
///
/// # Why a probe and not a measurement on the read path
///
/// Measuring lag inline would add a round trip to every replica-served read —
/// the cost of the routing would exceed the cost of the read. The probe runs on
/// its own cadence and the read path consults its last result, which means the
/// result is *old*, and eligibility has to account for that.
///
/// # The staleness bound
///
/// A replica is eligible while `lag_at_probe + probe_age <= max_lag`.
///
/// That sum is a true upper bound on the replica's staleness right now, not an
/// approximation: replay lag measured as "how long ago was the newest applied
/// transaction committed" grows at most one millisecond per millisecond of wall
/// clock (it grows exactly 1:1 while replay is stalled and shrinks whenever
/// replay advances), so however far the probe has aged, the lag cannot have
/// grown by more than that age.
///
/// It also makes the gate self-closing. Nothing has to notice that probing
/// stopped — a probe that stops being refreshed ages past any budget on its own,
/// and the replica falls out of rotation.
struct ReplicaHealth {
    /// Replay lag in milliseconds at [`probed_at_unix_ms`](Self::probed_at_unix_ms),
    /// or [`LAG_UNMEASURABLE`] when the last probe could not produce one.
    lag_ms:             AtomicU64,
    /// Unix-epoch milliseconds of the last *successful* probe; `0` = never
    /// probed, which reads as infinitely aged and therefore ineligible.
    probed_at_unix_ms:  AtomicU64,
    /// `pg_is_in_recovery()` as of the last successful probe. Seeded from the
    /// boot health check so the first probe can report a *transition* — a
    /// replica that left recovery between boot and now was promoted by a
    /// failover, and is a writable server whose contents may diverge from the
    /// primary in both directions.
    in_recovery:        AtomicBool,
    /// `pg_is_in_recovery()` at the boot health check, which is what makes the
    /// transition legible.
    ///
    /// Booting outside recovery is *accepted* with a warning, because a dev rig
    /// legitimately stands a plain database in for a replica; a server that
    /// booted as a standby and later is not one has been promoted, and no dev
    /// rig does that by accident. Only the second case stops taking reads —
    /// which is why the boot reading has to be kept, not just the latest one.
    booted_in_recovery: bool,
}

/// Sentinel [`ReplicaHealth::lag_ms`]: the last probe produced no lag reading.
///
/// A promoted replica (`pg_last_wal_replay_lsn()` is NULL outside recovery), a
/// standby that has replayed no transaction yet, or a probe that failed
/// outright. Saturating arithmetic keeps it above every budget, so "unknown"
/// routes to the primary rather than being read as "zero".
const LAG_UNMEASURABLE: u64 = u64::MAX;

/// One round trip returning a replica's recovery state and replay lag.
///
/// `pg_last_xact_replay_timestamp()` alone over-reports on an idle primary — it
/// answers "how old is the newest transaction I applied", which keeps growing
/// when there is nothing left to apply. The first branch is the standard
/// correction: a standby whose received WAL position equals its replayed one has
/// applied everything it has been sent, and is by definition not behind.
const REPLICA_PROBE_SQL: &str = "SELECT pg_is_in_recovery(), \
     CASE \
       WHEN NOT pg_is_in_recovery() THEN NULL::bigint \
       WHEN pg_last_wal_replay_lsn() IS NOT NULL \
            AND pg_last_wal_receive_lsn() = pg_last_wal_replay_lsn() THEN 0::bigint \
       WHEN pg_last_xact_replay_timestamp() IS NULL THEN NULL::bigint \
       ELSE GREATEST(0, EXTRACT(EPOCH FROM (now() - pg_last_xact_replay_timestamp())) * 1000)::bigint \
     END";

impl ReplicaHealth {
    /// Seed from the boot health check, before any probe has run.
    const fn booted(in_recovery: bool) -> Self {
        Self {
            lag_ms:             AtomicU64::new(LAG_UNMEASURABLE),
            probed_at_unix_ms:  AtomicU64::new(0),
            in_recovery:        AtomicBool::new(in_recovery),
            booted_in_recovery: in_recovery,
        }
    }

    /// Whether this replica is still the kind of server it booted as.
    ///
    /// `false` only for the failover case: it was a standby at boot and the last
    /// probe found it outside recovery, so something promoted it. That server is
    /// now accepting writes of its own, and reads served from it can diverge
    /// from the primary in *both* directions — unlike ordinary lag, which only
    /// ever shows the past. It stops taking reads whether or not a staleness
    /// budget is configured, because there is no budget under which "a different
    /// database" is acceptable.
    ///
    /// A replica that booted outside recovery keeps the acceptance the boot check
    /// gave it (dev rigs stand a plain database in for a standby, and the boot
    /// warning already says so).
    fn is_still_a_standby(&self) -> bool {
        !self.booted_in_recovery || self.in_recovery.load(Ordering::Relaxed)
    }

    /// Whether this replica may serve a read with at most `max_lag_ms` of
    /// staleness, as of `now_ms`. See the type docs for why the sum is a bound.
    fn within_lag_budget(&self, now_ms: u64, max_lag_ms: u64) -> bool {
        let probed_at = self.probed_at_unix_ms.load(Ordering::Relaxed);
        if probed_at == 0 {
            return false;
        }
        let lag = self.lag_ms.load(Ordering::Relaxed);
        if lag == LAG_UNMEASURABLE {
            return false;
        }
        lag.saturating_add(now_ms.saturating_sub(probed_at)) <= max_lag_ms
    }

    /// Record a probe result, logging a promotion the moment it is first seen.
    fn record(&self, index: usize, in_recovery: bool, lag_ms: Option<u64>) {
        if self.in_recovery.swap(in_recovery, Ordering::Relaxed) && !in_recovery {
            tracing::warn!(
                replica = index,
                "Read replica left recovery (pg_is_in_recovery() = false); a failover has \
                 promoted it to a writable server. Reads routed to it may diverge from the \
                 primary, and its replication lag is no longer measurable."
            );
        }
        self.lag_ms.store(lag_ms.unwrap_or(LAG_UNMEASURABLE), Ordering::Relaxed);
        // Only a probe that produced a reading counts as a probe: leaving the
        // timestamp behind is what ages an unreadable replica out of rotation.
        if lag_ms.is_some() {
            self.probed_at_unix_ms.store(now_unix_ms(), Ordering::Relaxed);
        }
    }

    /// Mark the last probe as failed, without disturbing the recovery state a
    /// successful probe established.
    fn record_failure(&self) {
        self.lag_ms.store(LAG_UNMEASURABLE, Ordering::Relaxed);
    }
}

/// Probe every replica in `set` forever, on its configured cadence.
///
/// Holds a [`Weak`] so the loop is not itself a reason for the replica set to
/// stay alive: when the last adapter clone drops, the upgrade fails and the task
/// ends. Probe failures are logged and retried on the next tick — a replica that
/// cannot be probed ages out of the lag budget on its own, and one configured
/// without a budget keeps serving, exactly as it did before probes existed.
async fn probe_read_replicas(set: Weak<ReadReplicaSet>) {
    loop {
        let Some(set) = set.upgrade() else { return };
        let interval = set.probe_interval;

        for (index, pool) in set.pools.iter().enumerate() {
            let health = &set.health[index];
            let client = match pool.get().await {
                Ok(client) => client,
                Err(e) => {
                    tracing::debug!(replica = index, error = %e, "Read replica health probe \
                         could not acquire a connection");
                    health.record_failure();
                    continue;
                },
            };
            match client.query_one(REPLICA_PROBE_SQL, &[]).await {
                Ok(row) => {
                    let in_recovery: bool = row.get(0);
                    let lag_ms: Option<i64> = row.get(1);
                    // Reason (try_from): a negative lag would mean the standby
                    // replayed a transaction the future has not committed yet;
                    // GREATEST(0, …) already excludes it, and treating any such
                    // reading as unmeasurable keeps the fail-closed side.
                    health.record(index, in_recovery, lag_ms.and_then(|ms| u64::try_from(ms).ok()));
                },
                Err(e) => {
                    tracing::debug!(
                        replica = index,
                        error = %pg_detail(&e),
                        "Read replica health probe failed"
                    );
                    health.record_failure();
                },
            }
        }

        // Drop the strong reference before sleeping: holding it across the sleep
        // would keep every replica pool open for one interval after the adapter
        // is gone.
        drop(set);
        tokio::time::sleep(interval).await;
    }
}

/// Whether a read at `now_ms` falls inside the post-write primary pin.
///
/// Pure so the boundary cases are unit-testable without a clock: a never-armed
/// watermark (`last_write_ms == 0`) is only "recent" while the process's clock
/// is within `pin_ms` of the epoch, i.e. never in practice.
const fn pin_active(now_ms: u64, last_write_ms: u64, pin_ms: u64) -> bool {
    now_ms.saturating_sub(last_write_ms) < pin_ms
}

/// Current Unix time in milliseconds.
fn now_unix_ms() -> u64 {
    use std::time::{SystemTime, UNIX_EPOCH};
    // Reason (map_or 0): a pre-epoch system clock degrades to "always pinned",
    // which is the safe (primary-only) side.
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| u64::try_from(d.as_millis()).unwrap_or(u64::MAX))
}

impl std::fmt::Debug for PostgresAdapter {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PostgresAdapter")
            .field("mutation_timing_enabled", &self.mutation_timing_enabled)
            .field("timing_variable_name", &self.timing_variable_name)
            .field("pool", &"<Pool>")
            .field(
                "read_replicas",
                &self.read_replicas.as_ref().map(|r| r.pools.len()).unwrap_or_default(),
            )
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
                min_size:      0,
                max_size:      DEFAULT_POOL_SIZE,
                timeout_secs:  None,
                search_path:   None,
                // libpq's default: negotiate TLS when the server offers it. Callers
                // that need a guarantee go through `with_pool_config`.
                tls:           PostgresTlsConfig::default(),
                read_replicas: None,
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
            message:   format!("Failed to connect to database: {}", pg_detail(&e)),
            sql_state: e.code().map(|c| c.code().to_string()),
        })?;

        // Drop client back to the pool before pre-warming so that the health-check
        // connection counts as idle slot #1.
        drop(client);

        // Replica pools are built from the SAME config as the primary — search
        // path, TLS, sizing — so per-tenant isolation applies to every pool this
        // adapter ever reads through, not just the one it writes through (#407).
        let read_replicas = match &cfg.read_replicas {
            None => None,
            Some(rc) => {
                let set = Arc::new(build_read_replica_set(rc, &cfg).await?);
                // Weak, so the probe loop ends when the last adapter clone drops
                // rather than keeping the pools alive for the process's lifetime.
                tokio::spawn(probe_read_replicas(Arc::downgrade(&set)));
                Some(set)
            },
        };

        let adapter = Self {
            pool,
            read_replicas,
            mutation_timing_enabled: false,
            timing_variable_name: "fraiseql.started_at".to_string(),
        };

        // Pre-warm to `min_size` — NOT `min_size - 1`. The health-check connection
        // above is returned to the pool, so the first warm acquisition recycles it
        // rather than opening a new one; subtracting for it left the pool one
        // connection short of `min_size` (#937). Acquiring `min_size` concurrently
        // is self-correcting: whatever is already idle gets reused, the rest are
        // opened, and the pool's own `max_size` caps the total.
        let warm_target = cfg.min_size.min(cfg.max_size);
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
                read_replicas: None,
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

        // Acquire every guard CONCURRENTLY and hold them all until the last one
        // is in hand. Acquiring one at a time — or spawning tasks that release
        // as they finish — lets a later acquisition recycle a connection an
        // earlier one just returned, so the pool settles below `count` and
        // `pool_min_size` silently under-delivers (#937).
        let result =
            timeout(Duration::from_secs(10), join_all((0..count).map(|_| self.pool.get()))).await;

        let (succeeded, failed) = match result {
            Ok(guards) => {
                let s = guards.iter().filter(|r| r.is_ok()).count();
                // Released together, so `s` distinct connections stay in the pool.
                drop(guards);
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
            // Report what the pool actually holds, not what was attempted.
            tracing::info!(
                idle_connections = self.pool.status().available,
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
        routing: ReadRouting,
    ) -> Result<Vec<JsonbValue>> {
        // Read-only by construction (`SELECT data FROM <view>`): replica-eligible.
        let client = self.acquire_read_connection_with_retry(routing).await?;

        let rows: Vec<Row> =
            client.query(sql, params).await.map_err(|e| FraiseQLError::Database {
                message:   format!("Query execution failed: {}", pg_detail(&e)),
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
        routing: ReadRouting,
    ) -> Result<Vec<JsonbValue>> {
        // Read-only by construction; `set_config` + SELECT run fine inside a
        // hot-standby read-only transaction, so this stays replica-eligible.
        let mut client = self.acquire_read_connection_with_retry(routing).await?;
        let txn =
            client.build_transaction().start().await.map_err(|e| FraiseQLError::Database {
                message:   format!("Failed to start session-var transaction: {}", pg_detail(&e)),
                sql_state: e.code().map(|c| c.code().to_string()),
            })?;

        database::apply_session_vars(&txn, session_vars).await?;

        let rows: Vec<Row> = txn.query(sql, params).await.map_err(|e| FraiseQLError::Database {
            message:   format!("Query execution failed: {}", pg_detail(&e)),
            sql_state: e.code().map(|c| c.code().to_string()),
        })?;

        txn.commit().await.map_err(|e| FraiseQLError::Database {
            message:   format!("Failed to commit session-var transaction: {}", pg_detail(&e)),
            sql_state: e.code().map(|c| c.code().to_string()),
        })?;

        rows.into_iter().map(|row| jsonb_cell(&row, 0, sql)).collect()
    }

    /// Record a mutation-pipeline write for read-your-writes pinning (#407).
    ///
    /// Called at both entry and successful exit of every mutation-pipeline
    /// method: entry so a read racing a slow write already pins, exit because
    /// replication lag is measured from commit. A no-op without replicas.
    pub(super) fn mark_write(&self) {
        if let Some(replicas) = &self.read_replicas {
            replicas.last_write_unix_ms.store(now_unix_ms(), Ordering::Relaxed);
        }
    }

    /// Acquire a connection for a **structurally read-only** statement (#407).
    ///
    /// Routing, in order:
    /// 1. no replicas configured → primary;
    /// 2. inside the post-write pin window → primary (read-your-writes);
    /// 3. otherwise round-robin over the replicas, skipping any outside the bounded-staleness
    ///    budget and any whose acquisition fails, falling back to the primary when none is left.
    ///
    /// Only methods that can never write may call this; anything mixed-use
    /// (`execute_raw_query`, stats, health, DDL) stays on
    /// [`acquire_connection_with_retry`](Self::acquire_connection_with_retry).
    ///
    /// # Errors
    ///
    /// Returns `FraiseQLError::ConnectionPool` when every replica *and* the
    /// primary fallback fail to yield a connection.
    pub(super) async fn acquire_read_connection_with_retry(
        &self,
        routing: ReadRouting,
    ) -> Result<deadpool_postgres::Client> {
        let Some(replicas) = &self.read_replicas else {
            return self.acquire_connection_with_retry().await;
        };

        // A query annotated `primary` is asking not to be served stale, which no
        // replica can promise (#957).
        if !routing.allows_replica() {
            return self.acquire_connection_with_retry().await;
        }

        let now_ms = now_unix_ms();
        // `replica` opts out of the pin: the pin exists so a client reads its own
        // writes, and this query has said that is not what it is for.
        if routing.honours_write_pin()
            && pin_active(
                now_ms,
                replicas.last_write_unix_ms.load(Ordering::Relaxed),
                replicas.pin_after_write_ms,
            )
        {
            return self.acquire_connection_with_retry().await;
        }

        let max_lag_ms = routing.effective_max_lag_ms(replicas.max_lag_ms);

        let count = replicas.pools.len();
        let start = replicas.next.fetch_add(1, Ordering::Relaxed);
        let mut ineligible = 0_usize;
        for offset in 0..count {
            let index = (start.wrapping_add(offset)) % count;
            // The lag gate comes before acquisition, not after: a replica outside
            // the budget must not be *reached*, and checking afterwards would
            // burn a connection to learn what the last probe already knew.
            let health = &replicas.health[index];
            let eligible = health.is_still_a_standby()
                && max_lag_ms.is_none_or(|budget| health.within_lag_budget(now_ms, budget));
            if !eligible {
                ineligible += 1;
                continue;
            }
            match replicas.pools[index].get().await {
                Ok(client) => return Ok(client),
                Err(e) => {
                    tracing::warn!(
                        replica = index,
                        error = %e,
                        "Read replica connection acquisition failed; trying the next candidate"
                    );
                },
            }
        }

        if ineligible > 0 {
            tracing::debug!(
                ineligible,
                replicas = count,
                "Read replicas outside the bounded-staleness budget, or promoted out of \
                 recovery; serving the read from the primary"
            );
        } else {
            tracing::warn!("All read replicas unavailable; serving the read from the primary");
        }
        self.acquire_connection_with_retry().await
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

        self.execute_raw(&sql, &param_refs, ReadRouting::Any).await
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
/// `SELECT COUNT(*) FROM {view} [WHERE …]` and its parameters (#938).
///
/// Deliberately shares [`PostgresWhereGenerator`] with the row query rather than
/// formatting its own predicate: the count and the page it describes have to
/// agree, and two independent renderings of the same `WhereClause` is how they
/// would stop agreeing. No `ORDER BY` (it cannot change a count, and sorting the
/// rows first is pure cost) and no `LIMIT`/`OFFSET` (the total is the whole
/// point).
pub(super) fn build_count_sql(
    view: &str,
    where_clause: Option<&WhereClause>,
) -> Result<(String, Vec<QueryParam>)> {
    let mut sql = format!("SELECT COUNT(*) FROM {}", quote_postgres_identifier(view));
    let typed_params: Vec<QueryParam> = if let Some(clause) = where_clause {
        let generator = PostgresWhereGenerator::new(PostgresDialect);
        let (where_sql, where_params) = generator.generate(clause)?;
        sql.push_str(" WHERE ");
        sql.push_str(&where_sql);
        where_params.into_iter().map(QueryParam::from).collect()
    } else {
        Vec::new()
    };
    Ok((sql, typed_params))
}

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
