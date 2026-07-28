//! `AppState` — server state passed to all GraphQL route handlers.

use std::{path::PathBuf, sync::Arc};

use arc_swap::ArcSwap;
use fraiseql_core::{
    apq::{ApqMetrics, ArcApqStorage},
    db::traits::DatabaseAdapter,
    runtime::{Executor, RuntimeConfig},
    schema::CompiledSchema,
    security::IntrospectionPolicy,
};
use tracing::{info, warn};

use super::{tenant_key::DomainRegistry, tenant_registry::TenantExecutorRegistry};
#[cfg(feature = "auth")]
use crate::auth::rate_limiting::{AuthRateLimitConfig, KeyedRateLimiter};
use crate::{
    config::error_sanitization::ErrorSanitizer, error::GraphQLError,
    metrics_server::MetricsCollector, usage::aggregator::UsageAggregator,
};

/// How to rebuild the executor for a new compiled schema, preserving whatever
/// capability the booting constructor gave it.
///
/// `Executor::with_config` for the plain path, `Executor::with_config_and_relay`
/// for the relay path. `Server` records one at construction and threads it into
/// [`AppState`], so a hot-reload is the *same* construction path rather than a
/// fourth one that drifted (#750).
pub type ExecutorRebuilder<A> =
    Arc<dyn Fn(CompiledSchema, Arc<A>, RuntimeConfig) -> Executor<A> + Send + Sync>;

