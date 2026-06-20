//! SAML SP-initiated login and Assertion Consumer Service axum handlers (#381).
//!
//! - `GET  /auth/saml/login?idp=<name>` — [`saml_login`]: build an `AuthnRequest`, bind a
//!   single-use `RelayState` (carrying the IdP name and request ID), and 302 to the IdP.
//! - `POST /auth/saml/acs` — [`saml_acs`]: consume the `RelayState`, verify the `SAMLResponse`
//!   (signature/conditions/replay), resolve a local user via the account store, and create a
//!   session.

use std::{collections::HashMap, sync::Arc};

use axum::{
    Json, Router,
    extract::{Form, Query, State},
    http::StatusCode,
    response::{IntoResponse, Redirect, Response},
    routing::{get, post},
};
use serde::Deserialize;

use super::{
    SamlIdpConfig, SamlReplayCache, effective_saml_email_verified, verify::verify_saml_response,
};
use crate::{
    account_linking::AccountStore,
    audit::logger::{AuditEventType, SecretType, get_audit_logger},
    handlers::generate_secure_state,
    session::{SessionStore, unix_now},
    state_store::StateStore,
};

/// Separator between the IdP name and the in-flight `AuthnRequest` ID inside the stored
/// `RelayState` payload. A newline cannot appear in either token, so it round-trips
/// unambiguously.
const RELAY_PAYLOAD_SEPARATOR: char = '\n';

/// `RelayState` / `AuthnRequest` time-to-live: 10 minutes.
const LOGIN_STATE_TTL_SECS: u64 = 600;

/// Session lifetime granted on a successful ACS: 7 days.
const SESSION_TTL_SECS: u64 = 7 * 24 * 60 * 60;

/// Shared state for the SAML endpoints.
#[derive(Clone)]
pub struct SamlAuthState {
    /// Configured IdPs keyed by logical name.
    idps:          Arc<HashMap<String, Arc<SamlIdpConfig>>>,
    /// CSRF/`RelayState` store (in-memory or Redis) — also binds the in-flight request ID.
    state_store:   Arc<dyn StateStore>,
    /// Session backend used to mint tokens after a verified assertion.
    session_store: Arc<dyn SessionStore>,
    /// Account store for resolving the assertion to a stable local user. When absent the
    /// raw `"saml:<idp>:<NameID>"` is used as the user ID.
    user_store:    Option<Arc<dyn AccountStore>>,
    /// Single-use assertion replay cache.
    replay:        Arc<SamlReplayCache>,
}

impl SamlAuthState {
    /// Create SAML auth state with no IdPs registered.
    #[must_use]
    pub fn new(state_store: Arc<dyn StateStore>, session_store: Arc<dyn SessionStore>) -> Self {
        Self {
            idps: Arc::new(HashMap::new()),
            state_store,
            session_store,
            user_store: None,
            replay: Arc::new(SamlReplayCache::new()),
        }
    }

    /// Register an IdP under its [`SamlIdpConfig::idp_name`]. Builder-style; ignores a
    /// duplicate-free invariant by last-write-wins (configuration is operator-controlled).
    #[must_use]
    pub fn with_idp(mut self, idp: SamlIdpConfig) -> Self {
        let idps = Arc::make_mut(&mut self.idps);
        idps.insert(idp.idp_name.clone(), Arc::new(idp));
        self
    }

    /// Set the account store used for user resolution / linking.
    #[must_use]
    pub fn with_user_store(mut self, user_store: Arc<dyn AccountStore>) -> Self {
        self.user_store = Some(user_store);
        self
    }

    /// Names of all registered IdPs (sorted; primarily for tests/introspection).
    #[must_use]
    pub fn idp_names(&self) -> Vec<String> {
        let mut names: Vec<String> = self.idps.keys().cloned().collect();
        names.sort();
        names
    }
}

/// Build the SAML router: `GET /auth/saml/login` and `POST /auth/saml/acs`.
pub fn saml_routes(state: SamlAuthState) -> Router {
    Router::new()
        .route("/auth/saml/login", get(saml_login))
        .route("/auth/saml/acs", post(saml_acs))
        .with_state(state)
}

/// Query parameters for `GET /auth/saml/login`.
#[derive(Debug, Deserialize)]
pub struct LoginQuery {
    /// Logical IdP name to start SSO with.
    pub idp: String,
}

/// Form body for `POST /auth/saml/acs` (HTTP-POST binding).
#[derive(Debug, Deserialize)]
pub struct AcsForm {
    /// Base64-encoded `Response` element.
    #[serde(rename = "SAMLResponse")]
    pub saml_response: String,
    /// Opaque state echoed back by the IdP; the SP's single-use binding token.
    #[serde(rename = "RelayState", default)]
    pub relay_state:   String,
}

fn json_error(status: StatusCode, message: &str) -> Response {
    (status, Json(serde_json::json!({ "error": message }))).into_response()
}

