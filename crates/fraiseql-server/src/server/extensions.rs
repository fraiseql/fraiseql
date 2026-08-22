//! Server extensions: relay pagination, Arrow Flight service, and observer runtime
//! initialization.

use std::sync::Arc;

#[cfg(feature = "arrow")]
use fraiseql_arrow::FraiseQLFlightService;
use fraiseql_core::{
    cache::CachedDatabaseAdapter,
    db::traits::{DatabaseAdapter, RelayDatabaseAdapter},
    runtime::{Executor, SubscriptionManager},
    schema::CompiledSchema,
};
#[cfg(feature = "observers")]
use tokio::sync::RwLock;
#[cfg(any(feature = "observers", feature = "mcp"))]
use tracing::info;
#[cfg(feature = "observers")]
use tracing::warn;

#[cfg(feature = "observers")]
use super::{ObserverRuntime, ObserverRuntimeConfig};
use super::{Result, Server, ServerConfig};

impl<A: DatabaseAdapter + RelayDatabaseAdapter + Clone + Send + Sync + 'static>
    Server<CachedDatabaseAdapter<A>>
{
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
    /// Returns error if OIDC validator initialization fails.
    ///
    /// # Example
    ///
    /// ```text
    /// // Requires: running PostgreSQL database and compiled schema file.
    /// let adapter = Arc::new(PostgresAdapter::new(db_url).await?);
    /// let server = Server::with_relay_pagination(config, schema, adapter, None).await?;
    /// server.serve().await?;
    /// ```
    pub async fn with_relay_pagination(
        config: ServerConfig,
        schema: CompiledSchema,
        adapter: Arc<A>,
        db_pool: Option<sqlx::PgPool>,
    ) -> Result<Self> {
        // Same boot gates as `Server::new` — these must not drift by constructor (H16).
        // Refuse to boot if any field is marked for at-rest encryption (H12); the write
        // path does not encrypt, so the data would be stored in plaintext.
        crate::server::initialization::field_encryption_unsupported_check(&schema)?;
        // Build the runtime config from the compiled schema (validates format version,
        // reads the audit flag, applies the #421 page-size ceiling + change-log toggle).
        let executor_config = crate::server::initialization::executor_runtime_config(
            &schema, &config,
        )
        .map_err(|msg| {
            super::ServerError::ConfigError(format!("Incompatible compiled schema: {msg}"))
        })?;

        // Read every schema-derived subsystem through the one shared seam.
        let subsystems = Self::schema_subsystems(&schema, &config).await?;

        // The one shared cache seam — see `build_cached_adapter` (#889).
        let (cached, cache_config) = crate::server::initialization::build_cached_adapter(
            &schema,
            config.cache_enabled,
            adapter,
        )
        .await?;
        let executor = Arc::new(Executor::with_config_and_relay(
            schema.clone(),
            Arc::new(cached),
            executor_config,
        ));
        let subscription_manager = Arc::new(SubscriptionManager::new(Arc::new(schema)));

        // Boxed: `from_executor` constructs every subsystem, and its future is
        // large enough that inlining it tips each public constructor past
        // clippy's `large_futures` stack budget — and, more to the point, puts a
        // multi-KiB frame on every caller's stack.
        let mut server = Box::pin(Self::from_executor(
            config,
            executor,
            subscription_manager,
            subsystems,
            db_pool,
            #[cfg(feature = "arrow")]
            None,
        ))
        .await?;

        server.adapter_cache_enabled = cache_config.enabled;
        // Record the relay-capable rebuild. This is the only scope where the
        // `RelayDatabaseAdapter` bound holds, so it is the only place that can
        // teach the hot-reload path to preserve relay dispatch (#750).
        server.executor_rebuilder = Arc::new(Executor::with_config_and_relay);

        server.apply_compiled_config()
    }
}