/// Server state containing executor and configuration.
#[derive(Clone)]
pub struct AppState<A: DatabaseAdapter> {
    /// Query executor (atomically swappable for schema hot-reload).
    pub executor: Arc<ArcSwap<Executor<A>>>,
    /// Metrics collector.
    pub metrics: Arc<MetricsCollector>,
    /// Query result cache (optional).
    #[cfg(feature = "arrow")]
    pub cache: Option<Arc<fraiseql_arrow::cache::QueryCache>>,
    /// Server configuration (optional).
    pub config: Option<Arc<crate::config::HttpServerConfig>>,
    /// Rate limiter for GraphQL validation errors (per IP).
    #[cfg(feature = "auth")]
    pub graphql_rate_limiter: Arc<KeyedRateLimiter>,
    /// Secrets manager (optional, configured via `[fraiseql.secrets]`).
    #[cfg(feature = "secrets")]
    pub secrets_manager: Option<Arc<crate::secrets_manager::SecretsManager>>,
    /// Field encryption service for transparent encrypt/decrypt of marked fields.
    #[cfg(feature = "secrets")]
    pub field_encryption: Option<Arc<crate::encryption::middleware::FieldEncryptionService>>,
    /// Federation circuit breaker manager (optional, enabled via `fraiseql.toml`).
    #[cfg(feature = "federation")]
    pub circuit_breaker:
        Option<Arc<crate::federation::circuit_breaker::FederationCircuitBreakerManager>>,
    /// Federation subgraph latency histogram tracker.
    #[cfg(feature = "federation")]
    pub federation_latency: Arc<fraiseql_core::federation::SubgraphLatencyTracker>,
    /// Federation entity resolution counter metrics.
    #[cfg(feature = "federation")]
    pub federation_entity_metrics: Arc<fraiseql_core::federation::EntityResolutionMetrics>,
    /// Federation query plan cache for plan visualization.
    #[cfg(feature = "federation")]
    pub federation_plan_cache: Option<Arc<fraiseql_core::federation::QueryPlanCache>>,
    /// Error sanitizer — strips internal details before sending responses to clients.
    pub error_sanitizer: Arc<ErrorSanitizer>,
    /// State encryption service (optional, enabled via `[security.state_encryption]`).
    #[cfg(feature = "auth")]
    pub state_encryption: Option<Arc<crate::auth::state_encryption::StateEncryptionService>>,
    /// API key authenticator (optional, enabled via `[security.api_keys]`).
    pub api_key_authenticator: Option<Arc<crate::api_key::ApiKeyAuthenticator>>,
    /// Service-account authenticator (optional, enabled via `[security.service_accounts]`;
    /// ADR-0018).
    pub service_account_authenticator:
        Option<Arc<crate::service_account::ServiceAccountAuthenticator>>,
    /// APQ persistent query store (optional, enabled via compiled schema config).
    pub apq_store: Option<ArcApqStorage>,
    /// Trusted document store (optional, enabled via `[security.trusted_documents]`).
    pub trusted_docs: Option<Arc<crate::trusted_documents::TrustedDocumentStore>>,
    /// APQ metrics tracker.
    pub apq_metrics: Arc<ApqMetrics>,
    /// Request validator (depth/complexity limits, configured from compiled schema).
    pub validator: crate::validation::RequestValidator,
    /// Debug configuration (optional, from `[debug]` in `fraiseql.toml`).
    pub debug_config: Option<fraiseql_core::schema::DebugConfig>,
    /// Maximum byte length for a query string delivered via HTTP GET.
    ///
    /// Defaults to `100_000` (100 `KiB`).  Configurable via
    /// `ServerConfig::max_get_query_bytes`.
    pub max_get_query_bytes: usize,
    /// Introspection policy for the GraphQL request path.
    ///
    /// Derived from `ServerConfig::introspection_enabled` /
    /// `introspection_require_auth` via [`IntrospectionPolicy::from_config`] in
    /// `build_app_state`. Defaults to [`IntrospectionPolicy::Disabled`]
    /// (fail-closed) when no config is wired, matching the server's
    /// introspection-off-by-default posture.
    pub introspection_policy: IntrospectionPolicy,
    /// Connection pool auto-tuner (optional, enabled via `[pool_tuning]` config).
    pub pool_tuner: Option<Arc<crate::pool::PoolSizingAdvisor>>,
    /// Observer runtime handle for health probes (optional, requires `observers` feature).
    #[cfg(feature = "observers")]
    pub observer_runtime: Option<Arc<tokio::sync::RwLock<crate::observers::ObserverRuntime>>>,
    /// Schema file path for reload operations.
    pub schema_path: Option<PathBuf>,
    /// Database adapter reference for constructing new executors on reload.
    pub(crate) reload_adapter: Option<Arc<A>>,
    /// How the booting constructor built its executor, so a reload rebuilds the
    /// *same kind*.
    ///
    /// Relay dispatch requires `A: RelayDatabaseAdapter`, a bound `AppState` does
    /// not carry — so only the constructor that had it can say how to rebuild.
    /// Reload used to call `Executor::new` unconditionally, which dropped relay
    /// dispatch and made every relay query fail validation until the process
    /// restarted (#750).
    pub(crate) reload_rebuilder: Option<ExecutorRebuilder<A>>,
    /// Reload mutex to serialize concurrent reload attempts.
    pub(crate) reload_lock: Arc<tokio::sync::Mutex<()>>,
    /// Whether the adapter-level query result cache is active.
    ///
    /// Set to `true` when `ServerConfig::cache_enabled = true` and the server
    /// was built via `Server::new` or `Server::with_relay_pagination`.
    /// This reflects the adapter-level `CachedDatabaseAdapter` state, NOT the
    /// Arrow flight cache (`AppState::cache`).
    pub adapter_cache_enabled: bool,
    /// Multi-tenant executor registry (optional).
    ///
    /// When `Some`, the server operates in multi-tenant mode: each request's
    /// tenant key selects an executor from this registry. When `None`,
    /// single-tenant mode is in effect and all requests use `self.executor`.
    pub tenant_registry: Option<Arc<TenantExecutorRegistry<A>>>,
    /// Factory for creating tenant executors from schema JSON + pool config.
    ///
    /// Type-erased so that the management API handler does not need
    /// `A: FromPoolConfig` on its generic bounds.
    pub tenant_executor_factory: Option<crate::tenancy::TenantExecutorFactory<A>>,
    /// Domain-to-tenant mapping for Host header-based tenant resolution.
    pub domain_registry: Arc<DomainRegistry>,
    /// Tenant audit log (optional, for lifecycle event recording).
    pub tenant_audit_log: Option<crate::tenancy::audit::AuditLogHandle>,
    /// Usage aggregator — shared with the `MutationAuditLayer` tracing subscriber.
    ///
    /// Always present (never `Option`): when audit logging is disabled the
    /// aggregator simply receives no events and every query returns empty counts.
    pub usage: Arc<UsageAggregator>,
    /// Before-mutation hooks from the functions subsystem (optional).
    ///
    /// When `Some`, every GraphQL mutation is checked against the trigger registry
    /// before execution. The check is a single `HashMap::get` returning `None`
    /// when no hooks are registered — zero overhead for mutations without hooks.
    pub before_mutation_hooks: Option<Arc<crate::subsystems::BeforeMutationHooks>>,

