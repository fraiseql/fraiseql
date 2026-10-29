//! Custom extractors for GraphQL handlers.
//!
//! Provides extractors for `SecurityContext` and other request-level data.

#[cfg(test)]
mod tests;

use std::{convert::Infallible, future::Future, net::SocketAddr};

use axum::{
    extract::{ConnectInfo, FromRequestParts, rejection::ExtensionRejection},
    http::request::Parts,
};
use fraiseql_core::security::SecurityContext;

use crate::middleware::{AuthUser, TenantClaim};

/// Extractor for the TCP peer IP address.
///
/// Reads the peer address from `ConnectInfo<SocketAddr>` in request extensions.
/// Returns only the IP part (no port), so connections from the same client share
/// the same rate-limit key regardless of ephemeral port churn.
///
/// Falls back to `"unknown"` when:
/// - The server was not started with `into_make_service_with_connect_info`
/// - Running in test mode (direct `oneshot` without `ConnectInfo`)
pub struct PeerIp(pub String);

impl<S> FromRequestParts<S> for PeerIp
where
    S: Send + Sync,
{
    type Rejection = Infallible;

    fn from_request_parts(
        parts: &mut Parts,
        _state: &S,
    ) -> impl Future<Output = Result<Self, Self::Rejection>> + Send {
        let ip = parts
            .extensions
            .get::<ConnectInfo<SocketAddr>>()
            .map_or_else(|| "unknown".to_string(), |ci| ci.0.ip().to_string());
        async move { Ok(PeerIp(ip)) }
    }
}

/// Extractor for optional `SecurityContext` from authenticated user and headers.
///
/// When used in a handler, automatically extracts:
/// 1. `AuthUser` from request extensions (if present)
/// 2. Request metadata from HTTP headers (request ID, IP, tenant ID)
/// 3. Creates `SecurityContext` from both
///
/// If authentication is not present, returns `None` (optional extraction).
///
/// # Example
///
/// ```text
/// // Requires: running Axum server with authentication middleware configured.
/// async fn graphql_handler(
///     State(state): State<AppState>,
///     OptionalSecurityContext(context): OptionalSecurityContext,
/// ) -> Result<Response> {
///     // context is Option<SecurityContext>
/// }
/// ```
#[derive(Debug, Clone)]
pub struct OptionalSecurityContext(pub Option<SecurityContext>);

impl<S> FromRequestParts<S> for OptionalSecurityContext
where
    S: Send + Sync + 'static,
{
    type Rejection = ExtensionRejection;

    #[allow(clippy::manual_async_fn)] // Reason: axum's FromRequestParts requires explicit Future type in return position
    fn from_request_parts(
        parts: &mut Parts,
        _state: &S,
    ) -> impl Future<Output = Result<Self, Self::Rejection>> + Send {
        async move {
            // #1650: a service account the auth layer admitted is served as itself.
            if let Some(crate::service_account::ServicePrincipal(context)) =
                parts.extensions.get().cloned()
            {
                return Ok(OptionalSecurityContext(Some(context)));
            }

            // Try to extract AuthUser from extensions
            let auth_user: Option<AuthUser> = parts.extensions.get::<AuthUser>().cloned();
            let tenant_claim = parts.extensions.get::<TenantClaim>().cloned();

            // Extract request headers
            let headers = &parts.headers;

            // Create SecurityContext if auth user is present
            let security_context = auth_user.map(|auth_user| {
                let request_id = extract_request_id(headers);
                let mut context = SecurityContext::from_token_claims(
                    &auth_user.0,
                    request_id,
                    tenant_claim.as_ref().map(|c| &*c.0),
                );
                context.ip_address = extract_ip_address(headers);
                context
            });

            Ok(OptionalSecurityContext(security_context))
        }
    }
}

/// Extract request ID from headers or generate a new one.
pub(crate) fn extract_request_id(headers: &axum::http::HeaderMap) -> String {
    headers
        .get("x-request-id")
        .and_then(|v| v.to_str().ok())
        .map_or_else(|| format!("req-{}", uuid::Uuid::new_v4()), |s| s.to_string())
}

/// Extract client IP address.
///
/// # Security
///
/// Does NOT trust X-Forwarded-For or X-Real-IP headers from clients, as these
/// are trivially spoofable. IP address should be set from `ConnectInfo<SocketAddr>`
/// at the handler level, or via `ProxyConfig::extract_client_ip()` which validates
/// the proxy chain before trusting forwarding headers.
pub(crate) const fn extract_ip_address(_headers: &axum::http::HeaderMap) -> Option<String> {
    // SECURITY: IP extraction from headers removed. User-supplied X-Forwarded-For
    // and X-Real-IP headers are trivially spoofable and must not be trusted without
    // proxy chain validation. Use ConnectInfo<SocketAddr> or ProxyConfig instead.
    None
}
