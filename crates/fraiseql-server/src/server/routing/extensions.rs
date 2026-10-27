//! Extension route mounting: the core admin APIs, every capability's routes, storage,
//! and admission control.

use axum::{Router, middleware};
use tracing::info;

use super::super::{BearerAuthState, Server, bearer_auth_middleware};
use crate::{capability::REGISTRY, routes::graphql::AppState};

impl Server {
    /// Mount the core admin APIs, every capability's routes, storage, and admission
    /// control.
    pub(super) fn mount_extensions(&self, mut app: Router, state: &AppState) -> Router {
        // Async operations (#391): mounted only when `[async_operations]` is
        // configured (boot already refused a config with no usable table), and
        // behind the deployment's auth layer like every data-serving transport
        // (#812's single-seam rule). The handlers additionally hard-require an
        // authenticated principal — a submission snapshots the caller's
        // security context for the background execution.
        if let Some(ref runtime) = self.async_operations {
            let ops_router = crate::routes::async_operations::router(
                crate::routes::async_operations::AsyncOperationsState {
                    runtime: runtime.clone(),
                    app:     state.clone(),
                },
            );
            app = app.merge(self.attach_auth(
                ops_router,
                super::AuthPosture::Authenticated,
                "async-operations",
            ));
            info!("Async-operations endpoints mounted (/operations/v1)");
        }

        // Tenant admin credentials (#1089): minted and revoked by the deployment
        // administrator only, so behind the platform bearer gate, where a tenant admin token
        // is just a wrong token.
        if let (Some(token), Some(tokens)) = (self.config.admin_token.as_ref(), &self.admin_tokens)
        {
            let auth_state =
                BearerAuthState::with_limiter(token.clone(), self.admin_failure_limiter.clone());
            let router =
                crate::api::admin_token_management_router(crate::api::AdminTokenManagementState {
                    tokens: tokens.clone(),
                })
                .route_layer(middleware::from_fn_with_state(auth_state, bearer_auth_middleware));
            app = app.merge(router);
            info!("Tenant admin credentials managed at /api/admin-tokens (admin bearer token)");
        }

        // RBAC Management API: core, on every build with the admin token and a pool.
        app = self.mount_rbac(app);

        // API-key management (#627) — mounted only when the authenticator is
        // Postgres-backed, behind the same admin bearer gate as RBAC. Operates
        // on the SAME store the authenticator reads.
        if let Some(store) = self.api_key_authenticator.as_ref().and_then(|a| a.postgres_store()) {
            if let Some(ref token) = self.config.admin_token {
                info!("API-key management endpoints enabled (admin bearer token required)");
                let mgmt_state = crate::api::ApiKeyManagementState {
                    store: std::sync::Arc::new(store.clone()),
                };
                let auth_state = BearerAuthState::with_limiter(
                    token.clone(),
                    self.admin_failure_limiter.clone(),
                );
                let mgmt_router = crate::api::api_key_management_router(mgmt_state).route_layer(
                    middleware::from_fn_with_state(auth_state, bearer_auth_middleware),
                );
                app = app.merge(mgmt_router);
            } else {
                tracing::error!(
                    "API-key management disabled — [security.api_keys] storage = \"postgres\" \
                     is active but admin_token is not set. Keys can be authenticated but not \
                     managed over HTTP; set admin_token to enable the management endpoints."
                );
            }
        }

        // SCIM 2.0 provisioning (#946). Two surfaces, two credentials, on purpose: the
        // `/scim/v2/*` routes take a provisioning token that grants provisioning and
        // nothing else, while `/api/scim/tokens` — which mints those credentials — sits
        // behind the admin bearer.
        #[cfg(feature = "auth")]
        if let (Some(scim_cfg), Some(db_pool), Some(rbac)) =
            (self.config.scim.as_ref(), self.enrichment_pool.as_ref(), self.rbac.as_ref())
        {
            if scim_cfg.enabled {
                if let Some(ref token) = self.config.admin_token {
                    let tokens = std::sync::Arc::new(fraiseql_auth::scim::PgScimTokenStore::new(
                        db_pool.clone(),
                    ));
                    let scim_state = crate::api::ScimState {
                        pool:          db_pool.clone(),
                        tokens:        tokens.clone(),
                        session_store: std::sync::Arc::new(
                            fraiseql_auth::PostgresSessionStore::new(db_pool.clone()),
                        ),
                        rbac:          rbac.clone(),
                        base_url:      scim_cfg.base_url.clone(),
                    };
                    app = app.merge(crate::api::scim_router(scim_state));

                    // Accepts tenant administrators (#1089); the router enforces their tenant.
                    let auth_state = self.admin_principal_state(token);
                    let token_router = crate::api::scim_token_management_router(
                        crate::api::ScimTokenManagementState { tokens },
                    )
                    .route_layer(middleware::from_fn_with_state(
                        auth_state,
                        crate::middleware::admin_principal_middleware,
                    ));
                    app = app.merge(token_router);
                    info!(
                        "SCIM 2.0 provisioning mounted at /scim/v2 (provisioning bearer token); \
                         credentials managed at /api/scim/tokens (admin bearer token)"
                    );
                } else {
                    // validate() refuses this combination; an embedder that skips it lands
                    // here and must not get a provisioning surface with no way to issue a
                    // credential for it.
                    tracing::error!(
                        "SCIM disabled — [scim] enabled = true but admin_token is not set, so \
                         no provisioning credential could ever be minted."
                    );
                }
            }
        }

        // Every capability's routes (MCP, the SAML IdP admin API, the identity-cache flush,
        // the email suppression API, the observer admin API, the inbound webhook receiver,
        // REST), before the admission layer below, which wraps whatever is merged by then.
        app = self.mount_capability_routes(app, state);

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

    /// Merge the routes of every compiled-in capability that has some, recording each
    /// one `/health` reports as serving.
    fn mount_capability_routes(&self, mut app: Router, state: &AppState) -> Router {
        let cx = crate::server::capabilities::ServeCx {
            server: self,
            app:    state,
        };
        for registration in REGISTRY.iter().filter(|registration| registration.compiled_in) {
            if let Some(routes) = registration.capability.routes(&cx) {
                app = app.merge(routes);
                self.capability_starts.record_serving(registration.meta.name);
            }
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
    pub(in crate::server) fn add_inbound_routes(&self, app: Router, state: &AppState) -> Router {
        let Some(ref db_pool) = self.db_pool else {
            if !self.webhook_routes.is_empty() {
                tracing::error!(
                    "Inbound webhook routes NOT mounted — a database pool is required but none is configured"
                );
            }
            return app;
        };
        if self.webhook_routes.is_empty() {
            return app;
        }

        // #1321: the routes were built and validated in the builder, where a bad
        // configuration could still stop the boot. This mounts what was built —
        // there is no second construction from the same config, so nothing here can
        // disagree with what boot accepted, and nothing here can fail.
        let mut inbound_state = crate::inbound::WebhookInboundState::new(
            db_pool.clone(),
            &self.webhook_routes,
            |name| std::env::var(name).ok(),
        );
        // #1175: validated in `provision_persistent_schemas`, so the error arm is unreachable.
        if let Ok(settings) = self.config.inbound_dispatch.settings() {
            inbound_state = inbound_state.with_dispatch_lease(settings.lease);
        }
        if let Some(ref hooks) = state.before_mutation_hooks {
            inbound_state = inbound_state.with_hooks(std::sync::Arc::clone(hooks));
            // #594: thread the request-path executor factory so after:ingest
            // functions can `fraiseql_query` write back under their `run_as` ceiling
            // — the same factory the after:mutation route handlers use.
            inbound_state = inbound_state.with_query_executor_factory(
                crate::routes::after_mutation::make_query_executor_factory(
                    crate::query_bridge::ExecutorSource::of(state),
                ),
            );
        }
        info!(
            routes = self.webhook_routes.len(),
            "Inbound webhook routes mounted at POST /webhooks/{{provider}}"
        );
        app.merge(crate::inbound::webhook_router(inbound_state))
    }

    #[cfg(feature = "mcp")]
    pub(in crate::server) fn mount_mcp(
        &self,
        mut app: Router,
        state: &AppState,
        mcp_cfg: &fraiseql_core::schema::McpConfig,
    ) -> Router {
        if mcp_cfg.transport.serves_http() {
            // The same two auth modes `/graphql` accepts (#376 auth parity):
            // OIDC (`[auth]`) or local HS256 (`[auth_hs256]`). Before this, an
            // HS256-only deployment could never authenticate an MCP call and
            // `require_auth = true` refused to mount the endpoint entirely.
            let validator = self
                .oidc_validator
                .clone()
                .map(crate::mcp::handler::McpTokenValidator::Oidc)
                .or_else(|| {
                    self.hs256_auth.clone().map(crate::mcp::handler::McpTokenValidator::Hs256)
                });

            // SECURITY: Check require_auth flag before mounting.
            let mount_mcp = if mcp_cfg.require_auth {
                if let Some(ref v) = validator {
                    info!(
                        path = %mcp_cfg.path,
                        validator = ?v,
                        "MCP HTTP endpoint: require_auth=true, token validator present. \
                         Per-request Bearer tokens are validated and tool calls fail closed \
                         without a valid security context."
                    );
                    true
                } else {
                    tracing::error!(
                        path = %mcp_cfg.path,
                        "MCP HTTP endpoint NOT mounted — require_auth=true but neither an \
                         OIDC validator ([auth]) nor an HS256 validator ([auth_hs256]) is \
                         configured. Configure one, or set require_auth=false (development \
                         only)."
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
                use rmcp::transport::streamable_http_server::session::local::LocalSessionManager;

                // The session is built from the whole `AppState`, so an MCP tool
                // call reaches the same tenant registry, domain registry and error
                // sanitizer the `/graphql` handler does (#858).
                let session_state = state.clone();
                let cfg = mcp_cfg.clone();
                let oidc = self.oidc_validator.clone();
                let hs256 = self.hs256_auth.clone();
                // The `[session_state]` store, for per-thread continuity (#967).
                // Handed over unconditionally; `[mcp] session_state` decides
                // whether the service uses it, so the two switches stay separate:
                // having a store is the deployment's, using it for MCP is the
                // operator's.
                #[cfg(feature = "auth")]
                let session_store = self.session_state.clone();
                let mcp_service = crate::mcp::http::streamable_http_service(
                    move || {
                        let validator =
                            oidc.clone().map(crate::mcp::handler::McpTokenValidator::Oidc).or_else(
                                || hs256.clone().map(crate::mcp::handler::McpTokenValidator::Hs256),
                            );
                        let service = crate::mcp::handler::FraiseQLMcpService::new(
                            session_state.clone(),
                            cfg.clone(),
                        )
                        .with_token_validator(validator);
                        #[cfg(feature = "auth")]
                        let service = service.with_session_state(session_store.clone());
                        service
                    },
                    std::sync::Arc::new(LocalSessionManager::default()),
                    mcp_cfg.require_auth,
                );
                app = app.nest_service(&mcp_cfg.path, mcp_service);
                info!(path = %mcp_cfg.path, "MCP HTTP endpoint mounted");
            }
        }
        app
    }

    /// The gate for admin routers that accept tenant administrators (#1089): `admin_token`
    /// as the platform, plus tenant admin tokens when a database pool exists.
    pub(in crate::server) fn admin_principal_state(
        &self,
        admin_token: &str,
    ) -> crate::middleware::AdminPrincipalState {
        crate::middleware::AdminPrincipalState::with_limiter(
            admin_token.to_string(),
            self.admin_tokens.clone(),
            self.admin_failure_limiter.clone(),
        )
    }

    fn mount_rbac(&self, mut app: Router) -> Router {
        // `rbac` is built whenever the admin token and a pool are set (#1632).
        if let (Some(token), Some(rbac)) = (self.config.admin_token.as_ref(), self.rbac.as_ref()) {
            info!("RBAC Management API endpoints enabled (admin bearer token required)");
            let rbac_state = crate::api::RbacManagementState { db: rbac.clone() };
            // Accepts tenant administrators (#1089); the router enforces their tenant.
            let auth_state = self.admin_principal_state(token);
            let rbac_router = crate::api::rbac_management_router(rbac_state).route_layer(
                middleware::from_fn_with_state(
                    auth_state,
                    crate::middleware::admin_principal_middleware,
                ),
            );
            app = app.merge(rbac_router);
        } else if self.config.admin_token.is_some() {
            tracing::warn!(
                "RBAC Management API not mounted: admin_token is set but the server has no \
                 database pool to hold role assignments."
            );
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
        // GHSA-7482-9g53-rj7f: the store both HTTP auth middlewares check.
        let revocation = self.revocation_manager.clone();

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
                    let revocation = revocation.clone();
                    async move {
                        use axum::http::{StatusCode, header};
                        use axum::response::IntoResponse;
                        use crate::middleware::oidc_auth::extract_access_token_cookie;

                        let token = request
                            .headers()
                            .get(header::AUTHORIZATION)
                            .and_then(|v| v.to_str().ok())
                            .and_then(fraiseql_core::security::bearer_credential)
                            .map(str::to_owned)
                            .or_else(|| extract_access_token_cookie(request.headers()));

                        if let Some(token) = token {
                            if let Some(user) = storage_admin_user(&token, storage_token.as_deref())
                            {
                                request.extensions_mut().insert(user);
                            } else if let Some(ref validator) = validator {
                                match validator.validate_token(&token).await {
                                    Ok(user) => {
                                        if crate::middleware::oidc_auth::check_revocation(
                                            revocation.as_ref(),
                                            &user,
                                            &token,
                                        )
                                        .await
                                        .is_err()
                                        {
                                            tracing::debug!("Storage auth: token is revoked");
                                            return (
                                                StatusCode::UNAUTHORIZED,
                                                "Invalid or expired token",
                                            )
                                                .into_response();
                                        }
                                        let storage_user = fraiseql_storage::StorageUser {
                                            user_id: Some(user.user_id.to_string()),
                                            roles:   user.scopes,
                                            // #974: the OIDC path is the only
                                            // one carrying claims, so it is the
                                            // only one where a `require_claims`
                                            // rule can match.
                                            claims:  fraiseql_storage::normalise_claims(
                                                &user.extra_claims,
                                            ),
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
            // The static token carries no claims. It does not need any: the
            // storage-admin role bypasses policy evaluation entirely.
            claims:  fraiseql_storage::ClaimValues::new(),
        })
    } else {
        None
    }
}

#[cfg(test)]
mod tests;
