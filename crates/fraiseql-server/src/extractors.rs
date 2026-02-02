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

    fn from_request_parts(
        parts: &mut Parts,
        _state: &S,
    ) -> impl Future<Output = Result<Self, Self::Rejection>> + Send {
        async move {
            // Try to extract AuthUser from extensions
            let auth_user: Option<AuthUser> = parts
                .extensions
                .get::<AuthUser>()
                .cloned();

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

/// Extract client IP address from headers.
fn extract_ip_address(headers: &axum::http::HeaderMap) -> Option<String> {
    // Check X-Forwarded-For first (for proxied requests)
    if let Some(forwarded_for) = headers
        .get("x-forwarded-for")
        .and_then(|v| v.to_str().ok())
    {
        // X-Forwarded-For can contain multiple IPs, use the first one
        return forwarded_for.split(',').next().map(|ip| ip.trim().to_string());
    }

    // Check X-Real-IP
    if let Some(real_ip) = headers
        .get("x-real-ip")
        .and_then(|v| v.to_str().ok())
    {
        return Some(real_ip.to_string());
    }

    None
}

/// Extract tenant ID from headers.
fn extract_tenant_id(headers: &axum::http::HeaderMap) -> Option<String> {
    headers
        .get("x-tenant-id")
        .and_then(|v| v.to_str().ok())
        .map(|s| s.to_string())
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
    fn test_extract_ip_address_from_x_forwarded_for() {
        let mut headers = axum::http::HeaderMap::new();
        headers.insert("x-forwarded-for", "192.0.2.1, 10.0.0.1".parse().unwrap());

        let ip = extract_ip_address(&headers);
        assert_eq!(ip, Some("192.0.2.1".to_string()));
    }

    #[test]
    fn test_extract_ip_address_from_x_real_ip() {
        let mut headers = axum::http::HeaderMap::new();
        headers.insert("x-real-ip", "10.0.0.2".parse().unwrap());

        let ip = extract_ip_address(&headers);
        assert_eq!(ip, Some("10.0.0.2".to_string()));
    }

    #[test]
    fn test_extract_ip_address_none_when_missing() {
        let headers = axum::http::HeaderMap::new();
        let ip = extract_ip_address(&headers);
        assert_eq!(ip, None);
    }

    #[test]
    fn test_extract_tenant_id_from_header() {
        let mut headers = axum::http::HeaderMap::new();
        headers.insert("x-tenant-id", "tenant-acme".parse().unwrap());

        let tenant_id = extract_tenant_id(&headers);
        assert_eq!(tenant_id, Some("tenant-acme".to_string()));
    }

    #[test]
    fn test_extract_tenant_id_none_when_missing() {
        let headers = axum::http::HeaderMap::new();
        let tenant_id = extract_tenant_id(&headers);
        assert_eq!(tenant_id, None);
    }
}
