//! JWT validation, claims parsing, and token generation.
use std::collections::HashMap;

use jsonwebtoken::{Algorithm, DecodingKey, EncodingKey, Header, Validation, decode, encode};
use serde::{Deserialize, Serialize};

use crate::{
    audit::logger::{AuditEventType, SecretType, get_audit_logger},
    error::{AuthError, Result},
};

/// Maximum age of a JWT token measured from its `iat` (issued-at) claim.
///
/// Tokens whose `iat` is more than this many seconds in the past are rejected
/// as potentially replayed credentials.  24 h is a conservative upper bound —
/// short-lived access tokens expire via `exp` long before this limit is reached;
/// this guard targets long-lived or replayed tokens that somehow passed `exp` checks.
pub const MAX_TOKEN_AGE_SECS: u64 = 86_400;

/// Maximum allowed clock skew for `iat` and `nbf` claim checks.
///
/// A 5-minute window accommodates minor time drift between issuer and validator
/// without opening a meaningful forgery window.
pub const MAX_CLOCK_SKEW_SECS: u64 = 300;

/// Whether the `aud` claim is required in every validated token.
///
/// Always `true`: the `aud` claim MUST be present and MUST exactly match the
/// configured audience(s).  A missing `aud` is rejected with
/// [`AuthError::MissingClaim`]; a mismatched `aud` is rejected with
/// [`AuthError::InvalidToken`].
///
/// This constant is provided for documentation and for downstream code that
/// needs to assert the validation posture at compile time.  It prevents
/// cross-service token replay: a token issued for service A cannot be accepted
/// by service B.
pub const REQUIRE_AUD: bool = true;

