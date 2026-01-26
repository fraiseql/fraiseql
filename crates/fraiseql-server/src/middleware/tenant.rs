// Multi-tenancy middleware for extracting and enforcing org_id
// Extracts org_id from JWT claims or request headers and adds to request context

use axum::{body::Body, http::Request, middleware::Next, response::Response};
use tracing::debug;

use crate::middleware::oidc_auth::AuthUser;

/// Extract org_id from request and add to context
///
/// Attempts to extract org_id from:
/// 1. JWT claims (if authenticated) via custom claim "org_id"
/// 2. X-Org-ID header (for service-to-service or explicit tenant routing)
/// 3. None if not provided (unauthenticated requests)
///
/// # Tenant Routing Priority
/// 1. **Authenticated with JWT claim "org_id"**: Use JWT claim (most secure)
/// 2. **X-Org-ID header**: Use header (for service-to-service)
/// 3. **Neither**: Request is not tenant-scoped (public/unauthenticated)
pub async fn tenant_middleware(mut request: Request<Body>, next: Next) -> Response {
    let mut org_id: Option<String> = None;

    // Try to extract org_id from authenticated user's JWT claims
    if let Some(auth_user) = request.extensions().get::<AuthUser>() {
        // org_id should be in custom claims (provider-specific)
        // Common claim names: org_id, organization_id, tenant_id, oid
        // This will be populated by the OAuth provider during token exchange
        debug!(
            user_id = %auth_user.0.user_id,
            "Authenticated user context available for tenant extraction"
        );

        // Note: Actual org_id extraction happens at the GraphQL handler level
        // where we have access to full JWT claims via AuthenticatedUser or request context
    }

    // Try to extract org_id from X-Org-ID header
    // This allows service-to-service requests to specify tenant
    if let Some(header_value) = request.headers().get("X-Org-ID") {
        if let Ok(org_id_str) = header_value.to_str() {
            org_id = Some(org_id_str.to_string());
            debug!(
                org_id = %org_id_str,
                source = "header",
                "Extracted org_id from X-Org-ID header"
            );
        }
    }

    // Store org_id in request extensions for downstream handlers
    request.extensions_mut().insert(TenantContext { org_id });

    next.run(request).await
}

/// Tenant context extracted from request
#[derive(Debug, Clone)]
pub struct TenantContext {
    /// Organization/Tenant ID for multi-tenancy enforcement
    pub org_id: Option<String>,
}

impl TenantContext {
    /// Check if tenant is explicitly set
    pub fn is_tenant_scoped(&self) -> bool {
        self.org_id.is_some()
    }

    /// Get tenant ID if available
    pub fn get_org_id(&self) -> Option<&str> {
        self.org_id.as_deref()
    }

    /// Require tenant ID (for operations that must be tenant-scoped)
    pub fn require_org_id(&self) -> Result<&str, String> {
        self.org_id
            .as_deref()
            .ok_or_else(|| "Request must be tenant-scoped (missing org_id)".to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_tenant_context_scoped() {
        let ctx = TenantContext {
            org_id: Some("org-123".to_string()),
        };
        assert!(ctx.is_tenant_scoped());
        assert_eq!(ctx.get_org_id(), Some("org-123"));
    }

    #[test]
    fn test_tenant_context_unscoped() {
        let ctx = TenantContext { org_id: None };
        assert!(!ctx.is_tenant_scoped());
        assert_eq!(ctx.get_org_id(), None);
    }

    #[test]
    fn test_require_org_id_success() {
        let ctx = TenantContext {
            org_id: Some("org-123".to_string()),
        };
        assert!(ctx.require_org_id().is_ok());
        assert_eq!(ctx.require_org_id().unwrap(), "org-123");
    }

    #[test]
    fn test_require_org_id_failure() {
        let ctx = TenantContext { org_id: None };
        assert!(ctx.require_org_id().is_err());
        assert_eq!(
            ctx.require_org_id().unwrap_err(),
            "Request must be tenant-scoped (missing org_id)"
        );
    }
}
