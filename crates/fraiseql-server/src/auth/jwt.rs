// JWT validation and claims parsing
use std::collections::HashMap;

use jsonwebtoken::{Algorithm, DecodingKey, Validation, decode};
#[cfg(test)]
use jsonwebtoken::{EncodingKey, Header, encode};
use serde::{Deserialize, Serialize};

use crate::auth::error::{AuthError, Result};

/// Standard JWT claims with support for custom claims
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Claims {
    /// Subject (typically user ID)
    pub sub:   String,
    /// Issued at (Unix timestamp)
    pub iat:   u64,
    /// Expiration time (Unix timestamp)
    pub exp:   u64,
    /// Issuer
    pub iss:   String,
    /// Audience
    pub aud:   Vec<String>,
    /// Additional custom claims
    #[serde(flatten)]
    pub extra: HashMap<String, serde_json::Value>,
}

impl Claims {
    /// Get a custom claim by name
    pub fn get_custom(&self, key: &str) -> Option<&serde_json::Value> {
        self.extra.get(key)
    }

    /// Check if token is expired
    pub fn is_expired(&self) -> bool {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();
        self.exp <= now
    }
}

/// JWT validator configuration and validation logic
pub struct JwtValidator {
    validation: Validation,
    issuer:     String,
}

impl JwtValidator {
    /// Create a new JWT validator for a specific issuer
    ///
    /// # Arguments
    /// * `issuer` - The expected issuer URL
    /// * `algorithm` - The signing algorithm (e.g., RS256, HS256)
    ///
    /// # Errors
    /// Returns error if configuration is invalid
    pub fn new(issuer: &str, algorithm: Algorithm) -> Result<Self> {
        if issuer.is_empty() {
            return Err(AuthError::ConfigError {
                message: "Issuer cannot be empty".to_string(),
            });
        }

        let mut validation = Validation::new(algorithm);
        validation.set_issuer(&[issuer]);
        validation.validate_aud = false; // Allow any audience for now, can be restricted later

        Ok(Self {
            validation,
            issuer: issuer.to_string(),
        })
    }

    /// Validate a JWT token and extract claims
    ///
    /// # Arguments
    /// * `token` - The JWT token string
    /// * `key` - The public key bytes for signature verification
    ///
    /// # Errors
    /// Returns various errors: invalid token, expired token, invalid signature, etc.
    pub fn validate(&self, token: &str, key: &[u8]) -> Result<Claims> {
        let decoding_key = DecodingKey::from_rsa_pem(key).map_err(|e| AuthError::InvalidToken {
            reason: format!("Failed to parse public key: {}", e),
        })?;

        let token_data = decode::<Claims>(token, &decoding_key, &self.validation).map_err(|e| {
            use jsonwebtoken::errors::ErrorKind;
            match e.kind() {
                ErrorKind::ExpiredSignature => AuthError::TokenExpired,
                ErrorKind::InvalidSignature => AuthError::InvalidSignature,
                ErrorKind::InvalidIssuer => AuthError::InvalidToken {
                    reason: format!("Invalid issuer, expected: {}", self.issuer),
                },
                ErrorKind::MissingRequiredClaim(claim) => AuthError::MissingClaim {
                    claim: claim.clone(),
                },
                _ => AuthError::InvalidToken {
                    reason: e.to_string(),
                },
            }
        })?;

        let claims = token_data.claims;

        // Additional validation: check if token is expired (redundant but explicit)
        if claims.is_expired() {
            return Err(AuthError::TokenExpired);
        }

        Ok(claims)
    }

    /// Validate with HMAC secret (symmetric key)
    ///
    /// # Arguments
    /// * `token` - The JWT token string
    /// * `secret` - The shared secret for HMAC algorithms
    ///
    /// # Errors
    /// Returns various errors similar to `validate`
    pub fn validate_hmac(&self, token: &str, secret: &[u8]) -> Result<Claims> {
        let decoding_key = DecodingKey::from_secret(secret);

        let token_data = decode::<Claims>(token, &decoding_key, &self.validation).map_err(|e| {
            use jsonwebtoken::errors::ErrorKind;
            match e.kind() {
                ErrorKind::ExpiredSignature => AuthError::TokenExpired,
                ErrorKind::InvalidSignature => AuthError::InvalidSignature,
                ErrorKind::InvalidIssuer => AuthError::InvalidToken {
                    reason: format!("Invalid issuer, expected: {}", self.issuer),
                },
                ErrorKind::MissingRequiredClaim(claim) => AuthError::MissingClaim {
                    claim: claim.clone(),
                },
                _ => AuthError::InvalidToken {
                    reason: e.to_string(),
                },
            }
        })?;

        let claims = token_data.claims;

        if claims.is_expired() {
            return Err(AuthError::TokenExpired);
        }

        Ok(claims)
    }
}