    /// Enrichment-profile identity resolver (#539). `Some` when
    /// `[identity.enrichment].enabled` and an auth DB pool is present; every
    /// authenticated request then resolves its DB identity and fail-closes
    /// before dispatch (read-scoping via the `fraiseql.enriched.*` namespace).
    #[cfg(feature = "auth")]
    pub identity_resolver: Option<Arc<crate::identity::IdentityResolver>>,

    /// Token revocation manager, when `[security.token_revocation]` is configured.
    ///
    /// Reachable from `AppState` so `POST /admin/v1/users/{id}/revoke` can actually
    /// revoke. It used to answer `{"success": true, "message": "All sessions
    /// revoked"}` without touching any store, because the manager lived on `Server`
    /// and the handler had no way to reach it (#749). `None` is now an explicit
    /// `501`, never a fabricated success.
    pub revocation_manager: Option<Arc<crate::token_revocation::TokenRevocationManager>>,

    /// When this state was constructed — i.e. when the server started.
    ///
    /// `GET /admin/v1/health/detailed` reported `uptime_secs` as *seconds since the
    /// Unix epoch*, so a freshly-booted server claimed roughly 1.8 billion seconds
    /// of uptime. A real instant is the only way to answer the question asked.
    pub started_at: std::time::Instant,
}

impl<A: DatabaseAdapter> AppState<A> {
    /// Create new application state.
    #[must_use]
    pub fn new(executor: Arc<Executor<A>>) -> Self {
        Self {
            executor: Arc::new(ArcSwap::from(executor)),
            metrics: Arc::new(MetricsCollector::new()),
            #[cfg(feature = "arrow")]
            cache: None,
            config: None,
            #[cfg(feature = "auth")]
            graphql_rate_limiter: Arc::new(KeyedRateLimiter::new(
                AuthRateLimitConfig::per_ip_standard(),
            )),
            #[cfg(feature = "secrets")]
            secrets_manager: None,
            #[cfg(feature = "secrets")]
            field_encryption: None,
            #[cfg(feature = "federation")]
            circuit_breaker: None,
            #[cfg(feature = "federation")]
            federation_latency: Arc::new(fraiseql_core::federation::SubgraphLatencyTracker::new()),
            #[cfg(feature = "federation")]
            federation_entity_metrics: Arc::new(
                fraiseql_core::federation::EntityResolutionMetrics::new(),
            ),
            #[cfg(feature = "federation")]
            federation_plan_cache: None,
            error_sanitizer: Arc::new(ErrorSanitizer::disabled()),
            #[cfg(feature = "auth")]
            state_encryption: None,
            api_key_authenticator: None,
            service_account_authenticator: None,
            apq_store: None,
            trusted_docs: None,
            apq_metrics: Arc::new(ApqMetrics::default()),
            validator: crate::validation::RequestValidator::new(),
            debug_config: None,
            pool_tuner: None,
            #[cfg(feature = "observers")]
            observer_runtime: None,
            max_get_query_bytes: 100_000,
            introspection_policy: IntrospectionPolicy::Disabled,
            schema_path: None,
            reload_adapter: None,
            reload_rebuilder: None,
            reload_lock: Arc::new(tokio::sync::Mutex::new(())),
            adapter_cache_enabled: false,
            tenant_registry: None,
            tenant_executor_factory: None,
            domain_registry: Arc::new(DomainRegistry::new()),
            tenant_audit_log: None,
            usage: Arc::clone(crate::usage::aggregator::global_aggregator()),
            before_mutation_hooks: None,
            #[cfg(feature = "auth")]
            identity_resolver: None,
            revocation_manager: None,
            started_at: std::time::Instant::now(),
        }
    }

    /// Attach the token revocation manager so the admin revoke endpoint can use it.
    #[must_use]
    pub fn with_revocation_manager(
        mut self,
        manager: Arc<crate::token_revocation::TokenRevocationManager>,
    ) -> Self {
        self.revocation_manager = Some(manager);
        self
    }

