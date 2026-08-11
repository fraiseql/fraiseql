//! SAML SP-initiated login and Assertion Consumer Service axum handlers (#381).
//!
//! - `GET  /auth/saml/login?idp=<name>` — [`saml_login`]: build an `AuthnRequest`, bind a
//!   single-use `RelayState` (carrying the IdP name and request ID), and 302 to the IdP.
//! - `POST /auth/saml/acs` — [`saml_acs`]: consume the `RelayState`, verify the `SAMLResponse`
//!   (signature/conditions/replay), resolve a local user via the account store, and create a
//!   session.

use std::sync::Arc;

use axum::{
    Json, Router,
    extract::{Form, Query, State},
    http::StatusCode,
    response::{IntoResponse, Redirect, Response},
    routing::{get, post},
};
use serde::Deserialize;

use super::{
    SamlIdpConfig, SamlReplayCache, effective_saml_email_verified, registry::SamlIdpRegistry,
    replay::SamlReplayStore, verify::verify_saml_response,
};
use crate::{
    account_linking::AccountStore,
    audit::logger::{AuditEventType, SecretType, get_audit_logger},
    handlers::generate_secure_state,
    session::{SessionStore, unix_now},
    state_store::StateStore,
};

/// Field separator inside the stored `RelayState` payload
/// (`idp_name \n tenant \n request_id`). A newline cannot appear in any of the three, so
/// the payload round-trips unambiguously.
const RELAY_PAYLOAD_SEPARATOR: char = '\n';

/// `RelayState` / `AuthnRequest` time-to-live: 10 minutes.
const LOGIN_STATE_TTL_SECS: u64 = 600;

/// Session lifetime granted on a successful ACS: 7 days.
const SESSION_TTL_SECS: u64 = 7 * 24 * 60 * 60;

/// Shared state for the SAML endpoints.
///
/// # Rate limiting (#788)
///
/// [`saml_login`] and [`saml_acs`] write to the bounded CSRF [`StateStore`] with
/// no per-IP throttle. As with the multi-provider router, **no fraiseql-server
/// route mounts [`saml_routes`] today**, so the gap is not reachable from the
/// shipped binary; when these routes are mounted (the auth-enterprise wave), gate
/// them on the same per-IP `RateLimiters` middleware the transport already applies.
#[derive(Clone)]
pub struct SamlAuthState {
    /// Config-file and stored IdPs, and the tenant-scoped resolution between them (#947).
    registry:      SamlIdpRegistry,
    /// CSRF/`RelayState` store (in-memory or Redis) — also binds the in-flight request ID.
    state_store:   Arc<dyn StateStore>,
    /// Session backend used to mint tokens after a verified assertion.
    session_store: Arc<dyn SessionStore>,
    /// Account store for resolving the assertion to a stable local user. When absent the
    /// raw `"saml:<idp>:<NameID>"` is used as the user ID.
    user_store:    Option<Arc<dyn AccountStore>>,
    /// Single-use assertion replay store. In-process by default; the server swaps in
    /// the Postgres-backed store so replay protection holds across replicas (#949).
    replay:        Arc<dyn SamlReplayStore>,
}

impl SamlAuthState {
    /// Create SAML auth state with no IdPs registered.
    #[must_use]
    pub fn new(state_store: Arc<dyn StateStore>, session_store: Arc<dyn SessionStore>) -> Self {
        Self {
            registry: SamlIdpRegistry::new(),
            state_store,
            session_store,
            user_store: None,
            replay: Arc::new(SamlReplayCache::new()),
        }
    }

    /// Swap in a shared replay store.
    ///
    /// The default is the in-process [`SamlReplayCache`], which is correct for exactly
    /// one server instance. Behind more than one replica a captured assertion replays
    /// against a replica that has never seen its ID, so the server passes the
    /// Postgres-backed store here (#949).
    #[must_use]
    pub fn with_replay_store(mut self, replay: Arc<dyn SamlReplayStore>) -> Self {
        self.replay = replay;
        self
    }

    /// Whether replay protection is shared across processes.
    #[must_use]
    pub fn replay_is_distributed(&self) -> bool {
        self.replay.is_distributed()
    }

    /// Register a config-file IdP under its [`SamlIdpConfig::idp_name`]. Builder-style;
    /// last write wins (configuration is operator-controlled).
    #[must_use]
    pub fn with_idp(mut self, idp: SamlIdpConfig) -> Self {
        self.registry = self.registry.with_config_idp(idp);
        self
    }

    /// Swap in the IdP registry — the multi-tenant path, where a durable store is attached
    /// and hot-reloaded (#947).
    #[must_use]
    pub fn with_registry(mut self, registry: SamlIdpRegistry) -> Self {
        self.registry = registry;
        self
    }

