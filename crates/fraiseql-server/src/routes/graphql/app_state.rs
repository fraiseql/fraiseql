//! `AppState` — server state passed to all GraphQL route handlers.

use std::{path::PathBuf, sync::Arc};

use arc_swap::ArcSwap;
use fraiseql_core::{
    apq::{ApqMetrics, ArcApqStorage},
    db::traits::DatabaseAdapter,
    runtime::Executor,
    schema::CompiledSchema,
};
use tracing::info;

use super::{tenant_key::DomainRegistry, tenant_registry::TenantExecutorRegistry};
#[cfg(feature = "auth")]
use crate::auth::rate_limiting::{AuthRateLimitConfig, KeyedRateLimiter};
use crate::{
    config::error_sanitization::ErrorSanitizer, error::GraphQLError,
    metrics_server::MetricsCollector, usage::aggregator::UsageAggregator,
};

/// Server state containing executor and configuration.
#[derive(Clone)]
pub struct AppState<A: DatabaseAdapter> {
    /// Query executor (atomically swappable for schema hot-reload).
    pub executor:                  Arc<ArcSwap<Executor<A>>>,
    /// Metrics collector.
    pub metrics:                   Arc<MetricsCollector>,
    /// Query result cache (optional).
    #[cfg(feature = "arrow")]
    pub cache:                     Option<Arc<fraiseql_arrow::cache::QueryCache>>,
    /// Server configuration (optional).
    pub config:                    Option<Arc<crate::config::HttpServerConfig>>,
    /// Rate limiter for GraphQL validation errors (per IP).
    #[cfg(feature = "auth")]
    pub graphql_rate_limiter:      Arc<KeyedRateLimiter>,
    /// Secrets manager (optional, configured via `[fraiseql.secrets]`).
    #[cfg(feature = "secrets")]
    pub secrets_manager:           Option<Arc<crate::secrets_manager::SecretsManager>>,
    /// Field encryption service for transparent encrypt/decrypt of marked fields.
    #[cfg(feature = "secrets")]
    pub field_encryption: Option<Arc<crate::encryption::middleware::FieldEncryptionService>>,
    /// Federation circuit breaker manager (optional, enabled via `fraiseql.toml`).
    #[cfg(feature = "federation")]
    pub circuit_breaker:
        Option<Arc<crate::federation::circuit_breaker::FederationCircuitBreakerManager>>,
    /// Federation subgraph latency histogram tracker.
    #[cfg(feature = "federation")]
    pub federation_latency:        Arc<fraiseql_core::federation::SubgraphLatencyTracker>,
    /// Federation entity resolution counter metrics.
    #[cfg(feature = "federation")]
    pub federation_entity_metrics: Arc<fraiseql_core::federation::EntityResolutionMetrics>,
    /// Federation query plan cache for plan visualization.
    #[cfg(feature = "federation")]
    pub federation_plan_cache:     Option<Arc<fraiseql_core::federation::QueryPlanCache>>,
    /// Error sanitizer — strips internal details before sending responses to clients.
    pub error_sanitizer:           Arc<ErrorSanitizer>,
    /// State encryption service (optional, enabled via `[security.state_encryption]`).
    #[cfg(feature = "auth")]
    pub state_encryption: Option<Arc<crate::auth::state_encryption::StateEncryptionService>>,
    /// API key authenticator (optional, enabled via `[security.api_keys]`).
    pub api_key_authenticator:     Option<Arc<crate::api_key::ApiKeyAuthenticator>>,
    /// APQ persistent query store (optional, enabled via compiled schema config).
    pub apq_store:                 Option<ArcApqStorage>,
    /// Trusted document store (optional, enabled via `[security.trusted_documents]`).
    pub trusted_docs:              Option<Arc<crate::trusted_documents::TrustedDocumentStore>>,
    /// APQ metrics tracker.
    pub apq_metrics:               Arc<ApqMetrics>,
    /// Request validator (depth/complexity limits, configured from compiled schema).
    pub validator:                 crate::validation::RequestValidator,
    /// Debug configuration (optional, from `[debug]` in `fraiseql.toml`).
    pub debug_config:              Option<fraiseql_core::schema::DebugConfig>,
    /// Maximum byte length for a query string delivered via HTTP GET.
    ///
    /// Defaults to `100_000` (100 `KiB`).  Configurable via
    /// `ServerConfig::max_get_query_bytes`.
    pub max_get_query_bytes:       usize,
    /// Connection pool auto-tuner (optional, enabled via `[pool_tuning]` config).
    pub pool_tuner:                Option<Arc<crate::pool::PoolSizingAdvisor>>,
    /// Observer runtime handle for health probes (optional, requires `observers` feature).
    #[cfg(feature = "observers")]
    pub observer_runtime: Option<Arc<tokio::sync::RwLock<crate::observers::ObserverRuntime>>>,
    /// Schema file path for reload operations.
    pub schema_path:               Option<PathBuf>,
    /// Database adapter reference for constructing new executors on reload.
    pub(crate) reload_adapter:     Option<Arc<A>>,
    /// Reload mutex to serialize concurrent reload attempts.
    pub(crate) reload_lock:        Arc<tokio::sync::Mutex<()>>,
    /// Whether the adapter-level query result cache is active.
    ///
    /// Set to `true` when `ServerConfig::cache_enabled = true` and the server
    /// was built via `Server::new` or `Server::with_relay_pagination`.
    /// This reflects the adapter-level `CachedDatabaseAdapter` state, NOT the
    /// Arrow flight cache (`AppState::cache`).
    pub adapter_cache_enabled:     bool,
    /// Multi-tenant executor registry (optional).
    ///
    /// When `Some`, the server operates in multi-tenant mode: each request's
    /// tenant key selects an executor from this registry. When `None`,
    /// single-tenant mode is in effect and all requests use `self.executor`.
    pub tenant_registry:           Option<Arc<TenantExecutorRegistry<A>>>,
    /// Factory for creating tenant executors from schema JSON + pool config.
    ///
    /// Type-erased so that the management API handler does not need
    /// `A: FromPoolConfig` on its generic bounds.
    pub tenant_executor_factory:   Option<crate::tenancy::TenantExecutorFactory<A>>,
    /// Domain-to-tenant mapping for Host header-based tenant resolution.
    pub domain_registry:           Arc<DomainRegistry>,
    /// Tenant audit log (optional, for lifecycle event recording).
    pub tenant_audit_log:          Option<crate::tenancy::audit::AuditLogHandle>,
    /// Usage aggregator — shared with the `MutationAuditLayer` tracing subscriber.
    ///
    /// Always present (never `Option`): when audit logging is disabled the
    /// aggregator simply receives no events and every query returns empty counts.
    pub usage:                     Arc<UsageAggregator>,
    /// Before-mutation hooks from the functions subsystem (optional).
    ///
    /// When `Some`, every GraphQL mutation is checked against the trigger registry
    /// before execution. The check is a single `HashMap::get` returning `None`
    /// when no hooks are registered — zero overhead for mutations without hooks.
    pub before_mutation_hooks:     Option<Arc<crate::subsystems::BeforeMutationHooks>>,

