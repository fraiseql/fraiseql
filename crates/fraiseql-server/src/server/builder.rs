//! Server constructors and builder methods.

use std::sync::Arc;

#[cfg(feature = "arrow")]
use fraiseql_arrow::FraiseQLFlightService;
use fraiseql_core::{
    cache::CachedDatabaseAdapter,
    db::traits::DatabaseAdapter,
    runtime::{Executor, SubscriptionManager},
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
    /// Parsed `[security.api_keys]` config. The authenticator is built in
    /// `from_executor`, where the database pool is in scope — `storage =
    /// "postgres"` needs it, and a config that demands it without one refuses
    /// to boot there (#627).
    pub api_key_config: Option<crate::api_key::ApiKeyConfig>,
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
        // audit-logging flag, applies the #421 page-size ceiling, the change-log
        // toggle, and the runtime [validation] override on the executor gate
        // (#379). Doing it first preserves "reject bad version before any
        // further setup".
        let executor_config = crate::server::initialization::executor_runtime_config(
            &schema, &config,
        )
        .map_err(|msg| ServerError::ConfigError(format!("Incompatible compiled schema: {msg}")))?;

        // Refuse to boot if any field is marked for at-rest encryption: the write path does
        // not encrypt (H12), so those fields would be stored in plaintext. Fail loud rather
        // than silently storing sensitive data unencrypted.
        crate::server::initialization::field_encryption_unsupported_check(&schema)?;

        // Read every schema-derived subsystem through the one shared seam.
        let subsystems = Self::schema_subsystems(&schema, &config).await?;

        // Build the result cache and wrap the adapter — the one seam every
        // constructor shares, so `cache_enabled` cannot mean three different
        // things by constructor (#889). Runs the cache+RLS gates.
        let (cached, cache_config) = crate::server::initialization::build_cached_adapter(
            &schema,
            config.cache_enabled,
            adapter,
        )
        .await?;

        // `executor_config` was built from the compiled schema at the top of this
        // constructor (the H16 seam — audit flag, #421 page-size, change-log toggle).
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
            #[cfg(feature = "arrow")]
            None,
        ))
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
        let pkce_store = super::initialization::pkce_store_from_schema_in(
            schema,
            state_encryption.as_ref(),
            crate::ServerConfig::is_production_mode(),
        )
        .await?;
        #[cfg(feature = "auth")]
        let oidc_server_client = Self::oidc_server_client_from_schema(schema).await;
        // Both configuration sources resolved together, so the boot guards run on
        // whatever actually takes effect (#837) and CLI/env overrides win (#774).
        let rate_limiter = super::initialization::resolve_rate_limiter(schema, config).await?;
        // #787/#781: a configured inbound webhook route must be servable — known
        // provider, its secret env set (production), and a public_url when the
        // provider signs the request URL — or the server refuses to boot instead
        // of mounting routes that 404/500 every genuine delivery.
        #[cfg(feature = "inbound")]
        crate::inbound::webhook_routes_check(
            &config.webhooks,
            |name| std::env::var(name).ok(),
            crate::ServerConfig::is_production_mode(),
        )?;
        let api_key_config = crate::api_key::api_key_config_from_schema(schema);
        let service_account_authenticator =
            crate::service_account::service_account_authenticator_from_schema(schema);
        if service_account_authenticator.is_some() {
            info!("Service-account authentication enabled");
        }
        let revocation_manager =
            crate::token_revocation::revocation_manager_from_schema(schema).await?;
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
            api_key_config,
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
        // #874: every entry point — the binary, `fraiseql run`, and the
        // documented library embedding (`from_file` + `Server::new`) — faces the
        // same production safety gates. `main.rs` also validates (earlier, for a
        // friendlier error before any subsystem work); this is the backstop the
        // library path used to lack, which let a public playground, a zero pool
        // timeout, or OIDC+HS256 both configured boot without complaint.
        config.validate().map_err(ServerError::ConfigError)?;

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
            api_key_config,
            service_account_authenticator,
            revocation_manager,
            trusted_docs,
            // Reason: `tasks` is only mutated by the auth-gated PKCE cleanup spawn below
            #[cfg_attr(not(feature = "auth"), allow(unused_mut))]
            mut tasks,
        } = subsystems;

        // Build the API-key authenticator here, where `db_pool` is in scope.
        // `storage = "postgres"` without a pool must refuse to boot — the
        // alternative is #627's original defect: an authenticator with zero
        // keys that authenticates nothing, silently.
        let api_key_authenticator = match &api_key_config {
            Some(cfg) if cfg.enabled && cfg.storage == "postgres" => {
                let pool = db_pool.clone().ok_or_else(|| {
                    ServerError::ConfigError(
                        "[security.api_keys] storage = \"postgres\" requires a database \
                         pool, and this server was constructed without one. Pass a PgPool \
                         to Server::new (the binary does this automatically when \
                         database_url is set), or use storage = \"env\"."
                            .to_string(),
                    )
                })?;
                let authenticator = crate::api_key::ApiKeyAuthenticator::from_config(cfg)
                    .ok_or_else(|| {
                        ServerError::ConfigError(
                            "[security.api_keys] is enabled but invalid — see the warnings \
                             above for the offending field"
                                .to_string(),
                        )
                    })?
                    .with_postgres(crate::api_key::postgres::PgApiKeyStore::new(pool));
                info!("API key authentication enabled (postgres-backed store)");
                Some(Arc::new(authenticator))
            },
            Some(cfg) => {
                let authenticator =
                    crate::api_key::ApiKeyAuthenticator::from_config(cfg).map(Arc::new);
                if authenticator.is_some() {
                    info!("API key authentication enabled");
                }
                authenticator
            },
            None => None,
        };

        // Build the async-operations subsystem (#391) here, where `db_pool` is
        // in scope. A configured surface whose table cannot be initialised
        // refuses to boot — the routes and workers must never half-mount.
        let async_operations = Self::build_async_operations(&config, db_pool.as_ref()).await?;

        // Build the outbound CDC drains (#382) here, where `db_pool` is in
        // scope: a configured section whose broker is unreachable or whose
        // delivery-state DDL fails must refuse to boot, not start a server
        // that silently replicates nothing.
        #[cfg(feature = "cdc-outbound")]
        let cdc_drains =
            crate::cdc_outbound::build_drains(config.cdc_outbound.as_ref(), db_pool.as_ref())
                .await
                .map_err(ServerError::ConfigError)?
                .unwrap_or_default();

        // Build the session-state subsystem (#389) here, where `db_pool` is in
        // scope. `backend = "postgres"` without a pool, or with a table that
        // cannot be initialised, must refuse to boot — never downgrade to the
        // volatile in-memory backend (the P21 backend rule).
        #[cfg(feature = "auth")]
        let session_state = Self::build_session_state(&config, db_pool.as_ref()).await?;

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

        // Social-login state (#368): built here, where the pool is in scope.
        // The compiled `[auth.social]` block declares the providers; every
        // configured-but-unusable shape (missing [auth_hs256], unset secret
        // env, no pool, unreachable Google discovery) refuses to boot.
        #[cfg(feature = "auth")]
        let social_login = match executor.schema().auth.as_ref().and_then(|a| a.social.as_ref()) {
            Some(social_cfg) => {
                // Boxed for the same reason as `build_local_auth_states` below.
                Some(
                    Box::pin(Self::build_social_state(social_cfg, &config, db_pool.clone()))
                        .await?,
                )
            },
            None => None,
        };

        // `[auth.local]` states (#367): password / OTP / MFA / anonymous. Built
        // here for the same reason as the social states — the pool is in scope,
        // and every enabled method needs it.
        #[cfg(feature = "auth")]
        let local_auth = match executor.schema().auth.as_ref().and_then(|a| a.local.as_ref()) {
            // Boxed: this builds four route states plus their stores, and
            // inlining that into `from_executor`'s future tips it past clippy's
            // `large_futures` stack budget at every call site.
            Some(local_cfg) => {
                Box::pin(crate::auth_local::build_local_auth_states(
                    local_cfg,
                    &config,
                    db_pool.clone(),
                ))
                .await?
            },
            None => crate::auth_local::LocalAuthStates {
                otp:      None,
                mfa:      None,
                password: None,
                anon:     None,
            },
        };

        // SAML SP state (#381): built here, where the pool is in scope. The
        // `[saml]` section is validated by ServerConfig::validate (shape +
        // [auth_hs256] requirement); this adds the runtime requirements — a
        // database pool (sessions + account linking) and parseable IdP
        // metadata — failing loud on each.
        #[cfg(feature = "auth-saml")]
        let saml_state = match &config.saml {
            Some(saml_cfg) => {
                let pool = db_pool.clone().ok_or_else(|| {
                    ServerError::ConfigError(
                        "[saml] requires a database pool: verified assertions mint \
                         Postgres-backed sessions and resolve accounts through the \
                         account store. The binary provides one when database_url is \
                         set; library embedders must pass a PgPool to Server::new."
                            .to_string(),
                    )
                })?;
                let hs = config.auth_hs256.as_ref().ok_or_else(|| {
                    // validate() enforces this; belt-and-braces for embedders that
                    // construct ServerConfig programmatically and skip nothing else.
                    ServerError::ConfigError("[saml] requires [auth_hs256]".to_string())
                })?;
                let secret = hs.load_secret().map_err(ServerError::ConfigError)?;
                // Mint sessions with the claims the configured HS256 validator
                // demands — defaults would 401 on the first validated request.
                let session_store = Arc::new(
                    fraiseql_auth::PostgresSessionStore::with_hs256_secret(
                        pool.clone(),
                        secret.into_bytes(),
                    )
                    .with_token_claims(
                        hs.issuer.clone().unwrap_or_else(|| {
                            fraiseql_auth::session_postgres::DEFAULT_TOKEN_ISSUER.to_string()
                        }),
                        hs.audience.clone().unwrap_or_else(|| {
                            fraiseql_auth::session_postgres::DEFAULT_TOKEN_AUDIENCE.to_string()
                        }),
                    ),
                );
                let account_store = Arc::new(fraiseql_auth::PostgresAccountStore::new(pool));
                let state_store = Arc::new(fraiseql_auth::InMemoryStateStore::new());
                let mut state = fraiseql_auth::saml::SamlAuthState::new(state_store, session_store)
                    .with_user_store(account_store);
                for (name, entry) in &saml_cfg.idps {
                    let xml = match (&entry.metadata_xml, &entry.metadata_xml_path) {
                        (Some(xml), _) => xml.clone(),
                        (None, Some(path)) => std::fs::read_to_string(path).map_err(|e| {
                            ServerError::ConfigError(format!(
                                "[saml.idps.{name}] cannot read metadata_xml_path {}: {e}",
                                path.display()
                            ))
                        })?,
                        (None, None) => {
                            // validate() enforces exactly one source; a
                            // programmatic config that skips validate is
                            // caught by the same gate in from_executor.
                            return Err(ServerError::ConfigError(format!(
                                "[saml.idps.{name}] needs exactly one of metadata_xml / \
                                 metadata_xml_path"
                            )));
                        },
                    };
                    let idp = fraiseql_auth::saml::SamlIdpConfig::builder(
                        name.clone(),
                        entry.sp_entity_id.clone(),
                        entry.acs_url.clone(),
                    )
                    .idp_metadata_xml(&xml)
                    .map_err(|e| {
                        ServerError::ConfigError(format!(
                            "[saml.idps.{name}] metadata does not parse: {e}"
                        ))
                    })?
                    .tenant_id(entry.tenant_id.clone())
                    .trust_asserted_email(entry.trust_asserted_email)
                    .build()
                    .map_err(|e| {
                        ServerError::ConfigError(format!(
                            "[saml.idps.{name}] configuration invalid: {e}"
                        ))
                    })?;
                    state = state.with_idp(idp);
                }
                info!(
                    idps = saml_cfg.idps.len(),
                    "SAML SP-initiated SSO enabled (/auth/saml/login, /auth/saml/acs)"
                );
                Some(state)
            },
            None => None,
        };

        // Refuse to start if FRAISEQL_REQUIRE_REDIS is set and any running
        // shared-auth-state subsystem is per-process (#874): PKCE, rate
        // limiting, and token revocation are all part of the operator's
        // "distributed state" assertion, not just PKCE.
        Self::check_redis_requirement(&crate::server::initialization::SharedStateBackends {
            #[cfg(feature = "auth")]
            pkce_in_memory: pkce_store.as_ref().is_some_and(|s| s.is_in_memory()),
            #[cfg(not(feature = "auth"))]
            pkce_in_memory: false,
            rate_limiter_in_memory: rate_limiter.as_ref().is_some_and(|rl| !rl.is_distributed()),
            revocation_in_memory: revocation_manager
                .as_ref()
                .is_some_and(|rm| !rm.is_distributed()),
        })?;

        // Spawn background PKCE state cleanup task (every 5 minutes).
        #[cfg(feature = "auth")]
        Self::spawn_pkce_cleanup(pkce_store.as_ref(), &mut tasks);

        // Spawn the session-state eviction sweep (#389) at the configured cadence.
        #[cfg(feature = "auth")]
        Self::spawn_session_state_eviction(
            session_state.as_ref(),
            config.session_state.as_ref().map_or(300, |c| c.evict_interval_secs),
            &mut tasks,
        );

        Ok(Self {
            config,
            executor,
            subscription_manager,
            subscription_lifecycle: Arc::new(crate::subscriptions::NoopLifecycle),
            subscription_drain: Arc::new(tokio::sync::watch::channel(false).0),
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
            social_login,
            #[cfg(feature = "auth")]
            mfa_state: local_auth.mfa,
            #[cfg(feature = "auth")]
            anon_signup_state: local_auth.anon,
            #[cfg(feature = "auth")]
            otp_state: local_auth.otp,
            #[cfg(feature = "auth")]
            local_password_state: local_auth.password,
            #[cfg(feature = "auth")]
            session_state,
            async_operations,
            #[cfg(feature = "cdc-outbound")]
            cdc_drains,
            #[cfg(feature = "auth-saml")]
            saml_state,
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
            #[cfg(feature = "rest")]
            rest_router_builder: None,
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

    /// The session-state subsystem (#389), when `[session_state]` is configured.
    ///
    /// The per-request consumer surface: the MCP session-continuity binding
    /// reads it, and library embedders can reach the store here to share
    /// threads with their own transports.
    #[cfg(feature = "auth")]
    #[must_use]
    pub const fn session_state(&self) -> Option<&Arc<fraiseql_auth::session_state::SessionState>> {
        self.session_state.as_ref()
    }

    /// Spawn the configured number of async-operation workers (#391) onto the
    /// server's [`JoinSet`](tokio::task::JoinSet). No-op when the subsystem is
    /// not configured.
    pub(super) fn spawn_async_operation_workers(
        &mut self,
        state: &crate::routes::graphql::AppState<A>,
    ) {
        let Some(runtime) = self.async_operations.clone() else {
            return;
        };
        for _ in 0..runtime.config.workers {
            let runtime = runtime.clone();
            let state = state.clone();
            self.tasks.spawn(crate::async_operations::worker::run(runtime, state));
        }
    }

    /// Build the async-operations subsystem from `[async_operations]` (#391).
    ///
    /// `None` when the section is absent. A configured section without a
    /// database pool, or whose `_system.async_operations` table cannot be
    /// initialised, is a **boot refusal** — the surface must never half-mount
    /// (routes accepting submissions no worker will ever execute).
    ///
    /// `#[doc(hidden)] pub`: the boot-refusal matrix is pinned by tests that
    /// drive this seam directly.
    ///
    /// # Errors
    ///
    /// Returns [`ServerError::ConfigError`] on a missing pool or failed init.
    #[doc(hidden)]
    pub async fn build_async_operations(
        config: &ServerConfig,
        db_pool: Option<&sqlx::PgPool>,
    ) -> std::result::Result<Option<crate::async_operations::AsyncOperationsRuntime>, ServerError>
    {
        let Some(cfg) = config.async_operations.as_ref() else {
            return Ok(None);
        };
        let pool = db_pool.cloned().ok_or_else(|| {
            ServerError::ConfigError(
                "[async_operations] requires a database pool — operations are durable state. \
                 The binary provides one when database_url is set; library embedders must pass \
                 a PgPool to Server::new."
                    .to_string(),
            )
        })?;
        let store = crate::async_operations::AsyncOperationStore::new(pool);
        store.init().await.map_err(|e| {
            ServerError::ConfigError(format!(
                "[async_operations] could not initialise _system.async_operations: {e}. \
                 Refusing to boot rather than mounting a surface whose submissions nothing \
                 could execute."
            ))
        })?;
        info!(
            operations = ?cfg.operations,
            workers = cfg.workers,
            "Async operations enabled"
        );
        Ok(Some(crate::async_operations::AsyncOperationsRuntime {
            store:  Arc::new(store),
            config: Arc::new(cfg.clone()),
        }))
    }

    /// Build the session-state subsystem from `[session_state]` (#389).
    ///
    /// `None` when the section is absent. `backend = "postgres"` without a
    /// database pool, or whose `_system.session_state` table cannot be
    /// initialised, is a **boot refusal** — the alternative is a silent
    /// downgrade to the volatile in-memory backend, which is exactly the
    /// configured-but-unusable-backend defect class this program exists to
    /// close (the P21 rule).
    ///
    /// `#[doc(hidden)] pub`: the boot-refusal matrix is pinned by tests that
    /// must drive this seam directly (`Server::new` needs a bound listener).
    ///
    /// # Errors
    ///
    /// Returns [`ServerError::ConfigError`] on an unsupported backend token, a
    /// missing pool, or a failed table initialisation.
    #[cfg(feature = "auth")]
    #[doc(hidden)]
    pub async fn build_session_state(
        config: &ServerConfig,
        db_pool: Option<&sqlx::PgPool>,
    ) -> std::result::Result<Option<Arc<fraiseql_auth::session_state::SessionState>>, ServerError>
    {
        use fraiseql_auth::session_state::{
            InMemorySessionStateStore, PostgresSessionStateStore, SessionState, SessionStateBackend,
        };

        let Some(cfg) = config.session_state.as_ref() else {
            return Ok(None);
        };
        let backend = match cfg.backend.as_str() {
            "memory" => {
                tracing::warn!(
                    "[session_state] backend = \"memory\" is volatile — every thread is lost on \
                     restart. Use backend = \"postgres\" in production."
                );
                SessionStateBackend::InMemory(InMemorySessionStateStore::new())
            },
            "postgres" => {
                let pool = db_pool.cloned().ok_or_else(|| {
                    ServerError::ConfigError(
                        "[session_state] backend = \"postgres\" requires a database pool, and \
                         this server was constructed without one. Pass a PgPool to Server::new \
                         (the binary does this automatically when database_url is set), or use \
                         backend = \"memory\" for local development."
                            .to_string(),
                    )
                })?;
                let store = PostgresSessionStateStore::new(pool);
                store.init().await.map_err(|e| {
                    ServerError::ConfigError(format!(
                        "[session_state] backend = \"postgres\" could not initialise \
                         _system.session_state: {e}. Refusing to boot rather than silently \
                         downgrading to the volatile in-memory backend."
                    ))
                })?;
                info!("Session state enabled (postgres-backed, durable)");
                SessionStateBackend::Postgres(store)
            },
            // ServerConfig::validate() refuses unknown tokens before this runs;
            // kept as a hard error so this seam is safe to drive directly.
            other => {
                return Err(ServerError::ConfigError(format!(
                    "[session_state] backend = \"{other}\" is not supported — use \"memory\" or \
                     \"postgres\"."
                )));
            },
        };
        Ok(Some(Arc::new(SessionState::new(backend, cfg.default_ttl_secs))))
    }

    /// Spawn the periodic session-state eviction sweep into the server's [`JoinSet`].
    ///
    /// Expired entries are already invisible to reads; the sweep reclaims their
    /// storage at the configured cadence. Owned by `tasks` so graceful shutdown
    /// awaits its termination.
    #[cfg(feature = "auth")]
    pub(super) fn spawn_session_state_eviction(
        state: Option<&Arc<fraiseql_auth::session_state::SessionState>>,
        interval_secs: u64,
        tasks: &mut tokio::task::JoinSet<()>,
    ) {
        use std::time::Duration;

        use tokio::time::MissedTickBehavior;

        if let Some(state) = state {
            let state = Arc::clone(state);
            tasks.spawn(async move {
                let mut ticker = tokio::time::interval(Duration::from_secs(interval_secs.max(1)));
                ticker.set_missed_tick_behavior(MissedTickBehavior::Skip);
                ticker.tick().await; // skip the immediate first tick
                loop {
                    ticker.tick().await;
                    match state.evict_expired().await {
                        Ok(0) => {},
                        Ok(n) => tracing::debug!(evicted = n, "session-state eviction sweep"),
                        Err(e) => {
                            tracing::warn!(error = %e, "session-state eviction sweep failed");
                        },
                    }
                }
            });
        }
    }

    /// Build the social-login state from the compiled `[auth.social]` block
    /// (#368) — the trust-gated `multi_provider` flow, backed by
    /// Postgres-backed sessions and account linking.
    ///
    /// Fail-loud on every configured-but-unusable shape, in config-first order
    /// so DB-less misconfigurations surface before the pool requirement:
    /// missing `[auth_hs256]`, an unset provider `client_secret_env`, an
    /// invalid provider endpoint (SSRF-guarded), an unreachable Google
    /// discovery document, and finally a missing database pool.
    ///
    /// # Errors
    ///
    /// Returns [`ServerError::ConfigError`] naming the offending section for
    /// each of the shapes above.
    #[cfg(feature = "auth")]
    async fn build_social_state(
        social: &fraiseql_core::schema::SocialAuthConfig,
        config: &ServerConfig,
        db_pool: Option<sqlx::PgPool>,
    ) -> Result<Arc<fraiseql_auth::MultiProviderAuthState>> {
        use fraiseql_auth::provider::OAuthProvider;

        let hs = config.auth_hs256.as_ref().ok_or_else(|| {
            ServerError::ConfigError(
                "[auth.social] requires [auth_hs256]: the OAuth callback mints HS256-signed \
                 sessions this server itself validates. Add [auth_hs256] to the server config."
                    .to_string(),
            )
        })?;

        let read_secret = |provider: &str, env_name: &str| {
            std::env::var(env_name).map_err(|_| {
                ServerError::ConfigError(format!(
                    "[auth.social.{provider}] client_secret_env {env_name} is not set. The \
                     provider cannot exchange authorization codes without it — refusing to \
                     mount a login flow that can never complete."
                ))
            })
        };

        let mut providers: Vec<(&'static str, Arc<dyn OAuthProvider>)> = Vec::new();
        if let Some(g) = &social.google {
            let secret = read_secret("google", &g.client_secret_env)?;
            let issuer = g.discovery_url.as_deref().unwrap_or("https://accounts.google.com");
            let provider = fraiseql_auth::GoogleOAuth::with_issuer(
                g.client_id.clone(),
                secret,
                g.redirect_uri.clone(),
                issuer,
            )
            .await
            .map_err(|e| {
                ServerError::ConfigError(format!(
                    "[auth.social.google] provider construction failed (boot-time OIDC \
                     discovery against {issuer}): {e}"
                ))
            })?;
            providers.push(("google", Arc::new(provider)));
        }
        if let Some(g) = &social.github {
            let secret = read_secret("github", &g.client_secret_env)?;
            let provider = fraiseql_auth::GitHubOAuth::with_endpoints(
                g.client_id.clone(),
                secret,
                g.redirect_uri.clone(),
                g.base_url.clone().unwrap_or_else(|| "https://github.com".to_string()),
                g.api_base_url.clone().unwrap_or_else(|| "https://api.github.com".to_string()),
            )
            .map_err(|e| {
                ServerError::ConfigError(format!(
                    "[auth.social.github] provider construction failed: {e}"
                ))
            })?;
            providers.push(("github", Arc::new(provider)));
        }
        if providers.is_empty() {
            // The CLI refuses this at compile time; belt-and-braces for
            // programmatically constructed schemas.
            return Err(ServerError::ConfigError(
                "[auth.social] is configured but names no provider".to_string(),
            ));
        }

        let pool = db_pool.ok_or_else(|| {
            ServerError::ConfigError(
                "[auth.social] requires a database pool: the callback mints Postgres-backed \
                 sessions and resolves accounts through the account store. The binary provides \
                 one when database_url is set; library embedders must pass a PgPool to \
                 Server::new."
                    .to_string(),
            )
        })?;

        let secret = hs.load_secret().map_err(ServerError::ConfigError)?;
        // Mint sessions with the claims the configured HS256 validator demands
        // — defaults would 401 on the first validated request.
        let session_store = Arc::new(
            fraiseql_auth::PostgresSessionStore::with_hs256_secret(
                pool.clone(),
                secret.into_bytes(),
            )
            .with_token_claims(
                hs.issuer.clone().unwrap_or_else(|| {
                    fraiseql_auth::session_postgres::DEFAULT_TOKEN_ISSUER.to_string()
                }),
                hs.audience.clone().unwrap_or_else(|| {
                    fraiseql_auth::session_postgres::DEFAULT_TOKEN_AUDIENCE.to_string()
                }),
            ),
        );
        let account_store = Arc::new(fraiseql_auth::PostgresAccountStore::new(pool));
        let state_store = Arc::new(fraiseql_auth::InMemoryStateStore::new());
        let mut state = fraiseql_auth::MultiProviderAuthState::new(state_store, session_store)
            .with_user_store(account_store)
            .with_redirect_uri_allowlist(social.redirect_uri_allowlist.clone());
        let names: Vec<&str> = providers.iter().map(|(n, _)| *n).collect();
        for (name, provider) in providers {
            state.register_provider(name, provider);
        }
        info!(
            providers = ?names,
            allowlisted_redirects = social.redirect_uri_allowlist.len(),
            "Social login enabled (/auth/v1/authorize, /auth/v1/callback)"
        );
        Ok(Arc::new(state))
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

    /// Attach a pre-built social-login state, replacing whatever the compiled
    /// `[auth.social]` block produced (#368).
    ///
    /// When set, the server mounts `GET /auth/v1/providers`,
    /// `GET /auth/v1/authorize` and `GET /auth/v1/callback` — the trust-gated
    /// `multi_provider` flow. Prefer configuring `[auth.social]` in
    /// `fraiseql.toml`; this hook exists for library embedders wiring custom
    /// providers.
    #[cfg(feature = "auth")]
    #[must_use]
    pub fn with_social_login(
        mut self,
        social_login: Arc<crate::auth::MultiProviderAuthState>,
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

        let mcp_cfg = self.mcp_config.clone().ok_or_else(|| {
            ServerError::ConfigError(
                "FRAISEQL_MCP_STDIO=1 but MCP is not configured. \
                 Add [mcp] enabled = true to fraiseql.toml and recompile the schema."
                    .into(),
            )
        })?;

        // Built from the same `AppState` the HTTP mount uses (#858): stdio carries
        // no headers, but the tenant registry, the suspended-tenant gate and the
        // error sanitizer must apply on both transports, not just the one whose
        // construction path happened to be wired to them.
        // Same two auth modes as the HTTP mount (#376 parity) — inert on stdio
        // itself (no per-request credentials there), but kept identical so the
        // construction paths cannot drift (#858's lesson).
        let validator = self
            .oidc_validator
            .clone()
            .map(crate::mcp::handler::McpTokenValidator::Oidc)
            .or_else(|| self.hs256_auth.clone().map(crate::mcp::handler::McpTokenValidator::Hs256));
        let service = crate::mcp::handler::FraiseQLMcpService::new(self.build_app_state(), mcp_cfg)
            .with_token_validator(validator);

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

/// Builder methods that require the adapter to support mutations.
///
/// `Server<A>`'s lifecycle — `build_router`, `mount_extensions`, `serve_with_shutdown`,
/// `serve_on_listener` — is deliberately unbounded by
/// [`SupportsMutations`](fraiseql_core::db::traits::SupportsMutations), so read-only
/// adapters can be served. Anything needing that bound therefore has to be installed
/// from a call site that has it, which is what this block is for.
#[cfg(feature = "rest")]
impl<A> Server<A>
where
    A: DatabaseAdapter
        + fraiseql_core::db::traits::SupportsMutations
        + Clone
        + Send
        + Sync
        + 'static,
{
    /// Mount the REST **write** surface — `POST`/`PUT`/`PATCH`/`DELETE` on derived
    /// resources, plus the collection-level bulk update and delete routes.
    ///
    /// Without this call the server mounts `rest_query_router`, which serves reads and
    /// SSE only. That was the shipped state for two releases: `rest_router` had no
    /// production caller at all while the served `OpenAPI` document advertised every
    /// write path, so a client that followed the published contract received `405`
    /// (fraiseql/fraiseql#865, a regression of #227).
    ///
    /// Call it from the boot path, before `serve`. It is not called for read-only
    /// adapters (`SqliteAdapter`, `FraiseWireAdapter`) because they cannot satisfy the
    /// bound — the type system, not a runtime check, is what keeps writes off them.
    ///
    /// The router still passes through `Server::attach_auth` at the shared mount site,
    /// so enabling writes cannot accidentally place them on an unauthenticated
    /// transport (#812).
    #[must_use]
    pub fn with_rest_write_surface(mut self) -> Self {
        self.rest_router_builder = Some(Arc::new(crate::routes::rest::rest_router::<A>));
        self
    }
}
