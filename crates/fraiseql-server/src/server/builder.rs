//! Server constructors and builder methods.

use std::sync::Arc;

#[cfg(feature = "arrow")]
use fraiseql_arrow::FraiseQLFlightService;
use fraiseql_core::{
    cache::{CacheConfig, CachedDatabaseAdapter, QueryResultCache},
    db::traits::DatabaseAdapter,
    runtime::{Executor, RuntimeConfig, SubscriptionManager},
    schema::CompiledSchema,
    security::{AuthConfig, AuthMiddleware, OidcValidator},
};
use tracing::info;

use super::{RateLimiter, Result, Server, ServerConfig, ServerError};

/// Build an HS256 validator from the server config, if configured.
pub(super) fn build_hs256_auth(config: &ServerConfig) -> Result<Option<Arc<AuthMiddleware>>> {
    let Some(ref hs) = config.auth_hs256 else {
        return Ok(None);
    };
    // Reject incompatible config shapes *before* loading the secret, so a
    // dev environment with a real secret env-var but missing audience still
    // surfaces the audience error rather than silently booting.  Closes the
    // cross-service token-confusion gap for the HS256 path (#359).
    hs.validate()
        .map_err(|e| ServerError::ConfigError(format!("Failed to initialize HS256 auth: {e}")))?;
    let secret = hs
        .load_secret()
        .map_err(|e| ServerError::ConfigError(format!("Failed to initialize HS256 auth: {e}")))?;
    let mut auth_config = AuthConfig::with_hs256(&secret);
    if let Some(ref iss) = hs.issuer {
        auth_config = auth_config.with_issuer(iss);
    }
    if let Some(ref aud) = hs.audience {
        auth_config = auth_config.with_audience(aud);
    }
    info!(
        secret_env = %hs.secret_env,
        issuer = ?hs.issuer,
        audience = ?hs.audience,
        "Initializing HS256 authentication (local validation, no network)"
    );
    Ok(Some(Arc::new(AuthMiddleware::from_config(auth_config))))
}

/// Every subsystem a constructor reads out of the compiled schema (and the
/// resolved server config) before the schema is moved into the executor.
///
/// This exists so the three public constructors cannot disagree about *which*
/// subsystems a compiled schema produces. Each one used to build this set
/// inline, and `with_flight_service`'s copy had drifted: it gated the OIDC
/// validator behind `#[cfg(feature = "auth")]` where the others did not, so a
/// lean arrow build silently discarded `[auth]` and served every request
/// anonymously (#783).
pub(super) struct SchemaSubsystems {
    #[cfg(feature = "federation")]
    pub circuit_breaker:
        Option<Arc<crate::federation::circuit_breaker::FederationCircuitBreakerManager>>,
    pub error_sanitizer: Arc<crate::config::error_sanitization::ErrorSanitizer>,
    #[cfg(feature = "auth")]
    pub state_encryption: Option<Arc<crate::auth::state_encryption::StateEncryptionService>>,
    #[cfg(feature = "auth")]
    pub pkce_store: Option<Arc<crate::auth::PkceStateStore>>,
    #[cfg(feature = "auth")]
    pub oidc_server_client: Option<Arc<crate::auth::OidcServerClient>>,
    pub rate_limiter: Option<Arc<RateLimiter>>,
    pub api_key_authenticator: Option<Arc<crate::api_key::ApiKeyAuthenticator>>,
    pub service_account_authenticator:
        Option<Arc<crate::service_account::ServiceAccountAuthenticator>>,
    pub revocation_manager: Option<Arc<crate::token_revocation::TokenRevocationManager>>,
    pub trusted_docs: Option<Arc<crate::trusted_documents::TrustedDocumentStore>>,
    /// Lifecycle tasks spawned during subsystem construction; moved onto the
    /// `Server` so graceful shutdown can await them.
    pub tasks: tokio::task::JoinSet<()>,
}