    /// Attach the enrichment-profile identity resolver (#539).
    #[cfg(feature = "auth")]
    #[must_use]
    pub fn with_identity_resolver(
        mut self,
        resolver: Arc<crate::identity::IdentityResolver>,
    ) -> Self {
        self.identity_resolver = Some(resolver);
        self
    }

    /// Load the current executor.
    ///
    /// Returns a guard that keeps the executor alive for the duration of the
    /// request. This is wait-free (no lock).
    #[must_use]
    pub fn executor(&self) -> arc_swap::Guard<Arc<Executor<A>>> {
        self.executor.load()
    }

    /// Atomically swap the executor.
    ///
    /// In-flight requests that already called `executor()` continue using
    /// the old executor until their guard is dropped.
    pub fn swap_executor(&self, new_executor: Arc<Executor<A>>) {
        self.executor.store(new_executor);
    }

    /// Returns the executor for the given tenant key.
    ///
    /// In multi-tenant mode, delegates to the `TenantExecutorRegistry`. In
    /// single-tenant mode (no registry), ignores the key and returns the
    /// default executor.
    ///
    /// # Errors
    ///
    /// Returns `FraiseQLError::Authorization` if multi-tenant mode is enabled
    /// and the tenant key is explicit but not registered.
    pub fn executor_for_tenant(
        &self,
        tenant_key: Option<&str>,
    ) -> fraiseql_error::Result<arc_swap::Guard<Arc<Executor<A>>>> {
        match &self.tenant_registry {
            Some(registry) => registry.executor_for(tenant_key),
            None => Ok(self.executor()),
        }
    }

    /// Attach a multi-tenant executor registry.
    #[must_use]
    pub fn with_tenant_registry(mut self, registry: Arc<TenantExecutorRegistry<A>>) -> Self {
        self.tenant_registry = Some(registry);
        self
    }

    /// Get the tenant registry if multi-tenant mode is enabled.
    #[must_use]
    pub const fn tenant_registry(&self) -> Option<&Arc<TenantExecutorRegistry<A>>> {
        self.tenant_registry.as_ref()
    }

    /// Attach a tenant executor factory for the management API.
    #[must_use]
    pub fn with_tenant_executor_factory(
        mut self,
        factory: crate::tenancy::TenantExecutorFactory<A>,
    ) -> Self {
        self.tenant_executor_factory = Some(factory);
        self
    }

    /// Get the tenant executor factory if configured.
    #[must_use]
    pub const fn tenant_executor_factory(
        &self,
    ) -> Option<&crate::tenancy::TenantExecutorFactory<A>> {
        self.tenant_executor_factory.as_ref()
    }

    /// Get the domain registry for Host header-based tenant resolution.
    #[must_use]
    pub const fn domain_registry(&self) -> &Arc<DomainRegistry> {
        &self.domain_registry
    }

    /// Attach a custom domain registry.
    #[must_use]
    pub fn with_domain_registry(mut self, registry: Arc<DomainRegistry>) -> Self {
        self.domain_registry = registry;
        self
    }

    /// Replace the usage aggregator (primarily for testing with an isolated aggregator).
    #[must_use]
    pub fn with_usage(mut self, usage: Arc<UsageAggregator>) -> Self {
        self.usage = usage;
        self
    }

    /// Attach a tenant audit log for lifecycle event recording.
    #[must_use]
    pub fn with_tenant_audit_log(mut self, log: crate::tenancy::audit::AuditLogHandle) -> Self {
        self.tenant_audit_log = Some(log);
        self
    }

    /// Get the tenant audit log if configured.
    #[must_use]
    pub const fn tenant_audit_log(&self) -> Option<&crate::tenancy::audit::AuditLogHandle> {
        self.tenant_audit_log.as_ref()
    }

    /// Attach before-mutation hooks from the functions subsystem.
    ///
    /// When set, every incoming GraphQL mutation is checked against the trigger
    /// registry before execution. The check is a single `HashMap::get` returning
    /// `None` when no hooks exist — zero overhead for mutations without hooks.
    #[must_use]
    pub fn with_functions(mut self, hooks: Arc<crate::subsystems::BeforeMutationHooks>) -> Self {
        self.before_mutation_hooks = Some(hooks);
        self
    }

