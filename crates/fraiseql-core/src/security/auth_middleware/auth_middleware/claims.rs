//! Claim extraction and scope validation.

use super::{AuthMiddleware, types::JwtClaims};

impl AuthMiddleware {
    /// Extract scopes from JWT claims.
    ///
    /// Supports multiple formats:
    /// - `scope`: space-separated string (OAuth2 standard)
    /// - `scp`: array of strings (Microsoft)
    /// - `permissions`: array of strings (Auth0 RBAC)
    pub(super) fn extract_scopes_from_jwt_claims(&self, claims: &JwtClaims) -> Vec<String> {
        // Try space-separated scope string first (most common)
        if let Some(ref scope) = claims.scope {
            return scope.split_whitespace().map(String::from).collect();
        }

        // Try array of scopes (scp claim)
        if let Some(ref scp) = claims.scp {
            return scp.clone();
        }

        // Try permissions array (Auth0 RBAC)
        if let Some(ref permissions) = claims.permissions {
            return permissions.clone();
        }

        Vec::new()
    }
}
