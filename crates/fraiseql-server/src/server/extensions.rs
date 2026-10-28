//! Server extensions: relay pagination, Arrow Flight service, and observer runtime
//! initialization.

use std::sync::Arc;

#[cfg(feature = "arrow")]
use fraiseql_arrow::FraiseQLFlightService;
use fraiseql_core::{
    db::traits::{RelayDatabaseAdapter, Writer},
    runtime::Executor,
    schema::CompiledSchema,
};
use fraiseql_guard::deployment::Deployment;
#[cfg(feature = "observers")]
use tokio::sync::RwLock;
#[cfg(feature = "observers")]
use tracing::{info, warn};

#[cfg(feature = "observers")]
use super::{ObserverRuntime, ObserverRuntimeConfig};
use super::{Result, Server, ServerConfig};

impl Server {
    /// Create a server with relay pagination support enabled.
    ///
    /// The adapter must implement [`RelayDatabaseAdapter`]. Currently, only
    /// `PostgresAdapter` and `CachedDatabaseAdapter<PostgresAdapter>` satisfy this bound.
    ///
    /// Relay queries issued against a server created with [`Server::new`] return a
    /// `Validation` error at runtime; those issued against a server created with this
    /// constructor succeed.
    ///
    /// # Arguments
    ///
    /// * `config` - Server configuration
    /// * `schema` - Compiled GraphQL schema
    /// * `adapter` - Database adapter (must implement `RelayDatabaseAdapter`)
    /// * `db_pool` - Database connection pool (optional, required for observers)
    ///
    /// # Errors
    ///
    /// Same as [`Server::new`]: every boot gate it runs, this constructor runs.
    ///
    /// # Example
    ///
    /// ```text
    /// // Requires: running PostgreSQL database and compiled schema file.
    /// let adapter = Arc::new(PostgresAdapter::new(db_url).await?);
    /// let server = Server::with_relay_pagination(config, schema, adapter, None).await?;
    /// server.serve().await?;
    /// ```
    pub async fn with_relay_pagination<
        A: Writer + RelayDatabaseAdapter + Clone + Send + Sync + 'static,
    >(
        config: ServerConfig,
        schema: CompiledSchema,
        adapter: Arc<A>,
        db_pool: Option<sqlx::PgPool>,
    ) -> Result<Self> {
        // Boxed: `new_inner` nests the whole subsystem-construction future, which puts
        // it past clippy's 16-KiB `large_futures` threshold.
        Box::pin(Self::new_inner(
            config,
            schema,
            adapter,
            db_pool,
            Deployment::current(),
            Executor::with_config_and_relay,
            #[cfg(feature = "arrow")]
            None,
        ))
        .await
    }
}