    /// Configure reload support with a schema file path, database adapter, and
    /// the constructor's executor rebuilder.
    ///
    /// `rebuilder` is how the booting constructor built its executor; a reload
    /// uses the same one so it cannot silently downgrade the runtime's
    /// capabilities (#750). Pass `None` only where no such constructor exists —
    /// a directly-assembled test `AppState` — in which case reload refuses
    /// rather than guessing.
    #[must_use]
    pub fn with_reload_config(
        mut self,
        schema_path: PathBuf,
        adapter: Arc<A>,
        rebuilder: Option<ExecutorRebuilder<A>>,
    ) -> Self {
        self.schema_path = Some(schema_path);
        self.reload_adapter = Some(adapter);
        self.reload_rebuilder = rebuilder;
        self
    }

    /// Reload the compiled schema from a file path.
    ///
    /// Reads the schema file and hands it to the shared `swap_in_schema` seam.
    ///
    /// # Errors
    ///
    /// Returns an error if the file cannot be read, the JSON is invalid, schema
    /// validation fails, a boot-time safety gate refuses the new schema, or the
    /// new schema changes configuration that only a restart can apply. On error,
    /// the current executor is unchanged.
    pub async fn reload_schema(&self, path: &std::path::Path) -> Result<(), String> {
        // Take the lock and check the preconditions before touching the disk: the
        // lock exists to serialize *reloads*, and a second reload must be refused
        // before it starts reading, not after.
        let guard = self.begin_reload()?;
        let json = tokio::fs::read_to_string(path)
            .await
            .map_err(|e| format!("Failed to read schema file {}: {e}", path.display()))?;
        let schema = CompiledSchema::from_json(&json, false)
            .map_err(|e| format!("Invalid schema JSON: {e}"))?;
        self.swap_in_schema(schema, &guard).await
    }

    /// Reload the compiled schema from already-validated JSON bytes.
    ///
    /// This avoids re-reading the schema file from disk after validation,
    /// preventing TOCTOU race conditions where the file could change between
    /// validation and reload.
    ///
    /// # Errors
    ///
    /// Returns an error if the JSON is invalid, schema validation fails, a
    /// boot-time safety gate refuses the new schema, the new schema changes
    /// configuration that only a restart can apply, or a reload is already in
    /// progress. On error, the current executor is unchanged.
    pub async fn reload_schema_from_json(&self, json: &str) -> Result<(), String> {
        let guard = self.begin_reload()?;
        let schema = CompiledSchema::from_json(json, false)
            .map_err(|e| format!("Invalid schema JSON: {e}"))?;
        self.swap_in_schema(schema, &guard).await
    }

