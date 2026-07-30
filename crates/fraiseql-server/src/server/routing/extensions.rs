//! Extension route mounting: MCP, API, RBAC, observers, storage, functions, REST,
//! and admission control.

use axum::{Router, middleware};
use fraiseql_core::db::traits::DatabaseAdapter;
use tracing::info;

use super::super::{BearerAuthState, Server, api, bearer_auth_middleware};
#[cfg(feature = "rest")]
use super::AuthPosture;
use crate::routes::graphql::AppState;

impl<A: DatabaseAdapter + Clone + Send + Sync + 'static> Server<A> {
    /// Mount MCP, API routes, RBAC, observer hooks, storage, functions, REST,
    /// and admission control.
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

        // Identity-cache admin API (flush) — same admin bearer gate as RBAC,
        // mounted only when an enrichment resolver exists (#539). Lets an operator
        // propagate a revoke/provision immediately instead of waiting out the TTL.
        #[cfg(feature = "auth")]
        if let (Some(resolver), Some(token)) =
            (state.identity_resolver.as_ref(), self.config.admin_token.as_ref())
        {
            let auth_state = BearerAuthState::with_max_failures(
                token.clone(),
                self.config.admin_auth_max_failures,
            );
            let identity_router = crate::identity::identity_admin_router(resolver.clone())
                .route_layer(middleware::from_fn_with_state(auth_state, bearer_auth_middleware));
            app = app.merge(identity_router);
            info!(
                "Identity-cache admin API enabled (POST /api/identity/flush[-all]; admin bearer \
                 token required)"
            );
        }

        // Suppression admin API (append + query) — the operator surface for manual
        // do-not-contact entries (support removals, GDPR requests). Same admin
        // bearer gate; mounted only when a database pool, the admin token, and the
        // address-hash key (the server HMAC secret) are all present, since the
        // address must be hashed server-side before it touches the store.
        #[cfg(feature = "inbound-email")]
        if let (Some(pool), Some(token), Some(key)) = (
            self.db_pool.as_ref(),
            self.config.admin_token.as_ref(),
            self.build_address_hash_key(),
        ) {
            let tracker =
                std::sync::Arc::new(crate::inbound::email::PgSendTracker::new(pool.clone()));
            let suppression_state = std::sync::Arc::new(
                crate::inbound::email::SuppressionAdminState::new(tracker, key),
            );
            let auth_state = BearerAuthState::with_max_failures(
                token.clone(),
                self.config.admin_auth_max_failures,
            );
            let suppression_router = crate::inbound::email::suppression_admin_router(
                suppression_state,
            )
            .route_layer(middleware::from_fn_with_state(auth_state, bearer_auth_middleware));
            app = app.merge(suppression_router);
            info!(
                "Suppression admin API enabled (POST /api/email/suppress, POST \
                 /api/email/suppression; admin bearer token required)"
            );
        }

        // Observer routes (if enabled and compiled with feature)
        #[cfg(feature = "observers")]
        {
            app = self.add_observer_routes(app);
        }

        // Object storage routes (legacy backend)

        // Edge-function routes
        #[cfg(feature = "functions")]
        {
            app = self.mount_functions(app);
        }

        // Inbound webhook receiver (POST /webhooks/{provider})
        #[cfg(feature = "inbound")]
        {
            app = self.add_inbound_routes(app, state);
        }

        // REST transport.
        //
        // #812: this merge previously attached no authentication at all, so every
        // `/rest/v1/**` request reached the handlers with `security_context = None` —
        // disabling RLS-policy evaluation and session-variable tenant stamping — even
        // when the caller presented a valid bearer token. `route_layer` does not
        // propagate across `Router::merge`, so the layer must go on the REST router
        // itself, before it is merged.
        //
        // #865: the write half is mounted here too, when the boot path installed a
        // `rest_router_builder` — which it can only do for an adapter that implements
        // `SupportsMutations`. Read and write share this **one** mount site precisely so
        // they cannot diverge in auth posture: whichever router comes back goes through
        // the same `attach_auth` call below. `rest_router` previously had no production
        // caller at all, so every write path the served OpenAPI advertised answered 405.
        #[cfg(feature = "rest")]
        {
            use crate::routes::rest::rest_query_router;
            // Whether an auth layer will actually be attached below. Passed into the
            // router so the served OpenAPI advertises the security the server enforces
            // rather than a static template (#810).
            let authenticated = self.oidc_validator.is_some() || self.hs256_auth.is_some();
            let mount = crate::routes::rest::RestMountConfig {
                compression_enabled: self.config.compression_enabled,
                auth_layer_attached: authenticated,
                // #917: the operator's `[export]` table finally reaches the transport.
                // Every consumer used to build its own `ExportConfig::default()`.
                export:              std::sync::Arc::new(self.config.export.clone()),
            };
            let rest_app = match self.rest_router_builder.as_ref() {
                Some(build_with_writes) => build_with_writes(state, &mount),
                None => rest_query_router(state, &mount),
            };
            if let Some(rest_app) = rest_app {
                app = app.merge(self.attach_auth(rest_app, AuthPosture::Authenticated, "rest"));
            }
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
                "Admission control enabled — requests above the concurrency limit receive 503"
            );
            // A real gate, not an `Extension`. Inserting the controller into the request
            // extension map stored a value that nothing read back out, so the limit was
            // never applied while the log said it was (#860). The Extension stays for
            // embedders that inspect the controller.
            app = app
                .layer(axum::middleware::from_fn_with_state(
                    Arc::clone(&controller),
                    crate::resilience::admission_middleware,
                ))
                .layer(Extension(controller));
        }

        app
    }

    /// Mount the inbound webhook receiver at `POST /webhooks/{provider}`.
    ///
    /// Requires a database pool (the receiver pipeline is Postgres-backed) and at
    /// least one `[webhooks.*]` route; otherwise the receiver is not mounted. The
    /// spine and idempotency tables are created at startup in
    /// [`serve_with_shutdown`](Server::serve_with_shutdown). The function-dispatch
    /// hooks from `state` are attached so a persisted message fires its
    /// `after:ingest` functions.
    #[cfg(feature = "inbound")]
    fn add_inbound_routes(&self, app: Router, state: &AppState<A>) -> Router {
        let Some(ref db_pool) = self.db_pool else {
            if !self.config.webhooks.is_empty() {
                tracing::error!(
                    "Inbound webhook routes NOT mounted — a database pool is required but none is configured"
                );
            }
            return app;
        };
        if self.config.webhooks.is_empty() {
            return app;
        }

        let mut inbound_state = crate::inbound::WebhookInboundState::new(
            db_pool.clone(),
            &self.config.webhooks,
            |name| std::env::var(name).ok(),
        );
        if let Some(ref hooks) = state.before_mutation_hooks {
            inbound_state = inbound_state.with_hooks(std::sync::Arc::clone(hooks));
            // #594: thread the request-path executor factory so after:ingest
            // functions can `fraiseql_query` write back under their `run_as` ceiling
            // — the same factory the after:mutation route handlers use.
            inbound_state = inbound_state.with_query_executor_factory(
                crate::routes::after_mutation::make_query_executor_factory(state.executor.clone()),
            );
        }
        info!(
            routes = self.config.webhooks.len(),
            "Inbound webhook routes mounted at POST /webhooks/{{provider}}"
        );
        app.merge(crate::inbound::webhook_router(inbound_state))
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
                tracing::warn!(
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

                // The session is built from the whole `AppState`, so an MCP tool
                // call reaches the same tenant registry, domain registry and error
                // sanitizer the `/graphql` handler does (#858).
                let session_state = state.clone();
                let cfg = mcp_cfg.clone();
                let validator = self.oidc_validator.clone();
                let mcp_service = StreamableHttpService::new(
                    move || {
                        Ok(crate::mcp::handler::FraiseQLMcpService::new(
                            session_state.clone(),
                            cfg.clone(),
                        )
                        .with_oidc_validator(validator.clone()))
                    },
                    std::sync::Arc::new(LocalSessionManager::default()),
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
            let rbac_backend = std::sync::Arc::new(
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
        // Authentication is applied when EITHER a static `storage_token` is set OR
        // an OIDC validator is configured. For each request the bearer token (or
        // `__Host-access_token` cookie) is resolved as follows:
        //   1. matches `storage_token` (constant-time) → admin `StorageUser`;
        //   2. else, an OIDC validator present → validate (401 on failure), populating a per-user
        //      `StorageUser` for RLS;
        //   3. else (token-only mode), a non-matching token → 401.
        // A request with no token is left anonymous: RLS then permits only
        // PublicRead reads.
        let storage_token = self.config.storage_token.clone();
        let validator = self.oidc_validator.clone();

        // Fail closed (M-storage-legacy): with neither a storage_token nor an OIDC
        // validator there is no way to authenticate a caller, so no request could
        // ever carry an identity for RLS to scope. Refuse to mount rather than
        // expose an anonymous-only storage API by default.
        if storage_token.is_none() && validator.is_none() {
            tracing::error!(
                "SECURITY: storage API NOT mounted — neither storage_token nor an OIDC validator \
                 is configured, so no caller can be authenticated. Configure storage_token or an \
                 OIDC validator to enable the storage API."
            );
            return app;
        }

        let storage = fraiseql_storage::storage_router(storage_state.clone()).layer(
            middleware::from_fn(
                move |mut request: axum::extract::Request, next: axum::middleware::Next| {
                    let storage_token = storage_token.clone();
                    let validator = validator.clone();
                    async move {
                        use axum::http::{StatusCode, header};
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
                            if let Some(user) = storage_admin_user(&token, storage_token.as_deref())
                            {
                                request.extensions_mut().insert(user);
                            } else if let Some(ref validator) = validator {
                                match validator.validate_token(&token).await {
                                    Ok(user) => {
                                        let storage_user = fraiseql_storage::StorageUser {
                                            user_id: Some(user.user_id.to_string()),
                                            roles:   user.scopes,
                                        };
                                        request.extensions_mut().insert(storage_user);
                                    },
                                    Err(e) => {
                                        tracing::debug!(error = %e, "Storage auth: token validation failed");
                                        return (
                                            StatusCode::UNAUTHORIZED,
                                            "Invalid or expired token",
                                        )
                                            .into_response();
                                    },
                                }
                            } else {
                                // Token-only mode (no OIDC validator); the layer is
                                // mounted only when `storage_token` is set, so a
                                // non-matching token is a rejected admin attempt.
                                tracing::debug!("Storage auth: bearer did not match storage_token");
                                return (StatusCode::UNAUTHORIZED, "Invalid storage token")
                                    .into_response();
                            }
                        }
                        next.run(request).await
                    }
                },
            ),
        );
        app = app.merge(storage);
        info!("Storage API routes mounted at /storage/v1/");
        app
    }
}

/// Map a presented bearer token to an admin [`fraiseql_storage::StorageUser`]
/// when it matches the configured static `storage_token`.
///
/// The comparison is constant-time. Returns `None` when no `storage_token` is
/// configured, the configured token is empty, or the presented token does not
/// match — in which case the caller falls back to OIDC validation or rejects
/// the request. The admin user carries the storage-admin role
/// ([`fraiseql_storage::STORAGE_ADMIN_ROLE`]) recognised by the storage RLS
/// evaluator, granting full access regardless of bucket ownership.
fn storage_admin_user(
    presented: &str,
    configured: Option<&str>,
) -> Option<fraiseql_storage::StorageUser> {
    let configured = configured?;
    // Reject an empty configured token outright so a misconfigured
    // `storage_token = ""` cannot grant admin to a bare `Authorization: Bearer`.
    if configured.is_empty() {
        return None;
    }
    if crate::middleware::auth::constant_time_compare(presented, configured) {
        Some(fraiseql_storage::StorageUser {
            user_id: Some("storage-admin".to_string()),
            // Grant the explicit storage-admin role, NOT the generic `"admin"`,
            // so it stays in lockstep with the storage RLS evaluator (M-storage-scope).
            roles:   vec![fraiseql_storage::STORAGE_ADMIN_ROLE.to_string()],
        })
    } else {
        None
    }
}

#[cfg(test)]
mod tests;
