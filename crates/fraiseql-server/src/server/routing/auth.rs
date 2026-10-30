//! Authentication route mounting (PKCE, social login, MFA, session identity, revocation).
//!
//! All routes in this module are gated on `#[cfg(feature = "auth")]`.

use std::sync::Arc;

use axum::{
    Router, middleware,
    routing::{get, post},
};
use tracing::info;

use super::super::{
    AuthMeState, AuthPkceState, Server, auth_callback, auth_me, auth_start, oidc_auth_middleware,
};
use crate::{
    auth::{anon_signup, mfa_confirm, mfa_enroll, mfa_unenroll, mfa_verify},
    middleware::{AuthUser, hs256_auth_middleware},
};

/// Resolve the client a per-address auth budget keys on: the address a trusted proxy
/// reported, else the peer, exactly as the request limiter resolves it.
async fn client_address(
    axum::extract::State(limiter): axum::extract::State<Arc<crate::middleware::RateLimiter>>,
    axum::extract::ConnectInfo(peer): axum::extract::ConnectInfo<std::net::SocketAddr>,
    mut request: axum::extract::Request,
    next: middleware::Next,
) -> axum::response::Response {
    let address = crate::middleware::rate_limit::extract_real_ip(
        request.headers(),
        limiter.config().trust_proxy_headers,
        &limiter.config().trusted_proxy_cidrs,
        Some(peer.ip()),
    );
    request.extensions_mut().insert(fraiseql_auth::ClientAddress(address));
    next.run(request).await
}

/// Name the account the HS256 layer authenticated as the [`SessionSubject`] the MFA
/// handlers act on. A request the layer did not authenticate carries none, and the
/// handlers refuse it.
///
/// [`SessionSubject`]: fraiseql_auth::SessionSubject
async fn session_subject(
    mut request: axum::extract::Request,
    next: middleware::Next,
) -> axum::response::Response {
    let subject = request
        .extensions()
        .get::<AuthUser>()
        .map(|AuthUser(user)| user.user_id.to_string());
    if let Some(user_id) = subject {
        request.extensions_mut().insert(fraiseql_auth::SessionSubject(user_id));
    }
    next.run(request).await
}

impl Server {
    /// `router` behind the sessions this server mints: the HS256 layer, revocation
    /// included, naming the authenticated account as the [`SessionSubject`] its handlers
    /// act on. `None` without that layer: a route that acts on the caller's own account is
    /// not mounted at all rather than served to no one in particular. `[auth.local]`
    /// refuses to boot without `[auth_hs256]`, so `None` is for an embedder that attached
    /// such state by hand.
    ///
    /// [`SessionSubject`]: fraiseql_auth::SessionSubject
    fn behind_session(&self, router: Router) -> Option<Router> {
        let validator = self.hs256_auth.as_ref()?;
        let session = self.hs256_auth_state(Arc::clone(validator), self.hs256_realm());
        Some(
            router
                .route_layer(middleware::from_fn(session_subject))
                .route_layer(middleware::from_fn_with_state(session, hs256_auth_middleware)),
        )
    }

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

        // SAML SP-initiated SSO (#381) — mounted when [saml] is configured on an
        // auth-saml build. Construction (from_executor) already proved the pool,
        // the HS256 signing pair and every IdP's metadata; this is pure routing.
        #[cfg(feature = "auth-saml")]
        if let Some(ref saml) = self.saml_state {
            app = app.merge(fraiseql_auth::saml::saml_routes(saml.clone()));
            info!("SAML routes mounted: GET /auth/saml/login, POST /auth/saml/acs");
        }

