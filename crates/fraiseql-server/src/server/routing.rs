//! Application router construction and route registration.

#[cfg(any(feature = "auth", feature = "mcp", feature = "observers"))]
use std::sync::Arc;

use axum::{
    Router,
    extract::DefaultBodyLimit,
    middleware,
    routing::{get, post, put},
};
use fraiseql_core::db::traits::{DatabaseAdapter, SupportsMutations};
use tower_http::compression::{CompressionLayer, predicate::SizeAbove};
use tracing::{info, warn};

use super::{
    AppState, BearerAuthState, OidcAuthState, PlaygroundState, Server, SubscriptionState, api,
    bearer_auth_middleware, cors_layer_restricted, graphql_get_handler, graphql_handler,
    health_handler, introspection_handler, metrics_handler, metrics_json_handler,
    metrics_middleware, oidc_auth_middleware, playground_handler, readiness_handler,
    require_json_content_type, subscription_handler, trace_layer,
};
#[cfg(feature = "auth")]
use super::{AuthMeState, AuthPkceState, auth_callback, auth_me, auth_start};
use crate::middleware::{Hs256AuthState, hs256_auth_middleware};

impl<A: DatabaseAdapter + Clone + Send + Sync + 'static> Server<A> {
    /// Build application router with full REST support (mutations included).
    ///
    /// Requires `SupportsMutations` so that the full `rest_router` can be mounted.
    /// The returned `AppState` is needed by the lifecycle module for SIGUSR1 schema
    /// reload handling.
    pub(super) fn build_router(&self) -> (Router, AppState<A>)
    where
        A: SupportsMutations,
    {
        #[cfg(feature = "rest")]
        let make_rest = |state: &AppState<A>, compress: bool| {
            crate::routes::rest::rest_router(state, compress)
        };
        #[cfg(not(feature = "rest"))]
        let make_rest = |_state: &AppState<A>, _compress: bool| -> Option<Router> { None };
        self.build_base_router(make_rest)
    }

    /// Build application router for read-only adapters (GET + SSE REST only).
    ///
    /// Mounts `rest_query_router` instead of the full `rest_router`, so no
    /// `SupportsMutations` bound is required.  Use this for `SqliteAdapter` or
    /// `FraiseWireAdapter`-backed servers.
    ///
    /// The returned `AppState` is needed by the lifecycle module for SIGUSR1 schema
    /// reload handling.
    pub(super) fn build_readonly_router(&self) -> (Router, AppState<A>) {
        #[cfg(feature = "rest")]
        let make_rest = |state: &AppState<A>, compress: bool| {
            crate::routes::rest::rest_query_router(state, compress)
        };
        #[cfg(not(feature = "rest"))]
        let make_rest = |_state: &AppState<A>, _compress: bool| -> Option<Router> { None };
        self.build_base_router(make_rest)
    }

    /// Internal router builder shared by [`build_router`] and [`build_readonly_router`].
    ///
    /// `make_rest` is called once with the fully-initialized `AppState` and the
    /// compression flag.  The caller supplies either `rest_router` (full REST,
    /// requires `SupportsMutations`) or `rest_query_router` (read-only).
    ///
    /// The returned `AppState` is needed by the lifecycle module for
    /// SIGUSR1 schema reload handling.
    #[allow(clippy::cognitive_complexity)] // Reason: route construction with many optional middleware layers and feature-gated endpoints
    fn build_base_router<F>(&self, make_rest: F) -> (Router, AppState<A>)
    where
        F: FnOnce(&AppState<A>, bool) -> Option<Router>,
    {
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

        // Attach APQ store if configured
        if let Some(ref store) = self.apq_store {
            state = state.with_apq_store(store.clone());
        }

        // Attach trusted document store if configured
        if let Some(ref store) = self.trusted_docs {
            state = state.with_trusted_docs(store.clone());
        }

        let metrics = state.metrics.clone();

        // Build GraphQL route (possibly with auth + Content-Type enforcement).
        // Supports both GET and POST per GraphQL over HTTP spec.
        // OIDC and HS256 are mutually exclusive (enforced by ServerConfig::validate).
        let graphql_router = if let Some(ref validator) = self.oidc_validator {
            info!(
                graphql_path = %self.config.graphql_path,
                "GraphQL endpoint protected by OIDC authentication (GET and POST)"
            );
            let auth_state = OidcAuthState::new(validator.clone());
            let router = Router::new()
                .route(
                    &self.config.graphql_path,
                    get(graphql_get_handler::<A>).post(graphql_handler::<A>),
                )
                .route_layer(middleware::from_fn_with_state(auth_state, oidc_auth_middleware));

            if self.config.require_json_content_type {
                router
                    .route_layer(middleware::from_fn(require_json_content_type))
                    .with_state(state.clone())
            } else {
                router.with_state(state.clone())
            }
        } else if let Some(ref validator) = self.hs256_auth {
            info!(
                graphql_path = %self.config.graphql_path,
                "GraphQL endpoint protected by HS256 authentication (GET and POST)"
            );
            let realm = self
                .config
                .auth_hs256
                .as_ref()
                .and_then(|h| h.issuer.clone())
                .unwrap_or_else(|| "fraiseql".to_string());
            let auth_state = Hs256AuthState::new(validator.clone(), realm);
            let router = Router::new()
                .route(
                    &self.config.graphql_path,
                    get(graphql_get_handler::<A>).post(graphql_handler::<A>),
                )
                .route_layer(middleware::from_fn_with_state(auth_state, hs256_auth_middleware));

            if self.config.require_json_content_type {
                router
                    .route_layer(middleware::from_fn(require_json_content_type))
                    .with_state(state.clone())
            } else {
                router.with_state(state.clone())
            }
        } else {
            let router = Router::new().route(
                &self.config.graphql_path,
                get(graphql_get_handler::<A>).post(graphql_handler::<A>),
            );

            if self.config.require_json_content_type {
                router
                    .route_layer(middleware::from_fn(require_json_content_type))
                    .with_state(state.clone())
            } else {
                router.with_state(state.clone())
            }
        };

        // Apply framework-level compression if enabled.
        // Disabled by default: in production, prefer reverse-proxy compression
        // (Nginx, Caddy, cloud LB) which offloads CPU and supports brotli.
        // When enabled, skip responses under 1 KiB — gzip overhead dominates
        // on tiny payloads (e.g. short GraphQL results, health responses).
        let graphql_router = if self.config.compression_enabled {
            graphql_router.layer(CompressionLayer::new().compress_when(SizeAbove::new(1024)))
        } else {
            graphql_router
        };

        // Build base routes (always available without auth)
        let mut app = Router::new()
            .route(&self.config.health_path, get(health_handler::<A>))
            .route(&self.config.readiness_path, get(readiness_handler::<A>))
            .with_state(state.clone())
            .merge(graphql_router);

        // Conditionally add playground route
        if self.config.playground_enabled {
            let playground_state =
                PlaygroundState::new(self.config.graphql_path.clone(), self.config.playground_tool);
            info!(
                playground_path = %self.config.playground_path,
                playground_tool = ?self.config.playground_tool,
                "GraphQL playground enabled"
            );
            let playground_router = Router::new()
                .route(&self.config.playground_path, get(playground_handler))
                .with_state(playground_state);
            app = app.merge(playground_router);
        }

        // Conditionally add /.well-known/security.txt (RFC 9116)
        if let Some(ref contact) = self.config.security_contact {
            info!(
                contact = %contact,
                "/.well-known/security.txt endpoint enabled"
            );
            let security_router = Router::new()
                .route(
                    "/.well-known/security.txt",
                    get(crate::routes::well_known::security_txt_handler),
                )
                .with_state(contact.clone());
            app = app.merge(security_router);
        }

        // Conditionally add subscription route (WebSocket)
        if self.config.subscriptions_enabled {
            let subscription_state = SubscriptionState::new(self.subscription_manager.clone())
                .with_lifecycle(self.subscription_lifecycle.clone())
                .with_max_subscriptions(self.max_subscriptions_per_connection);
            info!(
                subscription_path = %self.config.subscription_path,
                "GraphQL subscriptions enabled (graphql-transport-ws + graphql-ws protocols)"
            );
            let subscription_router = Router::new()
                .route(&self.config.subscription_path, get(subscription_handler))
                .with_state(subscription_state);
            app = app.merge(subscription_router);
        }

        // Conditionally add introspection endpoint (with optional auth)
        if self.config.introspection_enabled {
            if self.config.introspection_require_auth {
                if let Some(ref validator) = self.oidc_validator {
                    info!(
                        introspection_path = %self.config.introspection_path,
                        "Introspection endpoint enabled (OIDC auth required)"
                    );
                    let auth_state = OidcAuthState::new(validator.clone());
                    let introspection_router = Router::new()
                        .route(&self.config.introspection_path, get(introspection_handler::<A>))
                        .route_layer(middleware::from_fn_with_state(
                            auth_state.clone(),
                            oidc_auth_middleware,
                        ))
                        .with_state(state.clone());
                    app = app.merge(introspection_router);

                    // Schema export endpoints follow same auth as introspection
                    let schema_router = Router::new()
                        .route("/api/v1/schema.graphql", get(api::schema::export_sdl_handler::<A>))
                        .route("/api/v1/schema.json", get(api::schema::export_json_handler::<A>))
                        .route_layer(middleware::from_fn_with_state(
                            auth_state,
                            oidc_auth_middleware,
                        ))
                        .with_state(state.clone());
                    app = app.merge(schema_router);
                } else {
                    warn!(
                        "introspection_require_auth is true but no OIDC configured - introspection and schema export disabled"
                    );
                }
            } else {
                info!(
                    introspection_path = %self.config.introspection_path,
                    "Introspection endpoint enabled (no auth required - USE ONLY IN DEVELOPMENT)"
                );
                let introspection_router = Router::new()
                    .route(&self.config.introspection_path, get(introspection_handler::<A>))
                    .with_state(state.clone());
                app = app.merge(introspection_router);

                // Schema export endpoints available without auth when introspection enabled without
                // auth
                let schema_router = Router::new()
                    .route("/api/v1/schema.graphql", get(api::schema::export_sdl_handler::<A>))
                    .route("/api/v1/schema.json", get(api::schema::export_json_handler::<A>))
                    .with_state(state.clone());
                app = app.merge(schema_router);
            }
        }

        // Conditionally add metrics routes (protected by bearer token)
        if self.config.metrics_enabled {
            if let Some(ref token) = self.config.metrics_token {
                info!(
                    metrics_path = %self.config.metrics_path,
                    metrics_json_path = %self.config.metrics_json_path,
                    "Metrics endpoints enabled (bearer token required)"
                );

                let auth_state = BearerAuthState::new(token.clone());

                // Create a separate metrics router with auth middleware applied
                // The routes need relative paths since we use merge (not nest)
                let metrics_router = Router::new()
                    .route(&self.config.metrics_path, get(metrics_handler::<A>))
                    .route(&self.config.metrics_json_path, get(metrics_json_handler::<A>))
                    .route_layer(middleware::from_fn_with_state(auth_state, bearer_auth_middleware))
                    .with_state(state.clone());

                app = app.merge(metrics_router);
            } else {
                warn!(
                    "metrics_enabled is true but metrics_token is not set - metrics endpoints disabled"
                );
            }
        }

        // Conditionally add admin routes (protected by bearer token).
        //
        // When `admin_readonly_token` is configured the admin surface is split:
        //   • Write router  (admin_token)           — reload-schema, cache/clear
        //   • Read router   (admin_readonly_token)  — config, cache/stats, explain,
        //                                             query/explain, grafana-dashboard
        //
        // When only `admin_token` is set every route uses that single token
        // (backwards-compatible, but logged as a security advisory).
        if self.config.admin_api_enabled {
            if let Some(ref write_token) = self.config.admin_token {
                // Destructive-operation router — always uses admin_token.
                let write_auth = BearerAuthState::new(write_token.clone());
                let admin_write_router = Router::new()
                    .route(
                        "/api/v1/admin/reload-schema",
                        post(api::admin::reload_schema_handler::<A>),
                    )
                    .route("/api/v1/admin/cache/clear", post(api::admin::cache_clear_handler::<A>))
                    // Tenant management write endpoints (multi-tenant mode)
                    .route(
                        "/api/v1/admin/tenants/{key}",
                        put(api::tenant_admin::upsert_tenant_handler::<A>)
                            .delete(api::tenant_admin::delete_tenant_handler::<A>),
                    )
                    // Domain management write endpoints (multi-tenant mode)
                    .route(
                        "/api/v1/admin/domains/{domain}",
                        put(api::tenant_admin::upsert_domain_handler::<A>)
                            .delete(api::tenant_admin::delete_domain_handler::<A>),
                    )
                    .route_layer(middleware::from_fn_with_state(write_auth, bearer_auth_middleware))
                    .with_state(state.clone());
                app = app.merge(admin_write_router);

                // Read-only router — uses admin_readonly_token when configured, otherwise
                // falls back to admin_token (single-token mode, logs a warning).
                let read_token = self.config.admin_readonly_token.as_ref().unwrap_or(write_token);

                if self.config.admin_readonly_token.is_none() {
                    // SECURITY (H14): single token grants destructive + read-only access.
                    warn!(
                        admin_write_routes = "reload-schema, cache/clear",
                        admin_read_routes =
                            "cache/stats, config, explain, query/explain, grafana-dashboard",
                        "Admin API running in single-token mode: admin_token grants ALL operations \
                         including destructive ones. Set admin_readonly_token to scope access."
                    );
                } else {
                    info!(
                        "Admin API running in split-token mode: \
                         admin_token=write-only, admin_readonly_token=read-only"
                    );
                }

                let read_auth = BearerAuthState::new(read_token.clone());
                let admin_read_router = Router::new()
                    .route("/api/v1/admin/cache/stats", get(api::admin::cache_stats_handler::<A>))
                    .route("/api/v1/admin/config", get(api::admin::config_handler::<A>))
                    .route("/api/v1/admin/explain", post(api::admin::explain_handler::<A>))
                    // Tenant management read endpoints (multi-tenant mode)
                    .route(
                        "/api/v1/admin/tenants",
                        get(api::tenant_admin::list_tenants_handler::<A>),
                    )
                    .route(
                        "/api/v1/admin/tenants/{key}",
                        get(api::tenant_admin::get_tenant_handler::<A>),
                    )
                    .route(
                        "/api/v1/admin/tenants/{key}/health",
                        get(api::tenant_admin::tenant_health_handler::<A>),
                    )
                    // Domain management read endpoints (multi-tenant mode)
                    .route(
                        "/api/v1/admin/domains",
                        get(api::tenant_admin::list_domains_handler::<A>),
                    )
                    // /api/v1/query/explain is here (not in the open api::routes()) so that
                    // query-plan details are always protected by an admin token (H13).
                    .route("/api/v1/query/explain", post(api::query::explain_handler::<A>))
                    .route(
                        "/api/v1/admin/grafana-dashboard",
                        get(api::admin::grafana_dashboard_handler::<A>),
                    )
                    .route_layer(middleware::from_fn_with_state(read_auth, bearer_auth_middleware))
                    .with_state(state.clone());
                app = app.merge(admin_read_router);

                info!("Admin API endpoints enabled (bearer token required)");
            } else {
                warn!(
                    "admin_api_enabled is true but admin_token is not set - admin endpoints disabled"
                );
            }
        }

        // Conditionally add design audit endpoints (with optional auth)
        if self.config.design_api_require_auth {
            if let Some(ref validator) = self.oidc_validator {
                info!("Design audit API endpoints enabled (OIDC auth required)");
                let auth_state = OidcAuthState::new(validator.clone());
                let design_router = Router::new()
                    .route(
                        "/design/federation-audit",
                        post(api::design::federation_audit_handler::<A>),
                    )
                    .route("/design/cost-audit", post(api::design::cost_audit_handler::<A>))
                    .route("/design/cache-audit", post(api::design::cache_audit_handler::<A>))
                    .route("/design/auth-audit", post(api::design::auth_audit_handler::<A>))
                    .route(
                        "/design/compilation-audit",
                        post(api::design::compilation_audit_handler::<A>),
                    )
                    .route("/design/audit", post(api::design::overall_design_audit_handler::<A>))
                    .route_layer(middleware::from_fn_with_state(auth_state, oidc_auth_middleware))
                    .with_state(state.clone());
                app = app.nest("/api/v1", design_router);
            } else {
                // SECURITY: design_api_require_auth is true but no OIDC validator is configured.
                // Fail-closed: do NOT mount design endpoints unprotected.
                warn!(
                    "SECURITY: design_api_require_auth is true but no OIDC configured — \
                     design API endpoints are DISABLED. Configure an OIDC validator \
                     or set design_api_require_auth = false (development only)."
                );
            }
        } else {
            info!("Design audit API endpoints enabled (no auth required)");
            let design_router = Router::new()
                .route("/design/federation-audit", post(api::design::federation_audit_handler::<A>))
                .route("/design/cost-audit", post(api::design::cost_audit_handler::<A>))
                .route("/design/cache-audit", post(api::design::cache_audit_handler::<A>))
                .route("/design/auth-audit", post(api::design::auth_audit_handler::<A>))
                .route(
                    "/design/compilation-audit",
                    post(api::design::compilation_audit_handler::<A>),
                )
                .route("/design/audit", post(api::design::overall_design_audit_handler::<A>))
                .with_state(state.clone());
            app = app.nest("/api/v1", design_router);
        }

        // PKCE OAuth2 auth routes — mounted only when both pkce and [auth] are configured.
        #[cfg(feature = "auth")]
        if let (Some(store), Some(client)) = (&self.pkce_store, &self.oidc_server_client) {
            let auth_state = Arc::new(AuthPkceState {
                pkce_store:              Arc::clone(store),
                oidc_client:             Arc::clone(client),
                http_client:             Arc::new(
                    reqwest::Client::builder()
                        .timeout(std::time::Duration::from_secs(30))
                        .build()
                        .unwrap_or_default(),
                ),
                post_login_redirect_uri: None,
            });
            let auth_router = Router::new()
                .route("/auth/start", get(auth_start))
                .route("/auth/callback", get(auth_callback))
                .with_state(auth_state);
            app = app.merge(auth_router);
            info!("PKCE auth routes mounted: GET /auth/start, GET /auth/callback");
        }

        // /auth/me session-identity endpoint — mounted when:
        // 1. The `auth` feature is compiled in.
        // 2. An OIDC validator is present (token validation capability).
        // 3. `[auth.me] enabled = true` in the compiled schema / ServerConfig.
        //
        // Gated on the OIDC *validator* (not on pkce_store) because the endpoint
        // only needs to validate tokens; it does not participate in the PKCE flow.
        // This lets operators use /auth/me even when tokens are issued by an
        // external mechanism rather than FraiseQL's built-in PKCE routes.
        #[cfg(feature = "auth")]
        if let (Some(ref validator), Some(me_cfg)) = (
            &self.oidc_validator,
            self.config.auth.as_ref().and_then(|a| a.me.as_ref()).filter(|m| m.enabled),
        ) {
            // Build enrichment state if configured and a pool is available
            let enrichment = me_cfg.enrichment.as_ref().and_then(|enr_cfg| {
                if let Some(ref pool) = self.enrichment_pool {
                    Some(crate::routes::AuthMeEnrichmentState {
                        config: enr_cfg.clone(),
                        pool: pool.clone(),
                        cache: Arc::new(crate::routes::enrichment::EnrichmentCache::new()),
                    })
                } else {
                    tracing::warn!(
                        "Claims enrichment configured but no database pool available — \
                         enrichment will be skipped"
                    );
                    None
                }
            });

            let me_state = Arc::new(AuthMeState {
                expose_claims: me_cfg.expose_claims.clone(),
                enrichment,
            });
            let auth_state = OidcAuthState::new(Arc::clone(validator));
            let me_router = Router::new()
                .route("/auth/me", get(auth_me))
                .route_layer(middleware::from_fn_with_state(auth_state, oidc_auth_middleware))
                .with_state(me_state);
            app = app.merge(me_router);
            info!(
                expose_claims = ?me_cfg.expose_claims,
                enrichment = me_cfg.enrichment.is_some(),
                "Session identity route mounted: GET /auth/me"
            );
        }

        // Token revocation routes — mounted only when revocation is configured.
        #[cfg(feature = "auth")]
        if let Some(ref rev_mgr) = self.revocation_manager {
            let rev_state = Arc::new(crate::routes::RevocationRouteState {
                revocation_manager: Arc::clone(rev_mgr),
            });
            let rev_router = Router::new()
                .route("/auth/revoke", post(crate::routes::revoke_token))
                .route("/auth/revoke-all", post(crate::routes::revoke_all_tokens))
                .with_state(rev_state);
            app = app.merge(rev_router);
            info!("Token revocation routes mounted: POST /auth/revoke, POST /auth/revoke-all");
        }

        // MCP (Model Context Protocol) route — mounted when mcp feature is compiled in
        // and mcp_config is present.
        #[cfg(feature = "mcp")]
        if let Some(ref mcp_cfg) = self.mcp_config {
            if mcp_cfg.transport == "http" || mcp_cfg.transport == "both" {
                // SECURITY: Check require_auth flag before mounting.
                // If require_auth=true but no OIDC is configured, refuse to mount (fail-closed).
                // Full per-request OIDC enforcement for MCP is tracked separately.
                let mount_mcp = if mcp_cfg.require_auth {
                    if self.oidc_validator.is_some() {
                        warn!(
                            path = %mcp_cfg.path,
                            "MCP HTTP endpoint: require_auth=true, OIDC validator present. \
                             Note: per-request MCP auth enforcement requires MCP middleware. \
                             Ensure your MCP transport layer validates tokens."
                        );
                        true
                    } else {
                        // SECURITY: require_auth=true but no OIDC — fail closed.
                        tracing::error!(
                            path = %mcp_cfg.path,
                            "MCP HTTP endpoint NOT mounted — require_auth=true but no OIDC \
                             validator is configured. Configure an OIDC validator or set \
                             require_auth=false (development only)."
                        );
                        false
                    }
                } else {
                    warn!(
                        path = %mcp_cfg.path,
                        "MCP HTTP endpoint mounted without authentication (require_auth=false). \
                         Enable require_auth in production."
                    );
                    true
                };

                if mount_mcp {
                    use rmcp::transport::{
                        StreamableHttpServerConfig, StreamableHttpService,
                        streamable_http_server::session::local::LocalSessionManager,
                    };

                    // Capture ArcSwap so new MCP sessions get the current executor
                    let executor_swap = state.executor.clone();
                    let cfg = mcp_cfg.clone();
                    let mcp_service = StreamableHttpService::new(
                        move || {
                            let executor = executor_swap.load_full();
                            let schema = Arc::new(executor.schema().clone());
                            Ok(crate::mcp::handler::FraiseQLMcpService::new(
                                schema,
                                executor,
                                cfg.clone(),
                            ))
                        },
                        Arc::new(LocalSessionManager::default()),
                        StreamableHttpServerConfig::default(),
                    );
                    app = app.nest_service(&mcp_cfg.path, mcp_service);
                    info!(path = %mcp_cfg.path, "MCP HTTP endpoint mounted");
                }
            }
        }

        // REST transport — GET queries, SSE streams, OpenAPI spec, and (when the
        // adapter supports it) mutation routes (POST/PUT/PATCH/DELETE).
        // `make_rest` is provided by the caller: `build_router` supplies the full
        // `rest_router` (requires `SupportsMutations`); `build_readonly_router` supplies
        // `rest_query_router` (GET + SSE only, no `SupportsMutations` required).
        if let Some(rest) = make_rest(&state, self.config.compression_enabled) {
            app = app.merge(rest);
        }

        // Remaining API routes (query intelligence, federation)
        let api_router = api::routes(state.clone());
        app = app.nest("/api/v1", api_router);

        // RBAC Management API (if database pool available)
        // SECURITY: RBAC endpoints must be protected by admin bearer token.
        // Without auth, any client could read or modify role assignments.
        #[cfg(feature = "observers")]
        if let Some(ref db_pool) = self.db_pool {
            if let Some(ref token) = self.config.admin_token {
                info!("RBAC Management API endpoints enabled (admin bearer token required)");
                let rbac_backend = Arc::new(
                    crate::api::rbac_management::db_backend::RbacDbBackend::new(db_pool.clone()),
                );
                // Schema is initialized by serve_with_shutdown() before this
                // function is called; build_router() is sync so no await here.
                let rbac_state = crate::api::RbacManagementState { db: rbac_backend };
                let auth_state = BearerAuthState::new(token.clone());
                let rbac_router = crate::api::rbac_management_router(rbac_state).route_layer(
                    middleware::from_fn_with_state(auth_state, bearer_auth_middleware),
                );
                app = app.merge(rbac_router);
            } else {
                // SECURITY: Refuse to mount RBAC endpoints without authentication.
                tracing::error!(
                    "RBAC Management API disabled — admin_token is not set. \
                     Set admin_token in server configuration to enable RBAC management endpoints."
                );
            }
        }

        // Add HTTP metrics middleware (tracks requests and response status codes)
        // This runs on ALL routes, even when metrics endpoints are disabled
        app = app.layer(middleware::from_fn_with_state(metrics, metrics_middleware));

        // Observer routes (if enabled and compiled with feature)
        #[cfg(feature = "observers")]
        {
            app = self.add_observer_routes(app);
        }

        // Add middleware
        if self.config.tracing_enabled {
            app = app.layer(trace_layer());
        }

        if self.config.cors_enabled {
            // Use restricted CORS with configured origins
            let origins = if self.config.cors_origins.is_empty() {
                // Default to localhost for development if no origins configured
                tracing::warn!(
                    "CORS enabled but no origins configured. Using localhost:3000 as default. \
                     Set cors_origins in config for production."
                );
                vec!["http://localhost:3000".to_string()]
            } else {
                self.config.cors_origins.clone()
            };
            app = app.layer(cors_layer_restricted(&origins));
        }

        // Add request body size limit (default 1 MB — prevents memory exhaustion)
        if self.config.max_request_body_bytes > 0 {
            info!(
                max_bytes = self.config.max_request_body_bytes,
                "Request body size limit enabled"
            );
            app = app.layer(DefaultBodyLimit::max(self.config.max_request_body_bytes));
        }

        // Add HTTP header count and size limits (prevents header-flooding DoS)
        {
            let max_header_count = self.config.max_header_count;
            let max_header_bytes = self.config.max_header_bytes;
            info!(max_header_count, max_header_bytes, "HTTP header limits enabled");
            app = app.layer(axum::middleware::from_fn(move |req, next| {
                crate::middleware::header_limits_middleware(
                    req,
                    next,
                    max_header_count,
                    max_header_bytes,
                )
            }));
        }

        // Add per-request timeout (optional — defence against runaway DB queries).
        if let Some(timeout_secs) = self.config.request_timeout_secs {
            use std::time::Duration;

            use tower_http::timeout::TimeoutLayer;

            info!(timeout_secs, "Request timeout enabled");
            app = app.layer(TimeoutLayer::with_status_code(
                axum::http::StatusCode::REQUEST_TIMEOUT,
                Duration::from_secs(timeout_secs),
            ));
        }

        // Add rate limiting middleware if configured.
        // Uses a named function (not an inline closure) to keep the Axum layer type
        // tree shallow — anonymous closure types caused rustc-ICE on nightly due to
        // type-checker stack overflow when inferring deeply-nested Layered<…> types.
        // The limiter is threaded via `Extension` so `rate_limit_middleware` can read
        // it from request extensions without capturing it in a closure.
        if let Some(ref limiter) = self.rate_limiter {
            use axum::Extension;

            use crate::middleware::rate_limit::rate_limit_middleware;

            info!("Enabling rate limiting middleware");
            app = app
                .layer(middleware::from_fn(rate_limit_middleware))
                .layer(Extension(limiter.clone()));
        }

        // Wire admission controller into the router via Extension so that handlers
        // can extract `Extension<Arc<AdmissionController>>` when needed.
        // Full Tower middleware wiring (returning 503 before the handler runs) is
        // tracked as a follow-up; the Extension approach makes the controller
        // reachable from production code and removes the ghost-code classification.
        if let Some(ref admission_cfg) = self.config.admission_control {
            use std::sync::Arc;

            use axum::Extension;

            use crate::resilience::backpressure::AdmissionController;

            let controller = Arc::new(AdmissionController::new(
                admission_cfg.max_concurrent,
                admission_cfg.max_queue_depth,
            ));
            info!(
                max_concurrent = admission_cfg.max_concurrent,
                max_queue_depth = admission_cfg.max_queue_depth,
                "Admission controller enabled and attached to request extensions"
            );
            app = app.layer(Extension(controller));
        }

        (app, state)
    }

    /// Add observer-related routes to the router.
    ///
    /// # PostgreSQL requirement
    ///
    /// The `observers` feature requires a PostgreSQL connection pool (`db_pool`).
    /// When this feature is enabled, `Server::new()` must receive a `Some(PgPool)` as the
    /// `db_pool` argument. If no pool is provided, observer management routes are skipped
    /// and an error is logged rather than panicking, so the server can still serve other
    /// requests. Callers should treat a missing pool as a configuration error.
    #[cfg(feature = "observers")]
    pub(super) fn add_observer_routes(&self, app: Router) -> Router {
        use crate::observers::{
            ChangelogState, DlqState, ObserverRepository, ObserverState, RuntimeHealthState,
            observer_changelog_routes, observer_dlq_routes, observer_routes,
            observer_runtime_routes,
        };

        // Management API requires a PostgreSQL pool. If no pool was supplied at
        // construction time, log an error and skip observer routes entirely rather
        // than panicking. Callers should pass `Some(db_pool)` to `Server::new()`
        // when the `observers` feature is compiled in.
        let Some(db_pool) = self.db_pool.clone() else {
            tracing::error!(
                "Observer management routes not mounted: \
                 the `observers` feature requires a PostgreSQL pool (`db_pool`). \
                 Pass `Some(sqlx::PgPool)` to Server::new() to enable observer endpoints."
            );
            return app;
        };

        // Management API (always available with feature)
        let observer_state = ObserverState {
            repository: ObserverRepository::new(db_pool.clone()),
        };

        // Changelog + checkpoint API (always available with a pool)
        let changelog_state = ChangelogState { pool: db_pool };

        let app = app
            .nest("/api/observers", observer_routes(observer_state))
            .nest("/api/observers", observer_changelog_routes(changelog_state));

        // Runtime health API and DLQ delivery status (only if runtime present)
        if let Some(ref runtime) = self.observer_runtime {
            info!(
                path = "/api/observers",
                "Observer management, runtime health, and DLQ delivery status endpoints enabled"
            );

            let runtime_state = RuntimeHealthState {
                runtime: runtime.clone(),
            };

            let dlq_state = DlqState {
                runtime: runtime.clone(),
            };

            app.merge(observer_runtime_routes(runtime_state))
                .nest("/api/observers", observer_dlq_routes(dlq_state))
        } else {
            app
        }
    }
}
