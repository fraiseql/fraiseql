//! Server extensions: relay pagination, Arrow Flight service, and observer runtime
//! initialization.

use std::sync::Arc;

#[cfg(feature = "arrow")]
use fraiseql_arrow::FraiseQLFlightService;
use fraiseql_core::{
    cache::{CacheConfig, CachedDatabaseAdapter, QueryResultCache},
    db::traits::{DatabaseAdapter, RelayDatabaseAdapter},
    runtime::{Executor, RuntimeConfig, SubscriptionManager},
    schema::CompiledSchema,
};
#[cfg(feature = "observers")]
use tokio::sync::RwLock;
#[cfg(any(feature = "observers", feature = "mcp"))]
use tracing::info;
#[cfg(any(feature = "observers", feature = "arrow"))]
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
    /// # Panics
    ///
    /// Panics if the `adapter` `Arc` has been cloned before calling this constructor
    /// (refcount > 1). The builder must have exclusive ownership to unwrap the adapter
    /// for `CachedDatabaseAdapter` construction.
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
        // Validate cache + RLS safety — the same shared check `Server::new` runs.
        crate::server::initialization::tenant_isolation_declaration_check(
            &schema,
            config.cache_enabled,
        )?;

        // Same boot gates as `Server::new` — these must not drift by constructor (H16).
        // Refuse to boot if any field is marked for at-rest encryption (H12); the write
        // path does not encrypt, so the data would be stored in plaintext.
        crate::server::initialization::field_encryption_unsupported_check(&schema)?;
        // Build the runtime config from the compiled schema (validates format version,
        // reads the audit flag, applies the #421 page-size ceiling + change-log toggle).
        let executor_config = RuntimeConfig::from_compiled_schema(&schema).map_err(|msg| {
            super::ServerError::ConfigError(format!("Incompatible compiled schema: {msg}"))
        })?;

        // Read every schema-derived subsystem through the one shared seam.
        let subsystems = Self::schema_subsystems(&schema, &config).await?;

        let cache_config = CacheConfig::from(config.cache_enabled);
        let cache = QueryResultCache::new(cache_config);
        // Unwrap Arc: refcount is 1 here — adapter has not been cloned since being passed in.
        let inner = Arc::into_inner(adapter)
            .expect("CachedDatabaseAdapter wrapping requires exclusive Arc ownership at startup");
        let cached = CachedDatabaseAdapter::new(inner, cache, schema.content_hash())
            .with_cache_metadata_from_schema(&schema)
            .with_rls(schema.has_rls_configured());
        crate::server::initialization::verify_declared_rls(
            &schema,
            &cached,
            cache_config.rls_enforcement,
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

impl<A: DatabaseAdapter + Clone + Send + Sync + 'static> Server<A> {
    /// Create new server with pre-configured Arrow Flight service.
    ///
    /// Use this constructor when you want to provide a Flight service with a real database adapter.
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
    /// Returns error if OIDC validator initialization fails.
    #[cfg(feature = "arrow")]
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
        // Build the runtime config from the compiled schema (validates format version,
        // reads the audit flag, applies the #421 page-size ceiling + change-log toggle).
        let executor_config = RuntimeConfig::from_compiled_schema(&schema).map_err(|msg| {
            super::ServerError::ConfigError(format!("Incompatible compiled schema: {msg}"))
        })?;

        // Read every schema-derived subsystem through the one shared seam.
        let subsystems = Self::schema_subsystems(&schema, &config).await?;

        // No `CachedDatabaseAdapter` is built on this path — the arrow/Flight
        // constructor keeps the raw adapter, because `main.rs` has already cloned
        // it into `create_flight_service` and `CachedDatabaseAdapter` requires
        // exclusive ownership. `config.cache_enabled` therefore does nothing here.
        // Say so rather than letting the operator believe the TOML took effect.
        if config.cache_enabled {
            warn!(
                "[cache] enabled = true has no effect in an Arrow build: the Flight \
                 constructor shares the raw database adapter with the Flight service and \
                 cannot wrap it in a result cache. Queries are served uncached. Use a \
                 non-Arrow build if query-result caching is required."
            );
        }
        // Run the shared cache+RLS gate anyway, with the *effective* cache state,
        // so this constructor is on the same footing as its siblings rather than
        // being the one that quietly skips a boot gate (#758).
        crate::server::initialization::tenant_isolation_declaration_check(&schema, false)?;

        let executor = Arc::new(Executor::with_config(schema.clone(), adapter, executor_config));
        let subscription_manager = Arc::new(SubscriptionManager::new(Arc::new(schema)));

        // Boxed: `from_executor` constructs every subsystem, and its future is
        // large enough that inlining it tips each public constructor past
        // clippy's `large_futures` stack budget — and, more to the point, puts a
        // multi-KiB frame on every caller's stack.
        let server = Box::pin(Self::from_executor(
            config,
            executor,
            subscription_manager,
            subsystems,
            db_pool,
            flight_service,
        ))
        .await?;

        server.apply_compiled_config()
    }

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
            .with_log_payloads(observer_config.runtime.log_payloads);

        let runtime = ObserverRuntime::new(runtime_config);
        Ok(Some(Arc::new(RwLock::new(runtime))))
    }
}
