//! AppState — server state passed to all GraphQL route handlers.

use std::sync::Arc;

use fraiseql_core::{
    apq::{ApqMetrics, ArcApqStorage},
    db::traits::DatabaseAdapter,
    runtime::Executor,
};

#[cfg(feature = "auth")]
use crate::auth::rate_limiting::{AuthRateLimitConfig, KeyedRateLimiter};
use crate::{
    config::error_sanitization::ErrorSanitizer, error::GraphQLError,
    metrics_server::MetricsCollector,
};

/// Server state containing executor and configuration.
#[derive(Clone)]
pub struct AppState<A: DatabaseAdapter> {
    /// Query executor.
    pub executor:              Arc<Executor<A>>,
    /// Metrics collector.
    pub metrics:               Arc<MetricsCollector>,
    /// Query result cache (optional).
    #[cfg(feature = "arrow")]
    pub cache:                 Option<Arc<fraiseql_arrow::cache::QueryCache>>,
    /// Server configuration (optional).
    pub config:                Option<Arc<crate::config::HttpServerConfig>>,
    /// Rate limiter for GraphQL validation errors (per IP).
    #[cfg(feature = "auth")]
    pub graphql_rate_limiter:  Arc<KeyedRateLimiter>,
    /// Secrets manager (optional, configured via `[fraiseql.secrets]`).
    #[cfg(feature = "secrets")]
    pub secrets_manager:       Option<Arc<crate::secrets_manager::SecretsManager>>,
    /// Field encryption service for transparent encrypt/decrypt of marked fields.
    #[cfg(feature = "secrets")]
    pub field_encryption:      Option<Arc<crate::encryption::middleware::FieldEncryptionService>>,
    /// Federation circuit breaker manager (optional, enabled via `fraiseql.toml`).
    pub circuit_breaker:
        Option<Arc<crate::federation::circuit_breaker::FederationCircuitBreakerManager>>,
    /// Error sanitizer — strips internal details before sending responses to clients.
    pub error_sanitizer:       Arc<ErrorSanitizer>,
    /// State encryption service (optional, enabled via `[security.state_encryption]`).
    #[cfg(feature = "auth")]
    pub state_encryption:      Option<Arc<crate::auth::state_encryption::StateEncryptionService>>,
    /// API key authenticator (optional, enabled via `[security.api_keys]`).
    pub api_key_authenticator: Option<Arc<crate::api_key::ApiKeyAuthenticator>>,
    /// APQ persistent query store (optional, enabled via compiled schema config).
    pub apq_store:             Option<ArcApqStorage>,
    /// Trusted document store (optional, enabled via `[security.trusted_documents]`).
    pub trusted_docs:          Option<Arc<crate::trusted_documents::TrustedDocumentStore>>,
    /// APQ metrics tracker.
    pub apq_metrics:           Arc<ApqMetrics>,
    /// Request validator (depth/complexity limits, configured from compiled schema).
    pub validator:             crate::validation::RequestValidator,
    /// Debug configuration (optional, from `[debug]` in `fraiseql.toml`).
    pub debug_config:          Option<fraiseql_core::schema::DebugConfig>,
    /// Maximum byte length for a query string delivered via HTTP GET.
    ///
    /// Defaults to `100_000` (100 KiB).  Configurable via
    /// `ServerConfig::max_get_query_bytes`.
    pub max_get_query_bytes:   usize,
    /// Connection pool auto-tuner (optional, enabled via `[pool_tuning]` config).
    pub pool_tuner:            Option<Arc<crate::pool::PoolSizingAdvisor>>,
}

impl<A: DatabaseAdapter> AppState<A> {
    /// Create new application state.
    #[must_use]
    pub fn new(executor: Arc<Executor<A>>) -> Self {
        Self {
            executor,
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
            max_get_query_bytes: 100_000,
        }
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

    /// Sanitize a batch of errors before sending them to the client.
    pub fn sanitize_errors(&self, errors: Vec<GraphQLError>) -> Vec<GraphQLError> {
        self.error_sanitizer.sanitize_all(errors)
    }
}
