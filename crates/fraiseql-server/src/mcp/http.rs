//! The MCP Streamable HTTP transport, as mounted at `[mcp] path`.
//!
//! One constructor, so the route and its tests build the same service.

use std::sync::Arc;

use axum::{
    Router,
    extract::{Request, State},
    http::{StatusCode, header},
    middleware::Next,
    response::{IntoResponse, Response},
};
use rmcp::transport::{
    StreamableHttpServerConfig, StreamableHttpService,
    streamable_http_server::session::local::LocalSessionManager,
};

use super::handler::{FraiseQLMcpService, McpTokenValidator, extract_bearer, identify_caller};
use crate::{
    admission::{AdmissionPolicy, Refusal},
    routes::graphql::AppState,
};

/// The tower service mounted at `[mcp] path`.
pub type McpHttpService = StreamableHttpService<FraiseQLMcpService, LocalSessionManager>;

/// The words a request to the endpoint that carries no credential is refused with.
pub const CREDENTIAL_REQUIRED: &str = "Authentication required";

/// What a credential is resolved against under `[mcp] require_auth`: the state (service
/// accounts, API keys, the store of ended sessions, the schema's tenant claim) and the
/// validator bearer tokens are checked by.
pub struct McpCredentialGate {
    state:     AppState,
    validator: McpTokenValidator,
}

impl McpCredentialGate {
    /// The gate over `state`, its bearer tokens checked by `validator`.
    #[must_use]
    pub const fn new(state: AppState, validator: McpTokenValidator) -> Self {
        Self { state, validator }
    }
}

/// Who may reach the transport: `[mcp] require_auth`.
#[derive(Clone)]
pub enum McpAuth {
    /// `require_auth = true`: every request carries a credential that resolves to a
    /// principal, checked by the gate before the transport reads it.
    Required(Arc<McpCredentialGate>),
    /// `require_auth = false` (development only): no credential is asked for, and only a
    /// loopback `Host` is served.
    Open,
}

impl std::fmt::Debug for McpAuth {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Required(_) => f.write_str("McpAuth::Required"),
            Self::Open => f.write_str("McpAuth::Open"),
        }
    }
}

/// Build the Streamable HTTP service over `sessions`.
///
/// `factory` builds one [`FraiseQLMcpService`] per MCP session. A tool call resolves its
/// caller again inside that service; what stands in front of the transport is the router
/// [`mcp_router`] builds.
pub fn streamable_http_service(
    factory: impl Fn() -> FraiseQLMcpService + Send + Sync + 'static,
    sessions: Arc<LocalSessionManager>,
    require_auth: bool,
) -> McpHttpService {
    StreamableHttpService::new(move || Ok(factory()), sessions, transport_config(require_auth))
}

/// The router mounted at `[mcp] path`: the transport, and under `require_auth` a gate in
/// front of it.
///
/// The transport answers an `initialize` by opening a session and a `tools/list` by
/// naming every tool, and authenticates nothing itself before a tool call. So under
/// `require_auth` the gate refuses every request whose credential does not resolve to a
/// principal, whatever its method, before the transport reads its body or allocates
/// anything for it.
pub fn mcp_router(
    path: &str,
    factory: impl Fn() -> FraiseQLMcpService + Send + Sync + 'static,
    sessions: Arc<LocalSessionManager>,
    auth: McpAuth,
) -> Router {
    let required = matches!(auth, McpAuth::Required(_));
    let router =
        Router::new().nest_service(path, streamable_http_service(factory, sessions, required));
    match auth {
        McpAuth::Required(gate) => {
            router.layer(axum::middleware::from_fn_with_state(gate, require_credential))
        },
        McpAuth::Open => router,
    }
}

/// Refuse a request whose credential does not resolve to a principal.
async fn require_credential(
    State(gate): State<Arc<McpCredentialGate>>,
    request: Request,
    next: Next,
) -> Response {
    let McpCredentialGate { state, validator } = &*gate;
    let token = extract_bearer(request.headers());
    let tenant_claim = state.executor().schema().tenant_claim().to_owned();
    let identified = identify_caller(
        state,
        Some(validator),
        &tenant_claim,
        token,
        uuid::Uuid::new_v4().to_string(),
        request.headers(),
        &AdmissionPolicy::MCP_ENDPOINT,
    )
    .await;
    match identified {
        Ok(identified) if identified.security_context().is_some() => next.run(request).await,
        Ok(_) => unauthorized(CREDENTIAL_REQUIRED),
        Err(refusal) => refused(&refusal),
    }
}

/// The answer to a credential admission refused. Without enrichment only the credential
/// classes are reachable; the others keep their own status should a policy ever add them.
fn refused(refusal: &Refusal) -> Response {
    match refusal {
        Refusal::TokenExpired
        | Refusal::TokenInvalid
        | Refusal::TokenRevoked
        | Refusal::AmbiguousCredentials
        | Refusal::InvalidApiKey => unauthorized(refusal.message().unwrap_or_default()),
        Refusal::IdentityDenied => {
            (StatusCode::FORBIDDEN, refusal.message().unwrap_or_default()).into_response()
        },
        Refusal::IdentityUnavailable => {
            (StatusCode::SERVICE_UNAVAILABLE, refusal.message().unwrap_or_default()).into_response()
        },
        Refusal::Tenant(error) | Refusal::Dispatch(error) | Refusal::Cost(error) => {
            let status = StatusCode::from_u16(error.status_code()).unwrap_or(StatusCode::FORBIDDEN);
            (status, error.to_string()).into_response()
        },
    }
}

fn unauthorized(message: &str) -> Response {
    (
        StatusCode::UNAUTHORIZED,
        [(header::WWW_AUTHENTICATE, "Bearer")],
        message.to_owned(),
    )
        .into_response()
}

/// The transport's configuration: rmcp's defaults, with the `Host` check set by
/// `require_auth`.
///
/// rmcp accepts only loopback `Host` values by default, against DNS rebinding: a page in
/// the browser of someone running a server locally rebinds its own name to 127.0.0.1 and
/// calls it. That default refused every request to a deployment's own hostname, and every
/// tenant addressed by its domain, with 403. With `require_auth` every request needs a
/// credential, which the browser never attaches to a rebound host, so the check guards
/// nothing and is off. Without it (development only) the loopback check stays.
fn transport_config(require_auth: bool) -> StreamableHttpServerConfig {
    let config = StreamableHttpServerConfig::default();
    if require_auth {
        config.disable_allowed_hosts()
    } else {
        config
    }
}
