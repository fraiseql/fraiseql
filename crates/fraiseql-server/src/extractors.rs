//! Custom extractors for GraphQL handlers.
//!
//! Provides extractors for SecurityContext and other request-level data.

use std::future::Future;

use axum::{
    extract::{FromRequestParts, rejection::ExtensionRejection},
    http::request::Parts,
};
use fraiseql_core::security::SecurityContext;

use crate::middleware::AuthUser;

/// Extractor for optional SecurityContext from authenticated user and headers.
///
/// When used in a handler, automatically extracts:
/// 1. AuthUser from request extensions (if present)
/// 2. Request metadata from HTTP headers (request ID, IP, tenant ID)
/// 3. Creates SecurityContext from both
///
/// If authentication is not present, returns `None` (optional extraction).
///
/// # Example
///
/// ```ignore
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

    #[allow(clippy::manual_async_fn)]
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

                let mut context = SecurityContext::from_user(authenticated_user, request_id);
                context.ip_address = ip_address;
                context.tenant_id = tenant_id;
                context
            });

            Ok(OptionalSecurityContext(security_context))
        }
    }
}

/// Extract request ID from headers or generate a new one.
fn extract_request_id(headers: &axum::http::HeaderMap) -> String {
    headers
        .get("x-request-id")
        .and_then(|v| v.to_str().ok())
        .map(|s| s.to_string())
        .unwrap_or_else(|| format!("req-{}", uuid::Uuid::new_v4()))
}

/// Extract client IP address.
///
/// # Security
///
/// Does NOT trust X-Forwarded-For or X-Real-IP headers from clients, as these
/// are trivially spoofable. IP address should be set from `ConnectInfo<SocketAddr>`
/// at the handler level, or via `ProxyConfig::extract_client_ip()` which validates
/// the proxy chain before trusting forwarding headers.
fn extract_ip_address(_headers: &axum::http::HeaderMap) -> Option<String> {
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
fn extract_tenant_id(_headers: &axum::http::HeaderMap) -> Option<String> {
    // SECURITY: Tenant ID extraction from headers removed. The X-Tenant-ID header
    // is user-controlled and could be used for tenant isolation bypass. Tenant context
    // should come from the authenticated tenant_middleware or JWT claims.
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_request_id_from_header() {
        let mut headers = axum::http::HeaderMap::new();
        headers.insert("x-request-id", "req-12345".parse().unwrap());

        let request_id = extract_request_id(&headers);
        assert_eq!(request_id, "req-12345");
    }

    #[test]
    fn test_extract_request_id_generates_default() {
        let headers = axum::http::HeaderMap::new();
        let request_id = extract_request_id(&headers);
        // Should start with "req-"
        assert!(request_id.starts_with("req-"));
        // Should contain a UUID: "req-" (4) + UUID (36) = 40 chars
        assert_eq!(request_id.len(), 40);
    }

    #[test]
    fn test_extract_ip_ignores_x_forwarded_for() {
        // SECURITY: X-Forwarded-For must NOT be trusted without proxy validation
        let mut headers = axum::http::HeaderMap::new();
        headers.insert("x-forwarded-for", "192.0.2.1, 10.0.0.1".parse().unwrap());

        let ip = extract_ip_address(&headers);
        assert_eq!(ip, None, "Must not trust X-Forwarded-For header");
    }

    #[test]
    fn test_extract_ip_ignores_x_real_ip() {
        // SECURITY: X-Real-IP must NOT be trusted without proxy validation
        let mut headers = axum::http::HeaderMap::new();
        headers.insert("x-real-ip", "10.0.0.2".parse().unwrap());

        let ip = extract_ip_address(&headers);
        assert_eq!(ip, None, "Must not trust X-Real-IP header");
    }

    #[test]
    fn test_extract_ip_address_none_when_missing() {
        let headers = axum::http::HeaderMap::new();
        let ip = extract_ip_address(&headers);
        assert_eq!(ip, None);
    }

    #[test]
    fn test_extract_tenant_id_ignores_header() {
        // SECURITY: X-Tenant-ID must NOT be trusted from headers
        let mut headers = axum::http::HeaderMap::new();
        headers.insert("x-tenant-id", "tenant-acme".parse().unwrap());

        let tenant_id = extract_tenant_id(&headers);
        assert_eq!(tenant_id, None, "Must not trust X-Tenant-ID header");
    }

    #[test]
    fn test_extract_tenant_id_none_when_missing() {
        let headers = axum::http::HeaderMap::new();
        let tenant_id = extract_tenant_id(&headers);
        assert_eq!(tenant_id, None);
    }

    #[test]
    fn test_optional_security_context_creation_from_auth_user() {
        use chrono::Utc;

        // Simulate an authenticated user from the OIDC middleware
        let auth_user = crate::middleware::AuthUser(fraiseql_core::security::AuthenticatedUser {
            user_id:    "user123".to_string(),
            scopes:     vec!["read:user".to_string(), "write:post".to_string()],
            expires_at: Utc::now() + chrono::Duration::hours(1),
        });

        // Create headers with additional metadata
        let mut headers = axum::http::HeaderMap::new();
        headers.insert("x-request-id", "req-test-123".parse().unwrap());
        headers.insert("x-tenant-id", "tenant-acme".parse().unwrap());
        headers.insert("x-forwarded-for", "192.0.2.100".parse().unwrap());

        // Create security context using extractor helper logic
        let security_context = Some(auth_user).map(|auth_user| {
            let authenticated_user = auth_user.0;
            let request_id = extract_request_id(&headers);
            let ip_address = extract_ip_address(&headers);
            let tenant_id = extract_tenant_id(&headers);

            let mut context =
                fraiseql_core::security::SecurityContext::from_user(authenticated_user, request_id);
            context.ip_address = ip_address;
            context.tenant_id = tenant_id;
            context
        });

        // Verify context was created correctly
        assert!(security_context.is_some());
        let sec_ctx = security_context.unwrap();
        assert_eq!(sec_ctx.user_id, "user123");
        assert_eq!(sec_ctx.scopes, vec!["read:user".to_string(), "write:post".to_string()]);
        // SECURITY: Tenant ID is no longer extracted from headers (spoofable).
        // Should come from TenantContext (authenticated tenant_middleware) or JWT claims.
        assert_eq!(sec_ctx.tenant_id, None);
        assert_eq!(sec_ctx.request_id, "req-test-123");
        // SECURITY: IP is no longer extracted from headers (spoofable).
        // Should be set from ConnectInfo<SocketAddr> at handler level.
        assert_eq!(sec_ctx.ip_address, None);
    }
}