/// `GET /auth/saml/login?idp=<name>` — start SP-initiated SSO.
///
/// Builds an `AuthnRequest`, stores a single-use `RelayState` carrying the IdP name and the
/// request ID (so the ACS can require a matching `InResponseTo`), and 302-redirects the
/// browser to the IdP's HTTP-Redirect SSO endpoint.
pub async fn saml_login(
    State(state): State<SamlAuthState>,
    Query(q): Query<LoginQuery>,
) -> Response {
    let Some(idp) = state.idps.get(&q.idp) else {
        return json_error(StatusCode::BAD_REQUEST, "unknown SAML IdP");
    };

    let Some(sso_url) = idp.sso_redirect_url() else {
        tracing::error!(idp = %q.idp, "IdP metadata has no HTTP-Redirect SSO endpoint");
        return json_error(StatusCode::INTERNAL_SERVER_ERROR, "IdP configuration error");
    };

    let authn_request = match idp.service_provider().make_authentication_request(&sso_url) {
        Ok(req) => req,
        Err(e) => {
            tracing::error!(error = %e, "failed to build AuthnRequest");
            return json_error(StatusCode::INTERNAL_SERVER_ERROR, "could not start SAML login");
        },
    };

    let relay_state = generate_secure_state();
    let payload = format!("{}{RELAY_PAYLOAD_SEPARATOR}{}", idp.idp_name, authn_request.id);
    let Ok(now) = unix_now() else {
        return json_error(StatusCode::INTERNAL_SERVER_ERROR, "system clock error");
    };
    if let Err(e) = state
        .state_store
        .store(relay_state.clone(), payload, now + LOGIN_STATE_TTL_SECS)
        .await
    {
        tracing::error!(error = %e, "failed to store SAML RelayState");
        return json_error(StatusCode::INTERNAL_SERVER_ERROR, "could not start SAML login");
    }

    match authn_request.redirect(&relay_state) {
        Ok(Some(url)) => Redirect::to(url.as_str()).into_response(),
        Ok(None) => {
            json_error(StatusCode::INTERNAL_SERVER_ERROR, "IdP has no redirect destination")
        },
        Err(e) => {
            tracing::error!(error = %e, "failed to build SAML redirect");
            json_error(StatusCode::INTERNAL_SERVER_ERROR, "could not start SAML login")
        },
    }
}

/// `POST /auth/saml/acs` — Assertion Consumer Service.
///
/// Consumes the `RelayState`, verifies the `SAMLResponse`, resolves a local user, and
/// returns session tokens as JSON. Verification failures return a generic `400`/`401`
/// (detail is logged, never surfaced).
pub async fn saml_acs(State(state): State<SamlAuthState>, Form(form): Form<AcsForm>) -> Response {
    let logger = get_audit_logger();

    if form.relay_state.is_empty() {
        return json_error(StatusCode::BAD_REQUEST, "missing RelayState");
    }

    // Consume the single-use RelayState (atomic remove) → (idp_name, request_id).
    let Ok((payload, expiry)) = state.state_store.retrieve(&form.relay_state).await else {
        return json_error(StatusCode::BAD_REQUEST, "invalid or expired RelayState");
    };
    let (idp_name, request_id) = match payload.split_once(RELAY_PAYLOAD_SEPARATOR) {
        Some((idp, rid)) => (idp.to_string(), rid.to_string()),
        None => return json_error(StatusCode::BAD_REQUEST, "malformed RelayState"),
    };

    let Ok(now_secs) = unix_now() else {
        return json_error(StatusCode::INTERNAL_SERVER_ERROR, "system clock error");
    };
    if now_secs > expiry {
        return json_error(StatusCode::BAD_REQUEST, "RelayState expired");
    }

    let Some(idp) = state.idps.get(&idp_name) else {
        tracing::error!(idp = %idp_name, "RelayState referenced an unknown IdP");
        return json_error(StatusCode::INTERNAL_SERVER_ERROR, "IdP configuration error");
    };

    // The security core. Bind the response to the request ID we issued (InResponseTo).
    let assertion = match verify_saml_response(
        idp,
        &form.saml_response,
        &[request_id.as_str()],
        &state.replay,
        chrono::Utc::now(),
    ) {
        Ok(a) => a,
        Err(e) => {
            tracing::warn!(idp = %idp_name, error = %e, "SAML assertion verification failed");
            logger.log_failure(
                AuditEventType::AuthFailure,
                SecretType::SessionToken,
                None,
                "saml_acs",
                &format!("verification_failed:{idp_name}"),
            );
            let status = match e {
                super::SamlError::Replay => StatusCode::UNAUTHORIZED,
                _ => StatusCode::BAD_REQUEST,
            };
            return json_error(status, "SAML authentication failed");
        },
    };

    // Resolve the local user. Email auto-linking is tenant-bounded (default off).
    let provider = idp.provider_key();
    let email_verified = effective_saml_email_verified(idp);
    let local_user_id = if let Some(store) = &state.user_store {
        match store
            .link_or_create_user(
                assertion.email.as_deref(),
                email_verified,
                &provider,
                &assertion.name_id,
            )
            .await
        {
            Ok(result) => result.user_id,
            Err(e) => {
                tracing::error!(error = %e, "account store lookup failed");
                return json_error(StatusCode::INTERNAL_SERVER_ERROR, "user resolution failed");
            },
        }
    } else {
        format!("{provider}:{}", assertion.name_id)
    };

    let session = match state
        .session_store
        .create_session(&local_user_id, now_secs + SESSION_TTL_SECS)
        .await
    {
        Ok(tokens) => tokens,
        Err(e) => {
            tracing::error!(error = %e, "session creation failed");
            return json_error(StatusCode::INTERNAL_SERVER_ERROR, "session could not be created");
        },
    };

    logger.log_success(
        AuditEventType::AuthSuccess,
        SecretType::SessionToken,
        Some(local_user_id),
        &format!("saml_acs:{idp_name}"),
    );

    Json(serde_json::json!({
        "access_token":  session.access_token,
        "refresh_token": session.refresh_token,
        "token_type":    "Bearer",
        "expires_in":    session.expires_in,
        "provider":      provider,
    }))
    .into_response()
}