/// Standard JWT claims with support for custom claims
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Claims {
    /// Subject (typically user ID)
    pub sub:   String,
    /// Issued at (Unix timestamp)
    pub iat:   u64,
    /// Expiration time (Unix timestamp)
    pub exp:   u64,
    /// Not-before time (Unix timestamp) — optional per RFC 7519 §4.1.5.
    ///
    /// When present, the token MUST NOT be accepted before this time (plus
    /// [`MAX_CLOCK_SKEW_SECS`]).  When absent, the not-before check is skipped.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub nbf:   Option<u64>,
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
    ///
    /// SECURITY: If system time cannot be determined, returns true (treats token as expired)
    /// This is a fail-safe approach to prevent accepting tokens when we can't verify expiry
    pub fn is_expired(&self) -> bool {
        let now = match std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH) {
            Ok(duration) => duration.as_secs(),
            Err(e) => {
                // CRITICAL: System time failure - treat token as expired (fail-safe)
                // Log this critical error for operators to investigate
                tracing::error!(
                    error = %e,
                    "CRITICAL: System time error in token expiry check — \
                     this indicates a system clock issue. Token rejected as safety measure."
                );
                // Return current time as far in the future to ensure token is expired
                u64::MAX
            },
        };
        self.exp <= now
    }

    /// Validate temporal claims: `iat` staleness/skew and `nbf` not-before.
    ///
    /// Enforces three RFC 7519 temporal guards beyond `exp`:
    ///
    /// - `iat` must not be more than [`MAX_CLOCK_SKEW_SECS`] seconds in the future (forgery guard —
    ///   a future `iat` is implausible for a legitimately issued token).
    /// - `iat` must not be more than [`MAX_TOKEN_AGE_SECS`] seconds in the past (replay guard — a
    ///   stale `iat` indicates a replayed or abnormally long-lived token).
    /// - `nbf` (if present) must not be more than [`MAX_CLOCK_SKEW_SECS`] seconds in the future
    ///   (RFC 7519 §4.1.5 not-before enforcement).
    ///
    /// # Errors
    ///
    /// - [`AuthError::TokenIssuedInFuture`] if `iat > now + MAX_CLOCK_SKEW_SECS`.
    /// - [`AuthError::TokenTooOld`] if `now - iat > MAX_TOKEN_AGE_SECS`.
    /// - [`AuthError::TokenNotYetValid`] if `nbf > now + MAX_CLOCK_SKEW_SECS`.
    /// - [`AuthError::SystemTimeError`] if the system clock cannot be read.
    pub fn validate_temporal_claims(&self) -> Result<()> {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_err(|e| AuthError::SystemTimeError {
                message: format!("Cannot determine current time for temporal validation: {e}"),
            })?
            .as_secs();

        // iat: must not be substantially in the future (forgery / clock-skew guard).
        if self.iat > now.saturating_add(MAX_CLOCK_SKEW_SECS) {
            return Err(AuthError::TokenIssuedInFuture);
        }

        // iat: must not be older than MAX_TOKEN_AGE_SECS (replay guard).
        if now.saturating_sub(self.iat) > MAX_TOKEN_AGE_SECS {
            return Err(AuthError::TokenTooOld);
        }

        // nbf: not-before — token must not be used before the claim (with clock skew).
        if let Some(nbf) = self.nbf {
            if nbf > now.saturating_add(MAX_CLOCK_SKEW_SECS) {
                return Err(AuthError::TokenNotYetValid);
            }
        }

        Ok(())
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
        // Require the `aud` claim to be present in every token.
        // `validate_aud = true` without a configured expected audience means any non-empty
        // `aud` value is accepted; callers should further restrict this by calling
        // `with_audiences()` to pin the validator to specific service audiences.
        // Setting `validate_aud = false` (the previous default) silently accepts tokens
        // issued for any service — a cross-service token replay vulnerability.
        validation.validate_aud = true;

        Ok(Self {
            validation,
            issuer: issuer.to_string(),
        })
    }

    /// Set the audiences that this validator will accept.
    ///
    /// Recommended for production to restrict JWT usage to specific services.
    ///
    /// # Errors
    ///
    /// Returns [`AuthError::ConfigError`] if `audiences` is empty.
    pub fn with_audiences(mut self, audiences: &[&str]) -> Result<Self> {
        if audiences.is_empty() {
            return Err(AuthError::ConfigError {
                message: "At least one audience must be configured".to_string(),
            });
        }

        self.validation
            .set_audience(&audiences.iter().map(|s| (*s).to_string()).collect::<Vec<_>>());
        self.validation.validate_aud = true;

        Ok(self)
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
            let error = match e.kind() {
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
            };

            // Audit log: JWT validation failure
            let audit_logger = get_audit_logger();
            audit_logger.log_failure(
                AuditEventType::JwtValidation,
                SecretType::JwtToken,
                None, // Subject not yet known at this point
                "validate",
                &e.to_string(),
            );

            error
        })?;

        let claims = token_data.claims;

        // Additional validation: check if token is expired (redundant but explicit)
        if claims.is_expired() {
            let audit_logger = get_audit_logger();
            audit_logger.log_failure(
                AuditEventType::JwtValidation,
                SecretType::JwtToken,
                Some(claims.sub),
                "validate",
                "Token expired",
            );
            return Err(AuthError::TokenExpired);
        }

        // Temporal claims validation: iat staleness/skew and nbf not-before (S40).
        if let Err(e) = claims.validate_temporal_claims() {
            let audit_logger = get_audit_logger();
            audit_logger.log_failure(
                AuditEventType::JwtValidation,
                SecretType::JwtToken,
                Some(claims.sub),
                "validate",
                &e.to_string(),
            );
            return Err(e);
        }

        // Audit log: JWT validation success
        let audit_logger = get_audit_logger();
        audit_logger.log_success(
            AuditEventType::JwtValidation,
            SecretType::JwtToken,
            Some(claims.sub.clone()),
            "validate",
        );

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

        // Temporal claims validation: iat staleness/skew and nbf not-before (S40).
        claims.validate_temporal_claims()?;

        Ok(claims)
    }
}