        // Social login (#368) — the trust-gated multi_provider flow, mounted
        // when `[auth.social]` is configured (or a state was attached by an
        // embedder). Rate limiting: `/auth/v1/authorize` and
        // `/auth/v1/callback` are governed by the shared per-IP path buckets
        // derived from `[security.rate_limiting]`'s auth_start/auth_callback
        // settings (#788) — the same limiter that guards /auth/start.
        if let Some(ref social) = self.social_login {
            app = app.merge(social_router(Arc::clone(social)));
            info!(
                providers = ?social.provider_names(),
                "Social login routes mounted: GET /auth/v1/{{providers,authorize,callback}}"
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

        // Email OTP / magic-link (#367) — mounted when [auth.local] otp = true.
        if let Some(ref otp) = self.otp_state {
            let otp_router = Router::new()
                .route("/auth/v1/otp", post(crate::auth::otp_send))
                .route("/auth/v1/verify", post(crate::auth::otp_verify))
                .with_state(Arc::clone(otp));
            app = app.merge(otp_router);
            info!("Email OTP routes mounted: POST /auth/v1/otp, POST /auth/v1/verify");
        }

        // Local email+password (#367) — mounted when [auth.local] password = true.
        if let Some(ref pw) = self.local_password_state {
            app = app.merge(fraiseql_auth::local_password_routes(Arc::clone(pw)));
            info!(
                "Local password routes mounted: POST /auth/v1/password/{{signup,login,reset}}, \
                 POST /auth/v1/password/reset/confirm"
            );
        }

        // Email verification (#945) — mounted when [auth.local] email_verification = true.
        // Both routes act only on the caller's own account, so they sit behind the session.
        if let Some(ref verify) = self.email_verification_state {
            if let Some(routes) =
                self.behind_session(fraiseql_auth::email_verification_routes(Arc::clone(verify)))
            {
                app = app.merge(routes);
                info!(
                    "Email verification routes mounted: POST \
                     /auth/v1/email/verify/{{start,confirm}}"
                );
            } else {
                tracing::warn!(
                    "email verification not mounted: it acts on the signed-in account, and no \
                     [auth_hs256] layer resolves one"
                );
            }
        }

        // TOTP MFA endpoints — mounted when mfa_state is configured.
        if let Some(ref mfa) = self.mfa_state {
            // Enroll, confirm and unenroll act on the signed-in account, so they sit behind
            // the session.
            let management = self.behind_session(
                Router::new()
                    .route("/auth/v1/mfa/enroll", post(mfa_enroll))
                    // `/confirm` completes what `/enroll` begins. Without it an enrollment
                    // stays unconfirmed forever and `create_challenge` refuses to issue
                    // against it, so MFA was enrollable but unusable through the API (#950).
                    .route("/auth/v1/mfa/confirm", post(mfa_confirm))
                    .route("/auth/v1/mfa/unenroll", post(mfa_unenroll))
                    .with_state(Arc::clone(mfa)),
            );
            if let Some(routes) = management {
                app = app.merge(routes);
            } else {
                tracing::warn!(
                    "TOTP MFA enroll/confirm/unenroll not mounted: they act on the signed-in \
                     account, and no [auth_hs256] layer resolves one"
                );
            }
            // Verify redeems the challenge a first factor earned; the challenge is issued
            // where every sign-in path mints its session, never on a route of its own.
            let mut mfa_router = Router::new()
                .route("/auth/v1/mfa/verify", post(mfa_verify))
                .with_state(Arc::clone(mfa));
            // Its per-client budget keys on the client a trusted proxy reported, by the rule
            // the request limiter applies; without a limiter there is no proxy setting, and
            // the handler keys on the peer.
            if let Some(ref limiter) = self.rate_limiter {
                mfa_router = mfa_router.route_layer(middleware::from_fn_with_state(
                    Arc::clone(limiter),
                    client_address,
                ));
            }
            app = app.merge(mfa_router);
            info!(
                "TOTP MFA routes mounted: POST \
                 /auth/v1/mfa/{{enroll,confirm,verify,unenroll}}"
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
            let auth_state = self.oidc_auth_state(Arc::clone(validator));
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
                let auth_state = self.oidc_auth_state(Arc::clone(validator));
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

/// Build the social-login route group (#368). Factored out of
/// `mount_auth_routes` so the route syntax is validated by an unconditional
/// construction test (the axum-bump checklist), not only when a database-backed
/// e2e suite runs.
fn social_router(social: Arc<fraiseql_auth::MultiProviderAuthState>) -> Router {
    // #943: Apple requests the `name`/`email` scopes, which makes Apple deliver
    // the callback as `response_mode=form_post` — a POST with a form body, not
    // the GET every other provider makes. The POST variant is mounted only when
    // a provider that needs it is registered, so no server grows a second
    // callback shape it has no use for.
    let callback = if social.get_provider("apple").is_some() {
        get(fraiseql_auth::multi_provider::callback)
            .post(fraiseql_auth::multi_provider::callback_form_post)
    } else {
        get(fraiseql_auth::multi_provider::callback)
    };
    Router::new()
        .route("/auth/v1/providers", get(fraiseql_auth::multi_provider::list_providers))
        .route("/auth/v1/authorize", get(fraiseql_auth::multi_provider::authorize))
        .route("/auth/v1/callback", callback)
        .with_state(social)
}

#[cfg(test)]
mod router_construction {
    use super::*;

    /// Session store double: never invoked — the test only constructs the router.
    struct NoopSessionStore;

    fn double_error() -> fraiseql_auth::AuthError {
        fraiseql_auth::AuthError::ConfigError {
            message: "construction-only double".to_string(),
        }
    }

    #[async_trait::async_trait]
    impl fraiseql_auth::SessionStore for NoopSessionStore {
        async fn create_session(
            &self,
            _user_id: &str,
            _expires_at: u64,
        ) -> fraiseql_auth::Result<fraiseql_auth::TokenPair> {
            Err(double_error())
        }

        async fn get_session(
            &self,
            _refresh_token_hash: &str,
        ) -> fraiseql_auth::Result<fraiseql_auth::SessionData> {
            Err(double_error())
        }

        async fn revoke_session(&self, _refresh_token_hash: &str) -> fraiseql_auth::Result<()> {
            Ok(())
        }

        async fn revoke_all_sessions(&self, _user_id: &str) -> fraiseql_auth::Result<()> {
            Ok(())
        }
    }

    /// axum validates path-capture syntax inside `Router::route`, so a bad
    /// literal panics here in `cargo test` rather than at first server boot.
    #[tokio::test]
    async fn social_router_constructs() {
        let state = fraiseql_auth::MultiProviderAuthState::new(
            Arc::new(fraiseql_auth::InMemoryStateStore::new()),
            Arc::new(NoopSessionStore),
        );
        let _router = social_router(Arc::new(state));
    }

    /// Same gate for the `[auth.local]` OTP group (#367).
    #[tokio::test]
    async fn otp_router_constructs() {
        let _router: Router = Router::new()
            .route("/auth/v1/otp", post(crate::auth::otp_send))
            .route("/auth/v1/verify", post(crate::auth::otp_verify))
            .with_state(Arc::new(fraiseql_auth::OtpRouteState {
                otp_store:      Arc::new(fraiseql_auth::InMemoryOtpStore::new()),
                email_delivery: Arc::new(fraiseql_auth::NoopEmailDelivery),
                session_store:  Arc::new(NoopSessionStore),
                account_store:  None,
            }));
    }

    /// Same gate for the `[auth.local]` MFA group (#367).
    #[tokio::test]
    async fn mfa_router_constructs() {
        let _router: Router = Router::new()
            .route("/auth/v1/mfa/enroll", post(mfa_enroll))
            .route("/auth/v1/mfa/confirm", post(mfa_confirm))
            .route("/auth/v1/mfa/verify", post(mfa_verify))
            .route("/auth/v1/mfa/unenroll", post(mfa_unenroll))
            .with_state(Arc::new(fraiseql_auth::MfaRouteState {
                mfa_store:      Arc::new(fraiseql_auth::InMemoryMfaStore::new()),
                session_store:  Arc::new(NoopSessionStore),
                issuer:         "FraiseQL".to_string(),
                verify_limiter: Arc::new(fraiseql_auth::KeyedRateLimiter::new(
                    fraiseql_auth::AuthRateLimitConfig::per_ip_strict(),
                )),
            }));
    }
}
