//! Application router construction and route registration.

use super::*;

impl<A: DatabaseAdapter + Clone + Send + Sync + 'static> Server<A> {
    /// Build application router.
    pub(super) fn build_router(&self) -> Router {
        let mut state = AppState::new(self.executor.clone());

        // Attach secrets manager if configured
        #[cfg(feature = "secrets")]
        if let Some(ref secrets_manager) = self.secrets_manager {
            state = state.with_secrets_manager(secrets_manager.clone());
            info!("SecretsManager attached to AppState");
        }

        // Attach federation circuit breaker if configured
        if let Some(ref cb) = self.circuit_breaker {
            state = state.with_circuit_breaker(cb.clone());
            info!("Federation circuit breaker attached to AppState");
        }

        // Attach error sanitizer (always present; disabled by default)
        state = state.with_error_sanitizer(self.error_sanitizer.clone());
        if self.error_sanitizer.is_enabled() {
            info!("Error sanitizer enabled — internal error details will be stripped from responses");
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

        // Build RequestValidator from compiled schema validation config
        let mut validator = crate::validation::RequestValidator::new();
        if let Some(ref vc) = self.executor.schema().validation_config {
            if let Some(depth) = vc.max_query_depth {
                validator = validator.with_max_depth(depth as usize);
                info!(max_query_depth = depth, "Custom query depth limit configured");
            }
            if let Some(complexity) = vc.max_query_complexity {
                validator = validator.with_max_complexity(complexity as usize);
                info!(max_query_complexity = complexity, "Custom query complexity limit configured");
            }
        }
        state = state.with_validator(validator);

        // Attach debug config from compiled schema
        state.debug_config.clone_from(&self.executor.schema().debug_config);

        // Attach APQ store if configured
        if let Some(ref store) = self.apq_store {
            state = state.with_apq_store(store.clone());
        }

        // Attach trusted document store if configured
        if let Some(ref store) = self.trusted_docs {
            state = state.with_trusted_docs(store.clone());
        }

        let metrics = state.metrics.clone();

        // Build GraphQL route (possibly with OIDC auth + Content-Type enforcement)
        // Supports both GET and POST per GraphQL over HTTP spec
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
        } else {
            let router = Router::new()
                .route(
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

        // Build base routes (always available without auth)
        let mut app = Router::new()
            .route(&self.config.health_path, get(health_handler::<A>))
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

        // Conditionally add admin routes (protected by bearer token)
        if self.config.admin_api_enabled {
            if let Some(ref token) = self.config.admin_token {
                info!("Admin API endpoints enabled (bearer token required)");

                let auth_state = BearerAuthState::new(token.clone());

                // Create a separate admin router with auth middleware applied
                let admin_router = Router::new()
                    .route(
                        "/api/v1/admin/reload-schema",
                        post(api::admin::reload_schema_handler::<A>),
                    )
                    .route("/api/v1/admin/cache/clear", post(api::admin::cache_clear_handler::<A>))
                    .route("/api/v1/admin/cache/stats", get(api::admin::cache_stats_handler::<A>))
                    .route("/api/v1/admin/config", get(api::admin::config_handler::<A>))
                    .route_layer(middleware::from_fn_with_state(auth_state, bearer_auth_middleware))
                    .with_state(state.clone());

                app = app.merge(admin_router);
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
                warn!(
                    "design_api_require_auth is true but no OIDC configured - design endpoints unprotected"
                );
                // Add unprotected design endpoints
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
                    .with_state(state.clone());
                app = app.nest("/api/v1", design_router);
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
                http_client:             Arc::new(reqwest::Client::new()),
                post_login_redirect_uri: None,
            });
            let auth_router = Router::new()
                .route("/auth/start",    get(auth_start))
                .route("/auth/callback", get(auth_callback))
                .with_state(auth_state);
            app = app.merge(auth_router);
            info!("PKCE auth routes mounted: GET /auth/start, GET /auth/callback");
        }

        // Token revocation routes — mounted only when revocation is configured.
        #[cfg(feature = "auth")]
        if let Some(ref rev_mgr) = self.revocation_manager {
            let rev_state = Arc::new(crate::routes::RevocationRouteState {
                revocation_manager: Arc::clone(rev_mgr),
            });
            let rev_router = Router::new()
                .route("/auth/revoke",     post(crate::routes::revoke_token))
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
                use rmcp::transport::{StreamableHttpServerConfig, StreamableHttpService};
                use rmcp::transport::streamable_http_server::session::local::LocalSessionManager;

                let schema = Arc::new(self.executor.schema().clone());
                let executor = self.executor.clone();
                let cfg = mcp_cfg.clone();
                let mcp_service = StreamableHttpService::new(
                    move || {
                        Ok(crate::mcp::handler::FraiseQLMcpService::new(
                            schema.clone(),
                            executor.clone(),
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

        // Remaining API routes (query intelligence, federation)
        let api_router = api::routes(state.clone());
        app = app.nest("/api/v1", api_router);

        // RBAC Management API (if database pool available)
        #[cfg(feature = "observers")]
        if let Some(ref db_pool) = self.db_pool {
            info!("Adding RBAC Management API endpoints");
            let rbac_backend = Arc::new(
                crate::api::rbac_management::db_backend::RbacDbBackend::new(db_pool.clone()),
            );
            let rbac_state = crate::api::RbacManagementState { db: rbac_backend };
            let rbac_router = crate::api::rbac_management_router(rbac_state);
            app = app.merge(rbac_router);
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
            app = app.layer(cors_layer_restricted(origins));
        }

        // Add request body size limit (default 1 MB — prevents memory exhaustion)
        if self.config.max_request_body_bytes > 0 {
            info!(
                max_bytes = self.config.max_request_body_bytes,
                "Request body size limit enabled"
            );
            app = app.layer(DefaultBodyLimit::max(self.config.max_request_body_bytes));
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

        app
    }

    /// Add observer-related routes to the router
    #[cfg(feature = "observers")]
    pub(super) fn add_observer_routes(&self, app: Router) -> Router {
        use crate::observers::{
            ObserverRepository, ObserverState, RuntimeHealthState, observer_routes,
            observer_runtime_routes,
        };

        // Management API (always available with feature)
        let observer_state = ObserverState {
            repository: ObserverRepository::new(
                self.db_pool.clone().expect("Pool required for observers"),
            ),
        };

        let app = app.nest("/api/observers", observer_routes(observer_state));

        // Runtime health API (only if runtime present)
        if let Some(ref runtime) = self.observer_runtime {
            info!(
                path = "/api/observers",
                "Observer management and runtime health endpoints enabled"
            );

            let runtime_state = RuntimeHealthState {
                runtime: runtime.clone(),
            };

            app.merge(observer_runtime_routes(runtime_state))
        } else {
            app
        }
    }
}