impl<A: DatabaseAdapter + Clone + Send + Sync + 'static> Server<CachedDatabaseAdapter<A>> {
    /// Create new server.
    ///
    /// Relay pagination queries will return a `Validation` error at runtime. Use
    /// [`Server::with_relay_pagination`] when the adapter implements
    /// [`RelayDatabaseAdapter`](fraiseql_core::db::traits::RelayDatabaseAdapter)
    /// and relay support is required.
    ///
    /// # Arguments
    ///
    /// * `config` - Server configuration
    /// * `schema` - Compiled GraphQL schema
    /// * `adapter` - Database adapter
    /// * `db_pool` — forwarded to the observer runtime; `None` when observers are disabled.
    ///
    /// # Errors
    ///
    /// Returns error if OIDC validator initialization fails (e.g., unable to
    /// fetch discovery document or JWKS).
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
    /// let config = ServerConfig::default();
    /// let schema = CompiledSchema::from_json(schema_json, false)?;
    /// let adapter = Arc::new(PostgresAdapter::new(db_url).await?);
    ///
    /// let server = Server::new(config, schema, adapter, None).await?;
    /// server.serve().await?;
    /// ```
    #[allow(clippy::cognitive_complexity)] // Reason: server construction with subsystem initialization (auth, rate-limit, observers, etc.)
    pub async fn new(
        config: ServerConfig,
        schema: CompiledSchema,
        adapter: Arc<A>,
        db_pool: Option<sqlx::PgPool>,
    ) -> Result<Self> {
        // Build the runtime config from the compiled schema. This is the single
        // seam every server constructor routes through (H16): it validates the
        // schema format version (warns on legacy, rejects incompatible), reads the
        // audit-logging flag, applies the #421 page-size ceiling, and the
        // change-log toggle. Doing it first preserves "reject bad version before
        // any further setup".
        let executor_config = RuntimeConfig::from_compiled_schema(&schema).map_err(|msg| {
            ServerError::ConfigError(format!("Incompatible compiled schema: {msg}"))
        })?;

        // Refuse to boot if any field is marked for at-rest encryption: the write path does
        // not encrypt (H12), so those fields would be stored in plaintext. Fail loud rather
        // than silently storing sensitive data unencrypted.
        crate::server::initialization::field_encryption_unsupported_check(&schema)?;

        // Read every schema-derived subsystem through the one shared seam.
        let subsystems = Self::schema_subsystems(&schema, &config).await?;

        // Validate cache + RLS safety at startup. One shared check for every
        // constructor — see `tenant_isolation_declaration_check`.
        crate::server::initialization::tenant_isolation_declaration_check(
            &schema,
            config.cache_enabled,
        )?;

        // Build cache from config.
        let cache_config = CacheConfig::from(config.cache_enabled);
        let cache = QueryResultCache::new(cache_config);

        // Log cache state before consuming config.
        if cache_config.enabled {
            tracing::info!(
                max_entries   = cache_config.max_entries,
                ttl_seconds   = cache_config.ttl_seconds,
                rls_enforcement = ?cache_config.rls_enforcement,
                "Query result cache: active"
            );
        } else {
            tracing::info!("Query result cache: disabled");
        }

        // Unwrap Arc: refcount is 1 here — adapter has not been cloned since being passed in.
        let inner = Arc::into_inner(adapter)
            .expect("CachedDatabaseAdapter wrapping requires exclusive Arc ownership at startup");
        let cached = CachedDatabaseAdapter::new(inner, cache, schema.content_hash())
            .with_ttl_overrides_from_schema(&schema)
            .with_rls(schema.has_rls_configured());

        // Turn the RLS *declaration* into a checked claim against the live catalog.
        crate::server::initialization::verify_declared_rls(
            &schema,
            &cached,
            cache_config.rls_enforcement,
        )
        .await?;

        // `executor_config` was built from the compiled schema at the top of this
        // constructor (the H16 seam — audit flag, #421 page-size, change-log toggle).
        let executor =
            Arc::new(Executor::with_config(schema.clone(), Arc::new(cached), executor_config));
        let subscription_manager = Arc::new(SubscriptionManager::new(Arc::new(schema)));

        let mut server = Self::from_executor(
            config,
            executor,
            subscription_manager,
            subsystems,
            db_pool,
            #[cfg(feature = "arrow")]
            None,
        )
        .await?;

        server.adapter_cache_enabled = cache_config.enabled;

        server.apply_compiled_config()
    }
}

