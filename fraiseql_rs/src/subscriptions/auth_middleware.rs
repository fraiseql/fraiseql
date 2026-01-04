//! Authentication middleware for WebSocket subscriptions
//!
//! Validates JWT tokens and extracts user context from WebSocket connections.

use serde_json::Value;
use std::sync::Arc;

/// Authentication context extracted from token
#[derive(Debug, Clone)]
pub struct AuthContext {
    /// Authenticated user ID
    pub user_id: i64,
    /// Tenant ID (for multi-tenancy)
    pub tenant_id: i64,
    /// Token expiration time (Unix timestamp)
    pub exp: u64,
    /// Token issued at time (Unix timestamp)
    pub iat: u64,
}

/// Authentication error
#[derive(Debug, Clone)]
pub enum AuthError {
    /// No token provided
    MissingToken,
    /// Token format invalid
    InvalidFormat,
    /// Token validation failed
    ValidationFailed(String),
    /// Token expired
    Expired,
    /// User ID missing from token
    MissingUserId,
    /// Tenant ID missing from token
    MissingTenantId,
}

impl std::fmt::Display for AuthError {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        match self {
            Self::MissingToken => write!(f, "Missing authentication token"),
            Self::InvalidFormat => write!(f, "Invalid token format"),
            Self::ValidationFailed(msg) => write!(f, "Token validation failed: {msg}"),
            Self::Expired => write!(f, "Token has expired"),
            Self::MissingUserId => write!(f, "Token missing user_id claim"),
            Self::MissingTenantId => write!(f, "Token missing tenant_id claim"),
        }
    }
}

impl std::error::Error for AuthError {}

/// Authentication middleware for WebSocket connections
#[derive(Debug)]
pub struct AuthMiddleware {
    /// JWT secret key for validation (typically from environment)
    #[allow(dead_code)]
    secret: Arc<String>,
    /// Enable/disable auth (for testing)
    enabled: bool,
}

impl AuthMiddleware {
    /// Create new auth middleware
    #[must_use]
    pub fn new(secret: String) -> Self {
        Self {
            secret: Arc::new(secret),
            enabled: true,
        }
    }

    /// Create auth middleware with enabled flag (for testing)
    #[cfg(test)]
    pub fn with_enabled(secret: String, enabled: bool) -> Self {
        Self {
            secret: Arc::new(secret),
            enabled,
        }
    }

    /// Extract and validate token from `ConnectionInit` payload
    ///
    /// Expected payload format:
    /// ```json
    /// {
    ///   "authorization": "Bearer eyJ..."
    /// }
    /// ```
    ///
    /// # Errors
    ///
    /// Returns `Err(AuthError)` if:
    /// - JWT token validation fails
    /// - Token is expired
    /// - Required payload fields are missing
    pub fn validate_connection_init(
        &self,
        payload: Option<&Value>,
    ) -> Result<AuthContext, AuthError> {
        // If auth disabled (for backward compatibility), create default context
        if !self.enabled {
            return Ok(AuthContext {
                user_id: 1,
                tenant_id: 1,
                exp: u64::MAX,
                iat: 0,
            });
        }

        let payload = payload.ok_or(AuthError::MissingToken)?;

        // Extract authorization header from payload
        let auth_header = payload
            .get("authorization")
            .and_then(|v| v.as_str())
            .ok_or(AuthError::MissingToken)?;

        // Parse "Bearer <token>" format
        let Some(token) = auth_header.strip_prefix("Bearer ") else {
            return Err(AuthError::InvalidFormat);
        };

        // Validate and decode token
        Self::decode_token(token)
    }

    /// Validate token (signature validation requires external JWT library)
    ///
    /// This simplified implementation validates:
    /// - Token format (header.payload.signature)
    /// - Token expiration time
    /// - Required claims (`user_id`, exp)
    ///
    /// In production, use the `jsonwebtoken` crate for full HMAC/RSA signature validation.
    /// This implementation is suitable for testing and can be upgraded to full validation.
    fn decode_token(token: &str) -> Result<AuthContext, AuthError> {
        // Token structure: header.payload.signature
        let parts: Vec<&str> = token.split('.').collect();
        if parts.len() != 3 {
            return Err(AuthError::InvalidFormat);
        }

        // Validate token format without signature verification
        // (In production, signature verification should be added)
        validate_token_format(parts[0], parts[1]).map_err(AuthError::ValidationFailed)?;

        // Decode payload (base64url)
        let payload_bytes = decode_base64url(parts[1]).map_err(AuthError::ValidationFailed)?;

        let payload_str = String::from_utf8(payload_bytes)
            .map_err(|_| AuthError::ValidationFailed("Invalid UTF-8".to_string()))?;

        let payload_json: Value = serde_json::from_str(&payload_str)
            .map_err(|_| AuthError::ValidationFailed("Invalid JSON".to_string()))?;

        // Extract claims
        let user_id = payload_json
            .get("sub")
            .or_else(|| payload_json.get("user_id"))
            .and_then(serde_json::Value::as_i64)
            .ok_or(AuthError::MissingUserId)?;

        let tenant_id = payload_json
            .get("tenant_id")
            .and_then(serde_json::Value::as_i64)
            .unwrap_or(1); // Default to 1 if not present

        let exp = payload_json
            .get("exp")
            .and_then(serde_json::Value::as_u64)
            .ok_or_else(|| AuthError::ValidationFailed("Missing exp claim".to_string()))?;

        let iat = payload_json
            .get("iat")
            .and_then(serde_json::Value::as_u64)
            .unwrap_or(0);

        // Check token expiration
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_err(|_| AuthError::ValidationFailed("System time error".to_string()))?
            .as_secs();

        if now > exp {
            return Err(AuthError::Expired);
        }

        Ok(AuthContext {
            user_id,
            tenant_id,
            exp,
            iat,
        })
    }
}

