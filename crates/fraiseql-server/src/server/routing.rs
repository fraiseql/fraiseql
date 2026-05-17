//! Application router construction and route registration.

#[cfg(any(feature = "auth", feature = "mcp", feature = "observers"))]
use std::sync::Arc;

use axum::{
    Router,
    extract::DefaultBodyLimit,
    middleware,
    routing::{get, post, put},
};
use fraiseql_core::db::traits::DatabaseAdapter;
use tower_http::compression::{CompressionLayer, predicate::SizeAbove};
use tracing::{info, warn};

use super::{
    AppState, BearerAuthState, BroadcastState, OidcAuthState, PlaygroundState, Server,
    SubscriptionState, api, bearer_auth_middleware, broadcast_handler, cors_layer_restricted,
    graphql_get_handler, graphql_handler, health_handler, introspection_handler, metrics_handler,
    metrics_json_handler, metrics_middleware, oidc_auth_middleware, playground_handler,
    readiness_handler, require_json_content_type, subscription_handler, trace_layer,
};
#[cfg(feature = "auth")]
use super::{AuthMeState, AuthPkceState, auth_callback, auth_me, auth_start};
#[cfg(feature = "auth")]
use crate::auth::anon_signup;
#[cfg(feature = "auth")]
use crate::auth::social::social_authorize;
#[cfg(feature = "auth")]
use crate::auth::{mfa_challenge, mfa_enroll, mfa_unenroll, mfa_verify};
use crate::middleware::{Hs256AuthState, hs256_auth_middleware};

