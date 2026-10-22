//! Service-account authentication (ADR-0018).
//!
//! A **service account** grants an external daemon a named, auditable, ceiling-bounded
//! identity: a `[service_accounts.<name>]` block declaring an **env-indirected** static
//! secret and a `run_as` ceiling (`roles`/`scopes`/`tenant`). It extends the static
//! API-key seam ([`crate::api_key`]) — same header, same SHA-256 + constant-time
//! compare — but carries a full ceiling minted through
//! [`SecurityContext::service_account`] (`ActorType::ServiceAccount`,
//! `user_id = service_account:<name>`), rather than scopes only.
//!
//! # Credential presentation (ADR-0018, amended 2026-07-16)
//!
//! The secret is presented on the **`x-api-key`** header (optionally `ApiKey <secret>` /
//! `Bearer <secret>` in the value), NOT on `Authorization: Bearer`. The original ADR said
//! `Authorization: Bearer`, but that header is consumed by the JWT auth middleware before
//! any api-key seam runs, so a bearer secret would be 401'd as an invalid JWT whenever
//! OIDC/HS256 is configured.
//!
//! # Where it is resolved (#1650)
//!
//! Once per request, by the auth layer every transport mounts (`admit`): the OIDC or
//! HS256 middleware, or [`service_account_middleware`] on a deployment with no JWT
//! authentication. The layer inserts a [`ServicePrincipal`], which
//! [`OptionalSecurityContext`](crate::extractors::OptionalSecurityContext) serves to
//! every handler. Resolving it in a handler left every other transport serving the
//! account as anonymous, and under OIDC with `required = true` the layer refused the
//! request before any handler ran.
//!
//! # Fail-closed
//!
//! - Unknown account / bad secret are **indistinguishable** — a present-but-unmatched secret yields
//!   no context (the caller 401s), with no account-existence oracle.
//! - An account with an empty ceiling authenticates but has **no authority** (RLS / field-authz
//!   deny its writes).
//! - The secret plaintext is read once at startup and discarded — only its SHA-256 hash lives in
//!   process memory; the compiled schema/config holds only the env-var *name*.

use std::{collections::HashMap, sync::Arc};

use axum::{
    body::Body,
    extract::State,
    http::{HeaderMap, HeaderName, Request, StatusCode, header},
    middleware::Next,
    response::{IntoResponse, Response},
};
use fraiseql_core::security::{ENRICHED_NAMESPACE_PREFIX, SecurityContext};
use subtle::ConstantTimeEq;
use tracing::{debug, warn};

use crate::api_key::sha256_hash;

/// The header a service account presents its secret on (shared with static API keys).
const SA_HEADER: &str = "x-api-key";

/// A `[service_accounts.<name>]` config block. Holds only **non-secret** material — the
/// secret plaintext lives in the environment variable named by `secret_env`.
///
/// The compiled shape — the schema seam owns it (#977), so the compiled
/// artefact and this server share one type.
pub use fraiseql_core::schema::ServiceAccountConfig;

/// The outcome of [`ServiceAccountAuthenticator::resolve`] — the shared decision every
/// entry point maps onto its own 401/response shape.
#[derive(Debug)]
#[non_exhaustive]
pub enum SaAuth {
    /// No secret header present — proceed with the existing (JWT / anonymous) context.
    NoSecret,
    /// A JWT principal **and** a secret header on one request — ambiguous identity;
    /// the caller must reject (fail-closed, no silent precedence).
    Ambiguous,
    /// The secret matched a service account — use this context.
    Authenticated(Box<SecurityContext>),
    /// A secret header is present but matched no service account. The caller may try
    /// another secret authenticator (a static API key), else 401 — a bad secret is
    /// **indistinguishable** from an unknown account (no oracle).
    Unmatched,
}

/// A service account with its secret resolved to a hash (plaintext discarded).
#[derive(Debug, Clone)]
struct ResolvedServiceAccount {
    name:            String,
    secret_hash:     [u8; 32],
    roles:           Vec<String>,
    scopes:          Vec<String>,
    tenant:          Option<String>,
    static_enriched: HashMap<String, serde_json::Value>,
}

/// Authenticates service-account secrets presented on the api-key header.
pub struct ServiceAccountAuthenticator {
    header_name: HeaderName,
    accounts:    Vec<ResolvedServiceAccount>,
}

