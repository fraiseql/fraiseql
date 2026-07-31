//! `WebSocket` subscription handler with protocol negotiation.
//!
//! Supports both the modern `graphql-transport-ws` protocol and the legacy
//! `graphql-ws` (Apollo subscriptions-transport-ws) protocol. Protocol
//! selection happens during the `WebSocket` upgrade via the `Sec-WebSocket-Protocol`
//! header.
//!
//! # Lifecycle Hooks
//!
//! Configurable callbacks are invoked at key points in the subscription
//! lifecycle: `on_connect`, `on_disconnect`, `on_subscribe`, `on_unsubscribe`.
//!
//! # Example
//!
//! ```text
//! // Requires: running server with initialized subscription manager.
//! use fraiseql_server::routes::subscriptions::{subscription_handler, SubscriptionState};
//!
//! let state = SubscriptionState::new(subscription_manager);
//!
//! let app = Router::new()
//!     .route("/ws", get(subscription_handler))
//!     .with_state(state);
//! ```

use std::{
    collections::HashMap,
    sync::{
        Arc,
        atomic::{AtomicU64, Ordering},
    },
    time::Duration,
};

use axum::{
    extract::{
        State,
        ws::{Message, WebSocket, WebSocketUpgrade},
    },
    http::HeaderMap,
    response::IntoResponse,
};
use fraiseql_core::{
    runtime::{
        SubscriptionId, SubscriptionManager, SubscriptionPayload,
        protocol::{
            ClientMessage, ClientMessageType, CloseCode, GraphQLError, ServerMessage,
            SubscribePayload,
        },
    },
    schema::{CompiledSchema, OwnerCondition, SubscriptionPolicy},
    security::{Authorizer, OperationKind, SecurityContext, authorizer::enforce_authz},
};
use futures::{SinkExt, StreamExt};
use tokio::sync::broadcast;
use tracing::{debug, error, info, warn};

use crate::{
    extractors::OptionalSecurityContext,
    routes::graphql::{DomainRegistry, TenantStatusSource},
    subscriptions::{
        lifecycle::SubscriptionLifecycle,
        protocol::{ProtocolCodec, WsProtocol},
    },
};

// ── Subscription metrics (module-level atomics) ──────────────────────

static WS_CONNECTIONS_ACCEPTED: AtomicU64 = AtomicU64::new(0);
static WS_CONNECTIONS_REJECTED: AtomicU64 = AtomicU64::new(0);
static WS_SUBSCRIPTIONS_ACCEPTED: AtomicU64 = AtomicU64::new(0);
static WS_SUBSCRIPTIONS_REJECTED: AtomicU64 = AtomicU64::new(0);

/// Subscription metrics for Prometheus export.
#[must_use]
pub fn subscription_metrics() -> SubscriptionMetrics {
    SubscriptionMetrics {
        connections_accepted:   WS_CONNECTIONS_ACCEPTED.load(Ordering::Relaxed),
        connections_rejected:   WS_CONNECTIONS_REJECTED.load(Ordering::Relaxed),
        subscriptions_accepted: WS_SUBSCRIPTIONS_ACCEPTED.load(Ordering::Relaxed),
        subscriptions_rejected: WS_SUBSCRIPTIONS_REJECTED.load(Ordering::Relaxed),
    }
}

/// Reset all subscription counters to zero.
///
/// Call this at the start of each test that checks counter values to avoid
/// cross-test interference from the module-level statics.
#[cfg(test)]
pub fn reset_metrics_for_test() {
    WS_CONNECTIONS_ACCEPTED.store(0, Ordering::SeqCst);
    WS_CONNECTIONS_REJECTED.store(0, Ordering::SeqCst);
    WS_SUBSCRIPTIONS_ACCEPTED.store(0, Ordering::SeqCst);
    WS_SUBSCRIPTIONS_REJECTED.store(0, Ordering::SeqCst);
}

/// Snapshot of subscription counters.
pub struct SubscriptionMetrics {
    /// Total `WebSocket` connections accepted (after `on_connect`).
    pub connections_accepted:   u64,
    /// Total `WebSocket` connections rejected by lifecycle hook.
    pub connections_rejected:   u64,
    /// Total subscriptions accepted (after `on_subscribe`).
    pub subscriptions_accepted: u64,
    /// Total subscriptions rejected (by hook or limit).
    pub subscriptions_rejected: u64,
}

/// Connection initialization timeout (5 seconds per graphql-ws spec).
const CONNECTION_INIT_TIMEOUT: Duration = Duration::from_secs(5);

/// Ping/keepalive interval.
const PING_INTERVAL: Duration = Duration::from_secs(30);

/// A live source of per-subscription row-visibility policies (#611).
///
/// Returns the CURRENT compiled schema's `subscription_policy` map. Installed at mount over
/// the reload-aware executor `ArcSwap`, so a policy added or changed by a hot-reload reaches
/// the next subscribe — not only a restart. Type-erased over the database adapter.
pub type LiveSubscriptionPolicies =
    Arc<dyn Fn() -> Arc<HashMap<String, SubscriptionPolicy>> + Send + Sync>;

/// State for subscription `WebSocket` handler.
#[derive(Clone)]
pub struct SubscriptionState {
    /// Subscription manager.
    pub manager: Arc<SubscriptionManager>,
    /// Lifecycle hooks.
    pub lifecycle: Arc<dyn SubscriptionLifecycle>,
    /// Maximum subscriptions per connection (`None` = unlimited).
    pub max_subscriptions_per_connection: Option<u32>,
    /// Subscription fields owned by remote subgraphs.
    ///
    /// Maps root subscription field name to the subgraph `WebSocket` URL.
    /// Empty when federation is disabled or no remote subscription fields are declared.
    pub remote_subscription_fields: Arc<HashMap<String, String>>,
    /// Host-header → tenant-key domain registry. `None` until a host binary
    /// installs one (mirrors `AppState::domain_registry`).
    pub domain_registry: Option<Arc<DomainRegistry>>,
    /// Reject conflicting tenant sources (JWT vs `X-Tenant-ID` vs Host) on the
    /// upgrade. Driven by `schema.has_rls_configured()`, mirroring the GraphQL
    /// handler's strict tenant validation.
    pub strict_tenant_validation: bool,
    /// Optional operation-level authorizer (#422). When set, each subscription is
    /// authorized at establishment with [`OperationKind::Subscription`], the
    /// subscription field name, and the connection's principal. `None` until a host
    /// binary installs one (from `Executor::config().authorizer`).
    pub authorizer: Option<Arc<dyn Authorizer>>,
    /// Optional tenant-status source (M-tenant-ws-suspended). When set, a new
    /// subscription whose resolved tenant is suspended is rejected, and event
    /// delivery to a connection whose tenant is suspended is paused. `None` until
    /// a host binary installs a multi-tenant registry.
    pub tenant_status_source: Option<Arc<dyn TenantStatusSource>>,
    /// Per-subscription-field row-visibility policies (#596), keyed by subscription
    /// field name, resolved at mount time from the target entity's compiled
    /// `subscription_policy`. A subscription named here derives a **server-owned** owner
    /// condition from the connection's enriched identity at subscribe time — fail-closed
    /// when the identity is unresolvable. A subscription with no policy keeps today's
    /// behavior (no back-compat break).
    pub subscription_policies: Arc<HashMap<String, SubscriptionPolicy>>,
    /// #611: live row-visibility policy source. When set (installed at mount from the
    /// reload-aware executor `ArcSwap`), each **new** subscription is evaluated against the
    /// **current** schema's policies, so a policy added or tightened by a hot-reload takes
    /// effect on the next subscribe rather than only on restart. `None` falls back to the
    /// mount-time `subscription_policies` snapshot — the behavior tests and hosts that do
    /// not wire a live source keep. Already-connected subscriptions are handled by
    /// [`policy_reload`](Self::policy_reload) (layer-2).
    pub live_subscription_policies: Option<LiveSubscriptionPolicies>,
    /// Enriched-identity resolver (#539). When set, the connection's `SecurityContext`
    /// is enriched at subscribe time (only for policy-declaring subscriptions) so the
    /// `fraiseql.enriched.*` owner field is server-resolved, never client-asserted.
    /// `None` disables enrichment — a policy-declaring subscription then fails closed.
    #[cfg(feature = "auth")]
    pub identity_resolver: Option<Arc<crate::identity::IdentityResolver>>,
    /// Service-account authenticator (ADR-0018). Lets a daemon authenticate the `/ws`
    /// upgrade with its secret on the api-key header — the same seam the GraphQL path
    /// uses — so a service principal can hold a policy-scoped subscription.
    pub service_account_authenticator:
        Option<Arc<crate::service_account::ServiceAccountAuthenticator>>,
    /// #611 (layer 2): hot-reload signal for row-visibility policies. Each bump of
    /// the watched generation makes every live connection re-derive the RLS
    /// conditions of its active subscriptions against the **current** policies
    /// (via [`live_subscription_policies`](Self::live_subscription_policies)):
    /// a subscription whose re-derivation refuses (fail-closed) is terminated
    /// with an error frame; one that re-derives is re-scoped in place. `None`
    /// leaves already-connected subscriptions on their subscribe-time boundary
    /// until they reconnect.
    pub policy_reload: Option<tokio::sync::watch::Receiver<u64>>,
    /// #571: server drain signal. When the watched value flips to `true`, every
    /// connection sends a graphql-transport-ws `Complete` frame per active
    /// operation and closes the socket with 1001 (Going Away), so clients see a
    /// clean end-of-stream during a rolling deploy instead of a transport-level
    /// abort. `None` keeps the abrupt-close behaviour (test harnesses).
    pub drain: Option<tokio::sync::watch::Receiver<bool>>,
    /// Token revocation manager (#771). When set, the periodic mid-stream
    /// authorization re-check also consults the revocation store, so revoking a
    /// token (or `revoke-all` for a user) terminates that user's live
    /// subscriptions within one [`auth_recheck_interval`](Self::auth_recheck_interval)
    /// instead of streaming until the client disconnects. `None` limits the
    /// re-check to token expiry.
    pub revocation_manager: Option<Arc<crate::token_revocation::TokenRevocationManager>>,
    /// How often the mid-stream authorization re-check runs (#771). Token expiry
    /// is additionally enforced on every event delivery, so this interval only
    /// bounds how long an idle expired stream, or a revoked-but-unexpired one,
    /// can stay open. `Duration::ZERO` disables the periodic check.
    pub auth_recheck_interval: Duration,
}

