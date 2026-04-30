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
    metrics_server::MetricsCollector,
};

/// Server state containing executor and configuration.
#[derive(Clone)]
pub struct AppState<A: DatabaseAdapter> {
    /// Query executor (atomically swappable for schema hot-reload).
    pub executor:                Arc<ArcSwap<Executor<A>>>,
    /// Metrics collector.
    pub metrics:                 Arc<MetricsCollector>,
    /// Query result cache (optional).
    #[cfg(feature = "arrow")]
    pub cache:                   Option<Arc<fraiseql_arrow::cache::QueryCache>>,
    /// Server configuration (optional).
    pub config:                  Option<Arc<crate::config::HttpServerConfig>>,
    /// Rate limiter for GraphQL validation errors (per IP).
    #[cfg(feature = "auth")]
    pub graphql_rate_limiter:    Arc<KeyedRateLimiter>,
    /// Secrets manager (optional, configured via `[fraiseql.secrets]`).
    #[cfg(feature = "secrets")]
    pub secrets_manager:         Option<Arc<crate::secrets_manager::SecretsManager>>,
    /// Field encryption service for transparent encrypt/decrypt of marked fields.
    #[cfg(feature = "secrets")]
    pub field_encryption:        Option<Arc<crate::encryption::middleware::FieldEncryptionService>>,
    /// Federation circuit breaker manager (optional, enabled via `fraiseql.toml`).
    #[cfg(feature = "federation")]
    pub circuit_breaker:
        Option<Arc<crate::federation::circuit_breaker::FederationCircuitBreakerManager>>,
    /// Error sanitizer — strips internal details before sending responses to clients.
    pub error_sanitizer:         Arc<ErrorSanitizer>,
    /// State encryption service (optional, enabled via `[security.state_encryption]`).
    #[cfg(feature = "auth")]
    pub state_encryption:        Option<Arc<crate::auth::state_encryption::StateEncryptionService>>,
    /// API key authenticator (optional, enabled via `[security.api_keys]`).
    pub api_key_authenticator:   Option<Arc<crate::api_key::ApiKeyAuthenticator>>,
    /// APQ persistent query store (optional, enabled via compiled schema config).
    pub apq_store:               Option<ArcApqStorage>,
    /// Trusted document store (optional, enabled via `[security.trusted_documents]`).
    pub trusted_docs:            Option<Arc<crate::trusted_documents::TrustedDocumentStore>>,
    /// APQ metrics tracker.
    pub apq_metrics:             Arc<ApqMetrics>,
    /// Request validator (depth/complexity limits, configured from compiled schema).
    pub validator:               crate::validation::RequestValidator,
    /// Debug configuration (optional, from `[debug]` in `fraiseql.toml`).
    pub debug_config:            Option<fraiseql_core::schema::DebugConfig>,
    /// Maximum byte length for a query string delivered via HTTP GET.
    ///
    /// Defaults to `100_000` (100 `KiB`).  Configurable via
    /// `ServerConfig::max_get_query_bytes`.
    pub max_get_query_bytes:     usize,
    /// Connection pool auto-tuner (optional, enabled via `[pool_tuning]` config).
    pub pool_tuner:              Option<Arc<crate::pool::PoolSizingAdvisor>>,
    /// Observer runtime handle for health probes (optional, requires `observers` feature).
    #[cfg(feature = "observers")]
    pub observer_runtime: Option<Arc<tokio::sync::RwLock<crate::observers::ObserverRuntime>>>,
    /// Schema file path for reload operations.
    pub schema_path:             Option<PathBuf>,
    /// Database adapter reference for constructing new executors on reload.
    pub(crate) reload_adapter:   Option<Arc<A>>,
    /// Reload mutex to serialize concurrent reload attempts.
    pub(crate) reload_lock:      Arc<tokio::sync::Mutex<()>>,
    /// Whether the adapter-level query result cache is active.
    ///
    /// Set to `true` when `ServerConfig::cache_enabled = true` and the server
    /// was built via `Server::new` or `Server::with_relay_pagination`.
    /// This reflects the adapter-level `CachedDatabaseAdapter` state, NOT the
    /// Arrow flight cache (`AppState::cache`).
    pub adapter_cache_enabled:   bool,
    /// Multi-tenant executor registry (optional).
    ///
    /// When `Some`, the server operates in multi-tenant mode: each request's
    /// tenant key selects an executor from this registry. When `None`,
    /// single-tenant mode is in effect and all requests use `self.executor`.
    pub tenant_registry:         Option<Arc<TenantExecutorRegistry<A>>>,
    /// Factory for creating tenant executors from schema JSON + pool config.
    ///
    /// Type-erased so that the management API handler does not need
    /// `A: FromPoolConfig` on its generic bounds.
    pub tenant_executor_factory: Option<crate::tenancy::TenantExecutorFactory<A>>,
    /// Domain-to-tenant mapping for Host header-based tenant resolution.
    pub domain_registry:         Arc<DomainRegistry>,
    /// Tenant audit log (optional, for lifecycle event recording).
    pub tenant_audit_log:        Option<crate::tenancy::audit::AuditLogHandle>,
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
        }
    }

    /// Load the current executor.
    ///
    /// Returns a guard that keeps the executor alive for the duration of the
    /// request. This is wait-free (no lock).
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
        let schema =
            CompiledSchema::from_json(&json).map_err(|e| format!("Invalid schema JSON: {e}"))?;

        // 3. Validate format version
        schema
            .validate_format_version()
            .map_err(|msg| format!("Incompatible compiled schema: {msg}"))?;

        // 4. Check if schema actually changed
        let current = self.executor.load();
        if current.schema().content_hash() == schema.content_hash() {
            return Ok(()); // Same schema, no-op
        }

        // TODO(#184): hot-reload does not re-wrap the adapter in a new CachedDatabaseAdapter.
        // Per-view TTL overrides from the new schema will not be applied until a full restart.
        // 5. Construct new executor (reuses same adapter/connection pool)
        let new_executor = Arc::new(Executor::new(schema, adapter.clone()));

        // 6. Atomic swap
        self.executor.store(new_executor);

        // 7. Clear caches (query plans reference old schema)
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
    pub const fn cache(&self) -> Option<&Arc<fraiseql_arrow::cache::QueryCache>> {
        self.cache.as_ref()
    }

    /// Get server configuration if configured.
    pub const fn server_config(&self) -> Option<&Arc<crate::config::HttpServerConfig>> {
        self.config.as_ref()
    }

    /// Get sanitized configuration for safe API exposure.
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
    pub fn sanitize_errors(&self, errors: Vec<GraphQLError>) -> Vec<GraphQLError> {
        self.error_sanitizer.sanitize_all(errors)
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)] // Reason: test code, panics acceptable
    #![allow(clippy::missing_panics_doc)] // Reason: test helpers
    #![allow(clippy::missing_errors_doc)] // Reason: test helpers
    #![allow(missing_docs)] // Reason: test code

    use std::sync::Arc;

    use async_trait::async_trait;
    use fraiseql_core::{
        db::{
            WhereClause,
            traits::DatabaseAdapter,
            types::{DatabaseType, JsonbValue, PoolMetrics},
        },
        error::Result as FraiseQLResult,
        runtime::Executor,
        schema::CompiledSchema,
    };

    use super::*;

    /// Minimal no-op database adapter for unit tests.
    #[derive(Debug, Clone)]
    struct StubAdapter;

    // Reason: async_trait required by DatabaseAdapter trait definition
    #[async_trait]
    impl DatabaseAdapter for StubAdapter {
        async fn execute_where_query(
            &self,
            _view: &str,
            _where_clause: Option<&WhereClause>,
            _limit: Option<u32>,
            _offset: Option<u32>,
            _order_by: Option<&[fraiseql_core::db::types::OrderByClause]>,
        ) -> FraiseQLResult<Vec<JsonbValue>> {
            Ok(vec![])
        }

        async fn execute_with_projection(
            &self,
            _view: &str,
            _projection: Option<&fraiseql_core::schema::SqlProjectionHint>,
            _where_clause: Option<&WhereClause>,
            _limit: Option<u32>,
            _offset: Option<u32>,
            _order_by: Option<&[fraiseql_core::db::types::OrderByClause]>,
        ) -> FraiseQLResult<Vec<JsonbValue>> {
            Ok(vec![])
        }

        fn database_type(&self) -> DatabaseType {
            DatabaseType::SQLite
        }

        async fn health_check(&self) -> FraiseQLResult<()> {
            Ok(())
        }

        fn pool_metrics(&self) -> PoolMetrics {
            PoolMetrics::default()
        }

        async fn execute_raw_query(
            &self,
            _sql: &str,
        ) -> FraiseQLResult<Vec<std::collections::HashMap<String, serde_json::Value>>> {
            Ok(vec![])
        }

        async fn execute_parameterized_aggregate(
            &self,
            _sql: &str,
            _params: &[serde_json::Value],
        ) -> FraiseQLResult<Vec<std::collections::HashMap<String, serde_json::Value>>> {
            Ok(vec![])
        }
    }

    fn make_state() -> AppState<StubAdapter> {
        let schema = CompiledSchema::default();
        let executor = Arc::new(Executor::new(schema, Arc::new(StubAdapter)));
        AppState::new(executor)
    }

    #[test]
    fn test_arcswap_executor_load() {
        let state = make_state();
        let guard = state.executor();
        assert_eq!(guard.schema().types.len(), 0);
    }

    #[test]
    fn test_arcswap_executor_swap() {
        let state = make_state();
        let hash_before = state.executor().schema().content_hash();

        // Create a schema with a different content hash by adding a query
        let mut new_schema = CompiledSchema::default();
        new_schema
            .queries
            .push(fraiseql_core::schema::QueryDefinition::new("users", "User"));
        let new_executor = Arc::new(Executor::new(new_schema, Arc::new(StubAdapter)));

        state.swap_executor(new_executor);

        let guard = state.executor();
        assert_ne!(guard.schema().content_hash(), hash_before);
        assert_eq!(guard.schema().queries.len(), 1);
    }

    #[tokio::test]
    async fn test_reload_schema_no_adapter_returns_error() {
        let state = make_state();
        let result = state.reload_schema(std::path::Path::new("/nonexistent")).await;
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("no adapter available"));
    }

    #[tokio::test]
    async fn test_reload_schema_nonexistent_file_returns_error() {
        let state = make_state()
            .with_reload_config("/nonexistent/schema.json".into(), Arc::new(StubAdapter));
        let result = state.reload_schema(std::path::Path::new("/nonexistent/schema.json")).await;
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("Failed to read schema file"));
    }

    #[tokio::test]
    async fn test_reload_same_hash_is_noop() {
        let schema = CompiledSchema::default();
        let hash_before = schema.content_hash();
        let adapter = Arc::new(StubAdapter);
        let executor = Arc::new(Executor::new(schema, adapter.clone()));
        let state = AppState::new(executor).with_reload_config("/tmp/test.json".into(), adapter);

        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("schema.json");
        let schema_json = serde_json::to_string(&CompiledSchema::default()).unwrap();
        std::fs::write(&path, &schema_json).unwrap();

        let result = state.reload_schema(&path).await;
        assert!(result.is_ok());
        assert_eq!(state.executor().schema().content_hash(), hash_before);
    }

    #[tokio::test]
    async fn test_concurrent_reload_serialized() {
        let adapter = Arc::new(StubAdapter);
        let executor = Arc::new(Executor::new(CompiledSchema::default(), adapter.clone()));
        let state = AppState::new(executor).with_reload_config("/tmp/test.json".into(), adapter);

        // Manually acquire the reload lock
        let _guard = state.reload_lock.lock().await;

        // A second reload should fail immediately with "Reload already in progress"
        let result = state.reload_schema(std::path::Path::new("/tmp/test.json")).await;
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("already in progress"));
    }

    // ── Multi-tenant dispatch tests ──────────────────────────────────────

    #[test]
    fn test_single_tenant_executor_for_tenant_ignores_key() {
        let state = make_state();
        // Single-tenant mode: no registry, any key returns default executor
        let exec = state.executor_for_tenant(None).unwrap();
        assert_eq!(exec.schema().queries.len(), 0);
        let exec2 = state.executor_for_tenant(Some("anything")).unwrap();
        assert_eq!(exec2.schema().queries.len(), 0);
    }

    #[test]
    fn test_multi_tenant_dispatch_to_tenant() {
        let state = make_state();
        let registry = super::TenantExecutorRegistry::new(state.executor.clone());
        let mut tenant_schema = CompiledSchema::default();
        tenant_schema
            .queries
            .push(fraiseql_core::schema::QueryDefinition::new("users", "User"));
        let tenant_exec = Arc::new(Executor::new(tenant_schema, Arc::new(StubAdapter)));
        registry.upsert("tenant-abc", tenant_exec);

        let state = state.with_tenant_registry(Arc::new(registry));

        // No key → default (0 queries)
        let exec = state.executor_for_tenant(None).unwrap();
        assert_eq!(exec.schema().queries.len(), 0);

        // tenant-abc → tenant executor (1 query)
        let exec = state.executor_for_tenant(Some("tenant-abc")).unwrap();
        assert_eq!(exec.schema().queries.len(), 1);
    }

    #[test]
    fn test_multi_tenant_rejects_unknown_key() {
        let state = make_state();
        let registry = super::TenantExecutorRegistry::new(state.executor.clone());
        let state = state.with_tenant_registry(Arc::new(registry));

        let result = state.executor_for_tenant(Some("unknown"));
        assert!(result.is_err());
    }

    #[test]
    fn test_tenant_registry_accessor() {
        let state = make_state();
        assert!(state.tenant_registry().is_none());

        let registry = Arc::new(super::TenantExecutorRegistry::new(state.executor.clone()));
        let state = state.with_tenant_registry(registry);
        assert!(state.tenant_registry().is_some());
    }
}