impl std::fmt::Debug for ServiceAccountAuthenticator {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ServiceAccountAuthenticator")
            .field("header_name", &self.header_name)
            .field("accounts_count", &self.accounts.len())
            .finish()
    }
}

impl ServiceAccountAuthenticator {
    /// Build an authenticator from the `[service_accounts]` config, resolving each
    /// account's secret via `resolve_secret` (production: `|env| std::env::var(env).ok()`).
    ///
    /// An account whose `secret_env` is unset/empty is **skipped with a warning** — it is
    /// simply unusable (fail-closed), never a silent anonymous grant. Returns `None` when
    /// no account resolves.
    #[must_use]
    pub fn from_config(
        accounts: &HashMap<String, ServiceAccountConfig>,
        resolve_secret: impl Fn(&str) -> Option<String>,
    ) -> Option<Arc<Self>> {
        let mut resolved = Vec::new();
        for (name, cfg) in accounts {
            match resolve_secret(&cfg.secret_env) {
                Some(secret) if !secret.is_empty() => {
                    resolved.push(ResolvedServiceAccount {
                        name:            name.clone(),
                        secret_hash:     sha256_hash(secret.as_bytes()),
                        roles:           cfg.roles.clone(),
                        scopes:          cfg.scopes.clone(),
                        tenant:          cfg.tenant.clone(),
                        static_enriched: cfg.static_enriched.clone(),
                    });
                },
                _ => warn!(
                    account = %name,
                    secret_env = %cfg.secret_env,
                    "service account skipped — its secret env var is unset or empty"
                ),
            }
        }
        if resolved.is_empty() {
            return None;
        }
        let header_name = HeaderName::from_static(SA_HEADER);
        Some(Arc::new(Self {
            header_name,
            accounts: resolved,
        }))
    }

    /// Resolve a service-account principal from `headers`, honoring the JWT-collision
    /// rule (ADR-0018 amendment / rider 2). `jwt_present` is whether the request carries
    /// a bearer token. Called by `admit`, the auth layer's one resolution.
    #[must_use]
    pub fn resolve(&self, headers: &HeaderMap, jwt_present: bool) -> SaAuth {
        if !self.header_present(headers) {
            return SaAuth::NoSecret;
        }
        if jwt_present {
            // A JWT principal AND a secret header on one request is an ambiguous
            // identity — reject fail-closed rather than silently pick one.
            return SaAuth::Ambiguous;
        }
        self.authenticate(headers).map_or(SaAuth::Unmatched, SaAuth::Authenticated)
    }

    /// Whether the request carries a (non-empty) service-account secret header. Used by
    /// callers to reject a request that also carries a JWT (ambiguous identity).
    #[must_use]
    pub fn header_present(&self, headers: &HeaderMap) -> bool {
        headers
            .get(&self.header_name)
            .and_then(|v| v.to_str().ok())
            .is_some_and(|s| !s.is_empty())
    }

    /// Authenticate a request. Returns the service account's [`SecurityContext`] on an
    /// exact constant-time secret match, or `None` when the header is absent **or**
    /// present-but-unmatched (the caller cannot distinguish the two — no oracle).
    #[must_use]
    pub fn authenticate(&self, headers: &HeaderMap) -> Option<Box<SecurityContext>> {
        let raw = headers.get(&self.header_name)?.to_str().ok()?;
        if raw.is_empty() {
            return None;
        }
        // Strip an optional `ApiKey ` / `Bearer ` prefix on the value (ASCII, byte-safe).
        let secret = strip_scheme_prefix(raw);
        let presented = sha256_hash(secret.as_bytes());

        for account in &self.accounts {
            if bool::from(presented.ct_eq(&account.secret_hash)) {
                debug!(account = %account.name, "service account authenticated");
                return Some(Box::new(build_context(account)));
            }
        }
        warn!("service-account authentication failed: no matching account");
        None
    }
}

/// Strip a leading `ApiKey ` / `Bearer ` scheme (case-insensitive) from a header value.
/// The prefix is ASCII, so the byte slice that follows is on a char boundary.
fn strip_scheme_prefix(raw: &str) -> &str {
    let bytes = raw.as_bytes();
    if bytes.len() > 7
        && (bytes[..7].eq_ignore_ascii_case(b"apikey ")
            || bytes[..7].eq_ignore_ascii_case(b"bearer "))
    {
        &raw[7..]
    } else {
        raw
    }
}