impl SubscriptionState {
    /// Create new subscription state.
    pub fn new(manager: Arc<SubscriptionManager>) -> Self {
        Self {
            manager,
            lifecycle: Arc::new(crate::subscriptions::lifecycle::NoopLifecycle),
            max_subscriptions_per_connection: None,
            remote_subscription_fields: Arc::new(HashMap::new()),
            domain_registry: None,
            strict_tenant_validation: false,
            authorizer: None,
            tenant_status_source: None,
            subscription_policies: Arc::new(HashMap::new()),
            live_subscription_policies: None,
            #[cfg(feature = "auth")]
            identity_resolver: None,
            service_account_authenticator: None,
            policy_reload: None,
            drain: None,
            revocation_manager: None,
            auth_recheck_interval: Duration::from_secs(
                crate::server_config::defaults::default_subscription_auth_recheck_secs(),
            ),
        }
    }

    /// Install the hot-reload policy signal (#611 layer 2). Each bump of the
    /// watched generation makes live connections re-derive (or terminate) their
    /// active subscriptions against the current row-visibility policies.
    #[must_use]
    pub fn with_policy_reload(
        mut self,
        policy_reload: Option<tokio::sync::watch::Receiver<u64>>,
    ) -> Self {
        self.policy_reload = policy_reload;
        self
    }

    /// Install the server drain signal (#571). When it flips to `true`, live
    /// connections complete their operations and close gracefully (1001).
    #[must_use]
    pub fn with_drain_signal(mut self, drain: Option<tokio::sync::watch::Receiver<bool>>) -> Self {
        self.drain = drain;
        self
    }

    /// Install the token revocation manager (#771) consulted by the periodic
    /// mid-stream authorization re-check. `None` limits the re-check to expiry.
    #[must_use]
    pub fn with_revocation_manager(
        mut self,
        manager: Option<Arc<crate::token_revocation::TokenRevocationManager>>,
    ) -> Self {
        self.revocation_manager = manager;
        self
    }

    /// Set how often the mid-stream authorization re-check runs (#771).
    /// `Duration::ZERO` disables the periodic check (per-delivery expiry
    /// enforcement remains).
    #[must_use]
    pub const fn with_auth_recheck_interval(mut self, interval: Duration) -> Self {
        self.auth_recheck_interval = interval;
        self
    }

    /// Install the per-subscription row-visibility policies (#596). Typically built by
    /// [`build_subscription_policies`] from the compiled schema at mount time. Used as the
    /// fallback when no live source (`with_live_subscription_policies`) is installed.
    #[must_use]
    pub fn with_subscription_policies(
        mut self,
        policies: Arc<HashMap<String, SubscriptionPolicy>>,
    ) -> Self {
        self.subscription_policies = policies;
        self
    }

    /// Install the live row-visibility policy source (#611). When set, each new subscription
    /// is evaluated against the current schema's policies, so a hot-reloaded policy applies
    /// on the next subscribe rather than only on restart. `None` keeps the mount-time
    /// snapshot behavior.
    #[must_use]
    pub fn with_live_subscription_policies(
        mut self,
        live: Option<LiveSubscriptionPolicies>,
    ) -> Self {
        self.live_subscription_policies = live;
        self
    }

    /// Install the service-account authenticator (ADR-0018) so a daemon can authenticate
    /// the `/ws` upgrade with its secret on the api-key header.
    #[must_use]
    pub fn with_service_account_authenticator(
        mut self,
        authenticator: Option<Arc<crate::service_account::ServiceAccountAuthenticator>>,
    ) -> Self {
        self.service_account_authenticator = authenticator;
        self
    }

    /// Install the enriched-identity resolver (#539) used to derive row-visibility owner
    /// boundaries at subscribe time. `None` leaves policy-declaring subscriptions
    /// fail-closed (refused).
    #[cfg(feature = "auth")]
    #[must_use]
    pub fn with_identity_resolver(
        mut self,
        resolver: Option<Arc<crate::identity::IdentityResolver>>,
    ) -> Self {
        self.identity_resolver = resolver;
        self
    }

    /// Install the tenant-status source (M-tenant-ws-suspended). When set, new
    /// subscriptions for a suspended tenant are rejected and event delivery to a
    /// suspended tenant is paused. Typically the `TenantExecutorRegistry`.
    #[must_use]
    pub fn with_tenant_status_source(
        mut self,
        source: Option<Arc<dyn TenantStatusSource>>,
    ) -> Self {
        self.tenant_status_source = source;
        self
    }

    /// Install the operation-level authorizer (#422). When set, every subscription is
    /// authorized at establishment; a `Deny` (or any policy error) rejects the
    /// subscription with a `FORBIDDEN` GraphQL-WS error. Typically populated from
    /// `Executor::config().authorizer`.
    #[must_use]
    pub fn with_authorizer(mut self, authorizer: Option<Arc<dyn Authorizer>>) -> Self {
        self.authorizer = authorizer;
        self
    }

