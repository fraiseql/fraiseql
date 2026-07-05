//! `AppState` construction and configuration wiring for the server router.

use fraiseql_core::{db::traits::DatabaseAdapter, security::IntrospectionPolicy};
use tracing::info;

use super::super::Server;
use crate::routes::graphql::AppState;

impl<A: DatabaseAdapter + Clone + Send + Sync + 'static> Server<A> {
    /// Build the shared `AppState` with all configured subsystems attached.
    pub(super) fn build_app_state(&self) -> AppState<A> {
        let mut state = AppState::new(self.executor.clone())
            .with_reload_config(self.config.schema_path.clone(), self.executor.adapter().clone());

        // Attach secrets manager if configured
        #[cfg(feature = "secrets")]
        if let Some(ref secrets_manager) = self.secrets_manager {
            state = state.with_secrets_manager(secrets_manager.clone());
            info!("SecretsManager attached to AppState");

            // Wire field encryption: scan schema for encrypted fields and build the service.
            // Requires the secrets manager (for key fetch) and the schema field map.
            let field_keys: std::collections::HashMap<String, String> = self
                .executor
                .schema()
                .types
                .iter()
                .flat_map(|t| t.fields.iter())
                .filter_map(|f| {
                    f.encryption.as_ref().map(|enc| (f.name.to_string(), enc.key_reference.clone()))
                })
                .collect();

            if !field_keys.is_empty() {
                use fraiseql_secrets::encryption::{
                    database_adapter::DatabaseFieldAdapter, middleware::FieldEncryptionService,
                };
                let adapter = std::sync::Arc::new(DatabaseFieldAdapter::new(
                    secrets_manager.clone(),
                    field_keys,
                ));
                let svc = std::sync::Arc::new(FieldEncryptionService::from_schema(
                    self.executor.schema(),
                    adapter,
                ));
                state = state.with_field_encryption(svc);
                info!("Field encryption service wired from schema");
            }
        }

        // Attach federation circuit breaker if configured
        #[cfg(feature = "federation")]
        if let Some(ref cb) = self.circuit_breaker {
            state = state.with_circuit_breaker(cb.clone());
            info!("Federation circuit breaker attached to AppState");
        }

        // Attach observer runtime for health probes
        #[cfg(feature = "observers")]
        if let Some(ref runtime) = self.observer_runtime {
            state = state.with_observer_runtime(runtime.clone());
            info!("Observer runtime attached to AppState for health probes");
        }

        // Thread adapter-level cache state through to admin handlers.
        state = state.with_adapter_cache_enabled(self.adapter_cache_enabled);

        // Wire usage aggregator (shared with MutationAuditLayer tracing subscriber).
        state = state.with_usage(self.usage.clone());

        // Attach error sanitizer (always present; disabled by default)
        state = state.with_error_sanitizer(self.error_sanitizer.clone());
        if self.error_sanitizer.is_enabled() {
            info!(
                "Error sanitizer enabled — internal error details will be stripped from responses"
            );
        }

        // Attach API key authenticator if configured
        if let Some(ref api_key_auth) = self.api_key_authenticator {
            state = state.with_api_key_authenticator(api_key_auth.clone());
            info!("API key authenticator attached to AppState");
        }

        // Attach the functions-runtime before-mutation hooks prepared at serve time
        // (modules loaded, runtimes registered, send_email wiring attached), so
        // after:mutation functions fire on the I/O-capable live host.
        #[cfg(feature = "functions-runtime")]
        if let Some(hooks) = self.functions_hooks.as_ref() {
            state = state.with_functions(hooks.clone());
        }

        // Enriched-identity resolution (#539). When `[identity.enrichment]` is
        // enabled, build the resolver on the unscoped auth pool and attach it, so
        // every authenticated request resolves its DB identity and fail-closes
        // before dispatch. `enabled = true` alone is the trigger (DESIGN §7).
        #[cfg(feature = "auth")]
        if let Some(enrichment) =
            self.config.identity.as_ref().and_then(|identity| identity.enrichment.as_ref())
        {
            if enrichment.enabled {
                if let Some(pool) = self.enrichment_pool.as_ref() {
                    state = state.with_identity_resolver(std::sync::Arc::new(
                        crate::identity::IdentityResolver::postgres(
                            enrichment.clone(),
                            pool.clone(),
                        ),
                    ));
                    if crate::identity::schema_declares_enrichment_consumer(self.executor.schema())
                    {
                        info!(
                            "Enriched-identity resolution enabled (#539): every authenticated \
                             request resolves and fail-closes"
                        );
                    } else {
                        tracing::warn!(
                            "[identity.enrichment] is enabled but no session variable or inject \
                             param uses an `enrichment` source — every authenticated request will \
                             resolve identity, yet nothing reads it (likely a misconfiguration)"
                        );
                    }
                } else {
                    tracing::warn!(
                        "[identity.enrichment] is enabled but no auth database pool is available \
                         — enrichment cannot run (a non-PostgreSQL backend, or DATABASE_URL is \
                         unset). Requests will NOT be enriched."
                    );
                }
            }
        }

        // Attach state encryption service if configured
        #[cfg(feature = "auth")]
        match &self.state_encryption {
            Some(svc) => {
                state = state.with_state_encryption(svc.clone());
                info!("State encryption: enabled");
            },
            None => {
                info!("State encryption: disabled (no key configured)");
            },
        }

        // Build RequestValidator from validation config.
        // Priority: runtime TOML > compiled schema > defaults.
        let mut validator = crate::validation::RequestValidator::new();
        let runtime_vc = self.config.validation.as_ref();
        let compiled_vc = self.executor.schema().validation_config.as_ref();

        let effective_depth = runtime_vc
            .and_then(|v| v.max_query_depth)
            .or_else(|| compiled_vc.and_then(|v| v.max_query_depth));
        let effective_complexity = runtime_vc
            .and_then(|v| v.max_query_complexity)
            .or_else(|| compiled_vc.and_then(|v| v.max_query_complexity));

        if let Some(depth) = effective_depth {
            validator = validator.with_max_depth(depth as usize);
            let source = if runtime_vc.and_then(|v| v.max_query_depth).is_some() {
                "runtime toml"
            } else {
                "compiled schema"
            };
            info!(max_query_depth = depth, source, "Query depth limit configured");
        }
        if let Some(complexity) = effective_complexity {
            validator = validator.with_max_complexity(complexity as usize);
            let source = if runtime_vc.and_then(|v| v.max_query_complexity).is_some() {
                "runtime toml"
            } else {
                "compiled schema"
            };
            info!(max_query_complexity = complexity, source, "Query complexity limit configured");
        }
        state = state.with_validator(validator);

        // Start pool auto-tuner if configured and enabled
        if let Some(ref cfg) = self.pool_tuning_config {
            if cfg.enabled {
                let tuner = std::sync::Arc::new(crate::pool::PoolSizingAdvisor::new(cfg.clone()));
                // Spawn background polling task (recommendation mode — no resize_fn supplied
                // because deadpool-postgres does not expose runtime resize).
                let _handle =
                    std::sync::Arc::clone(&tuner).start(self.executor.adapter().clone(), None);
                state = state.with_pool_tuner(tuner);
                info!(
                    tuning_interval_ms = cfg.tuning_interval_ms,
                    min = cfg.min_pool_size,
                    max = cfg.max_pool_size,
                    "Pool auto-tuner started (recommendation mode)"
                );
            }
        }

        // Attach debug config from compiled schema
        state.debug_config.clone_from(&self.executor.schema().debug_config);

        // Apply GET query size limit from server config.
        state.max_get_query_bytes = self.config.max_get_query_bytes;

        // Derive the introspection policy from the two server-config booleans.
        // This is the single source of truth shared with the REST
        // `/introspection` mount decision (`admin.rs`), so the GraphQL request
        // path and the REST endpoint agree by construction.
        state = state.with_introspection_policy(IntrospectionPolicy::from_config(
            self.config.introspection_enabled,
            self.config.introspection_require_auth,
        ));

        // Attach APQ store if configured
        if let Some(ref store) = self.apq_store {
            state = state.with_apq_store(store.clone());
        }

        // Attach trusted document store if configured
        if let Some(ref store) = self.trusted_docs {
            state = state.with_trusted_docs(store.clone());
        }

        // Multi-tenant executor runtime (#330). When enabled via
        // `[tenancy.runtime] enabled = true`, install the per-tenant executor
        // registry (seeded with the default executor), an in-memory audit log,
        // and — when the binary's PostgreSQL boot path supplied one — the executor
        // factory that `PUT /api/v1/admin/tenants/{key}` uses to provision tenants.
        // The domain registry is already present (an empty default) and is
        // populated at runtime via `PUT /api/v1/admin/domains/{domain}`.
        if self.config.tenancy.runtime.enabled {
            use crate::{
                routes::graphql::tenant_registry::TenantExecutorRegistry,
                tenancy::audit::InMemoryAuditLog,
            };

            // Seed the registry with the default executor (the `ArcSwap`-wrapped
            // one `AppState` holds), so requests with no/unknown tenant fall back
            // to it exactly as the GraphQL handler expects.
            let registry = TenantExecutorRegistry::new(state.executor.clone());
            state = state
                .with_tenant_registry(std::sync::Arc::new(registry))
                .with_tenant_audit_log(std::sync::Arc::new(InMemoryAuditLog::new()));

            if let Some(ref factory) = self.tenant_executor_factory {
                state = state.with_tenant_executor_factory(factory.clone());
            }

            info!(
                provisioning = self.tenant_executor_factory.is_some(),
                "Multi-tenant runtime enabled: tenant registry, dispatch, and \
                 /api/v1/admin/tenants/* mounted"
            );
        }

        state
    }
}