/// Mint the service account's [`SecurityContext`] from its ceiling, injecting any
/// `static_enriched` fields under the forge-proof `fraiseql.enriched.*` namespace.
fn build_context(account: &ResolvedServiceAccount) -> SecurityContext {
    let tenant = account.tenant.as_ref().map(fraiseql_core::types::TenantId::new);
    let request_id = format!("sa-{}", uuid::Uuid::new_v4());
    let mut ctx = SecurityContext::service_account(
        account.name.clone(),
        request_id,
        account.roles.clone(),
        account.scopes.clone(),
        tenant,
    );
    for (field, value) in &account.static_enriched {
        ctx.attributes
            .insert(format!("{ENRICHED_NAMESPACE_PREFIX}{field}"), value.clone());
    }
    ctx
}

/// The body of the refusal for a request carrying both a bearer token and a
/// service-account secret: one request, two identities, and no rule to pick one.
pub const AMBIGUOUS_CREDENTIALS: &str = "Ambiguous credentials: a bearer token and an API-key / \
                                         service-account secret were presented on the same \
                                         request";

/// Request extension: the service account the auth layer admitted the request as
/// (#1650).
///
/// Inserted by the auth layer, which is the one place a service-account secret is
/// resolved, and read by [`OptionalSecurityContext`](crate::extractors::OptionalSecurityContext),
/// so every transport behind the layer is served the same principal.
#[derive(Debug, Clone)]
pub struct ServicePrincipal(pub SecurityContext);

/// Decide a request's service-account credential, before any bearer token is
/// validated (#1650).
///
/// `token_presented` is whether the request carries a bearer token (header or
/// cookie). Returns `Ok(true)` when the secret matched an account (the
/// [`ServicePrincipal`] is inserted), `Ok(false)` when there is no secret or it
/// matched no account (the layer's own rules decide), and `Err` with a 401 when a
/// secret arrives beside a token, before either is trusted.
///
/// # Errors
///
/// The 401 response for ambiguous credentials.
// Reason: a `Response` is what axum middleware returns; boxing it would only move
// the allocation into every caller.
#[allow(clippy::result_large_err)]
pub(crate) fn admit(
    accounts: Option<&ServiceAccountAuthenticator>,
    request: &mut Request<Body>,
    token_presented: bool,
) -> Result<bool, Response> {
    let Some(accounts) = accounts else {
        return Ok(false);
    };
    match accounts.resolve(request.headers(), token_presented) {
        SaAuth::Authenticated(context) => {
            request.extensions_mut().insert(ServicePrincipal(*context));
            Ok(true)
        },
        SaAuth::Ambiguous => Err((
            StatusCode::UNAUTHORIZED,
            [(header::WWW_AUTHENTICATE, "Bearer error=\"invalid_request\"")],
            AMBIGUOUS_CREDENTIALS,
        )
            .into_response()),
        SaAuth::NoSecret | SaAuth::Unmatched => Ok(false),
    }
}

/// Auth layer for a deployment with service accounts and no JWT authentication:
/// admits a service account as itself and leaves every other request as it was.
pub async fn service_account_middleware(
    State(accounts): State<Arc<ServiceAccountAuthenticator>>,
    mut request: Request<Body>,
    next: Next,
) -> Response {
    // No bearer is validated here, so none can make the secret ambiguous.
    match admit(Some(&accounts), &mut request, false) {
        Ok(_) => next.run(request).await,
        Err(response) => response,
    }
}

/// Build a [`ServiceAccountAuthenticator`] from the compiled schema's
/// `security.service_accounts` block, resolving secrets from the process environment.
#[must_use]
pub fn service_account_authenticator_from_schema(
    schema: &fraiseql_core::schema::CompiledSchema,
) -> Option<Arc<ServiceAccountAuthenticator>> {
    let security = schema.security.as_ref()?;
    // A typed field (#977): a malformed or misspelled section is a load error at
    // the schema seam, never a warn-and-disable here.
    let accounts = security.service_accounts.as_ref()?;
    ServiceAccountAuthenticator::from_config(accounts, |env| std::env::var(env).ok())
}

#[cfg(test)]
mod tests;
