//! Base, studio, admin, introspection, metrics, and design audit routes.

use std::sync::Arc;

use axum::{
    Router, middleware,
    routing::{get, post, put},
};
use fraiseql_core::{db::traits::DatabaseAdapter, security::OidcValidator};
use tracing::{info, warn};

use super::super::{
    BearerAuthState, BroadcastState, OidcAuthState, PlaygroundState, Server, SubscriptionState,
    api, bearer_auth_middleware, broadcast_handler, health_handler, introspection_handler,
    metrics_handler, metrics_json_handler, oidc_auth_middleware, playground_handler,
    readiness_handler, subscription_handler,
};
use crate::routes::graphql::AppState;

impl<A: DatabaseAdapter + Clone + Send + Sync + 'static> Server<A> {
    /// Mount base routes (health, readiness), studio, admin API, playground,
    /// security.txt, subscriptions, broadcast, introspection, metrics, and
    /// design audit endpoints.
    #[allow(clippy::cognitive_complexity)] // Reason: many optional subsystems with feature gates
    pub(super) fn mount_base_and_admin_routes(
        &self,
        mut app: Router,
        state: &AppState<A>,
    ) -> Router {
        // Build base routes (always available without auth)
        let base_routes = Router::new()
            .route(&self.config.health_path, get(health_handler::<A>))
            .route(&self.config.readiness_path, get(readiness_handler::<A>))
            .with_state(state.clone());
        app = app.merge(base_routes);

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
                app = self.mount_studio_admin_api(app, state, token);
            }
        }

        // JWKS force-refresh — operator response to a known IdP key compromise (#361).
        // Auth-gated by the admin bearer token; mounted only when an OIDC validator is
        // configured (there is otherwise nothing to refresh).
        if self.config.admin_api_enabled {
            if let (Some(token), Some(validator)) =
                (self.config.admin_token.as_ref(), self.oidc_validator.as_ref())
            {
                app = self.mount_jwks_refresh(app, token, validator);
            }
        }

        // Conditionally add playground route (with optional independent auth).
        if self.config.playground_enabled {
            app = self.mount_playground(app, state);
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
        if self.config.subscriptions_enabled {
            app = self.mount_subscriptions(app, state);
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
            app = self.mount_introspection(app, state);
        }

        // Conditionally add metrics routes
        if self.config.metrics_enabled {
            app = self.mount_metrics(app, state);
        }

        // Conditionally add admin routes (protected by bearer token).
        if self.config.admin_api_enabled {
            app = self.mount_admin_api(app, state);
        }

        // Conditionally add design audit endpoints
        app = self.mount_design_audit(app, state);

        app
    }

    fn mount_studio_admin_api(&self, app: Router, state: &AppState<A>, token: &str) -> Router {
        use crate::routes::studio::{
            admin::{
                health_handler as studio_health_handler, schema_handler as studio_schema_handler,
            },
            auth_users::{
                invite_user_handler, list_users_handler, mfa_status_handler, revoke_user_handler,
            },
            data::{mutate_handler as data_mutate_handler, query_handler as data_query_handler},
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
                delete_object_handler, list_buckets_handler, list_objects_handler, presign_handler,
            },
        };
        let auth = BearerAuthState::with_max_failures(
            token.to_string(),
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
            .route("/admin/v1/storage/objects", axum::routing::delete(delete_object_handler::<A>))
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
        app.merge(studio_admin_router)
    }

    /// Mount `POST /admin/v1/auth/refresh-jwks` (#361): force an immediate JWKS
    /// refetch so an operator can close the stolen-key replay window the moment a
    /// key compromise is detected, instead of waiting up to `jwks_cache_ttl_secs`
    /// or restarting every replica. Gated by the admin bearer token.
    fn mount_jwks_refresh(
        &self,
        app: Router,
        token: &str,
        validator: &Arc<OidcValidator>,
    ) -> Router {
        let auth = BearerAuthState::with_max_failures(
            token.to_string(),
            self.config.admin_auth_max_failures,
        );
        let router = Router::new()
            .route(
                "/admin/v1/auth/refresh-jwks",
                post(crate::routes::jwks_admin::refresh_jwks_handler),
            )
            .route_layer(middleware::from_fn_with_state(auth, bearer_auth_middleware))
            .with_state(Arc::clone(validator));
        info!(
            "JWKS refresh endpoint mounted: POST /admin/v1/auth/refresh-jwks (admin token required)"
        );
        app.merge(router)
    }

    fn mount_playground(&self, mut app: Router, _state: &AppState<A>) -> Router {
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
                    .route_layer(middleware::from_fn_with_state(auth_state, oidc_auth_middleware))
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
        app
    }

    fn mount_subscriptions(&self, mut app: Router, state: &AppState<A>) -> Router {
        // Extract remote subscription fields from federation metadata (if enabled).
        #[cfg(feature = "federation")]
        let remote_sub_fields = self
            .executor
            .schema()
            .federation_metadata()
            .map(|m| m.remote_subscription_fields)
            .unwrap_or_default();

        // Mirror the GraphQL handler's tenant dispatch on the subscription
        // upgrade: install the Host-domain registry and drive strict cross-source
        // validation from the schema's RLS configuration (#331).
        let strict_tenant_validation = self.executor.schema().has_rls_configured();

        #[allow(unused_mut)] // Reason: `mut` is needed when the federation feature is enabled
        let mut subscription_state = SubscriptionState::new(self.subscription_manager.clone())
            .with_lifecycle(self.subscription_lifecycle.clone())
            .with_max_subscriptions(self.max_subscriptions_per_connection)
            .with_tenant_context(state.domain_registry().clone(), strict_tenant_validation)
            // #422: enforce the operation-level authorizer (if any) at subscribe-time.
            .with_authorizer(self.executor.config().authorizer.clone());

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
                    .route_layer(middleware::from_fn_with_state(auth_state, oidc_auth_middleware))
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
        app
    }

    fn mount_introspection(&self, mut app: Router, state: &AppState<A>) -> Router {
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
                    .route_layer(middleware::from_fn_with_state(auth_state, oidc_auth_middleware))
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
                    .route_layer(middleware::from_fn_with_state(auth_state, oidc_auth_middleware))
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
                    .route_layer(middleware::from_fn_with_state(auth_state, oidc_auth_middleware))
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
        app
    }

    fn mount_metrics(&self, mut app: Router, state: &AppState<A>) -> Router {
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
        app
    }

    fn mount_admin_api(&self, mut app: Router, state: &AppState<A>) -> Router {
        if let Some(ref write_token) = self.config.admin_token {
            // Destructive-operation router — always uses admin_token.
            let write_auth = BearerAuthState::with_max_failures(
                write_token.clone(),
                self.config.admin_auth_max_failures,
            );
            let admin_write_router = Router::new()
                .route("/api/v1/admin/reload-schema", post(api::admin::reload_schema_handler::<A>))
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

            // Read-only router
            let read_token = self.config.admin_readonly_token.as_ref().unwrap_or(write_token);

            if self.config.admin_readonly_token.is_none() {
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
                .route("/api/v1/admin/tenants", get(api::tenant_admin::list_tenants_handler::<A>))
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
                .route("/api/v1/admin/domains", get(api::tenant_admin::list_domains_handler::<A>))
                .route("/api/v1/query/explain", post(api::query::explain_handler::<A>))
                .route(
                    "/api/v1/admin/grafana-dashboard",
                    get(api::admin::grafana_dashboard_handler::<A>),
                )
                .route("/api/v1/admin/usage", get(api::usage::usage_handler::<A>))
                .route("/api/v1/admin/query-stats", get(api::query_stats::query_stats_handler::<A>))
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
        app
    }

    fn mount_design_audit(&self, mut app: Router, state: &AppState<A>) -> Router {
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
        app
    }
}