#[cfg(feature = "arrow")]
impl Server {
    /// Create new server with pre-configured Arrow Flight service.
    ///
    /// Use this constructor when you want to provide a Flight service with a real database adapter.
    ///
    /// The GraphQL side is wrapped in a
    /// [`CachedDatabaseAdapter`](fraiseql_core::cache::CachedDatabaseAdapter) exactly as
    /// [`Server::new`] wraps it — `cache_enabled` means the same thing on this boot
    /// path as on every other (#889). The Flight service keeps its own handle to the
    /// raw adapter; the two do not share a cache, which is why `flight_upload_tables`
    /// and `cache_enabled` are mutually exclusive (see Errors).
    ///
    /// The executor serves relay queries, as [`Server::with_relay_pagination`]'s does,
    /// so the adapter must implement [`RelayDatabaseAdapter`].
    ///
    /// # Arguments
    ///
    /// * `config` - Server configuration
    /// * `schema` - Compiled GraphQL schema
    /// * `adapter` - Database adapter
    /// * `db_pool` - Database connection pool (optional, required for observers)
    /// * `flight_service` - Pre-configured Flight service (only available with arrow feature)
    ///
    /// # Errors
    ///
    /// Returns `ServerError::ConfigError` when `cache_enabled = true` is combined with a
    /// non-empty `flight_upload_tables`: a Flight `Upload` is a direct INSERT that never
    /// reaches the mutation runner, so nothing invalidates the result cache for the views
    /// over the uploaded table and GraphQL reads would serve pre-upload rows until the TTL
    /// expired, or when `[tenancy.runtime]` is enabled with a non-empty
    /// `flight_upload_tables`. Otherwise the same as [`Server::new`]: every boot gate it
    /// runs, this constructor runs.
    pub async fn with_flight_service<
        A: Writer + RelayDatabaseAdapter + Clone + Send + Sync + 'static,
    >(
        config: ServerConfig,
        schema: CompiledSchema,
        adapter: Arc<A>,
        db_pool: Option<sqlx::PgPool>,
        flight_service: Option<FraiseQLFlightService>,
    ) -> Result<Self> {
        // The one combination the cache cannot be made honest for. Everything else on
        // this path caches exactly like `Server::new`; an allow-listed Upload writes
        // rows behind the cache's back, so refuse rather than serve stale reads.
        if config.cache_enabled && !config.flight_upload_tables.is_empty() {
            return Err(super::ServerError::ConfigError(
                "`cache_enabled = true` cannot be combined with a non-empty \
                 `flight_upload_tables`: a Flight Upload writes rows directly and does not \
                 pass the mutation pipeline, so it invalidates nothing and cached GraphQL \
                 reads would keep serving pre-upload rows until the TTL expired. Set \
                 `cache_enabled = false`, or leave `flight_upload_tables` empty to keep \
                 Upload disabled."
                    .to_string(),
            ));
        }

        // #1550: an Upload writes through the server-wide pool, never a tenant's executor,
        // so under the tenancy runtime a tenant's rows would land in the default database
        // (schema mode: the default `search_path`). Refused until uploads dispatch tenants.
        if config.tenancy.runtime.enabled && !config.flight_upload_tables.is_empty() {
            return Err(super::ServerError::ConfigError(
                "`[tenancy.runtime] enabled = true` cannot be combined with a non-empty \
                 `flight_upload_tables`: a Flight Upload writes through the server-wide pool, \
                 not the tenant's executor, so a tenant's rows would land in the default \
                 database (#1550). Leave `flight_upload_tables` empty to keep Upload disabled, \
                 or disable the tenancy runtime."
                    .to_string(),
            ));
        }

        // Relay-capable, as `with_relay_pagination` builds it: every `arrow` binary boots
        // through this constructor. Boxed for the same `large_futures` reason.
        Box::pin(Self::new_inner(
            config,
            schema,
            adapter,
            db_pool,
            Deployment::current(),
            Executor::with_config_and_relay,
            flight_service,
        ))
        .await
    }
}

impl Server {
    /// Initialize observer runtime from configuration.
    ///
    /// # Errors
    ///
    /// Returns `ServerError::ConfigError` when a non-Postgres observer transport
    /// is selected that this binary cannot run (feature not compiled in, or NATS
    /// without a URL) while in production mode (#350), or when the transport
    /// configuration is otherwise invalid. In development such a selection is
    /// downgraded to a warning and the runtime falls back to PostgreSQL.
    #[cfg(feature = "observers")]
    // Reason: one of the `init_*` family the boot sequence awaits uniformly; the
    // other members connect to their transports.
    #[allow(unknown_lints, clippy::unused_async_trait_impl)]
    pub(super) async fn init_observer_runtime(
        config: &ServerConfig,
        compiled: Option<&fraiseql_core::schema::ObserversConfig>,
        pool: Option<&sqlx::PgPool>,
        deployment: Deployment,
        documents: fraiseql_observers::listener::DocumentDecoder,
    ) -> crate::Result<Option<Arc<RwLock<ObserverRuntime>>>> {
        let enabled = crate::server_config::ObserverConfig::runtime_enabled(
            config.observers.as_ref(),
            compiled,
        )
        .map_err(crate::ServerError::ConfigError)?;
        if !enabled {
            info!("Observer runtime disabled");
            return Ok(None);
        }
        let default_tuning = crate::server_config::ObserverConfig::default();
        let observer_config = config.observers.as_ref().unwrap_or(&default_tuning);

        let Some(pool) = pool else {
            warn!("No database pool provided for observers");
            return Ok(None);
        };

        info!("Initializing observer runtime");

        let runtime_config =
            Self::observer_runtime_config(observer_config, compiled, pool, deployment)?
                .with_documents(documents);
        let runtime = ObserverRuntime::new(runtime_config);
        Ok(Some(Arc::new(RwLock::new(runtime))))
    }

