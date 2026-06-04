//! Extension route mounting: MCP, API, RBAC, observers, storage, functions, REST,
//! realtime, and admission control.

use std::sync::Arc;

use axum::{Router, middleware};
use fraiseql_core::db::traits::DatabaseAdapter;
use tracing::{info, warn};

use super::super::{BearerAuthState, Server, api, bearer_auth_middleware};
use crate::routes::graphql::AppState;

impl<A: DatabaseAdapter + Clone + Send + Sync + 'static> Server<A> {
    /// Mount MCP, API routes, RBAC, observer hooks, storage, functions, REST,
    /// realtime, and admission control.
    pub(super) fn mount_extensions(&self, mut app: Router, state: &AppState<A>) -> Router {
        // MCP (Model Context Protocol) route
        #[cfg(feature = "mcp")]
        if let Some(ref mcp_cfg) = self.mcp_config {
            app = self.mount_mcp(app, state, mcp_cfg);
        }

        // Remaining API routes (query intelligence, federation)
        let api_router = api::routes(state.clone());
        app = app.nest("/api/v1", api_router);

        // RBAC Management API (if database pool available)
        #[cfg(feature = "observers")]
        if let Some(ref db_pool) = self.db_pool {
            app = self.mount_rbac(app, db_pool);
        }

        // Observer routes (if enabled and compiled with feature)
        #[cfg(feature = "observers")]
        {
            app = self.add_observer_routes(app);
        }

        // Object storage routes (legacy backend)
        if let Some(ref backend) = self.storage_backend {
            app = self.mount_storage_backend(app, backend);
        }

        // Edge-function routes
        #[cfg(feature = "functions")]
        {
            app = self.mount_functions(app);
        }

        // REST transport (read-only GET + SSE routes)
        #[cfg(feature = "rest")]
        {
            use crate::routes::rest::rest_query_router;
            if let Some(rest_app) = rest_query_router(state, self.config.compression_enabled) {
                app = app.merge(rest_app);
            }
        }

        // Mount realtime WebSocket routes when a RealtimeState was configured.
        if let Some(rt_state) = &self.realtime_state {
            use crate::realtime::routes::realtime_router;
            app = app.merge(realtime_router(rt_state.clone()));
            info!("Realtime WebSocket routes mounted: GET /realtime/v1");
        }

        // Mount storage routes when StorageState was pre-built during server construction.
        if let Some(ref storage_state) = self.storage_state {
            app = self.mount_storage_state(app, storage_state);
        }

        // Wire admission controller into the router via Extension.
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

        app
    }

    #[cfg(feature = "mcp")]
    fn mount_mcp(
        &self,
        mut app: Router,
        state: &AppState<A>,
        mcp_cfg: &fraiseql_core::schema::McpConfig,
    ) -> Router {
        if mcp_cfg.transport == "http" || mcp_cfg.transport == "both" {
            // SECURITY: Check require_auth flag before mounting.
            let mount_mcp = if mcp_cfg.require_auth {
                if self.oidc_validator.is_some() {
                    info!(
                        path = %mcp_cfg.path,
                        "MCP HTTP endpoint: require_auth=true, OIDC validator present. \
                         Per-request Bearer tokens are validated and tool calls fail closed \
                         without a valid security context."
                    );
                    true
                } else {
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

                let executor_swap = state.executor.clone();
                let cfg = mcp_cfg.clone();
                let validator = self.oidc_validator.clone();
                let mcp_service = StreamableHttpService::new(
                    move || {
                        let executor = executor_swap.load_full();
                        let schema = Arc::new(executor.schema().clone());
                        Ok(crate::mcp::handler::FraiseQLMcpService::new(
                            schema,
                            executor,
                            cfg.clone(),
                        )
                        .with_oidc_validator(validator.clone()))
                    },
                    Arc::new(LocalSessionManager::default()),
                    StreamableHttpServerConfig::default(),
                );
                app = app.nest_service(&mcp_cfg.path, mcp_service);
                info!(path = %mcp_cfg.path, "MCP HTTP endpoint mounted");
            }
        }
        app
    }

    #[cfg(feature = "observers")]
    fn mount_rbac(&self, mut app: Router, db_pool: &sqlx::PgPool) -> Router {
        if let Some(ref token) = self.config.admin_token {
            info!("RBAC Management API endpoints enabled (admin bearer token required)");
            let rbac_backend = Arc::new(
                crate::api::rbac_management::db_backend::RbacDbBackend::new(db_pool.clone()),
            );
            let rbac_state = crate::api::RbacManagementState { db: rbac_backend };
            let auth_state = BearerAuthState::with_max_failures(
                token.clone(),
                self.config.admin_auth_max_failures,
            );
            let rbac_router = crate::api::rbac_management_router(rbac_state)
                .route_layer(middleware::from_fn_with_state(auth_state, bearer_auth_middleware));
            app = app.merge(rbac_router);
        } else {
            tracing::error!(
                "RBAC Management API disabled — admin_token is not set. \
                 Set admin_token in server configuration to enable RBAC management endpoints."
            );
        }
        app
    }

    fn mount_storage_backend(
        &self,
        mut app: Router,
        backend: &Arc<dyn crate::storage::StorageBackend>,
    ) -> Router {
        use crate::routes::storage::{StorageRouteState, storage_router};
        let storage_state = StorageRouteState::new(backend.clone())
            .with_max_upload_bytes(self.storage_max_upload_bytes);
        let base_router = storage_router(storage_state);

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
        app
    }

    #[cfg(feature = "functions")]
    fn mount_functions(&self, mut app: Router) -> Router {
        use crate::routes::functions::{FunctionsRouteState, functions_router};

        if let (Some(ref store), Some(ref runtime)) = (&self.function_store, &self.function_runtime)
        {
            let functions_state = FunctionsRouteState {
                store:   store.clone(),
                runtime: runtime.clone(),
            };
            app = app.merge(functions_router(functions_state));
            info!("Functions endpoint enabled: POST /functions/v1/{{name}}");
        }
        app
    }

    fn mount_storage_state(
        &self,
        mut app: Router,
        storage_state: &fraiseql_storage::StorageState,
    ) -> Router {
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
                        next.run(request).await
                    }
                },
            ))
        } else {
            storage
        };
        app = app.merge(storage);
        info!("Storage API routes mounted");
        app
    }
}