    /// Install the tenant-resolution context — the Host-header domain registry
    /// and strict-validation flag — so the subscription upgrade dispatches the
    /// tenant key the same way the GraphQL handler does (JWT `tenant_id` >
    /// `X-Tenant-ID` header > Host domain, with cross-source conflict rejection
    /// when `strict_tenant_validation` is set). See
    /// [`crate::routes::graphql::TenantKeyResolver`].
    #[must_use]
    pub fn with_tenant_context(
        mut self,
        domain_registry: Arc<DomainRegistry>,
        strict_tenant_validation: bool,
    ) -> Self {
        self.domain_registry = Some(domain_registry);
        self.strict_tenant_validation = strict_tenant_validation;
        self
    }

    /// Set lifecycle hooks.
    #[must_use]
    pub fn with_lifecycle(mut self, lifecycle: Arc<dyn SubscriptionLifecycle>) -> Self {
        self.lifecycle = lifecycle;
        self
    }

    /// Set maximum subscriptions per connection.
    #[must_use]
    pub const fn with_max_subscriptions(mut self, max: Option<u32>) -> Self {
        self.max_subscriptions_per_connection = max;
        self
    }

    /// Set remote subscription fields (federation passthrough).
    ///
    /// Maps subscription field names to the owning subgraph's `WebSocket` URL.
    #[must_use]
    pub fn with_remote_subscription_fields(mut self, fields: HashMap<String, String>) -> Self {
        self.remote_subscription_fields = Arc::new(fields);
        self
    }
}

/// Build the per-subscription row-visibility policy map (#596) from the compiled schema.
///
/// For each subscription field, resolve its target entity's `subscription_policy` (if
/// any) and key it by the subscription field name. The `/ws` handler consults this map
/// at subscribe time; an entry means "derive a server-owned owner condition and refuse
/// if the identity is unresolvable", absence means "unchanged behavior".
#[must_use]
pub fn build_subscription_policies(schema: &CompiledSchema) -> HashMap<String, SubscriptionPolicy> {
    let mut policies = HashMap::new();
    for sub in &schema.subscriptions {
        if let Some(type_def) = schema.types.iter().find(|t| t.name.as_str() == sub.return_type) {
            if let Some(policy) = &type_def.subscription_policy {
                policies.insert(sub.name.clone(), policy.clone());
            }
        }
    }
    policies
}

/// Resolve the server-owned RLS conditions to enforce for a subscribe request (#596).
///
/// - `Ok(vec![])` — the subscription declares no policy (back-compat) **or** the principal holds a
///   bypass role (full visibility);
/// - `Ok(vec![(owner_field, identity_value)])` — a scoped subscriber sees only its rows;
/// - `Err(reason)` — the policy applies but the identity is unresolvable; the caller **refuses**
///   the subscription (fail-closed, never deliver-all).
async fn resolve_subscription_rls(
    state: &SubscriptionState,
    subscription_name: &str,
    principal: Option<&SecurityContext>,
) -> Result<Vec<(String, serde_json::Value)>, String> {
    // #611: when a live source is installed, evaluate against the CURRENT schema's policies
    // so a hot-reloaded policy applies to this new subscription; otherwise the mount-time
    // snapshot. Fail-closed is preserved downstream: a policy that applies but whose owner
    // identity is unresolvable refuses the subscription (never deliver-all).
    let policies = state
        .live_subscription_policies
        .as_ref()
        .map_or_else(|| Arc::clone(&state.subscription_policies), |live| live());
    let Some(policy) = policies.get(subscription_name) else {
        return Ok(Vec::new());
    };

    // Enrich a copy of the principal so the owner boundary derives from the
    // server-resolved `fraiseql.enriched.*` namespace, never a client claim. A failed
    // or absent enrichment leaves the enriched field unresolvable → the derivation
    // refuses below (fail-closed). Enrichment runs only for policy-declaring
    // subscriptions, so unscoped streams pay nothing.
    #[cfg(feature = "auth")]
    let enriched = enrich_principal(state, principal).await;
    #[cfg(feature = "auth")]
    let effective = enriched.as_ref().or(principal);
    #[cfg(not(feature = "auth"))]
    let effective = principal;

    derive_policy_conditions(policy, effective)
}

/// Enrich a clone of the principal's `SecurityContext` (#539). Best-effort: on a denial
/// or resolver outage the enriched fields are simply absent, so a policy-declaring
/// subscription fails closed at derivation. Returns `None` when there is no principal
/// or no resolver configured.
#[cfg(feature = "auth")]
async fn enrich_principal(
    state: &SubscriptionState,
    principal: Option<&SecurityContext>,
) -> Option<SecurityContext> {
    let resolver = state.identity_resolver.as_ref()?;
    let mut ctx = principal?.clone();
    let _ = crate::identity::enrich_security_context(resolver, &mut ctx).await;
    Some(ctx)
}

/// Adapt a policy's seam-neutral [`OwnerCondition`] to the `/ws` seam's `(field, value)`
/// condition channel (#596). Fail-closed: `Refuse` → `Err`. A `None` principal (or one
/// lacking the enriched field) derives to `Refuse`, so anonymous subscribers cannot see
/// a policy-scoped entity's rows.
fn derive_policy_conditions(
    policy: &SubscriptionPolicy,
    principal: Option<&SecurityContext>,
) -> Result<Vec<(String, serde_json::Value)>, String> {
    let empty = HashMap::new();
    let attributes = principal.map_or(&empty, |ctx| &ctx.attributes);
    let roles: &[String] = principal.map_or(&[], |ctx| ctx.roles.as_slice());
    match policy.derive(attributes, roles) {
        OwnerCondition::Bypass => Ok(Vec::new()),
        OwnerCondition::Eq { field, value } => Ok(vec![(field, value)]),
        OwnerCondition::Refuse(reason) => Err(reason),
    }
}

/// The per-connection authorization guard for a long-lived stream (#771).
///
/// A subscription's principal is validated once at the upgrade and then held for an
/// unbounded duration; this guard is what keeps that trust honest. It is re-checked
/// periodically (every [`SubscriptionState::auth_recheck_interval`]) and — for expiry
/// — on every event delivery:
///
/// - **Expiry** is a clock comparison against the principal's `expires_at` (the JWT `exp` claim;
///   service-account principals carry their mint-time ceiling).
/// - **Revocation** consults the configured revocation store (never the `IdP`) with the token's
///   `jti`/`iat` claims, exactly like the HTTP middleware does at request time. It applies only to
///   JWT principals (the claims extension is the marker); a store outage follows the manager's
///   configured fail-open/fail-closed posture.
///
/// Any failure terminates the connection with close code 4401.
struct StreamAuthGuard {
    /// When the principal's token expires. `None` for anonymous connections.
    expires_at: Option<chrono::DateTime<chrono::Utc>>,
    /// The principal's subject, for the revocation `revoke-all` epoch check.
    sub:        Option<String>,
    /// The validated bearer token's `jti`/`iat` claims. `None` when the
    /// connection did not authenticate with a JWT — which also disables the
    /// revocation re-check for it.
    claims:     Option<crate::middleware::oidc_auth::SessionTokenClaims>,
    /// The revocation store to consult, when configured.
    revocation: Option<Arc<crate::token_revocation::TokenRevocationManager>>,
}

impl StreamAuthGuard {
    fn new(
        principal: Option<&SecurityContext>,
        claims: Option<crate::middleware::oidc_auth::SessionTokenClaims>,
        revocation: Option<Arc<crate::token_revocation::TokenRevocationManager>>,
    ) -> Self {
        Self {
            expires_at: principal.map(|p| p.expires_at),
            sub: principal.map(|p| p.user_id.to_string()),
            // Revocation is a JWT concern: without decoded token claims there is
            // no jti/iat to check (anonymous or service-account connection).
            revocation: if claims.is_some() { revocation } else { None },
            claims,
        }
    }

    /// Whether any mid-stream re-check applies to this connection.
    const fn applies(&self) -> bool {
        self.expires_at.is_some()
    }

    /// Cheap per-delivery check: has the principal's token expired?
    fn expired(&self) -> bool {
        self.expires_at.is_some_and(|exp| exp <= chrono::Utc::now())
    }