/// Generate a JWT token with RS256 signature
///
/// # Arguments
/// * `claims` - The JWT claims to sign
/// * `private_key_pem` - RSA private key in PEM format
///
/// # Errors
/// Returns error if token generation or signing fails
pub fn generate_rs256_token(claims: &Claims, private_key_pem: &[u8]) -> Result<String> {
    let encoding_key =
        EncodingKey::from_rsa_pem(private_key_pem).map_err(|e| AuthError::Internal {
            message: format!("Failed to parse private key: {}", e),
        })?;

    let header = Header::new(Algorithm::RS256);
    encode(&header, claims, &encoding_key).map_err(|e| AuthError::Internal {
        message: format!("Failed to generate RS256 token: {}", e),
    })
}

/// Generate a JWT token with HMAC secret (HS256)
///
/// # Arguments
/// * `claims` - The JWT claims to sign
/// * `secret` - The shared secret for HMAC
///
/// # Errors
/// Returns error if token generation or signing fails
pub fn generate_hs256_token(claims: &Claims, secret: &[u8]) -> Result<String> {
    let encoding_key = EncodingKey::from_secret(secret);
    encode(&Header::default(), claims, &encoding_key).map_err(|e| AuthError::Internal {
        message: format!("Failed to generate HS256 token: {}", e),
    })
}

/// Generate a JWT token (for testing and token creation)
///
/// # Errors
///
/// Returns `AuthError::Internal` if token encoding fails.
#[cfg(test)]
pub fn generate_test_token(claims: &Claims, secret: &[u8]) -> Result<String> {
    generate_hs256_token(claims, secret)
}

