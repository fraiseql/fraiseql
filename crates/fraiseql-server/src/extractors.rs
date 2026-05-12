//! Custom extractors for GraphQL handlers.
//!
//! Provides extractors for `SecurityContext` and other request-level data.

use std::future::Future;

use axum::{
    extract::{FromRequestParts, rejection::ExtensionRejection},
    http::request::Parts,
};
use fraiseql_core::security::SecurityContext;

use crate::middleware::AuthUser;

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
                for (key, value) in &authenticated_user.extra_claims {
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
