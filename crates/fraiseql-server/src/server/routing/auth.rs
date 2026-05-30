//! Authentication route mounting (PKCE, social login, MFA, session identity, revocation).
//!
//! All routes in this module are gated on `#[cfg(feature = "auth")]`.

use std::sync::Arc;

use axum::{
    Router, middleware,
    routing::{get, post},
};
use fraiseql_core::db::traits::DatabaseAdapter;
use tracing::info;

use super::super::{
    AuthMeState, AuthPkceState, OidcAuthState, Server, auth_callback, auth_me, auth_start,
    oidc_auth_middleware,
};
use crate::auth::{
    anon_signup, mfa_challenge, mfa_enroll, mfa_unenroll, mfa_verify, social::social_authorize,
};

impl<A: DatabaseAdapter + Clone + Send + Sync + 'static> Server<A> {
    /// Mount all `#[cfg(feature = "auth")]`-gated authentication routes.
    pub(super) fn mount_auth_routes(&self, mut app: Router) -> Router {
        // PKCE OAuth2 auth routes — mounted only when both pkce and [auth] are configured.
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
        if let Some(ref anon) = self.anon_signup_state {
            let anon_router = Router::new()
                .route("/auth/v1/signup", post(anon_signup))
                .with_state(Arc::clone(anon));
            app = app.merge(anon_router);
            info!("Anonymous signup route mounted: POST /auth/v1/signup");
        }

        // TOTP MFA endpoints — mounted when mfa_state is configured.
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
        // 1. An OIDC validator is present (token validation capability).
        // 2. `[auth.me] enabled = true` in the compiled schema / ServerConfig.
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

        // Token revocation routes — mounted only when revocation is configured
        // AND an OIDC validator is available to gate the requests. Without an
        // OIDC validator, these routes would be reachable unauthenticated —
        // an anonymous force-logout primitive (#358). Skipping the mount with
        // a loud warning is safer than silently mounting them open.
        if let Some(ref rev_mgr) = self.revocation_manager {
            if let Some(ref validator) = self.oidc_validator {
                let rev_state = Arc::new(crate::routes::RevocationRouteState {
                    revocation_manager: Arc::clone(rev_mgr),
                });
                let auth_state = OidcAuthState::new(Arc::clone(validator));
                let rev_router = Router::new()
                    .route("/auth/revoke", post(crate::routes::revoke_token))
                    .route("/auth/revoke-all", post(crate::routes::revoke_all_tokens))
                    .route_layer(middleware::from_fn_with_state(auth_state, oidc_auth_middleware))
                    .with_state(rev_state);
                app = app.merge(rev_router);
                info!(
                    "Token revocation routes mounted (auth-gated): POST /auth/revoke, POST /auth/revoke-all"
                );
            } else {
                tracing::warn!(
                    "Token revocation is configured but no OIDC validator is available; \
                     refusing to mount /auth/revoke and /auth/revoke-all unauthenticated. \
                     Configure [auth] in fraiseql.toml to enable token revocation."
                );
            }
        }

        app
    }
}