    /// Acquire the reload lock and check that reload is configured at all.
    ///
    /// Both preconditions are cheap and both are refusals rather than failures,
    /// so they run before any I/O: a concurrent reload must be told "already in
    /// progress" rather than racing on the file read, and an `AppState` with no
    /// adapter must say so rather than reporting a read error for a reload it
    /// could never have performed.
    fn begin_reload(&self) -> Result<tokio::sync::MutexGuard<'_, ()>, String> {
        let guard = self
            .reload_lock
            .try_lock()
            .map_err(|_| "Reload already in progress".to_string())?;
        if self.reload_adapter.is_none() {
            return Err("Reload not configured: no adapter available".to_string());
        }
        if self.reload_rebuilder.is_none() {
            return Err("Reload not configured: no executor rebuilder available. Reload is only \
                 supported on an AppState built by a Server constructor, which records how \
                 the executor must be rebuilt."
                .to_string());
        }
        Ok(guard)
    }

    /// The single hot-reload seam: validate, gate, rebuild, swap.
    ///
    /// This is a **construction path**, and it must produce the same configured
    /// runtime as boot does. It previously did not: it called `Executor::new`,
    /// which uses `RuntimeConfig::default()`, so a successful reload silently
    /// reverted mutation audit logging, the #421 page-size ceiling, the
    /// change-log toggle and relay dispatch (#750); and it ran none of the
    /// boot-time safety gates, so it could move a running server into a state
    /// boot would have refused (#782).
    ///
    /// # Errors
    ///
    /// Returns the operator-facing message for every refusal: an incompatible
    /// format version, a field marked for at-rest encryption (H12), a
    /// multi-tenant schema that cannot isolate its tenants under caching (#758),
    /// or boot-frozen configuration drift that requires a restart.
    async fn swap_in_schema(
        &self,
        schema: CompiledSchema,
        // Held by the caller for the whole reload — taken in `begin_reload`,
        // before any I/O. Threaded through so this seam cannot be reached without
        // it.
        _guard: &tokio::sync::MutexGuard<'_, ()>,
    ) -> Result<(), String> {
        let (Some(adapter), Some(rebuilder)) =
            (self.reload_adapter.as_ref(), self.reload_rebuilder.as_ref())
        else {
            // `begin_reload` already refused this case; unreachable in practice.
            return Err("Reload not configured".to_string());
        };

        schema
            .validate_format_version()
            .map_err(|msg| format!("Incompatible compiled schema: {msg}"))?;

        let current = self.executor.load();
        if current.schema().content_hash() == schema.content_hash() {
            return Ok(()); // Same schema, no-op
        }

        // The boot-time safety gates, run again. A reload that skips them is a
        // way to reach, at runtime, a configuration the server refused to start
        // in — which for the encryption gate means writing plaintext into a field
        // declared encrypted, and for the tenancy gate means serving one tenant's
        // cached rows to another (#782).
        crate::server::initialization::field_encryption_unsupported_check(&schema)
            .map_err(|e| e.to_string())?;
        crate::server::initialization::tenant_isolation_declaration_check(
            &schema,
            self.adapter_cache_enabled,
        )
        .map_err(|e| e.to_string())?;

        // Refuse rather than half-apply: everything a boot-time subsystem read
        // once cannot be changed by swapping the executor.
        super::reload_gate::check_reloadable(current.schema(), &schema)?;

        // #611: new subscriptions pick up policy changes immediately (layer-1); warn loudly
        // so operators know already-connected streams must reconnect to apply the change.
        warn_on_subscription_policy_reload(current.schema(), &schema);

        // Re-derive the schema-owned runtime settings on top of the *live* config,
        // so programmatically-installed pieces (authorizers, RLS policy, field
        // filter, query validation) survive the swap while the compiled and
        // environment-derived ones are recomputed — exactly what boot does.
        let config = current
            .config()
            .clone()
            .with_compiled_schema(&schema)
            .map_err(|msg| format!("Incompatible compiled schema: {msg}"))?;

        // Notify adapter of schema change (clears query result cache if applicable)
        adapter.on_schema_reload();

        // Rebuild through the constructor's own path, preserving relay dispatch.
        let new_executor = Arc::new(rebuilder(schema, adapter.clone(), config));

        // Atomic swap
        self.executor.store(new_executor);

        // Clear query plan caches (reference old schema)
        #[cfg(feature = "arrow")]
        if let Some(cache) = &self.cache {
            cache.clear();
        }

        info!("Schema executor swapped successfully");

        Ok(())
    }

    /// Create new application state with custom metrics collector.
    #[must_use]
    pub fn with_metrics(executor: Arc<Executor<A>>, metrics: Arc<MetricsCollector>) -> Self {
        Self::new(executor).set_metrics(metrics)
    }

    /// Create new application state with cache.
    #[cfg(feature = "arrow")]
    #[must_use]
    pub fn with_cache(
        executor: Arc<Executor<A>>,
        cache: Arc<fraiseql_arrow::cache::QueryCache>,
    ) -> Self {
        Self::new(executor).set_cache(cache)
    }

    /// Create new application state with cache and config.
    #[cfg(feature = "arrow")]
    #[must_use]
    pub fn with_cache_and_config(
        executor: Arc<Executor<A>>,
        cache: Arc<fraiseql_arrow::cache::QueryCache>,
        config: Arc<crate::config::HttpServerConfig>,
    ) -> Self {
        Self::new(executor).set_cache(cache).set_config(config)
    }

    fn set_metrics(mut self, metrics: Arc<MetricsCollector>) -> Self {
        self.metrics = metrics;
        self
    }

    #[cfg(feature = "arrow")]
    fn set_cache(mut self, cache: Arc<fraiseql_arrow::cache::QueryCache>) -> Self {
        self.cache = Some(cache);
        self
    }

    #[cfg(feature = "arrow")]
    fn set_config(mut self, config: Arc<crate::config::HttpServerConfig>) -> Self {
        self.config = Some(config);
        self
    }

    /// Get query cache if configured.
    #[cfg(feature = "arrow")]
    #[must_use]
    pub const fn cache(&self) -> Option<&Arc<fraiseql_arrow::cache::QueryCache>> {
        self.cache.as_ref()
    }

    /// Get server configuration if configured.
    #[must_use]
    pub const fn server_config(&self) -> Option<&Arc<crate::config::HttpServerConfig>> {
        self.config.as_ref()
    }

    /// Get sanitized configuration for safe API exposure.
    #[must_use]
    pub fn sanitized_config(&self) -> Option<crate::routes::api::types::SanitizedConfig> {
        self.config
            .as_ref()
            .map(|cfg| crate::routes::api::types::SanitizedConfig::from_config(cfg))
    }

    /// Set secrets manager (for credential and secret management).
    #[cfg(feature = "secrets")]
    #[must_use]
    pub fn with_secrets_manager(
        mut self,
        secrets_manager: Arc<crate::secrets_manager::SecretsManager>,
    ) -> Self {
        self.secrets_manager = Some(secrets_manager);
        self
    }

    /// Get secrets manager if configured.
    #[cfg(feature = "secrets")]
    #[must_use]
    pub const fn secrets_manager(&self) -> Option<&Arc<crate::secrets_manager::SecretsManager>> {
        self.secrets_manager.as_ref()
    }

    /// Attach a field encryption service (derived from schema and secrets manager).
    #[cfg(feature = "secrets")]
    #[must_use]
    pub fn with_field_encryption(
        mut self,
        service: Arc<crate::encryption::middleware::FieldEncryptionService>,
    ) -> Self {
        self.field_encryption = Some(service);
        self
    }

    /// Attach a federation circuit breaker manager.
    #[cfg(feature = "federation")]
    #[must_use]
    pub fn with_circuit_breaker(
        mut self,
        circuit_breaker: Arc<crate::federation::circuit_breaker::FederationCircuitBreakerManager>,
    ) -> Self {
        self.circuit_breaker = Some(circuit_breaker);
        self
    }

    /// Attach an error sanitizer (loaded from `compiled.security.error_sanitization`).
    #[must_use]
    pub fn with_error_sanitizer(mut self, sanitizer: Arc<ErrorSanitizer>) -> Self {
        self.error_sanitizer = sanitizer;
        self
    }

    /// Attach a state encryption service (loaded from `compiled.security.state_encryption`).
    #[cfg(feature = "auth")]
    #[must_use]
    pub fn with_state_encryption(
        mut self,
        svc: Arc<crate::auth::state_encryption::StateEncryptionService>,
    ) -> Self {
        self.state_encryption = Some(svc);
        self
    }

    /// Attach an API key authenticator (loaded from `compiled.security.api_keys`).
    #[must_use]
    pub fn with_api_key_authenticator(
        mut self,
        authenticator: Arc<crate::api_key::ApiKeyAuthenticator>,
    ) -> Self {
        self.api_key_authenticator = Some(authenticator);
        self
    }

    /// Attach a service-account authenticator (loaded from
    /// `compiled.security.service_accounts`; ADR-0018).
    #[must_use]
    pub fn with_service_account_authenticator(
        mut self,
        authenticator: Arc<crate::service_account::ServiceAccountAuthenticator>,
    ) -> Self {
        self.service_account_authenticator = Some(authenticator);
        self
    }

    /// Attach an APQ store for Automatic Persisted Queries.
    #[must_use]
    pub fn with_apq_store(mut self, store: ArcApqStorage) -> Self {
        self.apq_store = Some(store);
        self
    }

    /// Attach a trusted document store for query allowlist enforcement.
    #[must_use]
    pub fn with_trusted_docs(
        mut self,
        store: Arc<crate::trusted_documents::TrustedDocumentStore>,
    ) -> Self {
        self.trusted_docs = Some(store);
        self
    }

    /// Set the request validator (query depth/complexity limits).
    #[must_use]
    pub const fn with_validator(mut self, validator: crate::validation::RequestValidator) -> Self {
        self.validator = validator;
        self
    }

    /// Set the introspection policy for the GraphQL request path.
    ///
    /// Wired in `build_app_state` from the server config via
    /// [`IntrospectionPolicy::from_config`]; the default is
    /// [`IntrospectionPolicy::Disabled`] (fail-closed).
    #[must_use]
    pub const fn with_introspection_policy(mut self, policy: IntrospectionPolicy) -> Self {
        self.introspection_policy = policy;
        self
    }

    /// Attach an adaptive connection pool auto-tuner.
    #[must_use]
    pub fn with_pool_tuner(mut self, tuner: Arc<crate::pool::PoolSizingAdvisor>) -> Self {
        self.pool_tuner = Some(tuner);
        self
    }

    /// Set whether the adapter-level cache is active.
    ///
    /// Called from `build_router` to thread the cache state through to admin handlers.
    #[must_use]
    pub const fn with_adapter_cache_enabled(mut self, enabled: bool) -> Self {
        self.adapter_cache_enabled = enabled;
        self
    }

    /// Attach observer runtime for health probes.
    #[cfg(feature = "observers")]
    #[must_use]
    pub fn with_observer_runtime(
        mut self,
        runtime: Arc<tokio::sync::RwLock<crate::observers::ObserverRuntime>>,
    ) -> Self {
        self.observer_runtime = Some(runtime);
        self
    }

    /// Sanitize a batch of errors before sending them to the client.
    #[must_use]
    pub fn sanitize_errors(&self, errors: Vec<GraphQLError>) -> Vec<GraphQLError> {
        self.error_sanitizer.sanitize_all(errors)
    }
}