#[cfg(feature = "arrow")]
impl<A: DatabaseAdapter + Clone + Send + Sync + 'static> Server<CachedDatabaseAdapter<A>> {
    /// Create new server with pre-configured Arrow Flight service.
    ///
    /// Use this constructor when you want to provide a Flight service with a real database adapter.
    ///
    /// The GraphQL side is wrapped in a [`CachedDatabaseAdapter`] exactly as
    /// [`Server::new`] wraps it — `cache_enabled` means the same thing on this boot
    /// path as on every other (#889). The Flight service keeps its own handle to the
    /// raw adapter; the two do not share a cache, which is why `flight_upload_tables`
    /// and `cache_enabled` are mutually exclusive (see Errors).
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
    /// expired. Also returns an error if OIDC validator initialization fails.
    pub async fn with_flight_service(
        config: ServerConfig,
        schema: CompiledSchema,
        adapter: Arc<A>,
        #[allow(unused_variables)]
        // Reason: used inside #[cfg(feature = "observers")] block; unused when feature is off
        db_pool: Option<sqlx::PgPool>,
        flight_service: Option<FraiseQLFlightService>,
    ) -> Result<Self> {
        // Same boot gates as `Server::new` — these must not drift by constructor (H16).
        // Refuse to boot on at-rest-encryption-marked fields (H12, plaintext write path).
        crate::server::initialization::field_encryption_unsupported_check(&schema)?;

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

        // Build the runtime config from the compiled schema (validates format version,
        // reads the audit flag, applies the #421 page-size ceiling + change-log toggle).
        let executor_config = crate::server::initialization::executor_runtime_config(
            &schema, &config,
        )
        .map_err(|msg| {
            super::ServerError::ConfigError(format!("Incompatible compiled schema: {msg}"))
        })?;

        // Read every schema-derived subsystem through the one shared seam.
        let subsystems = Self::schema_subsystems(&schema, &config).await?;

        // The one shared cache seam — see `build_cached_adapter` (#889). This path
        // used to skip it entirely, which is what made `cache_enabled` inert here.
        let (cached, cache_config) = crate::server::initialization::build_cached_adapter(
            &schema,
            config.cache_enabled,
            adapter,
        )
        .await?;

        let executor =
            Arc::new(Executor::with_config(schema.clone(), Arc::new(cached), executor_config));
        let subscription_manager = Arc::new(SubscriptionManager::new(Arc::new(schema)));

        // Boxed: `from_executor` constructs every subsystem, and its future is
        // large enough that inlining it tips each public constructor past
        // clippy's `large_futures` stack budget — and, more to the point, puts a
        // multi-KiB frame on every caller's stack.
        let mut server = Box::pin(Self::from_executor(
            config,
            executor,
            subscription_manager,
            subsystems,
            db_pool,
            flight_service,
        ))
        .await?;

        server.adapter_cache_enabled = cache_config.enabled;

        server.apply_compiled_config()
    }
}

impl<A: DatabaseAdapter + Clone + Send + Sync + 'static> Server<A> {
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
        pool: Option<&sqlx::PgPool>,
    ) -> crate::Result<Option<Arc<RwLock<ObserverRuntime>>>> {
        use fraiseql_observers::config::TransportKind;

        // Check if enabled
        let observer_config = match &config.observers {
            Some(cfg) if cfg.enabled => cfg,
            _ => {
                info!("Observer runtime disabled");
                return Ok(None);
            },
        };

        let Some(pool) = pool else {
            warn!("No database pool provided for observers");
            return Ok(None);
        };

        info!("Initializing observer runtime");

        // Resolve the event transport from compiled config + env overrides, then
        // fail loud (#350) on a selection this binary cannot run before validating
        // the finer NATS/JetStream bounds — never a silent fallback to PostgreSQL.
        let mut transport = observer_config.runtime.transport.clone().with_env_overrides();
        let compiled_in = cfg!(feature = "observers-nats");
        let nats_url_present = !transport.nats.url.is_empty();
        crate::server::initialization::observer_transport_check(
            transport.transport,
            compiled_in,
            nats_url_present,
            crate::ServerConfig::is_production_mode(),
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

        let runtime_config = ObserverRuntimeConfig::new(pool.clone())
            .with_poll_interval(observer_config.runtime.poll_interval_ms)
            .with_batch_size(observer_config.runtime.batch_size)
            .with_channel_capacity(observer_config.runtime.channel_capacity)
            .with_max_dlq_size(observer_config.runtime.max_dlq_size)
            .with_transport(transport)
            .with_email(observer_config.runtime.email.clone())
            .with_log_payloads(observer_config.runtime.log_payloads)
            // #985: the Redis backend that makes a `type = "cache"` action work.
            // Same `[observers.runtime.redis]` block dedup/result-cache use.
            .with_redis(observer_config.runtime.redis.clone());

        let runtime = ObserverRuntime::new(runtime_config);
        Ok(Some(Arc::new(RwLock::new(runtime))))
    }
}