    /// Resolve `[observers.runtime]` plus its environment overrides into the
    /// runtime's configuration: the half of [`Self::init_observer_runtime`] that
    /// reads the environment and validates, without starting anything.
    ///
    /// # Errors
    ///
    /// See [`Self::init_observer_runtime`].
    #[cfg(feature = "observers")]
    pub(super) fn observer_runtime_config(
        observer_config: &crate::server_config::ObserverConfig,
        compiled: Option<&fraiseql_core::schema::ObserversConfig>,
        pool: &sqlx::PgPool,
        deployment: Deployment,
    ) -> crate::Result<ObserverRuntimeConfig> {
        use fraiseql_observers::config::TransportKind;

        // Resolve the event transport from compiled config + env overrides, then
        // fail loud (#350) on a selection this binary cannot run before validating
        // the finer NATS/JetStream bounds — never a silent fallback to PostgreSQL.
        if let Some(name) =
            UNRUN_TRANSPORT_VARS.iter().find(|name| std::env::var_os(name).is_some())
        {
            return Err(crate::ServerError::ConfigError(format!(
                "{name} is set, but fraiseql-server always runs the observer executors and runs no \
                 PostgreSQL → NATS bridge for it to configure; unset it"
            )));
        }
        let mut transport =
            observer_config.runtime.transport.to_runtime(compiled).with_env_overrides();
        let compiled_in = cfg!(feature = "observers-nats");
        let nats_url_present = !transport.nats.url.is_empty();
        crate::server::initialization::observer_transport_check(
            transport.transport,
            compiled_in,
            nats_url_present,
            deployment,
        )?;

        // In production an unrunnable selection already returned above; the only
        // way past the guard with an unrunnable transport is development, where it
        // was downgraded to a warning — fall back to PostgreSQL so local boot works.
        let usable = match transport.transport {
            TransportKind::Postgres | TransportKind::InMemory => true,
            TransportKind::Nats => compiled_in && nats_url_present,
            _ => false,
        };
        if !usable {
            transport.transport = TransportKind::Postgres;
        }

        transport.validate().map_err(|e| {
            crate::ServerError::ConfigError(format!("invalid observer transport config: {e}"))
        })?;

        let runtime = &observer_config.runtime;
        if runtime.auto_reload && runtime.reload_interval_secs == 0 {
            return Err(crate::ServerError::ConfigError(
                "[observers.runtime] reload_interval_secs = 0 with auto_reload = true would \
                 reload observers continuously; set an interval in seconds, or auto_reload = \
                 false to reload only through POST /api/observers/runtime/reload"
                    .to_string(),
            ));
        }

        let runtime_config = ObserverRuntimeConfig::new(pool.clone())
            .with_poll_interval(observer_config.runtime.poll_interval_ms)
            .with_reload(runtime.auto_reload, runtime.reload_interval_secs)
            .with_batch_size(observer_config.runtime.batch_size)
            .with_channel_capacity(observer_config.runtime.channel_capacity)
            .with_max_dlq_size(observer_config.runtime.max_dlq_size)
            .with_transport(transport)
            .with_email(observer_config.runtime.email.clone())
            .with_log_payloads(observer_config.runtime.log_payloads)
            // #985: the Redis backend that makes a `type = "cache"` action work.
            .with_redis(observer_redis(observer_config.runtime.redis.as_ref())?);

        Ok(runtime_config)
    }
}