    /// Borrow the registry (the admin API manages stored IdPs through it).
    #[must_use]
    pub const fn registry(&self) -> &SamlIdpRegistry {
        &self.registry
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
        self.registry.idp_names()
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
    pub idp:    String,
    /// Tenant the caller is starting SSO for (#947).
    ///
    /// Must equal the tenant bound to the IdP — absent for an untenanted IdP, present and
    /// matching for a tenant-bound one. See [`SamlIdpRegistry::resolve`].
    #[serde(default)]
    pub tenant: Option<String>,
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

/// What the single-use `RelayState` token binds: which IdP, for which tenant, answering
/// which in-flight `AuthnRequest`.
///
/// The tenant travels with the request so the ACS re-resolves through the *same* scoped
/// path the login took. Without it a login legitimately started for tenant A, whose IdP is
/// then re-bound or removed mid-flight, would be consumed by whatever the bare name resolves
/// to at ACS time.
#[derive(Debug, PartialEq, Eq)]
struct RelayPayload {
    idp_name:   String,
    tenant:     Option<String>,
    request_id: String,
}

impl RelayPayload {
    fn encode(&self) -> String {
        format!(
            "{}{RELAY_PAYLOAD_SEPARATOR}{}{RELAY_PAYLOAD_SEPARATOR}{}",
            self.idp_name,
            self.tenant.as_deref().unwrap_or_default(),
            self.request_id
        )
    }

    fn decode(payload: &str) -> Option<Self> {
        let mut parts = payload.splitn(3, RELAY_PAYLOAD_SEPARATOR);
        let idp_name = parts.next()?.to_string();
        let tenant = parts.next()?;
        let request_id = parts.next()?.to_string();
        Some(Self {
            idp_name,
            tenant: (!tenant.is_empty()).then(|| tenant.to_string()),
            request_id,
        })
    }
}

/// `GET /auth/saml/login?idp=<name>[&tenant=<id>]` — start SP-initiated SSO.
///
/// Builds an `AuthnRequest`, stores a single-use `RelayState` carrying the IdP name, the
/// tenant and the request ID (so the ACS can require a matching `InResponseTo` and re-check
/// the tenant), and redirects the browser to the IdP's HTTP-Redirect SSO endpoint.
///
/// An IdP the caller's tenant does not own answers `404`, identically to a name that does
/// not exist — the route must not report which other tenants' IdPs are configured (#947).
pub async fn saml_login(
    State(state): State<SamlAuthState>,
    Query(q): Query<LoginQuery>,
) -> Response {
    let Some(idp) = state.registry.resolve(&q.idp, q.tenant.as_deref()) else {
        return json_error(StatusCode::NOT_FOUND, "unknown SAML IdP");
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
    let payload = RelayPayload {
        idp_name:   idp.idp_name.clone(),
        tenant:     idp.tenant_id.clone(),
        request_id: authn_request.id.clone(),
    }
    .encode();
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

    // Consume the single-use RelayState (atomic remove) → (idp_name, tenant, request_id).
    let Ok((payload, expiry)) = state.state_store.retrieve(&form.relay_state).await else {
        return json_error(StatusCode::BAD_REQUEST, "invalid or expired RelayState");
    };
    let Some(RelayPayload {
        idp_name,
        tenant,
        request_id,
    }) = RelayPayload::decode(&payload)
    else {
        return json_error(StatusCode::BAD_REQUEST, "malformed RelayState");
    };

    let Ok(now_secs) = unix_now() else {
        return json_error(StatusCode::INTERNAL_SERVER_ERROR, "system clock error");
    };
    if now_secs > expiry {
        return json_error(StatusCode::BAD_REQUEST, "RelayState expired");
    }

    // Re-resolve through the same tenant-scoped path the login took. An IdP removed or
    // re-bound between login and ACS must fail the flow, not silently answer under a
    // different tenant's binding.
    let Some(idp) = state.registry.resolve(&idp_name, tenant.as_deref()) else {
        tracing::error!(
            idp = %idp_name,
            "RelayState referenced an IdP that no longer resolves for its tenant"
        );
        return json_error(StatusCode::BAD_REQUEST, "SAML authentication failed");
    };
    let idp = idp.as_ref();

    // The security core. Bind the response to the request ID we issued (InResponseTo).
    let assertion = match verify_saml_response(
        idp,
        &form.saml_response,
        &[request_id.as_str()],
        state.replay.as_ref(),
        chrono::Utc::now(),
    )
    .await
    {
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