    /// The periodic re-check: expiry plus revocation. Returns the close reason
    /// when the connection must be terminated.
    async fn check(&self) -> Result<(), &'static str> {
        if self.expired() {
            return Err("Token expired");
        }
        if let (Some(revocation), Some(sub), Some(claims)) =
            (self.revocation.as_ref(), self.sub.as_deref(), self.claims.as_ref())
        {
            use crate::token_revocation::TokenRejection;
            match revocation.check_token(claims.jti.as_deref(), sub, claims.iat).await {
                Ok(()) => {},
                Err(TokenRejection::Revoked) => return Err("Token revoked"),
                Err(TokenRejection::MissingJti) => return Err("Token lacks required jti claim"),
                // The manager already applied its fail-open posture internally; an
                // error here means fail-closed is configured — terminate.
                Err(TokenRejection::StoreUnavailable) => {
                    return Err("Revocation store unavailable");
                },
            }
        }
        Ok(())
    }
}

/// `WebSocket` upgrade handler for subscriptions.
///
/// Negotiates the `WebSocket` sub-protocol from the `Sec-WebSocket-Protocol`
/// header. Supports `graphql-transport-ws` (modern) and `graphql-ws` (legacy).
/// Defaults to `graphql-transport-ws` when no header is present.
/// Returns `400 Bad Request` for unrecognised protocols.
pub async fn subscription_handler(
    headers: HeaderMap,
    OptionalSecurityContext(security_context): OptionalSecurityContext,
    token_claims: Option<axum::Extension<crate::middleware::oidc_auth::SessionTokenClaims>>,
    ws: WebSocketUpgrade,
    State(state): State<SubscriptionState>,
) -> impl IntoResponse {
    let protocol_header = headers.get("sec-websocket-protocol").and_then(|v| v.to_str().ok());

    let protocol = match protocol_header {
        None => WsProtocol::GraphqlTransportWs,
        Some(header) => {
            if let Some(p) = WsProtocol::from_header(Some(header)) {
                p
            } else {
                warn!(header = %header, "Unknown WebSocket sub-protocol requested");
                return axum::http::StatusCode::BAD_REQUEST.into_response();
            }
        },
    };

    // ADR-0018: authenticate a service account presenting its secret on the api-key
    // header — the same seam the GraphQL path uses — so a service principal can hold a
    // policy-scoped subscription. A JWT principal AND a secret on one upgrade is
    // ambiguous (#602) → 401; a present-but-unmatched secret → 401 (indistinguishable
    // from an unknown account, no oracle).
    let mut security_context = security_context;
    if let Some(sa_auth) = state.service_account_authenticator.as_ref() {
        match sa_auth.resolve(&headers, security_context.is_some()) {
            crate::service_account::SaAuth::NoSecret => {},
            crate::service_account::SaAuth::Authenticated(ctx) => security_context = Some(*ctx),
            crate::service_account::SaAuth::Ambiguous
            | crate::service_account::SaAuth::Unmatched => {
                warn!(
                    "Subscription upgrade rejected: ambiguous or unmatched service-account secret"
                );
                return axum::http::StatusCode::UNAUTHORIZED.into_response();
            },
        }
    }

    // Resolve the tenant key exactly as the GraphQL handler does: the JWT
    // `tenant_id` (trusted) takes precedence over the `X-Tenant-ID` header and
    // the Host-domain lookup, and conflicting sources are rejected when strict
    // validation is enabled. Previously this passed `None, None, false`,
    // silently dropping JWT precedence, ignoring an installed domain registry,
    // and disabling strict cross-source validation (#331).
    let tenant_id = match resolve_subscription_tenant(security_context.as_ref(), &headers, &state) {
        Ok(tenant_id) => tenant_id,
        Err(e) => {
            warn!(error = %e, "Subscription tenant resolution rejected the upgrade");
            return axum::http::StatusCode::BAD_REQUEST.into_response();
        },
    };

    ws.protocols([protocol.as_str()])
        .on_upgrade(move |socket| {
            handle_subscription_connection(
                socket,
                state,
                protocol,
                tenant_id,
                security_context,
                token_claims.map(|axum::Extension(claims)| claims),
            )
        })
        .into_response()
}

/// Resolve the subscription's tenant key, mirroring the GraphQL handler's
/// dispatch (`routes/graphql/handler.rs`): JWT `tenant_id` > `X-Tenant-ID`
/// header > Host-domain registry, with cross-source conflict rejection when the
/// schema has RLS configured (`strict_tenant_validation`).
///
/// # Errors
///
/// Returns `FraiseQLError::Validation` when the `X-Tenant-ID` header is invalid,
/// or — under `strict_tenant_validation` — when the resolved sources disagree.
fn resolve_subscription_tenant(
    security_context: Option<&SecurityContext>,
    headers: &HeaderMap,
    state: &SubscriptionState,
) -> fraiseql_error::Result<Option<String>> {
    super::graphql::TenantKeyResolver::resolve(
        security_context,
        headers,
        state.domain_registry.as_deref(),
        state.strict_tenant_validation,
    )
}