    /// Realtime broadcast observer (optional, requires realtime subsystem).
    ///
    /// When `Some`, mutation completions are forwarded to the realtime delivery
    /// pipeline. The observer uses a bounded mpsc channel — events are dropped
    /// (not buffered) when the delivery pipeline is under backpressure, so
    /// mutation response latency is never affected.
    pub realtime_observer: Option<Arc<crate::realtime::observer::RealtimeBroadcastObserver>>,
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
            apq_store: None,
            trusted_docs: None,
            apq_metrics: Arc::new(ApqMetrics::default()),
            validator: crate::validation::RequestValidator::new(),
            debug_config: None,
            pool_tuner: None,
            #[cfg(feature = "observers")]
            observer_runtime: None,
            max_get_query_bytes: 100_000,
            schema_path: None,
            reload_adapter: None,
            reload_lock: Arc::new(tokio::sync::Mutex::new(())),
            adapter_cache_enabled: false,
            tenant_registry: None,
            tenant_executor_factory: None,
            domain_registry: Arc::new(DomainRegistry::new()),
            tenant_audit_log: None,
            usage: Arc::clone(crate::usage::aggregator::global_aggregator()),
            before_mutation_hooks: None,
            realtime_observer: None,
        }
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

    /// Attach a realtime broadcast observer for mutation event forwarding.
    ///
    /// When set, mutation completions are forwarded to the realtime delivery
    /// pipeline via a bounded mpsc channel.
    #[must_use]
    pub fn with_realtime_observer(
        mut self,
        observer: Arc<crate::realtime::observer::RealtimeBroadcastObserver>,
    ) -> Self {
        self.realtime_observer = Some(observer);
        self
    }

    /// Configure reload support with a schema file path and database adapter.
    #[must_use]
    pub fn with_reload_config(mut self, schema_path: PathBuf, adapter: Arc<A>) -> Self {
        self.schema_path = Some(schema_path);
        self.reload_adapter = Some(adapter);
        self
    }

    /// Reload the compiled schema from a file path.
    ///
    /// Reads the schema file, validates it, constructs a new `Executor<A>`,
    /// and atomically swaps it into the shared state. In-flight requests
    /// continue using the previous executor until their handler returns.
    ///
    /// When the adapter supports cache configuration (e.g. `CachedDatabaseAdapter`),
    /// per-view TTL overrides from the new schema are applied immediately and the
    /// query cache is cleared to prevent stale entries.
    ///
    /// # Errors
    ///
    /// Returns an error if the file cannot be read, the JSON is invalid, or
    /// schema validation fails. On error, the current executor is unchanged.
    pub async fn reload_schema(&self, path: &std::path::Path) -> Result<(), String> {
        // Serialize concurrent reloads
        let _guard = self
            .reload_lock
            .try_lock()
            .map_err(|_| "Reload already in progress".to_string())?;

        let adapter = self
            .reload_adapter
            .as_ref()
            .ok_or_else(|| "Reload not configured: no adapter available".to_string())?;

        // 1. Read schema file
        let json = tokio::fs::read_to_string(path)
            .await
            .map_err(|e| format!("Failed to read schema file {}: {e}", path.display()))?;

        // 2. Parse and validate
        let schema = CompiledSchema::from_json(&json, false)
            .map_err(|e| format!("Invalid schema JSON: {e}"))?;

        // 3. Validate format version
        schema
            .validate_format_version()
            .map_err(|msg| format!("Incompatible compiled schema: {msg}"))?;

        // 4. Check if schema actually changed
        let current = self.executor.load();
        if current.schema().content_hash() == schema.content_hash() {
            return Ok(()); // Same schema, no-op
        }

        // 5. Notify adapter of schema change (clears query result cache if applicable)
        adapter.on_schema_reload();

        // 6. Construct new executor (reuses same adapter/connection pool)
        let new_executor = Arc::new(Executor::new(schema, adapter.clone()));

        // 7. Atomic swap
        self.executor.store(new_executor);

        // 8. Clear query plan caches (reference old schema)
        #[cfg(feature = "arrow")]
        if let Some(cache) = &self.cache {
            cache.clear();
        }

        info!("Schema executor swapped successfully");

        Ok(())
    }

    /// Reload the compiled schema from already-validated JSON bytes.
    ///
    /// This avoids re-reading the schema file from disk after validation,
    /// preventing TOCTOU race conditions where the file could change between
    /// validation and reload.
    ///
    /// # Errors
    ///
    /// Returns an error if the JSON is invalid, schema validation fails, or
    /// a reload is already in progress.  On error, the current executor is
    /// unchanged.
    pub async fn reload_schema_from_json(&self, json: &str) -> Result<(), String> {
        // Serialize concurrent reloads
        let _guard = self
            .reload_lock
            .try_lock()
            .map_err(|_| "Reload already in progress".to_string())?;

        let adapter = self
            .reload_adapter
            .as_ref()
            .ok_or_else(|| "Reload not configured: no adapter available".to_string())?;

        // 1. Parse and validate
        let schema = CompiledSchema::from_json(json, false)
            .map_err(|e| format!("Invalid schema JSON: {e}"))?;

        // 2. Validate format version
        schema
            .validate_format_version()
            .map_err(|msg| format!("Incompatible compiled schema: {msg}"))?;

        // 3. Check if schema actually changed
        let current = self.executor.load();
        if current.schema().content_hash() == schema.content_hash() {
            return Ok(()); // Same schema, no-op
        }

        // 4. Notify adapter of schema change (clears query result cache if applicable)
        adapter.on_schema_reload();

        // 5. Construct new executor (reuses same adapter/connection pool)
        let new_executor = Arc::new(Executor::new(schema, adapter.clone()));

        // 6. Atomic swap
        self.executor.store(new_executor);

        // 7. Clear query plan caches (reference old schema)
        #[cfg(feature = "arrow")]
        if let Some(cache) = &self.cache {
            cache.clear();
        }

        info!("Schema executor swapped successfully (from validated JSON)");

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
