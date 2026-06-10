//! Custom extractors for GraphQL handlers.
//!
//! Provides extractors for `SecurityContext` and other request-level data.

use std::{convert::Infallible, future::Future, net::SocketAddr};

use axum::{
    extract::{ConnectInfo, FromRequestParts, rejection::ExtensionRejection},
    http::request::Parts,
};
use fraiseql_core::security::SecurityContext;

use crate::middleware::AuthUser;

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
            // Try to extract AuthUser from extensions
            let auth_user: Option<AuthUser> = parts.extensions.get::<AuthUser>().cloned();

            // Extract request headers
            let headers = &parts.headers;

            // Create SecurityContext if auth user is present
            let security_context = auth_user.map(|auth_user| {
                let authenticated_user = auth_user.0;
                let request_id = extract_request_id(headers);
                let ip_address = extract_ip_address(headers);
                let tenant_id = extract_tenant_id(headers);

                let mut context = SecurityContext::from_user(&authenticated_user, request_id);
                context.ip_address = ip_address;
                context.tenant_id = tenant_id.map(fraiseql_core::types::TenantId::new);

                // Forward JWT extra_claims to security context attributes.
                // This makes custom claims (org_id, roles, etc.) available to RLS policies
                // and session variable injection.
                //
                // Framework-reserved `fraiseql.`-namespaced attributes (the derived
                // actor classification, trace context, etc.) are NOT overwritable by
                // a JWT claim — a token that carried a claim literally named
                // `fraiseql.actor_type` must not be able to forge the recorded actor
                // (#390). Such a claim is skipped here.
                for (key, value) in &authenticated_user.extra_claims {
                    if key.starts_with("fraiseql.") {
                        continue;
                    }
                    context.attributes.insert(key.clone(), value.clone());
                }

                // Set tenant_id from org_id JWT claim when not already set from headers.
                // This is the standard multi-tenant pattern: the JWT org_id claim identifies
                // which tenant's data the authenticated user may access.
                if context.tenant_id.is_none() {
                    if let Some(org_id) =
                        authenticated_user.extra_claims.get("org_id").and_then(|v| v.as_str())
                    {
                        context.tenant_id = Some(fraiseql_core::types::TenantId::new(org_id));
                    }
                }

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

/// Extract tenant ID.
///
/// # Security
///
/// Does NOT trust the X-Tenant-ID header directly. An authenticated user could
/// set an arbitrary tenant ID to access another organization's data. Tenant ID
/// should be set from `TenantContext` (populated by the secured `tenant_middleware`
/// which requires authentication) or from JWT claims.
pub(crate) const fn extract_tenant_id(_headers: &axum::http::HeaderMap) -> Option<String> {
    // SECURITY: Tenant ID extraction from headers removed. The X-Tenant-ID header
    // is user-controlled and could be used for tenant isolation bypass. Tenant context
    // should come from the authenticated tenant_middleware or JWT claims.
    None
}