/// Handle a `WebSocket` subscription connection.
#[allow(clippy::cognitive_complexity)] // Reason: WebSocket protocol state machine with message routing and lifecycle management
async fn handle_subscription_connection(
    socket: WebSocket,
    state: SubscriptionState,
    protocol: WsProtocol,
    tenant_id: Option<String>,
    principal: Option<SecurityContext>,
    token_claims: Option<crate::middleware::oidc_auth::SessionTokenClaims>,
) {
    let connection_id = uuid::Uuid::new_v4().to_string();
    let codec = ProtocolCodec::new(protocol);
    info!(
        connection_id = %connection_id,
        protocol = %protocol.as_str(),
        "WebSocket connection established"
    );

    let (mut sender, mut receiver) = socket.split();

    // Wait for connection_init with timeout
    let init_result = tokio::time::timeout(CONNECTION_INIT_TIMEOUT, async {
        while let Some(msg) = receiver.next().await {
            match msg {
                Ok(Message::Text(text)) => {
                    if let Ok(client_msg) = codec.decode(&text) {
                        if client_msg.parsed_type() == Some(ClientMessageType::ConnectionInit) {
                            return Some(client_msg);
                        }
                    }
                },
                Ok(Message::Close(_)) => return None,
                Err(e) => {
                    error!(error = %e, "WebSocket error during init");
                    return None;
                },
                _ => {},
            }
        }
        None
    })
    .await;

    // Handle init timeout or failure
    let _init_payload = match init_result {
        Ok(Some(msg)) => {
            // Call lifecycle on_connect hook
            let params = msg.payload.clone().unwrap_or(serde_json::json!({}));
            if let Err(reason) = state.lifecycle.on_connect(&params, &connection_id).await {
                warn!(
                    connection_id = %connection_id,
                    reason = %reason,
                    "Lifecycle on_connect rejected connection"
                );
                WS_CONNECTIONS_REJECTED.fetch_add(1, Ordering::Relaxed);
                // Best-effort: connection is already being terminated.
                let _ = sender
                    .send(Message::Close(Some(axum::extract::ws::CloseFrame {
                        code:   4400,
                        reason: reason.into(),
                    })))
                    .await;
                return;
            }

            // Send connection_ack
            let ack = ServerMessage::connection_ack(None);
            if let Err(send_err) = send_server_message(&codec, &mut sender, ack).await {
                error!(connection_id = %connection_id, error = %send_err, "Failed to send connection_ack");
                return;
            }
            WS_CONNECTIONS_ACCEPTED.fetch_add(1, Ordering::Relaxed);
            info!(connection_id = %connection_id, "Connection initialized");
            msg.payload
        },
        Ok(None) => {
            warn!(connection_id = %connection_id, "Connection closed during init");
            return;
        },
        Err(_) => {
            warn!(connection_id = %connection_id, "Connection init timeout");
            // Best-effort: connection is already being terminated.
            let _ = sender
                .send(Message::Close(Some(axum::extract::ws::CloseFrame {
                    code:   CloseCode::ConnectionInitTimeout.code(),
                    reason: CloseCode::ConnectionInitTimeout.reason().into(),
                })))
                .await;
            return;
        },
    };

    // Track active operations (operation_id -> subscription id + name)
    let mut active_operations: HashMap<String, ActiveOperation> = HashMap::new();

    // #611 (layer 2): watch for hot-reloaded row-visibility policies. On each bump,
    // every active operation re-derives its conditions against the current policies —
    // re-scoped in place, or terminated when the new policy refuses (fail-closed).
    let mut policy_reload = state.policy_reload.clone();

    // #571: watch for server drain. On the signal, complete every active operation
    // and close gracefully instead of letting the shutdown abort the socket.
    let mut drain = state.drain.clone();

    // Remote subscription message output channel.
    //
    // Forwarder tasks (federation feature) send pre-encoded ServerMessage values here.
    // The channel is always present so the select! loop has a uniform branch regardless
    // of whether any remote subscriptions are active.
    let (remote_msg_tx, mut remote_msg_rx) = tokio::sync::mpsc::channel::<ServerMessage>(64);

    // Subscribe to event broadcast
    let mut event_receiver = state.manager.receiver();

    // Ping/keepalive timer
    let mut ping_interval = tokio::time::interval(PING_INTERVAL);
    ping_interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);

    // #771 (was A44): authorization for the life of the stream. The principal was
    // validated once at the upgrade; this guard re-checks token expiry (and, when a
    // revocation store is configured, revocation) periodically, and expiry again on
    // every event delivery. A failed check closes the socket with 4401.
    let auth_guard =
        StreamAuthGuard::new(principal.as_ref(), token_claims, state.revocation_manager.clone());
    let auth_recheck_enabled = auth_guard.applies() && !state.auth_recheck_interval.is_zero();
    let mut auth_recheck =
        tokio::time::interval(state.auth_recheck_interval.max(Duration::from_millis(1)));
    auth_recheck.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
    // The first `interval` tick fires immediately; consume it so the branch below
    // only fires after a full interval has elapsed (the token was just validated).
    auth_recheck.tick().await;

    // Main message loop
    loop {
        tokio::select! {
            msg = receiver.next() => {
                match msg {
                    Some(Ok(Message::Text(text))) => {
                        if let Err(close_code) = handle_client_message(
                            &text,
                            &connection_id,
                            &state,
                            &codec,
                            &mut active_operations,
                            remote_msg_tx.clone(),
                            &mut sender,
                            tenant_id.as_deref(),
                            principal.as_ref(),
                        ).await {
                            // Best-effort: connection is already being closed.
                            let _ = sender.send(Message::Close(Some(axum::extract::ws::CloseFrame {
                                code: close_code.code(),
                                reason: close_code.reason().into(),
                            }))).await;
                            break;
                        }
                    }
                    Some(Ok(Message::Ping(data))) => {
                        // Best-effort: if the connection is already dead the pong will fail.
                        let _ = sender.send(Message::Pong(data)).await;
                    }
                    Some(Ok(Message::Close(_))) => {
                        info!(connection_id = %connection_id, "Client closed connection");
                        break;
                    }
                    Some(Err(e)) => {
                        error!(connection_id = %connection_id, error = %e, "WebSocket error");
                        break;
                    }
                    None => {
                        info!(connection_id = %connection_id, "WebSocket stream ended");
                        break;
                    }
                    _ => {}
                }
            }

            draining = drain_signalled(&mut drain), if drain.is_some() => {
                if draining {
                    // #571: graceful drain — complete every active operation, then
                    // close with 1001 (Going Away) so clients see a clean
                    // end-of-stream rather than a transport abort.
                    info!(
                        connection_id = %connection_id,
                        active_operations = active_operations.len(),
                        "Server draining; completing active subscriptions"
                    );
                    for (op_id, op) in active_operations.drain() {
                        let complete = ServerMessage::complete(&op_id);
                        if let Err(e) = send_server_message(&codec, &mut sender, complete).await {
                            debug!(connection_id = %connection_id, error = %e, "Could not send Complete during drain");
                        }
                        if let Err(e) = state.manager.unsubscribe(op.subscription_id) {
                            debug!(connection_id = %connection_id, operation_id = %op_id, error = %e, "Failed to unsubscribe during drain");
                        }
                        state.lifecycle.on_unsubscribe(&op_id, &connection_id).await;
                    }
                    // Best-effort: the connection is being terminated either way.
                    let _ = sender.send(Message::Close(Some(axum::extract::ws::CloseFrame {
                        code: CloseCode::GoingAway.code(),
                        reason: CloseCode::GoingAway.reason().into(),
                    }))).await;
                    break;
                }
                // Sender gone without signalling: stop watching, keep serving.
                drain = None;
            }

            changed = policy_watch_changed(&mut policy_reload), if policy_reload.is_some() => {
                if changed {
                    rederive_operations_after_policy_reload(
                        &state,
                        &codec,
                        &mut sender,
                        &mut active_operations,
                        &connection_id,
                        principal.as_ref(),
                    ).await;
                } else {
                    // Sender gone (state torn down): stop watching, keep serving.
                    policy_reload = None;
                }
            }

            _ = auth_recheck.tick(), if auth_recheck_enabled => {
                // #771: periodic mid-stream authorization re-check. Bounds how long
                // an idle expired stream, or a revoked-but-unexpired one, stays open.
                if let Err(reason) = auth_guard.check().await {
                    warn!(
                        connection_id = %connection_id,
                        reason,
                        "Mid-stream authorization re-check failed; closing WebSocket"
                    );
                    // Best-effort: the connection is being terminated either way.
                    let _ = sender.send(Message::Close(Some(axum::extract::ws::CloseFrame {
                        code: CloseCode::Unauthorized.code(),
                        reason: reason.into(),
                    }))).await;
                    break;
                }
            }

            event = event_receiver.recv() => {
                match event {
                    Ok(payload) => {
                        // #771: never deliver an event on an expired token, regardless
                        // of where the periodic re-check is in its interval.
                        if auth_guard.expired() {
                            warn!(
                                connection_id = %connection_id,
                                "Token expired at delivery time; closing WebSocket"
                            );
                            // Best-effort: the connection is being terminated either way.
                            let _ = sender.send(Message::Close(Some(axum::extract::ws::CloseFrame {
                                code: CloseCode::Unauthorized.code(),
                                reason: "Token expired".into(),
                            }))).await;
                            break;
                        }
                        // Defense-in-depth tenant guard: when both the connection and the
                        // event carry an explicit tenant_id they must agree. Primary
                        // isolation is already guaranteed by subscription_id UUIDs, but
                        // this check catches any future path that introduces deterministic
                        // subscription IDs (which could collide across tenants).
                        let tenant_matches = match (
                            tenant_id.as_deref(),
                            payload.event.tenant_id.as_deref(),
                        ) {
                            (Some(conn_tid), Some(evt_tid)) => conn_tid == evt_tid,
                            _ => true, // either side absent → no conflict
                        };
                        // M-tenant-ws-suspended: pause delivery while this
                        // connection's tenant is suspended (re-checked per event so a
                        // suspension mid-stream stops further delivery).
                        let tenant_active = match (
                            tenant_id.as_deref(),
                            state.tenant_status_source.as_ref(),
                        ) {
                            (Some(tid), Some(src)) => !src.is_suspended(tid),
                            _ => true,
                        };
                        if tenant_matches && tenant_active {
                            if let Some((op_id, _)) = active_operations
                                .iter()
                                .find(|(_, op)| op.subscription_id == payload.subscription_id)
                            {
                                let msg = create_next_message(op_id, &payload);
                                if send_server_message(&codec, &mut sender, msg).await.is_err() {
                                    warn!(connection_id = %connection_id, "Failed to send event");
                                    break;
                                }
                            }
                        }
                    }
                    Err(broadcast::error::RecvError::Lagged(n)) => {
                        // #772: this connection's receiver fell behind the broadcast
                        // channel and events for its own subscriptions were skipped. A
                        // client that is not told cannot distinguish "nothing happened"
                        // from "events were dropped" — for a change-feed that is silent
                        // data loss. Terminate every active operation with an explicit
                        // EVENTS_LAGGED error so the client knows it must re-subscribe
                        // and re-query to resynchronize.
                        warn!(
                            connection_id = %connection_id,
                            lagged = n,
                            active_operations = active_operations.len(),
                            "Event receiver lagged; terminating operations with EVENTS_LAGGED"
                        );
                        terminate_operations_after_lag(
                            &state,
                            &codec,
                            &mut sender,
                            &mut active_operations,
                            &connection_id,
                            n,
                        )
                        .await;
                    }
                    Err(broadcast::error::RecvError::Closed) => {
                        error!(connection_id = %connection_id, "Event channel closed");
                        break;
                    }
                }
            }

            remote_msg = remote_msg_rx.recv() => {
                // Remote subscription event forwarded by a federation forwarder task.
                // The channel is always present; it only carries messages when the
                // federation feature is enabled and remote subscriptions are active.
                if let Some(msg) = remote_msg {
                    if send_server_message(&codec, &mut sender, msg).await.is_err() {
                        warn!(connection_id = %connection_id, "Failed to send remote subscription message");
                        break;
                    }
                }
            }

            _ = ping_interval.tick() => {
                let msg = ServerMessage::ping(None);
                if send_server_message(&codec, &mut sender, msg).await.is_err() {
                    warn!(connection_id = %connection_id, "Failed to send ping/keepalive");
                    break;
                }
            }
        }
    }

    // Cleanup
    state.manager.unsubscribe_connection(&connection_id);
    state.lifecycle.on_disconnect(&connection_id).await;
    info!(connection_id = %connection_id, "WebSocket connection closed");
}