/// Warn (loudly) when a schema hot-reload changes the subscription row-visibility
/// policies (#596/#611).
///
/// As of #611 layer-1, **new** subscriptions read the live policies (the `/ws` handler
/// resolves them from the reload-aware executor `ArcSwap`), so a reloaded change reaches
/// them on the next subscribe. **Already-connected** subscriptions keep their subscribe-time
/// boundary until they reconnect — this warning makes that window operator-visible so a
/// reconnect can be forced. Mid-stream re-derivation of live subscriptions is layer-2
/// (deferred; #611).
fn warn_on_subscription_policy_reload(
    old_schema: &CompiledSchema,
    new_schema: &CompiledSchema,
) -> bool {
    let old_policies = crate::routes::subscriptions::build_subscription_policies(old_schema);
    let new_policies = crate::routes::subscriptions::build_subscription_policies(new_schema);
    let changed = old_policies != new_policies;
    if changed {
        warn!(
            old = old_policies.len(),
            new = new_policies.len(),
            "SECURITY: schema reload changes subscription row-visibility policies. NEW \
             subscriptions pick up the change immediately (#611 layer-1); ALREADY-CONNECTED \
             subscriptions keep their subscribe-time boundary until they reconnect. Force a \
             reconnect (or restart) to apply the change to existing streams."
        );
    }
    changed
}