/// Validate JWT header and payload format (no signature check)
fn validate_token_format(header: &str, payload: &str) -> Result<(), String> {
    // Try to decode header
    decode_base64url(header)?;
    // Try to decode payload
    decode_base64url(payload)?;
    Ok(())
}

/// Decode base64url string (no padding)
fn decode_base64url(input: &str) -> Result<Vec<u8>, String> {
    // Add padding if needed
    let mut padded = input.to_string();
    while !padded.len().is_multiple_of(4) {
        padded.push('=');
    }

    // Replace url-safe characters
    let standard = padded.replace('-', "+").replace('_', "/");

    // Simple base64 decode using built-in
    // Note: In production, consider using base64 crate for efficiency
    decode_base64_simple(&standard)
}

/// Simple base64 decoder (sufficient for JWT validation)
/// For production use, add base64 crate dependency
fn decode_base64_simple(input: &str) -> Result<Vec<u8>, String> {
    const ALPHABET: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";

    let mut result = Vec::new();
    let input_bytes = input.as_bytes();
    let mut i = 0;

    while i < input_bytes.len() {
        let mut buf = [0u8; 4];
        let mut buf_len = 0;

        for j in 0..4 {
            if i + j >= input_bytes.len() {
                break;
            }

            let byte = input_bytes[i + j];
            if byte == b'=' {
                buf_len = j;
                break;
            }

            match ALPHABET.iter().position(|&b| b == byte) {
                Some(val) => {
                    buf[j] = val as u8;
                    buf_len = j + 1;
                }
                None => return Err(format!("Invalid base64 character: {}", byte as char)),
            }
        }

        match buf_len {
            2 => {
                result.push((buf[0] << 2) | (buf[1] >> 4));
            }
            3 => {
                result.push((buf[0] << 2) | (buf[1] >> 4));
                result.push((buf[1] << 4) | (buf[2] >> 2));
            }
            4 => {
                result.push((buf[0] << 2) | (buf[1] >> 4));
                result.push((buf[1] << 4) | (buf[2] >> 2));
                result.push((buf[2] << 6) | buf[3]);
            }
            _ => return Err("Invalid base64 length".to_string()),
        }

        i += 4;
    }

    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn test_auth_missing_token() {
        let auth = AuthMiddleware::new("secret".to_string());
        let result = auth.validate_connection_init(None);
        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), AuthError::MissingToken));
    }

    #[test]
    fn test_auth_empty_payload() {
        let auth = AuthMiddleware::new("secret".to_string());
        let empty_payload = json!({});
        let result = auth.validate_connection_init(Some(&empty_payload));
        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), AuthError::MissingToken));
    }

    #[test]
    fn test_auth_invalid_format() {
        let auth = AuthMiddleware::new("secret".to_string());
        let payload = json!({
            "authorization": "InvalidFormat"
        });
        let result = auth.validate_connection_init(Some(&payload));
        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), AuthError::InvalidFormat));
    }

    #[test]
    fn test_auth_malformed_token() {
        let auth = AuthMiddleware::new("secret".to_string());
        let payload = json!({
            "authorization": "Bearer invalid.token"
        });
        let result = auth.validate_connection_init(Some(&payload));
        assert!(result.is_err());
    }

    #[test]
    fn test_auth_disabled() {
        let auth = AuthMiddleware::with_enabled("secret".to_string(), false);
        let payload = json!({});
        let result = auth.validate_connection_init(Some(&payload));
        assert!(result.is_ok());
        let ctx = result.unwrap();
        assert_eq!(ctx.user_id, 1);
        assert_eq!(ctx.tenant_id, 1);
    }

    #[test]
    fn test_auth_context_creation() {
        let auth = AuthMiddleware::with_enabled("secret".to_string(), false);
        let result = auth.validate_connection_init(Some(&json!({})));
        assert!(result.is_ok());
        let ctx = result.unwrap();
        assert_eq!(ctx.user_id, 1);
        assert_eq!(ctx.tenant_id, 1);
        assert_eq!(ctx.exp, u64::MAX);
    }

    #[test]
    fn test_auth_error_display() {
        assert_eq!(
            AuthError::MissingToken.to_string(),
            "Missing authentication token"
        );
        assert_eq!(AuthError::InvalidFormat.to_string(), "Invalid token format");
        assert_eq!(AuthError::Expired.to_string(), "Token has expired");
        assert_eq!(
            AuthError::MissingUserId.to_string(),
            "Token missing user_id claim"
        );
    }
}