/// A live operation on a connection: the manager-side subscription id plus the
/// subscription field name it was established for (needed to re-derive its
/// row-visibility conditions on a policy hot-reload, #611).
struct ActiveOperation {
    subscription_id:   SubscriptionId,
    subscription_name: String,
}

/// Wait for the next policy-reload signal (#611). Returns `true` when the
/// generation changed, `false` when the sender is gone. Pends forever when no
/// watch is installed, so the `select!` branch never fires.
async fn policy_watch_changed(rx: &mut Option<tokio::sync::watch::Receiver<u64>>) -> bool {
    match rx {
        Some(rx) => rx.changed().await.is_ok(),
        None => std::future::pending().await,
    }
}

/// Wait for the server drain signal (#571). Returns `true` once draining is
/// signalled, `false` when the sender is gone without ever signalling. Pends
/// forever when no watch is installed.
async fn drain_signalled(rx: &mut Option<tokio::sync::watch::Receiver<bool>>) -> bool {
    match rx {
        Some(rx) => {
            // `wait_for` also observes a value set before this connection started
            // watching (a connection opened mid-drain still drains promptly).
            rx.wait_for(|draining| *draining).await.is_ok()
        },
        None => std::future::pending().await,
    }
}

/// Re-derive every active operation's row-visibility conditions after a policy
/// hot-reload (#611, layer 2).
///
/// For each operation, the current policies (via the live source) are re-derived
/// with the connection's principal — the same fail-closed path used at subscribe
/// time. An operation whose derivation still succeeds is re-scoped **in place**
/// (the manager swaps its conditions, effective from the next event); one whose
/// derivation now refuses is terminated with a `SUBSCRIPTION_REFUSED` error frame
/// and unsubscribed, so a tightened policy can never be outlived by a
/// pre-reload connection.
async fn rederive_operations_after_policy_reload(
    state: &SubscriptionState,
    codec: &ProtocolCodec,
    sender: &mut futures::stream::SplitSink<WebSocket, Message>,
    active_operations: &mut HashMap<String, ActiveOperation>,
    connection_id: &str,
    principal: Option<&SecurityContext>,
) {
    let ops: Vec<(String, SubscriptionId, String)> = active_operations
        .iter()
        .map(|(op_id, op)| (op_id.clone(), op.subscription_id, op.subscription_name.clone()))
        .collect();

    for (op_id, sub_id, name) in ops {
        match resolve_subscription_rls(state, &name, principal).await {
            Ok(conditions) => {
                // Re-scope in place; `NotActive` means the op raced a disconnect.
                if let Err(e) = state.manager.update_rls_conditions(sub_id, conditions) {
                    debug!(
                        connection_id = %connection_id,
                        operation_id = %op_id,
                        error = %e,
                        "Subscription gone during policy re-derivation"
                    );
                    active_operations.remove(&op_id);
                }
            },
            Err(reason) => {
                warn!(
                    connection_id = %connection_id,
                    operation_id = %op_id,
                    subscription = %name,
                    "Hot-reloaded row-visibility policy refused an active subscription \
                     (fail-closed); terminating the operation"
                );
                active_operations.remove(&op_id);
                let error = ServerMessage::error(
                    &op_id,
                    vec![GraphQLError::with_code(reason, "SUBSCRIPTION_REFUSED")],
                );
                if let Err(e) = send_server_message(codec, sender, error).await {
                    debug!(connection_id = %connection_id, error = %e, "Could not send policy refusal to client");
                }
                if let Err(e) = state.manager.unsubscribe(sub_id) {
                    debug!(connection_id = %connection_id, operation_id = %op_id, error = %e, "Failed to unsubscribe policy-refused operation");
                }
                state.lifecycle.on_unsubscribe(&op_id, connection_id).await;
            },
        }
    }
}