/// Transport variables that configured the PostgreSQL → NATS bridge or a
/// bridge-only node. Nothing reads them, so setting one is refused rather than
/// ignored (#1496).
#[cfg(feature = "observers")]
const UNRUN_TRANSPORT_VARS: [&str; 6] = [
    "FRAISEQL_NATS_RUN_EXECUTORS",
    "FRAISEQL_NATS_ENABLE_BRIDGE",
    "FRAISEQL_BRIDGE_TRANSPORT_NAME",
    "FRAISEQL_BRIDGE_BATCH_SIZE",
    "FRAISEQL_BRIDGE_POLL_INTERVAL_SECS",
    "FRAISEQL_BRIDGE_NOTIFY_CHANNEL",
];

/// `FRAISEQL_REDIS_*` variables nothing in the server reads, among them the
/// window of the removed Redis dedup store. Setting one is refused rather than
/// ignored.
#[cfg(feature = "observers")]
const UNUSED_REDIS_VARS: [&str; 3] = [
    "FRAISEQL_REDIS_POOL_SIZE",
    "FRAISEQL_REDIS_DEDUP_WINDOW_SECS",
    "FRAISEQL_REDIS_CACHE_TTL_SECS",
];

/// The `[observers.runtime.redis]` block with its `FRAISEQL_REDIS_*` overrides
/// applied, as the runtime's `RedisConfig`, validated (#1466).
///
/// A deployment that declares no block can supply one through
/// `FRAISEQL_REDIS_URL`: the URL is the one value without which there is no
/// Redis to configure, so the timeout variables alone leave the backend absent.
#[cfg(feature = "observers")]
fn observer_redis(
    declared: Option<&crate::server_config::ObserverRedisConfig>,
) -> crate::Result<Option<fraiseql_observers::config::RedisConfig>> {
    use crate::{ServerError::ConfigError, server_config::ObserverRedisConfig};

    if let Some(name) = UNUSED_REDIS_VARS.iter().find(|name| std::env::var_os(name).is_some()) {
        return Err(ConfigError(format!(
            "{name} is set, but fraiseql-server runs no Redis deduplication or result cache \
             for it to configure; unset it"
        )));
    }

    let env_url = std::env::var("FRAISEQL_REDIS_URL").ok();
    let Some(mut redis) = declared
        .cloned()
        .or_else(|| env_url.as_ref().map(|_| ObserverRedisConfig::default()))
    else {
        return Ok(None);
    };
    if let Some(url) = env_url {
        redis.url = url;
    }
    if let Some(secs) = env_secs("FRAISEQL_REDIS_CONNECT_TIMEOUT_SECS")? {
        redis.connect_timeout_secs = secs;
    }
    if let Some(secs) = env_secs("FRAISEQL_REDIS_COMMAND_TIMEOUT_SECS")? {
        redis.command_timeout_secs = secs;
    }

    let config = fraiseql_observers::config::RedisConfig {
        url: redis.url,
        connect_timeout_secs: redis.connect_timeout_secs,
        command_timeout_secs: redis.command_timeout_secs,
        ..fraiseql_observers::config::RedisConfig::default()
    };
    config.validate().map_err(|e| {
        ConfigError(format!(
            "invalid [observers.runtime.redis] (after FRAISEQL_REDIS_* overrides): {e}"
        ))
    })?;
    Ok(Some(config))
}

/// A whole-seconds environment variable; unset is `None`, unparseable an error.
#[cfg(feature = "observers")]
fn env_secs(name: &str) -> crate::Result<Option<u64>> {
    std::env::var(name).ok().map_or(Ok(None), |v| {
        v.parse().map(Some).map_err(|_| {
            crate::ServerError::ConfigError(format!(
                "{name}={v:?} is not a whole number of seconds"
            ))
        })
    })
}