impl<A: DatabaseAdapter + Clone + Send + Sync + 'static> Server<A> {
    /// Build application router and return the shared `AppState`.
    ///
    /// The returned `AppState` is needed by the lifecycle module for
    /// SIGUSR1 schema reload handling.
    #[allow(clippy::cognitive_complexity)] // Reason: route construction with many optional middleware layers and feature-gated endpoints
    pub(super) fn build_router(&self) -> (Router, AppState<A>) {
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

        // Studio admin dashboard — always mounted at /studio (SPA shell, no auth)
        {
            use crate::routes::studio::{studio_asset_handler, studio_handler};
            let studio_router = Router::new()
                .route("/studio", get(studio_handler))
                .route("/studio/assets/{file}", get(studio_asset_handler))
                .route("/studio/{*path}", get(studio_handler));
            info!("Studio admin dashboard mounted at /studio");
            app = app.merge(studio_router);
        }

        // Studio admin API — /admin/v1/* (protected by admin bearer token when configured)
        if self.config.admin_api_enabled {
            if let Some(ref token) = self.config.admin_token {
                use crate::routes::studio::{
                    admin::{
                        health_handler as studio_health_handler,
                        schema_handler as studio_schema_handler,
                    },
                    auth_users::{
                        invite_user_handler, list_users_handler, mfa_status_handler,
                        revoke_user_handler,
                    },
                    data::{
                        mutate_handler as data_mutate_handler, query_handler as data_query_handler,
                    },
                    function_ops::{
                        delete_secret_handler, function_logs_handler, invoke_function_handler,
                        list_functions_handler, list_secrets_handler, set_secret_handler,
                    },
                    metrics_summary::summary_handler as metrics_summary_handler,
                    realtime_monitor::{
                        broadcast_channels_handler, cdc_lag_handler, presence_rooms_handler,
                        stats_handler as realtime_stats_handler,
                    },
                    storage_browser::{
                        delete_object_handler, list_buckets_handler, list_objects_handler,
                        presign_handler,
                    },
                };
                let auth = BearerAuthState::with_max_failures(
                    token.clone(),
                    self.config.admin_auth_max_failures,
                );
                let studio_admin_router = Router::new()
                    // Schema + health
                    .route("/admin/v1/schema", get(studio_schema_handler::<A>))
                    .route("/admin/v1/health/detailed", get(studio_health_handler::<A>))
                    // Data browser
                    .route("/admin/v1/data/{entity}/query", post(data_query_handler::<A>))
                    .route("/admin/v1/data/{entity}/mutate", post(data_mutate_handler::<A>))
                    // Auth user management
                    .route("/admin/v1/users", get(list_users_handler::<A>))
                    .route("/admin/v1/users/invite", post(invite_user_handler::<A>))
                    .route("/admin/v1/users/{id}/revoke", post(revoke_user_handler::<A>))
                    .route("/admin/v1/users/{id}/mfa", get(mfa_status_handler::<A>))
                    // Storage browser
                    .route("/admin/v1/storage/buckets", get(list_buckets_handler::<A>))
                    .route("/admin/v1/storage/objects", get(list_objects_handler::<A>))
                    .route("/admin/v1/storage/objects/sign", post(presign_handler::<A>))
                    .route(
                        "/admin/v1/storage/objects",
                        axum::routing::delete(delete_object_handler::<A>),
                    )
                    // Realtime monitor
                    .route("/admin/v1/realtime/stats", get(realtime_stats_handler::<A>))
                    .route("/admin/v1/realtime/broadcast", get(broadcast_channels_handler::<A>))
                    .route("/admin/v1/realtime/presence", get(presence_rooms_handler::<A>))
                    .route("/admin/v1/realtime/cdc", get(cdc_lag_handler::<A>))
                    // Function operations
                    .route("/admin/v1/functions", get(list_functions_handler::<A>))
                    .route("/admin/v1/functions/{name}/invoke", post(invoke_function_handler::<A>))
                    .route("/admin/v1/functions/{name}/logs", get(function_logs_handler::<A>))
                    .route("/admin/v1/functions/{name}/secrets", get(list_secrets_handler::<A>))
                    .route(
                        "/admin/v1/functions/{name}/secrets/{key}",
                        put(set_secret_handler::<A>).delete(delete_secret_handler::<A>),
                    )
                    // Metrics summary
                    .route("/admin/v1/metrics/summary", get(metrics_summary_handler::<A>))
                    .route_layer(middleware::from_fn_with_state(auth, bearer_auth_middleware))
                    .with_state(state.clone());
                info!("Studio admin API mounted at /admin/v1/* (bearer token required)");
                app = app.merge(studio_admin_router);
            }
        }

        // Conditionally add playground route (with optional independent auth).
        // playground_require_auth falls back to introspection_require_auth when None.
        if self.config.playground_enabled {
            let playground_require_auth = self
                .config
                .playground_require_auth
                .unwrap_or(self.config.introspection_require_auth);

            let playground_state =
                PlaygroundState::new(self.config.graphql_path.clone(), self.config.playground_tool);

            if playground_require_auth {
                if let Some(ref validator) = self.oidc_validator {
                    info!(
                        playground_path = %self.config.playground_path,
                        playground_tool = ?self.config.playground_tool,
                        "GraphQL playground enabled (OIDC auth required)"
                    );
                    let auth_state = OidcAuthState::new(validator.clone());
                    let playground_router = Router::new()
                        .route(&self.config.playground_path, get(playground_handler))
                        .route_layer(middleware::from_fn_with_state(
                            auth_state,
                            oidc_auth_middleware,
                        ))
                        .with_state(playground_state);
                    app = app.merge(playground_router);
                } else {
                    warn!(
                        playground_path = %self.config.playground_path,
                        "playground_require_auth is true but no OIDC configured — playground disabled"
                    );
                }
            } else {
                info!(
                    playground_path = %self.config.playground_path,
                    playground_tool = ?self.config.playground_tool,
                    "GraphQL playground enabled (no auth required)"
                );
                let playground_router = Router::new()
                    .route(&self.config.playground_path, get(playground_handler))
                    .with_state(playground_state);
                app = app.merge(playground_router);
            }
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

        // Conditionally add subscription route (WebSocket).
        // subscription_require_auth falls back to introspection_require_auth when None.
        if self.config.subscriptions_enabled {
            // Extract remote subscription fields from federation metadata (if enabled).
            #[cfg(feature = "federation")]
            let remote_sub_fields = self
                .executor
                .schema()
                .federation_metadata()
                .map(|m| m.remote_subscription_fields)
                .unwrap_or_default();

            #[allow(unused_mut)] // Reason: `mut` is needed when the federation feature is enabled
            let mut subscription_state = SubscriptionState::new(self.subscription_manager.clone())
                .with_lifecycle(self.subscription_lifecycle.clone())
                .with_max_subscriptions(self.max_subscriptions_per_connection);

            #[cfg(feature = "federation")]
            if !remote_sub_fields.is_empty() {
                subscription_state =
                    subscription_state.with_remote_subscription_fields(remote_sub_fields);
            }

            let subscription_require_auth = self
                .config
                .subscription_require_auth
                .unwrap_or(self.config.introspection_require_auth);

            if subscription_require_auth {
                if let Some(ref validator) = self.oidc_validator {
                    info!(
                        subscription_path = %self.config.subscription_path,
                        "GraphQL subscriptions enabled (graphql-transport-ws + graphql-ws protocols, OIDC auth required)"
                    );
                    let auth_state = OidcAuthState::new(validator.clone());
                    let subscription_router = Router::new()
                        .route(&self.config.subscription_path, get(subscription_handler))
                        .route_layer(middleware::from_fn_with_state(
                            auth_state,
                            oidc_auth_middleware,
                        ))
                        .with_state(subscription_state);
                    app = app.merge(subscription_router);
                } else {
                    warn!(
                        subscription_path = %self.config.subscription_path,
                        "subscription_require_auth is true but no OIDC configured — subscriptions disabled"
                    );
                }
            } else {
                info!(
                    subscription_path = %self.config.subscription_path,
                    "GraphQL subscriptions enabled (graphql-transport-ws + graphql-ws protocols)"
                );
                let subscription_router = Router::new()
                    .route(&self.config.subscription_path, get(subscription_handler))
                    .with_state(subscription_state);
                app = app.merge(subscription_router);
            }
        }

        // Conditionally add broadcast endpoint
        if let Some(ref broadcast_manager) = self.broadcast_manager {
            let broadcast_state = BroadcastState::new(broadcast_manager.clone());
            info!("Broadcast endpoint enabled at /realtime/v1/broadcast");
            let broadcast_router = Router::new()
                .route("/realtime/v1/broadcast", post(broadcast_handler))
                .with_state(broadcast_state);
            app = app.merge(broadcast_router);
        }

        // Conditionally add introspection endpoint (with optional auth)
        if self.config.introspection_enabled {
            // Metadata and schema export auth can each be controlled independently.
            // When either override is None, it falls back to introspection_require_auth.
            let metadata_require_auth = self
                .config
                .metadata_require_auth
                .unwrap_or(self.config.introspection_require_auth);
            let schema_export_require_auth = self
                .config
                .schema_export_require_auth
                .unwrap_or(self.config.introspection_require_auth);

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
                            auth_state,
                            oidc_auth_middleware,
                        ))
                        .with_state(state.clone());
                    app = app.merge(introspection_router);
                } else {
                    warn!(
                        "introspection_require_auth is true but no OIDC configured - introspection disabled"
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
            }

            // Mount schema export endpoints with independent auth control.
            if schema_export_require_auth {
                if let Some(ref validator) = self.oidc_validator {
                    info!("Schema export endpoints enabled (OIDC auth required)");
                    let auth_state = OidcAuthState::new(validator.clone());
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
                        "schema_export_require_auth is true but no OIDC configured - schema export disabled"
                    );
                }
            } else {
                info!("Schema export endpoints enabled (no auth required)");
                let schema_router = Router::new()
                    .route("/api/v1/schema.graphql", get(api::schema::export_sdl_handler::<A>))
                    .route("/api/v1/schema.json", get(api::schema::export_json_handler::<A>))
                    .with_state(state.clone());
                app = app.merge(schema_router);
            }

            // Mount metadata endpoint with independent auth control
            if metadata_require_auth {
                if let Some(ref validator) = self.oidc_validator {
                    info!("Schema metadata endpoint enabled (OIDC auth required)");
                    let auth_state = OidcAuthState::new(validator.clone());
                    let metadata_router = Router::new()
                        .route("/api/v1/schema/metadata", get(api::metadata::metadata_handler::<A>))
                        .route_layer(middleware::from_fn_with_state(
                            auth_state,
                            oidc_auth_middleware,
                        ))
                        .with_state(state.clone());
                    app = app.merge(metadata_router);
                } else {
                    warn!(
                        "metadata_require_auth is true but no OIDC configured - metadata endpoint disabled"
                    );
                }
            } else {
                info!("Schema metadata endpoint enabled (no auth required)");
                let metadata_router = Router::new()
                    .route("/api/v1/schema/metadata", get(api::metadata::metadata_handler::<A>))
                    .with_state(state.clone());
                app = app.merge(metadata_router);
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

                let auth_state = BearerAuthState::with_max_failures(
                    token.clone(),
                    self.config.admin_auth_max_failures,
                );

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
                let write_auth = BearerAuthState::with_max_failures(
                    write_token.clone(),
                    self.config.admin_auth_max_failures,
                );
                let admin_write_router = Router::new()
                    .route(
                        "/api/v1/admin/reload-schema",
                        post(api::admin::reload_schema_handler::<A>),
                    )
                    .route("/api/v1/admin/cache/clear", post(api::admin::cache_clear_handler::<A>))
                    .route(
                        "/api/v1/admin/query-stats/reset",
                        post(api::query_stats::query_stats_reset_handler::<A>),
                    )
                    // Tenant management write endpoints (multi-tenant mode)
                    .route(
                        "/api/v1/admin/tenants/{key}",
                        put(api::tenant_admin::upsert_tenant_handler::<A>)
                            .delete(api::tenant_admin::delete_tenant_handler::<A>),
                    )
                    // Tenant lifecycle endpoints (multi-tenant mode)
                    .route(
                        "/api/v1/admin/tenants/{key}/suspend",
                        post(api::tenant_admin::suspend_tenant_handler::<A>),
                    )
                    .route(
                        "/api/v1/admin/tenants/{key}/resume",
                        post(api::tenant_admin::resume_tenant_handler::<A>),
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

                let read_auth = BearerAuthState::with_max_failures(
                    read_token.clone(),
                    self.config.admin_auth_max_failures,
                );
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
                    .route(
                        "/api/v1/admin/tenants/{key}/events",
                        get(api::tenant_admin::tenant_events_handler::<A>),
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
                    .route("/api/v1/admin/usage", get(api::usage::usage_handler::<A>))
                    .route(
                        "/api/v1/admin/query-stats",
                        get(api::query_stats::query_stats_handler::<A>),
                    )
                    .route(
                        "/api/v1/admin/query-stats/{queryid}",
                        get(api::query_stats::query_stats_detail_handler::<A>),
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

        // Unified social login entry point — mounted when social_login is configured.
        //
        // `GET /auth/v1/authorize?provider=<name>` looks up the named provider in the
        // registry and redirects the user to that provider's authorization URL with a
        // CSRF state token.
        #[cfg(feature = "auth")]
        if let Some(ref social) = self.social_login {
            let social_router = Router::new()
                .route("/auth/v1/authorize", get(social_authorize))
                .with_state(Arc::clone(social));
            app = app.merge(social_router);
            info!(
                providers = ?social.registry.names(),
                "Social login route mounted: GET /auth/v1/authorize"
            );
        }

        // Anonymous session signup — mounted when anon_signup_state is configured.
        //
        // POST /auth/v1/signup — issue a guest session (anon_ user_id, 7-day TTL)
        // Rate-limited per client IP; requires the server to be started with
        // into_make_service_with_connect_info (which the lifecycle module does).
        #[cfg(feature = "auth")]
        if let Some(ref anon) = self.anon_signup_state {
            let anon_router = Router::new()
                .route("/auth/v1/signup", post(anon_signup))
                .with_state(Arc::clone(anon));
            app = app.merge(anon_router);
            info!("Anonymous signup route mounted: POST /auth/v1/signup");
        }

        // TOTP MFA endpoints — mounted when mfa_state is configured.
        //
        // POST /auth/v1/mfa/enroll    — begin enrollment
        // POST /auth/v1/mfa/confirm   — confirm with first live TOTP code
        // POST /auth/v1/mfa/challenge — issue short-lived challenge token
        // POST /auth/v1/mfa/verify    — verify code and issue session
        // POST /auth/v1/mfa/unenroll  — remove MFA from an account
        #[cfg(feature = "auth")]
        if let Some(ref mfa) = self.mfa_state {
            let mfa_router = Router::new()
                .route("/auth/v1/mfa/enroll", post(mfa_enroll))
                .route("/auth/v1/mfa/challenge", post(mfa_challenge))
                .route("/auth/v1/mfa/verify", post(mfa_verify))
                .route("/auth/v1/mfa/unenroll", post(mfa_unenroll))
                .with_state(Arc::clone(mfa));
            app = app.merge(mfa_router);
            info!(
                "TOTP MFA routes mounted: POST /auth/v1/mfa/{{enroll,challenge,verify,unenroll}}"
            );
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
            let me_state = Arc::new(AuthMeState {
                expose_claims: me_cfg.expose_claims.clone(),
            });
            let auth_state = OidcAuthState::new(Arc::clone(validator));
            let me_router = Router::new()
                .route("/auth/me", get(auth_me))
                .route_layer(middleware::from_fn_with_state(auth_state, oidc_auth_middleware))
                .with_state(me_state);
            app = app.merge(me_router);
            info!(
                expose_claims = ?me_cfg.expose_claims,
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
                let auth_state = BearerAuthState::with_max_failures(
                    token.clone(),
                    self.config.admin_auth_max_failures,
                );
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

        // Object storage routes — mounted when a backend has been attached via
        // `Server::with_storage`. Provides upload, download, delete, and presigned
        // URL endpoints at `/storage/v1/object/{*key}`.
        if let Some(ref backend) = self.storage_backend {
            use crate::routes::storage::{StorageRouteState, storage_router};
            let storage_state = StorageRouteState::new(backend.clone())
                .with_max_upload_bytes(self.storage_max_upload_bytes);
            let base_router = storage_router(storage_state);

            // When `storage_token` is set, protect all storage routes with bearer auth.
            let storage_app = if let Some(ref token) = self.config.storage_token {
                info!(
                    max_upload_mib = self.storage_max_upload_bytes / (1024 * 1024),
                    "Storage API mounted at /storage/v1/ (bearer token required)"
                );
                let auth_state = BearerAuthState::with_max_failures(
                    token.clone(),
                    self.config.admin_auth_max_failures,
                );
                base_router
                    .route_layer(middleware::from_fn_with_state(auth_state, bearer_auth_middleware))
            } else {
                warn!(
                    max_upload_mib = self.storage_max_upload_bytes / (1024 * 1024),
                    "Storage API mounted at /storage/v1/ (no authentication — \
                     set storage_token in config for production)"
                );
                base_router
            };
            app = app.merge(storage_app);
        }

        // Edge-function routes (/functions/v1/) — mounted when store + runtime are configured.
        #[cfg(feature = "functions")]
        {
            use crate::routes::functions::{FunctionsRouteState, functions_router};

            if let (Some(ref store), Some(ref runtime)) =
                (&self.function_store, &self.function_runtime)
            {
                let functions_state = FunctionsRouteState {
                    store:   store.clone(),
                    runtime: runtime.clone(),
                };
                app = app.merge(functions_router(functions_state));
                info!("Functions endpoint enabled: POST /functions/v1/{{name}}");
            }
        }

        // REST transport (read-only GET + SSE routes).
        //
        // Uses `rest_query_router` which does not require `SupportsMutations`,
        // keeping the server generic over all `DatabaseAdapter` implementations.
        // Full CRUD REST (POST/PUT/PATCH/DELETE) requires `SupportsMutations`
        // and is available via `rest_router` for adapters that support mutations.
        #[cfg(feature = "rest")]
        {
            use crate::routes::rest::rest_query_router;
            if let Some(rest_app) = rest_query_router(&state, self.config.compression_enabled) {
                app = app.merge(rest_app);
            }
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

        // Mount realtime WebSocket routes when a RealtimeState was configured.
        //
        // The realtime endpoint is unauthenticated at the HTTP layer — the
        // WebSocket handler validates the `?token=` query parameter internally
        // before upgrading. No middleware layer is needed here.
        if let Some(rt_state) = &self.realtime_state {
            use crate::realtime::routes::realtime_router;
            app = app.merge(realtime_router(rt_state.clone()));
            info!("Realtime WebSocket routes mounted: GET /realtime/v1");
        }

        // Mount storage routes when StorageState was pre-built during server construction.
        // Auth is OPTIONAL for storage: public buckets allow anonymous access, so we never
        // reject requests for missing tokens. If a token is present (Bearer header or
        // __Host-access_token cookie), we validate it and map to StorageUser; if absent,
        // the request continues unauthenticated and handlers rely on RLS to enforce access.
        if let Some(ref storage_state) = self.storage_state {
            let storage = fraiseql_storage::storage_router(storage_state.clone());
            let storage = if let Some(ref validator) = self.oidc_validator {
                let validator = validator.clone();
                storage.layer(middleware::from_fn(
                    move |mut request: axum::extract::Request, next: axum::middleware::Next| {
                        let validator = validator.clone();
                        async move {
                            use axum::http::header;
                            use axum::response::IntoResponse;
                            use crate::middleware::oidc_auth::extract_access_token_cookie;

                            // Extract token: Bearer header takes precedence over cookie.
                            let token = request
                                .headers()
                                .get(header::AUTHORIZATION)
                                .and_then(|v| v.to_str().ok())
                                .and_then(|v| v.strip_prefix("Bearer "))
                                .map(str::to_owned)
                                .or_else(|| extract_access_token_cookie(request.headers()));

                            if let Some(token) = token {
                                match validator.validate_token(&token).await {
                                    Ok(user) => {
                                        let storage_user = fraiseql_storage::StorageUser {
                                            user_id: Some(user.user_id.to_string()),
                                            roles: user.scopes,
                                        };
                                        request.extensions_mut().insert(storage_user);
                                    }
                                    Err(e) => {
                                        tracing::debug!(error = %e, "Storage auth: token validation failed");
                                        return (
                                            axum::http::StatusCode::UNAUTHORIZED,
                                            "Invalid or expired token",
                                        )
                                            .into_response();
                                    }
                                }
                            }
                            // No token → continue without StorageUser (anonymous access).
                            // Handlers check RLS and return 401 for private buckets.
                            next.run(request).await
                        }
                    },
                ))
            } else {
                storage
            };
            app = app.merge(storage);
            info!("Storage API routes mounted");
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