/// Handle a client message.
///
/// Returns `Ok(())` on success, or `Err(CloseCode)` if the connection should be closed.
///
/// `remote_msg_tx` is used by federation forwarder tasks to send pre-encoded
/// `ServerMessage` values back to the client connection loop.
#[allow(clippy::cognitive_complexity)] // Reason: WebSocket message dispatch with subscribe/unsubscribe/query protocol handling
#[allow(clippy::too_many_arguments)] // Reason: WebSocket handler needs connection state, protocol codec, and tenant context
async fn handle_client_message(
    text: &str,
    connection_id: &str,
    state: &SubscriptionState,
    codec: &ProtocolCodec,
    active_operations: &mut HashMap<String, ActiveOperation>,
    remote_msg_tx: tokio::sync::mpsc::Sender<ServerMessage>,
    sender: &mut futures::stream::SplitSink<WebSocket, Message>,
    tenant_id: Option<&str>,
    principal: Option<&SecurityContext>,
) -> Result<(), CloseCode> {
    // remote_msg_tx is only consumed inside the #[cfg(feature = "federation")] block below.
    // When the feature is disabled the parameter goes unused; suppress the warning.
    #[cfg(not(feature = "federation"))]
    let _ = &remote_msg_tx;

    let client_msg: ClientMessage = codec.decode(text).map_err(|e| {
        warn!(error = %e, "Failed to parse client message");
        CloseCode::ProtocolError
    })?;

    match client_msg.parsed_type() {
        Some(ClientMessageType::Ping) => {
            let pong = ServerMessage::pong(client_msg.payload);
            // Best-effort: if the connection is already dead the pong will fail.
            let _ = send_server_message(codec, sender, pong).await;
        },

        Some(ClientMessageType::Pong) => {
            debug!(connection_id = %connection_id, "Received pong");
        },

        Some(ClientMessageType::Subscribe) => {
            let payload: SubscribePayload = client_msg.subscription_payload().ok_or_else(|| {
                warn!("Invalid subscribe payload");
                CloseCode::ProtocolError
            })?;

            let op_id = client_msg.id.ok_or_else(|| {
                warn!("Subscribe message missing operation ID");
                CloseCode::ProtocolError
            })?;

            // Check for duplicate operation ID
            if active_operations.contains_key(&op_id) {
                warn!(operation_id = %op_id, "Duplicate operation ID");
                return Err(CloseCode::SubscriberAlreadyExists);
            }

            // Enforce per-connection subscription limit
            if let Some(max) = state.max_subscriptions_per_connection {
                if active_operations.len() >= max as usize {
                    warn!(
                        connection_id = %connection_id,
                        active = active_operations.len(),
                        max = max,
                        "Subscription limit reached"
                    );
                    WS_SUBSCRIPTIONS_REJECTED.fetch_add(1, Ordering::Relaxed);
                    let error = ServerMessage::error(
                        &op_id,
                        vec![GraphQLError::with_code(
                            format!("Maximum subscriptions per connection ({max}) reached"),
                            "SUBSCRIPTION_LIMIT_REACHED",
                        )],
                    );
                    if let Err(e) = send_server_message(codec, sender, error).await {
                        debug!(connection_id = %connection_id, error = %e, "Could not send subscription limit error to client");
                    }
                    return Ok(());
                }
            }

            // Extract subscription name from query
            let Some(subscription_name) = extract_subscription_name(&payload.query) else {
                let error = ServerMessage::error(
                    &op_id,
                    vec![GraphQLError::with_code(
                        "Could not parse subscription query",
                        "PARSE_ERROR",
                    )],
                );
                if let Err(e) = send_server_message(codec, sender, error).await {
                    debug!(connection_id = %connection_id, error = %e, "Could not send parse error to client");
                }
                return Ok(());
            };

            // Call lifecycle on_subscribe hook
            // HashMap<String, Value> serialization is infallible; the error path cannot occur.
            let variables_value = serde_json::to_value(&payload.variables)
                .expect("HashMap<String, serde_json::Value> serialization is infallible");

            // #422: operation-level authorization at subscription establishment.
            // The per-event delivery does not route through the executor, so the
            // subscription is authorized once, here, with the connection's principal
            // (or `None` when anonymous). Fail-closed: a `Deny` or any policy error
            // rejects the subscription with a `FORBIDDEN` GraphQL-WS error.
            if let Some(authorizer) = state.authorizer.as_ref() {
                let ops = [(OperationKind::Subscription, subscription_name.clone())];
                if let Err(err) =
                    enforce_authz(authorizer.as_ref(), principal, &ops, Some(&variables_value))
                {
                    warn!(
                        connection_id = %connection_id,
                        subscription = %subscription_name,
                        "Operation authorizer denied the subscription"
                    );
                    WS_SUBSCRIPTIONS_REJECTED.fetch_add(1, Ordering::Relaxed);
                    let error = ServerMessage::error(
                        &op_id,
                        vec![GraphQLError::with_code(err.to_string(), "FORBIDDEN")],
                    );
                    if let Err(e) = send_server_message(codec, sender, error).await {
                        debug!(connection_id = %connection_id, error = %e, "Could not send authorization denial to client");
                    }
                    return Ok(());
                }
            }

            if let Err(reason) = state
                .lifecycle
                .on_subscribe(&subscription_name, &variables_value, connection_id)
                .await
            {
                warn!(
                    connection_id = %connection_id,
                    subscription = %subscription_name,
                    reason = %reason,
                    "Lifecycle on_subscribe rejected subscription"
                );
                WS_SUBSCRIPTIONS_REJECTED.fetch_add(1, Ordering::Relaxed);
                let error = ServerMessage::error(
                    &op_id,
                    vec![GraphQLError::with_code(reason, "SUBSCRIPTION_REJECTED")],
                );
                if let Err(e) = send_server_message(codec, sender, error).await {
                    debug!(connection_id = %connection_id, error = %e, "Could not send subscription rejection to client");
                }
                return Ok(());
            }

            // Forward to remote subgraph when the subscription field is owned remotely.
            #[cfg(feature = "federation")]
            if let Some(subgraph_url) = state.remote_subscription_fields.get(&subscription_name) {
                use fraiseql_federation::subscription_forwarder::{
                    ForwardedEvent, SubscriptionForwarder,
                };

                match SubscriptionForwarder::new(subgraph_url) {
                    Ok(forwarder) => {
                        // Create a channel so the forwarder task can send us raw events.
                        let (event_tx, mut event_rx) =
                            tokio::sync::mpsc::channel::<ForwardedEvent>(32);

                        // Task 1: run the WebSocket forwarder (sends ForwardedEvent to event_tx).
                        let fwd_op = op_id.clone();
                        let fwd_query = payload.query.clone();
                        let fwd_vars = variables_value.clone();
                        tokio::spawn(async move {
                            if let Err(e) =
                                forwarder.forward(&fwd_op, &fwd_query, fwd_vars, event_tx).await
                            {
                                warn!(error = %e, "Remote subscription forwarder failed");
                            }
                        });

                        // Task 2: relay ForwardedEvent → ServerMessage → client.
                        let relay_op = op_id.clone();
                        let relay_tx = remote_msg_tx.clone();
                        tokio::spawn(async move {
                            while let Some(event) = event_rx.recv().await {
                                let server_msg = match event {
                                    ForwardedEvent::Next(data) => {
                                        ServerMessage::next(&relay_op, data)
                                    },
                                    ForwardedEvent::Error(errors) => {
                                        let errors_vec = errors.as_array().map_or_else(
                                            || {
                                                vec![GraphQLError::with_code(
                                                    errors.to_string(),
                                                    "REMOTE_ERROR",
                                                )]
                                            },
                                            |arr| {
                                                arr.iter()
                                                    .map(|e| {
                                                        GraphQLError::with_code(
                                                            e.get("message")
                                                                .and_then(|v| v.as_str())
                                                                .unwrap_or("Remote subgraph error"),
                                                            "REMOTE_ERROR",
                                                        )
                                                    })
                                                    .collect()
                                            },
                                        );
                                        ServerMessage::error(&relay_op, errors_vec)
                                    },
                                    ForwardedEvent::Complete => ServerMessage::complete(&relay_op),
                                };
                                if relay_tx.send(server_msg).await.is_err() {
                                    break; // Client disconnected
                                }
                            }
                        });

                        WS_SUBSCRIPTIONS_ACCEPTED.fetch_add(1, Ordering::Relaxed);
                        info!(
                            connection_id = %connection_id,
                            operation_id = %op_id,
                            subscription = %subscription_name,
                            "Subscription forwarded to remote subgraph"
                        );
                        return Ok(());
                    },
                    Err(e) => {
                        let error = ServerMessage::error(
                            &op_id,
                            vec![GraphQLError::with_code(e.to_string(), "SUBSCRIPTION_ERROR")],
                        );
                        if let Err(send_err) = send_server_message(codec, sender, error).await {
                            debug!(connection_id = %connection_id, error = %send_err, "Could not send forwarding error to client");
                        }
                        return Ok(());
                    },
                }
            }

            // Validate client-provided tenant variable against server-resolved
            if let Some(server_tid) = tenant_id {
                if let Some(client_tid) = variables_value.get("tenant_id").and_then(|v| v.as_str())
                {
                    if client_tid != server_tid {
                        let error = ServerMessage::error(
                            &op_id,
                            vec![GraphQLError::with_code(
                                format!(
                                    "Tenant mismatch: client provided '{client_tid}', server resolved '{server_tid}'"
                                ),
                                "TENANT_MISMATCH",
                            )],
                        );
                        if let Err(send_err) = send_server_message(codec, sender, error).await {
                            debug!(connection_id = %connection_id, error = %send_err, "Could not send tenant mismatch error to client");
                        }
                        return Ok(());
                    }
                }
            }

            // M-tenant-ws-suspended: refuse to start a subscription whose tenant
            // is suspended, mirroring the GraphQL data plane's 503 response.
            if let (Some(tid), Some(src)) = (tenant_id, state.tenant_status_source.as_ref()) {
                if src.is_suspended(tid) {
                    WS_SUBSCRIPTIONS_REJECTED.fetch_add(1, Ordering::Relaxed);
                    let error = ServerMessage::error(
                        &op_id,
                        vec![GraphQLError::with_code(
                            format!("Tenant '{tid}' is suspended"),
                            "TENANT_SUSPENDED",
                        )],
                    );
                    if let Err(send_err) = send_server_message(codec, sender, error).await {
                        debug!(connection_id = %connection_id, error = %send_err, "Could not send tenant-suspended error to client");
                    }
                    return Ok(());
                }
            }

            // #596: derive the server-owned row-visibility condition for this
            // subscription from the target entity's `subscription_policy`. Fail-closed —
            // a policy that applies but whose identity is unresolvable refuses the
            // subscription rather than falling back to delivering every row.
            let rls_conditions = match resolve_subscription_rls(
                state,
                &subscription_name,
                principal,
            )
            .await
            {
                Ok(conditions) => conditions,
                Err(reason) => {
                    WS_SUBSCRIPTIONS_REJECTED.fetch_add(1, Ordering::Relaxed);
                    warn!(
                        connection_id = %connection_id,
                        subscription = %subscription_name,
                        "Row-visibility policy refused the subscription (fail-closed)"
                    );
                    let error = ServerMessage::error(
                        &op_id,
                        vec![GraphQLError::with_code(reason, "SUBSCRIPTION_REFUSED")],
                    );
                    if let Err(send_err) = send_server_message(codec, sender, error).await {
                        debug!(connection_id = %connection_id, error = %send_err, "Could not send row-visibility refusal to client");
                    }
                    return Ok(());
                },
            };

            // Build context with server-resolved tenant_id
            let mut context = serde_json::json!({});
            if let Some(tid) = tenant_id {
                context["tenant_id"] = serde_json::Value::String(tid.to_string());
            }

            // Subscribe locally (field is owned by this subgraph). The server-owned
            // `rls_conditions` (#596) are enforced on every delivered event (AND
            // semantics) and cannot be overridden by client-supplied variables/filters.
            match state.manager.subscribe_with_rls(
                &subscription_name,
                context,
                variables_value,
                connection_id,
                rls_conditions,
            ) {
                Ok(sub_id) => {
                    active_operations.insert(
                        op_id.clone(),
                        ActiveOperation {
                            subscription_id:   sub_id,
                            subscription_name: subscription_name.clone(),
                        },
                    );
                    WS_SUBSCRIPTIONS_ACCEPTED.fetch_add(1, Ordering::Relaxed);
                    info!(
                        connection_id = %connection_id,
                        operation_id = %op_id,
                        subscription = %subscription_name,
                        "Subscription started"
                    );
                },
                Err(e) => {
                    let error = ServerMessage::error(
                        &op_id,
                        vec![GraphQLError::with_code(e.to_string(), "SUBSCRIPTION_ERROR")],
                    );
                    if let Err(send_err) = send_server_message(codec, sender, error).await {
                        debug!(connection_id = %connection_id, error = %send_err, "Could not send subscription error to client");
                    }
                },
            }
        },

        Some(ClientMessageType::Complete) => {
            let op_id = client_msg.id.ok_or_else(|| {
                warn!("Complete message missing operation ID");
                CloseCode::ProtocolError
            })?;

            if let Some(op) = active_operations.remove(&op_id) {
                if let Err(e) = state.manager.unsubscribe(op.subscription_id) {
                    warn!(connection_id = %connection_id, operation_id = %op_id, error = %e, "Failed to unsubscribe; subscription may be leaked");
                }
                state.lifecycle.on_unsubscribe(&op_id, connection_id).await;
                info!(
                    connection_id = %connection_id,
                    operation_id = %op_id,
                    "Subscription completed"
                );
            }
        },

        Some(ClientMessageType::ConnectionInit) => {
            warn!(connection_id = %connection_id, "Duplicate connection_init");
            return Err(CloseCode::TooManyInitRequests);
        },

        None => {
            warn!(message_type = %client_msg.message_type, "Unknown message type");
        },
        // Reason: non_exhaustive requires catch-all for cross-crate matches
        _ => {
            warn!(message_type = %client_msg.message_type, "Unrecognized message type");
        },
    }

    Ok(())
}