#[cfg(test)]
mod tests {
    #[allow(clippy::wildcard_imports)]
    // Reason: test module — wildcard keeps test boilerplate minimal
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
            nbf:   None,
            iss:   "https://example.com".to_string(),
            aud:   vec!["api".to_string()],
            extra: HashMap::new(),
        }
    }

    #[test]
    fn test_jwt_validator_creation() {
        JwtValidator::new("https://example.com", Algorithm::HS256)
            .unwrap_or_else(|e| panic!("expected Ok for valid issuer: {e}"));
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

    /// Helper: create a validator configured for the test audience "api".
    fn make_test_validator() -> JwtValidator {
        JwtValidator::new("https://example.com", Algorithm::HS256)
            .expect("Failed to create validator")
            .with_audiences(&["api"])
            .expect("Failed to set audiences")
    }

    #[test]
    fn test_generate_and_validate_token() {
        let secret = b"test_secret_key_at_least_32_bytes_long";
        let validator = make_test_validator();

        let claims = create_test_claims();
        let token = generate_test_token(&claims, secret).expect("Failed to generate token");

        let validated_claims =
            validator.validate_hmac(&token, secret).expect("Failed to validate token");

        assert_eq!(validated_claims.sub, claims.sub);
        assert_eq!(validated_claims.iss, claims.iss);
    }

    #[test]
    fn test_validate_without_audience_rejects_token() {
        // A validator created without `with_audiences()` must reject tokens
        // (audience claim required but no expected audience configured).
        let secret = b"test_secret_key_at_least_32_bytes_long";
        let validator = JwtValidator::new("https://example.com", Algorithm::HS256)
            .expect("Failed to create validator");

        let claims = create_test_claims();
        let token = generate_test_token(&claims, secret).expect("Failed to generate token");

        let result = validator.validate_hmac(&token, secret);
        assert!(result.is_err(), "validator without configured audience must reject tokens");
    }

    #[test]
    fn test_validate_expired_token() {
        let secret = b"test_secret_key_at_least_32_bytes_long";
        let validator = make_test_validator();

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
        let validator = make_test_validator();

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

    // ── S40: iat / nbf temporal claim tests ──────────────────────────────────

    #[test]
    fn test_rejects_token_with_iat_too_far_in_future() {
        let secret = b"test_secret_key_at_least_32_bytes_long";
        let validator = make_test_validator();
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();
        let mut claims = create_test_claims();
        claims.iat = now + MAX_CLOCK_SKEW_SECS + 60; // 360s ahead — beyond the 300s skew window
        let token = generate_test_token(&claims, secret).expect("token generation must succeed");
        let result = validator.validate_hmac(&token, secret);
        assert!(
            matches!(result, Err(AuthError::TokenIssuedInFuture)),
            "expected TokenIssuedInFuture, got: {result:?}"
        );
    }

    #[test]
    fn test_accepts_token_with_iat_within_clock_skew() {
        let secret = b"test_secret_key_at_least_32_bytes_long";
        let validator = make_test_validator();
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();
        let mut claims = create_test_claims();
        claims.iat = now + 60; // 60s ahead — within MAX_CLOCK_SKEW_SECS=300
        let token = generate_test_token(&claims, secret).expect("token generation must succeed");
        validator
            .validate_hmac(&token, secret)
            .unwrap_or_else(|e| panic!("expected Ok for iat within clock skew: {e}"));
    }

    #[test]
    fn test_rejects_token_with_iat_too_old() {
        let secret = b"test_secret_key_at_least_32_bytes_long";
        let validator = make_test_validator();
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();
        let mut claims = create_test_claims();
        claims.iat = now - MAX_TOKEN_AGE_SECS - 60; // just past the 24 h limit
        claims.exp = now + 3600; // still not expired
        let token = generate_test_token(&claims, secret).expect("token generation must succeed");
        let result = validator.validate_hmac(&token, secret);
        assert!(
            matches!(result, Err(AuthError::TokenTooOld)),
            "expected TokenTooOld, got: {result:?}"
        );
    }

    #[test]
    fn test_accepts_token_at_iat_max_age_boundary() {
        let secret = b"test_secret_key_at_least_32_bytes_long";
        let validator = make_test_validator();
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();
        let mut claims = create_test_claims();
        claims.iat = now - MAX_TOKEN_AGE_SECS; // exactly at boundary — must be accepted
        claims.exp = now + 3600;
        let token = generate_test_token(&claims, secret).expect("token generation must succeed");
        validator
            .validate_hmac(&token, secret)
            .unwrap_or_else(|e| panic!("expected Ok for iat exactly at max-age boundary: {e}"));
    }

    #[test]
    fn test_rejects_token_with_nbf_in_future() {
        let secret = b"test_secret_key_at_least_32_bytes_long";
        let validator = make_test_validator();
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();
        let mut claims = create_test_claims();
        claims.nbf = Some(now + MAX_CLOCK_SKEW_SECS + 60); // beyond the skew window
        let token = generate_test_token(&claims, secret).expect("token generation must succeed");
        let result = validator.validate_hmac(&token, secret);
        assert!(
            matches!(result, Err(AuthError::TokenNotYetValid)),
            "expected TokenNotYetValid, got: {result:?}"
        );
    }

    #[test]
    fn test_accepts_token_with_nbf_in_past() {
        let secret = b"test_secret_key_at_least_32_bytes_long";
        let validator = make_test_validator();
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();
        let mut claims = create_test_claims();
        claims.nbf = Some(now - 600); // 10 min ago — valid
        let token = generate_test_token(&claims, secret).expect("token generation must succeed");
        validator
            .validate_hmac(&token, secret)
            .unwrap_or_else(|e| panic!("expected Ok for nbf in past: {e}"));
    }

    #[test]
    fn test_accepts_token_without_nbf() {
        let secret = b"test_secret_key_at_least_32_bytes_long";
        let validator = make_test_validator();
        let claims = create_test_claims(); // nbf is None
        let token = generate_test_token(&claims, secret).expect("token generation must succeed");
        validator
            .validate_hmac(&token, secret)
            .unwrap_or_else(|e| panic!("expected Ok for token without nbf: {e}"));
    }
}
