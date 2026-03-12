//! Bearer token extraction from HTTP headers.

use crate::security::errors::{Result, SecurityError};

use super::{AuthMiddleware, types::AuthRequest};

impl AuthMiddleware {
    /// Extract token from the authorization header
    pub(super) fn extract_token(&self, req: &AuthRequest) -> Result<String> {
        // If auth is not required and no header present, that's OK
        if !self.config.required && req.authorization_header.is_none() {
            return Err(SecurityError::AuthRequired); // Will be handled differently
        }

        req.extract_bearer_token()
    }
}