/// Terminate every active operation on a connection whose broadcast receiver
/// lagged (#772).
///
/// Broadcast lag means events for this connection's own subscriptions were
/// skipped; the stream can no longer be trusted as a complete change-feed. Each
/// operation receives a terminal `error` frame with the `EVENTS_LAGGED` code (a
/// documented resync signal: re-subscribe, then re-query for the missed state)
/// and is unsubscribed server-side. The connection itself stays open so the
/// client can re-subscribe immediately.
async fn terminate_operations_after_lag(
    state: &SubscriptionState,
    codec: &ProtocolCodec,
    sender: &mut futures::stream::SplitSink<WebSocket, Message>,
    active_operations: &mut HashMap<String, ActiveOperation>,
    connection_id: &str,
    lagged: u64,
) {
    for (op_id, op) in active_operations.drain() {
        let error = ServerMessage::error(
            &op_id,
            vec![GraphQLError::with_code(
                format!(
                    "Event stream lagged: {lagged} events were dropped for this connection. \
                     Re-subscribe and re-query to resynchronize."
                ),
                "EVENTS_LAGGED",
            )],
        );
        if let Err(e) = send_server_message(codec, sender, error).await {
            debug!(connection_id = %connection_id, error = %e, "Could not send EVENTS_LAGGED to client");
        }
        if let Err(e) = state.manager.unsubscribe(op.subscription_id) {
            warn!(connection_id = %connection_id, operation_id = %op_id, error = %e, "Failed to unsubscribe lagged operation");
        }
        state.lifecycle.on_unsubscribe(&op_id, connection_id).await;
    }
}

/// Send a server message through the codec, handling protocol translation.
async fn send_server_message(
    codec: &ProtocolCodec,
    sender: &mut futures::stream::SplitSink<WebSocket, Message>,
    msg: ServerMessage,
) -> Result<(), String> {
    match codec.encode(&msg) {
        Ok(Some(json)) => sender.send(Message::Text(json.into())).await.map_err(|e| e.to_string()),
        Ok(None) => Ok(()), // Message suppressed by codec (e.g. pong in legacy mode)
        Err(e) => Err(e.to_string()),
    }
}

/// Create a "next" message for a subscription event.
///
/// When the event carries a Change-Spine envelope (#425), it rides in the
/// graphql-transport-ws `ExecutionResult` `extensions` slot as `changeSpine` —
/// the spec-blessed, client-ignorable channel — leaving the resolved entity
/// `data` untouched. Events without an envelope produce the plain `next` message.
fn create_next_message(operation_id: &str, payload: &SubscriptionPayload) -> ServerMessage {
    let data = serde_json::json!({
        payload.subscription_name.clone(): payload.data
    });
    match &payload.event.change_spine {
        Some(envelope) => {
            let extensions = serde_json::json!({ "changeSpine": envelope });
            ServerMessage::next_with_extensions(operation_id, data, extensions)
        },
        None => ServerMessage::next(operation_id, data),
    }
}

/// Extract subscription name from a GraphQL subscription query.
pub(crate) fn extract_subscription_name(query: &str) -> Option<String> {
    let query = query.trim();

    let sub_idx = query.find("subscription")?;
    let after_sub = &query[sub_idx + "subscription".len()..];

    let brace_idx = after_sub.find('{')?;
    let after_brace = after_sub[brace_idx + 1..].trim_start();

    let name_end = after_brace
        .find(|c: char| !c.is_alphanumeric() && c != '_')
        .unwrap_or(after_brace.len());

    if name_end == 0 {
        return None;
    }

    Some(after_brace[..name_end].to_string())
}

#[cfg(test)]
mod tests;