/// Generate a JWT token (for testing and token creation)
#[cfg(test)]
pub fn generate_test_token(claims: &Claims, secret: &[u8]) -> Result<String> {
    let encoding_key = EncodingKey::from_secret(secret);
    encode(&Header::default(), claims, &encoding_key).map_err(|e| AuthError::Internal {
        message: format!("Failed to generate token: {}", e),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn create_test_claims() -> Claims {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();

        Claims {
            sub:   "user123".to_string(),
            iat:   now,
            exp:   now + 3600, // 1 hour expiry
            iss:   "https://example.com".to_string(),
            aud:   vec!["api".to_string()],
            extra: HashMap::new(),
        }
    }

    #[test]
    fn test_jwt_validator_creation() {
        let validator = JwtValidator::new("https://example.com", Algorithm::HS256);
        assert!(validator.is_ok());
    }

    #[test]
    fn test_jwt_validator_invalid_issuer() {
        let validator = JwtValidator::new("", Algorithm::HS256);
        assert!(matches!(validator, Err(AuthError::ConfigError { .. })));
    }

    #[test]
    fn test_claims_is_expired() {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();

        let mut claims = create_test_claims();
        claims.exp = now - 100; // Already expired

        assert!(claims.is_expired());
    }

    #[test]
    fn test_claims_not_expired() {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();

        let mut claims = create_test_claims();
        claims.exp = now + 3600; // Expires in 1 hour

        assert!(!claims.is_expired());
    }

    #[test]
    fn test_generate_and_validate_token() {
        let secret = b"test_secret_key_at_least_32_bytes_long";
        let validator = JwtValidator::new("https://example.com", Algorithm::HS256)
            .expect("Failed to create validator");

        let claims = create_test_claims();
        let token = generate_test_token(&claims, secret).expect("Failed to generate token");

        let validated_claims =
            validator.validate_hmac(&token, secret).expect("Failed to validate token");

        assert_eq!(validated_claims.sub, claims.sub);
        assert_eq!(validated_claims.iss, claims.iss);
    }

    #[test]
    fn test_validate_expired_token() {
        let secret = b"test_secret_key_at_least_32_bytes_long";
        let validator = JwtValidator::new("https://example.com", Algorithm::HS256)
            .expect("Failed to create validator");

        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();

        let mut claims = create_test_claims();
        claims.exp = now - 100; // Already expired

        let token = generate_test_token(&claims, secret).expect("Failed to generate token");

        let result = validator.validate_hmac(&token, secret);
        assert!(matches!(result, Err(AuthError::TokenExpired)));
    }

    #[test]
    fn test_validate_invalid_signature() {
        let secret = b"test_secret_key_at_least_32_bytes_long";
        let validator = JwtValidator::new("https://example.com", Algorithm::HS256)
            .expect("Failed to create validator");

        let claims = create_test_claims();
        let token = generate_test_token(&claims, secret).expect("Failed to generate token");

        let wrong_secret = b"wrong_secret_key_at_least_32_bytes_";
        let result = validator.validate_hmac(&token, wrong_secret);
        assert!(matches!(result, Err(AuthError::InvalidSignature)));
    }

    #[test]
    fn test_get_custom_claim() {
        let mut claims = create_test_claims();
        claims.extra.insert("email".to_string(), serde_json::json!("user@example.com"));
        claims.extra.insert("role".to_string(), serde_json::json!("admin"));

        assert_eq!(claims.get_custom("email"), Some(&serde_json::json!("user@example.com")));
        assert_eq!(claims.get_custom("role"), Some(&serde_json::json!("admin")));
        assert_eq!(claims.get_custom("nonexistent"), None);
    }
}