#[cfg(test)]
mod reload_policy_warn_tests {
    use fraiseql_core::schema::{
        CompiledSchema, SubscriptionDefinition, SubscriptionPolicy, TypeDefinition,
    };

    use super::warn_on_subscription_policy_reload;

    fn schema(policy: Option<SubscriptionPolicy>) -> CompiledSchema {
        let mut order = TypeDefinition::new("Order", "v_order");
        if let Some(p) = policy {
            order = order.with_subscription_policy(p);
        }
        CompiledSchema {
            types: vec![order],
            subscriptions: vec![SubscriptionDefinition::new("orderUpdated", "Order")],
            ..Default::default()
        }
    }

    fn policy() -> SubscriptionPolicy {
        SubscriptionPolicy {
            owner_path:     "$.owner_id".to_string(),
            identity_field: "user_id".to_string(),
            bypass_roles:   vec![],
        }
    }

    #[test]
    fn adding_a_policy_on_reload_is_flagged() {
        // #611: a policy added by a hot-reload is a change → operator-visible.
        assert!(warn_on_subscription_policy_reload(&schema(None), &schema(Some(policy()))));
    }

    #[test]
    fn an_unchanged_policy_set_is_not_flagged() {
        assert!(!warn_on_subscription_policy_reload(
            &schema(Some(policy())),
            &schema(Some(policy()))
        ));
        assert!(!warn_on_subscription_policy_reload(&schema(None), &schema(None)));
    }
}