impl<A: DatabaseAdapter + Clone + Send + Sync + 'static> Server<A> {
    /// Build every subsystem the compiled schema declares.
    ///
    /// The **one** place a compiled schema is turned into subsystems. Every
    /// public constructor calls it, so none can quietly build a different set —
    /// which is how `with_flight_service` came to gate the OIDC validator behind
    /// `#[cfg(feature = "auth")]` while `Server::new` did not (#783).
    ///
    /// # Errors
    ///
    /// Propagates every boot-time refusal these subsystems raise: an invalid
    /// state-encryption key, a rate-limiter guard violation (#837/#774), an
    /// unusable token-revocation backend.
    pub(super) async fn schema_subsystems(
        schema: &CompiledSchema,
        config: &ServerConfig,
    ) -> Result<SchemaSubsystems> {
        #[cfg(feature = "federation")]
        let circuit_breaker = schema.federation.as_ref().and_then(
            crate::federation::circuit_breaker::FederationCircuitBreakerManager::from_config,
        );
        let error_sanitizer = Self::error_sanitizer_from_schema(schema);
        #[cfg(feature = "auth")]
        let state_encryption = Self::state_encryption_from_schema(schema)?;
        #[cfg(feature = "auth")]
        let pkce_store = Self::pkce_store_from_schema(schema, state_encryption.as_ref()).await?;
        #[cfg(feature = "auth")]
        let oidc_server_client = Self::oidc_server_client_from_schema(schema);
        // Both configuration sources resolved together, so the boot guards run on
        // whatever actually takes effect (#837) and CLI/env overrides win (#774).
        let rate_limiter = super::initialization::resolve_rate_limiter(schema, config).await?;
        let api_key_authenticator = crate::api_key::api_key_authenticator_from_schema(schema);
        if api_key_authenticator.is_some() {
            info!("API key authentication enabled");
        }
        let service_account_authenticator =
            crate::service_account::service_account_authenticator_from_schema(schema);
        if service_account_authenticator.is_some() {
            info!("Service-account authentication enabled");
        }
        let revocation_manager = crate::token_revocation::revocation_manager_from_schema(schema)?;
        if revocation_manager.is_some() {
            info!("Token revocation enabled");
        }
        // Collect lifecycle task handles into a JoinSet that will be moved onto
        // the `Server` so graceful shutdown can await them.
        let mut tasks: tokio::task::JoinSet<()> = tokio::task::JoinSet::new();
        let trusted_docs = Self::trusted_docs_from_schema(schema, &mut tasks);

        Ok(SchemaSubsystems {
            #[cfg(feature = "federation")]
            circuit_breaker,
            error_sanitizer,
            #[cfg(feature = "auth")]
            state_encryption,
            #[cfg(feature = "auth")]
            pkce_store,
            #[cfg(feature = "auth")]
            oidc_server_client,
            rate_limiter,
            api_key_authenticator,
            service_account_authenticator,
            revocation_manager,
            trusted_docs,
            tasks,
        })
    }

    /// Apply every setting that lands on the `Server` *after* the executor is
    /// built: the compiled `[mcp]` and `[subscriptions]` blocks, and the server
    /// config's APQ and `[pool_tuning]` switches.
    ///
    /// The **one** place these are applied. `Server::new` used to do it inline
    /// and its two siblings each applied a different subset — `with_relay_pagination`
    /// and `with_flight_service` both dropped `[subscriptions]` (turning the
    /// documented fail-closed `on_connect`/`on_subscribe` webhook gates into a
    /// silent fail-open) and `[pool_tuning]` (#754).
    ///
    /// # Errors
    ///
    /// Returns `ServerError::ConfigError` when `[pool_tuning]` fails validation.
    pub(super) fn apply_compiled_config(mut self) -> Result<Self> {
        // Initialize MCP config from compiled schema when the feature is compiled in.
        #[cfg(feature = "mcp")]
        if let Some(cfg) = self.executor.schema().mcp_config.clone() {
            if cfg.enabled {
                let tool_count =
                    crate::mcp::tools::schema_to_tools(self.executor.schema(), &cfg).len();
                info!(
                    path = %cfg.path,
                    transport = %cfg.transport,
                    tools = tool_count,
                    "MCP server configured"
                );
                self.mcp_config = Some(cfg);
            }
        }

        // Initialize APQ store when enabled.
        if self.config.apq_enabled {
            let apq_store: fraiseql_core::apq::ArcApqStorage =
                Arc::new(fraiseql_core::apq::InMemoryApqStorage::default());
            self.apq_store = Some(apq_store);
            info!("APQ (Automatic Persisted Queries) enabled — in-memory backend");
        }

        // Apply subscription lifecycle/limits from compiled schema.
        if let Some(subs) = self.executor.schema().subscriptions_config.clone() {
            if let Some(max) = subs.max_subscriptions_per_connection {
                self.max_subscriptions_per_connection = Some(max);
            }
            if let Some(lifecycle) = crate::subscriptions::WebhookLifecycle::from_config(&subs) {
                info!(
                    "Subscription lifecycle webhooks enabled (on_connect/on_subscribe are \
                     fail-closed)"
                );
                self.subscription_lifecycle = Arc::new(lifecycle);
            }
        }

        // Apply pool tuning config from ServerConfig (if present).
        if let Some(pt) = self.config.pool_tuning.clone() {
            if pt.enabled {
                self = self
                    .with_pool_tuning(pt)
                    .map_err(|e| ServerError::ConfigError(format!("pool_tuning: {e}")))?;
            }
        }

        Ok(self)
    }

    /// Shared initialization path used by every public constructor.
    ///
    /// Accepts a pre-built executor so that relay vs. non-relay vs. Flight
    /// constructors can supply the appropriate variant without duplicating
    /// auth/rate-limiter/observer setup.
    ///
    /// `flight_service` is the caller-supplied Arrow Flight service (`None` to
    /// build a fresh one). Whichever it is, the OIDC validator built here is
    /// installed into it — the single place that wiring happens, so a service
    /// handed in by `main.rs` cannot be mounted without authentication (#783).
    #[allow(clippy::cognitive_complexity)] // Reason: internal constructor that assembles server from pre-built subsystems
    pub(super) async fn from_executor(
        config: ServerConfig,
        executor: Arc<Executor<A>>,
        subscription_manager: Arc<SubscriptionManager>,
        subsystems: SchemaSubsystems,
        // `db_pool` is forwarded to the observer runtime and/or auth enrichment.
        #[cfg_attr(
            not(any(feature = "observers", feature = "auth")),
            allow(unused_variables)
        )]
        db_pool: Option<sqlx::PgPool>,
        #[cfg(feature = "arrow")] flight_service: Option<FraiseQLFlightService>,
    ) -> Result<Self> {
        let SchemaSubsystems {
            #[cfg(feature = "federation")]
            circuit_breaker,
            error_sanitizer,
            #[cfg(feature = "auth")]
            state_encryption,
            #[cfg(feature = "auth")]
            pkce_store,
            #[cfg(feature = "auth")]
            oidc_server_client,
            rate_limiter,
            api_key_authenticator,
            service_account_authenticator,
            revocation_manager,
            trusted_docs,
            mut tasks,
        } = subsystems;

        // Initialize OIDC validator if auth is configured.
        //
        // Deliberately *not* `#[cfg(feature = "auth")]`-gated: `OidcValidator`
        // lives in `fraiseql-core` and does not depend on the server's `auth`
        // feature. `with_flight_service` used to gate it, so a
        // `--no-default-features --features cli,arrow` build silently discarded
        // `config.auth` and served every request as an anonymous principal (#783).
        let oidc_validator = if let Some(ref auth_config) = config.auth {
            info!(
                issuer = ?auth_config.issuer,
                "Initializing OIDC authentication"
            );
            let validator = OidcValidator::new(auth_config.clone())
                .await
                .map_err(|e| ServerError::ConfigError(format!("Failed to initialize OIDC: {e}")))?;
            Some(Arc::new(validator))
        } else {
            None
        };

        // Initialize HS256 validator if configured (mutually exclusive with OIDC).
        let hs256_auth = build_hs256_auth(&config)?;

        // Initialize observer runtime
        #[cfg(feature = "observers")]
        let observer_runtime = Self::init_observer_runtime(&config, db_pool.as_ref()).await?;

        // Install the OIDC validator into the Flight service — the caller's when
        // one was supplied, otherwise a fresh one. The Flight handshake is
        // fail-closed on a missing validator, so skipping this leaves the whole
        // Arrow Flight surface unreachable no matter how `[auth]` is set (#783).
        #[cfg(feature = "arrow")]
        let flight_service = {
            let mut service = flight_service.unwrap_or_default();
            if let Some(ref validator) = oidc_validator {
                info!("Enabling OIDC authentication for Arrow Flight");
                service.set_oidc_validator(validator.clone());
            } else {
                info!("Arrow Flight initialized without authentication (dev mode)");
            }
            Some(service)
        };

        // Warn if PKCE is configured but no OidcServerClient could be built.
        #[cfg(feature = "auth")]
        if pkce_store.is_some() && oidc_server_client.is_none() {
            tracing::error!(
                "pkce.enabled = true but no OIDC client is available. Auth routes \
                 (/auth/start, /auth/callback) will NOT be mounted. Building an \
                 OidcServerClient from the compiled schema's [auth] block is not yet \
                 functional (the compiled schema carries no auth/auth_endpoints) — \
                 tracked in #621."
            );
        }

        // Refuse to start if FRAISEQL_REQUIRE_REDIS is set and PKCE store is in-memory.
        #[cfg(feature = "auth")]
        Self::check_redis_requirement(pkce_store.as_ref())?;

        // Spawn background PKCE state cleanup task (every 5 minutes).
        #[cfg(feature = "auth")]
        Self::spawn_pkce_cleanup(pkce_store.as_ref(), &mut tasks);

        Ok(Self {
            config,
            executor,
            subscription_manager,
            subscription_lifecycle: Arc::new(crate::subscriptions::NoopLifecycle),
            max_subscriptions_per_connection: None,
            oidc_validator,
            hs256_auth,
            rate_limiter,
            #[cfg(feature = "secrets")]
            secrets_manager: None,
            #[cfg(feature = "federation")]
            circuit_breaker,
            error_sanitizer,
            #[cfg(feature = "auth")]
            state_encryption,
            #[cfg(feature = "auth")]
            pkce_store,
            #[cfg(feature = "auth")]
            oidc_server_client,
            #[cfg(feature = "auth")]
            social_login: None,
            #[cfg(feature = "auth")]
            mfa_state: None,
            #[cfg(feature = "auth")]
            anon_signup_state: None,
            api_key_authenticator,
            service_account_authenticator,
            revocation_manager,
            apq_store: None,
            trusted_docs,
            #[cfg(feature = "observers")]
            observer_runtime,
            #[cfg(feature = "auth")]
            enrichment_pool: db_pool.clone(),
            #[cfg(feature = "observers")]
            db_pool,
            storage_state: None,
            #[cfg(feature = "functions-runtime")]
            functions_hooks: None,
            tenant_executor_factory: None,
            #[cfg(feature = "arrow")]
            flight_service,
            #[cfg(feature = "mcp")]
            mcp_config: None,
            pool_tuning_config: None,
            adapter_cache_enabled: false,
            storage_max_upload_bytes: 100 * 1024 * 1024, // 100 MiB default
            #[cfg(feature = "functions")]
            function_store: None,
            #[cfg(feature = "functions")]
            function_runtime: None,
            usage: Arc::clone(crate::usage::aggregator::global_aggregator()),
            // The default construction path. `with_relay_pagination` overrides it
            // with the relay-capable one; nothing else may set it, so a reload
            // always rebuilds exactly what boot built (#750).
            executor_rebuilder: Arc::new(Executor::with_config),
            tasks,
        })
    }

    /// Spawn the periodic PKCE state-cleanup task into the server's [`JoinSet`].
    ///
    /// Cleanup runs every 5 minutes for the lifetime of the server. The handle
    /// is owned by `tasks` so graceful shutdown awaits its termination.
    #[cfg(feature = "auth")]
    pub(super) fn spawn_pkce_cleanup(
        pkce_store: Option<&Arc<crate::auth::PkceStateStore>>,
        tasks: &mut tokio::task::JoinSet<()>,
    ) {
        use std::time::Duration;

        use tokio::time::MissedTickBehavior;

        if let Some(store) = pkce_store {
            let store_clone = Arc::clone(store);
            tasks.spawn(async move {
                let mut ticker = tokio::time::interval(Duration::from_secs(300));
                ticker.set_missed_tick_behavior(MissedTickBehavior::Skip);
                loop {
                    ticker.tick().await;
                    store_clone.cleanup_expired().await;
                }
            });
        }
    }

    /// Set lifecycle hooks for `WebSocket` subscriptions.
    #[must_use]
    pub fn with_subscription_lifecycle(
        mut self,
        lifecycle: Arc<dyn crate::subscriptions::SubscriptionLifecycle>,
    ) -> Self {
        self.subscription_lifecycle = lifecycle;
        self
    }

    /// Set maximum subscriptions allowed per `WebSocket` connection.
    #[must_use]
    pub const fn with_max_subscriptions_per_connection(mut self, max: u32) -> Self {
        self.max_subscriptions_per_connection = Some(max);
        self
    }

    /// Attach the per-tenant executor factory used to provision tenants at
    /// runtime (`PUT /api/v1/admin/tenants/{key}`).
    ///
    /// Build it with
    /// [`make_executor_factory`](crate::tenancy::make_executor_factory) from the
    /// concrete adapter type (which must implement
    /// [`FromPoolConfig`](crate::tenancy::FromPoolConfig)). When the multi-tenant
    /// runtime is enabled (`[tenancy.runtime] enabled = true`), `build_app_state`
    /// installs it into `AppState`. Call this before `serve`.
    #[must_use]
    pub fn with_tenant_executor_factory(
        mut self,
        factory: crate::tenancy::TenantExecutorFactory<A>,
    ) -> Self {
        self.tenant_executor_factory = Some(factory);
        self
    }

    /// Enable adaptive connection pool sizing.
    ///
    /// When `config.enabled` is `true`, the server will spawn a background
    /// polling task that samples pool metrics and recommends or applies resizes.
    ///
    /// # Errors
    ///
    /// Returns an error string if the configuration fails validation (e.g. `min >= max`).
    pub fn with_pool_tuning(
        mut self,
        config: crate::config::pool_tuning::PoolPressureMonitorConfig,
    ) -> std::result::Result<Self, String> {
        config.validate()?;
        self.pool_tuning_config = Some(config);
        Ok(self)
    }

    /// Attach a unified social-login provider registry.
    ///
    /// When set, the server mounts `GET /auth/v1/authorize` which redirects
    /// users to the specified `OAuth` provider's authorization URL with a `CSRF`
    /// state token.
    ///
    /// # Example
    ///
    /// ```ignore
    /// use fraiseql_auth::social::{SocialLoginState, SocialProviderRegistry};
    /// let state = Arc::new(SocialLoginState { ... });
    /// let server = server.with_social_login(state);
    /// ```
    #[cfg(feature = "auth")]
    #[must_use]
    pub fn with_social_login(
        mut self,
        social_login: Arc<crate::auth::social::SocialLoginState>,
    ) -> Self {
        self.social_login = Some(social_login);
        self
    }

    /// Attach anonymous signup state to mount `POST /auth/v1/signup`.
    ///
    /// When set, any client can obtain a guest session without credentials.
    /// The returned `user_id` carries an `anon_` prefix and the session lasts
    /// 7 days.  Signups are rate-limited per client IP.
    #[cfg(feature = "auth")]
    #[must_use]
    pub fn with_anon_signup(mut self, state: Arc<crate::auth::AnonSignupState>) -> Self {
        self.anon_signup_state = Some(state);
        self
    }

    /// Attach `TOTP` `MFA` state to mount the `/auth/v1/mfa/` endpoints.
    ///
    /// Mounts four routes:
    /// - `POST /auth/v1/mfa/enroll` — begin enrollment, returns `otpauth://` `URI`
    /// - `POST /auth/v1/mfa/confirm` — confirm enrollment with a live `TOTP` code
    /// - `POST /auth/v1/mfa/challenge` — issue a short-lived challenge token
    /// - `POST /auth/v1/mfa/verify` — verify code and issue session
    /// - `POST /auth/v1/mfa/unenroll` — remove `MFA` from an account
    #[cfg(feature = "auth")]
    #[must_use]
    pub fn with_mfa(mut self, mfa_state: Arc<crate::auth::MfaRouteState>) -> Self {
        self.mfa_state = Some(mfa_state);
        self
    }

    /// Override the maximum allowed upload size for storage endpoints.
    ///
    /// Defaults to 100 `MiB`. Uploads exceeding this size are rejected with HTTP 413
    /// before the body is forwarded to the storage backend.
    #[must_use]
    pub const fn with_storage_max_upload_bytes(mut self, bytes: usize) -> Self {
        self.storage_max_upload_bytes = bytes;
        self
    }

    /// Attach a pre-built [`StorageState`](fraiseql_storage::StorageState) and
    /// mount the bucket-scoped `/storage/v1/*` routes (object upload/download/
    /// delete, list, presign) with per-bucket access policy and RLS.
    ///
    /// This is the **only** storage path. Until #813/#866 there was a second one
    /// — `with_storage`, mounting a metadata-less router with no per-object
    /// ownership — which duplicated the key validator and the Azure key encoder
    /// and carried its own copy of both defects. It has been removed; see the
    /// `### Breaking` note in the changelog.
    ///
    /// The attached backend also serves as the inbound-email attachment sink
    /// when `[mailbox]` names an `attachment_bucket`.
    ///
    /// Authentication is applied by `mount_storage_state`: a configured
    /// `storage_token` acts as an admin bearer and, when an OIDC validator is
    /// present, per-user tokens populate the request's `StorageUser` for RLS.
    #[must_use]
    pub fn with_storage_state(mut self, state: fraiseql_storage::StorageState) -> Self {
        self.storage_state = Some(state);
        self
    }

    /// Install a pre-built token-revocation manager, replacing whatever the generic
    /// construction path produced.
    ///
    /// Used by the PostgreSQL runtime path to install the Postgres-backed store
    /// (#357): `revocation_manager_from_schema` defers the `postgres` backend because
    /// it needs a database connection, so `main.rs` builds it with
    /// `build_postgres_revocation_manager` and installs it here.
    #[must_use]
    pub fn with_revocation_manager(
        mut self,
        manager: Arc<crate::token_revocation::TokenRevocationManager>,
    ) -> Self {
        self.revocation_manager = Some(manager);
        self
    }

    /// Attach a function deployment store and runtime, mounting `/functions/v1/` routes.
    ///
    /// When set, the server mounts `POST /functions/v1/{name}` which loads the
    /// function bytecode from `store`, executes it via `runtime`, and returns the
    /// JSON-encoded [`FunctionResult`](fraiseql_functions::FunctionResult).
    #[cfg(feature = "functions")]
    #[must_use]
    pub fn with_functions(
        mut self,
        store: Arc<dyn fraiseql_functions::FunctionStore>,
        runtime: Arc<dyn fraiseql_functions::runtime::SendFunctionRuntime>,
    ) -> Self {
        self.function_store = Some(store);
        self.function_runtime = Some(runtime);
        self
    }

    /// Set secrets manager for the server.
    ///
    /// This allows attaching a secrets manager after server creation for credential management.
    #[cfg(feature = "secrets")]
    pub fn set_secrets_manager(&mut self, manager: Arc<crate::secrets_manager::SecretsManager>) {
        self.secrets_manager = Some(manager);
        info!("Secrets manager attached to server");
    }

    /// Serve MCP over stdio (stdin/stdout) instead of HTTP.
    ///
    /// This is used when `FRAISEQL_MCP_STDIO=1` is set.  The server reads JSON-RPC
    /// messages from stdin and writes responses to stdout, following the MCP stdio
    /// transport specification.
    ///
    /// # Errors
    ///
    /// Returns an error if MCP is not configured or the stdio transport fails.
    #[cfg(feature = "mcp")]
    pub async fn serve_mcp_stdio(self) -> Result<()> {
        use rmcp::ServiceExt;

        let mcp_cfg = self.mcp_config.ok_or_else(|| {
            ServerError::ConfigError(
                "FRAISEQL_MCP_STDIO=1 but MCP is not configured. \
                 Add [mcp] enabled = true to fraiseql.toml and recompile the schema."
                    .into(),
            )
        })?;

        let schema = Arc::new(self.executor.schema().clone());
        let executor = self.executor.clone();

        let service = crate::mcp::handler::FraiseQLMcpService::new(schema, executor, mcp_cfg)
            .with_oidc_validator(self.oidc_validator.clone());

        info!("MCP stdio transport starting — reading from stdin, writing to stdout");

        let running = service
            .serve((tokio::io::stdin(), tokio::io::stdout()))
            .await
            .map_err(|e| ServerError::ConfigError(format!("MCP stdio init failed: {e}")))?;

        running
            .waiting()
            .await
            .map_err(|e| ServerError::ConfigError(format!("MCP stdio error: {e}")))?;

        Ok(())
    }
}
