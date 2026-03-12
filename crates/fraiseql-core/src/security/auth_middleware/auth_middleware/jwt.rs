//! JWT decoding, signature verification, and expiry checking.

use chrono::{DateTime, Utc};
use jsonwebtoken::{Validation, decode};

use crate::security::errors::{Result, SecurityError};

use super::{
    AuthMiddleware,
    signing_key::SigningKey,
    types::{AuthenticatedUser, JwtClaims, TokenClaims},
};

impl AuthMiddleware {
    /// Validate token with cryptographic signature verification.
    ///
    /// This is the secure path used when a signing key is configured.
    pub(super) fn validate_token_with_signature(
        &self,
        token: &str,
        signing_key: &SigningKey,
    ) -> Result<AuthenticatedUser> {
        // Get the decoding key
        let decoding_key = signing_key.to_decoding_key()?;

        // Build validation configuration
        let mut validation = Validation::new(signing_key.algorithm());

        // Configure issuer validation (only validate if configured)
        if let Some(ref issuer) = self.config.issuer {
            validation.set_issuer(&[issuer]);
        }
        // Note: If issuer is not set, validation.iss is None and won't be validated

        // Configure audience validation
        if let Some(ref audience) = self.config.audience {
            validation.set_audience(&[audience]);
        } else {
            validation.validate_aud = false;
        }

        // Set clock skew tolerance
        validation.leeway = self.config.clock_skew_secs;

        // Decode and validate the token
        let token_data = decode::<JwtClaims>(token, &decoding_key, &validation).map_err(|e| {
            match e.kind() {
                jsonwebtoken::errors::ErrorKind::ExpiredSignature => {
                    // Try to extract the actual expiry time from the token
                    SecurityError::TokenExpired {
                        expired_at: Utc::now(), // Approximate - actual time is not accessible
                    }
                },
                jsonwebtoken::errors::ErrorKind::InvalidSignature => SecurityError::InvalidToken,
                jsonwebtoken::errors::ErrorKind::InvalidIssuer => SecurityError::InvalidToken,
                jsonwebtoken::errors::ErrorKind::InvalidAudience => SecurityError::InvalidToken,
                jsonwebtoken::errors::ErrorKind::InvalidAlgorithm => {
                    SecurityError::InvalidTokenAlgorithm {
                        algorithm: format!("{:?}", signing_key.algorithm()),
                    }
                },
                jsonwebtoken::errors::ErrorKind::MissingRequiredClaim(claim) => {
                    SecurityError::TokenMissingClaim {
                        claim: claim.clone(),
                    }
                },
                _ => SecurityError::InvalidToken,
            }
        })?;

        let claims = token_data.claims;

        // Extract scopes (supports multiple formats)
        let scopes = self.extract_scopes_from_jwt_claims(&claims);

        // Extract user ID (required)
        let user_id = claims.sub.ok_or(SecurityError::TokenMissingClaim {
            claim: "sub".to_string(),
        })?;

        // Extract expiration (required)
        let exp = claims.exp.ok_or(SecurityError::TokenMissingClaim {
            claim: "exp".to_string(),
        })?;

        let expires_at =
            DateTime::<Utc>::from_timestamp(exp, 0).ok_or(SecurityError::InvalidToken)?;

        Ok(AuthenticatedUser {
            user_id,
            scopes,
            expires_at,
        })
    }

    /// Validate token structure only (no signature verification).
    ///
    /// WARNING: This is insecure and should only be used for testing
    /// or when signature verification is handled elsewhere.
    pub(super) fn validate_token_structure_only(&self, token: &str) -> Result<AuthenticatedUser> {
        // Validate basic structure
        self.validate_token_structure(token)?;

        // Parse claims
        let claims = self.parse_claims(token)?;

        // Extract and validate 'exp' claim (required)
        let exp = claims.exp.ok_or(SecurityError::TokenMissingClaim {
            claim: "exp".to_string(),
        })?;

        // Check expiry
        let expires_at =
            DateTime::<Utc>::from_timestamp(exp, 0).ok_or(SecurityError::InvalidToken)?;

        if expires_at <= Utc::now() {
            return Err(SecurityError::TokenExpired {
                expired_at: expires_at,
            });
        }

        // Extract and validate 'sub' claim (required)
        let user_id = claims.sub.ok_or(SecurityError::TokenMissingClaim {
            claim: "sub".to_string(),
        })?;

        // Extract optional claims
        let scopes = claims
            .scope
            .as_ref()
            .map(|s| s.split_whitespace().map(String::from).collect())
            .unwrap_or_default();

        Ok(AuthenticatedUser {
            user_id,
            scopes,
            expires_at,
        })
    }

    /// Validate token structure (basic checks)
    ///
    /// A real implementation would validate the signature here.
    /// For now, we just check basic structure.
    pub(super) fn validate_token_structure(&self, token: &str) -> Result<()> {
        // JWT has 3 parts separated by dots: header.payload.signature
        let parts: Vec<&str> = token.split('.').collect();
        if parts.len() != 3 {
            return Err(SecurityError::InvalidToken);
        }

        // Check that each part is non-empty
        if parts.iter().any(|p| p.is_empty()) {
            return Err(SecurityError::InvalidToken);
        }

        Ok(())
    }

    /// Parse JWT claims (simplified, for demo purposes)
    ///
    /// In a real implementation, this would decode and validate the JWT signature.
    /// For testing, we accept a special test token format: "test:{json_payload}"
    pub(super) fn parse_claims(&self, token: &str) -> Result<TokenClaims> {
        // Split the token
        let parts: Vec<&str> = token.split('.').collect();
        if parts.len() != 3 {
            return Err(SecurityError::InvalidToken);
        }

        // For testing, we use a simple format: part1.{json}.part3
        // where {json} is a base64-like encoded JSON
        // Since we don't have base64 in core dependencies, we'll try to parse directly
        let payload_part = parts[1];

        // Try to decode as hex (simpler than base64 and no dependencies)
        // For test tokens, we'll encode the JSON as hex
        if let Ok(decoded) = hex::decode(payload_part) {
            if let Ok(json_str) = std::str::from_utf8(&decoded) {
                if let Ok(json) = serde_json::from_str::<serde_json::Value>(json_str) {
                    return Ok(self.extract_claims_from_json(&json));
                }
            }
        }

        // If hex decoding fails, try to parse as UTF-8 directly (for test tokens created inline)
        if let Ok(json) = serde_json::from_str::<serde_json::Value>(payload_part) {
            return Ok(self.extract_claims_from_json(&json));
        }

        Err(SecurityError::InvalidToken)
    }

    /// Extract claims from parsed JSON
    pub(super) fn extract_claims_from_json(&self, json: &serde_json::Value) -> TokenClaims {
        let sub = json["sub"].as_str().map(String::from);
        let exp = json["exp"].as_i64();
        let scope = json["scope"].as_str().map(String::from);
        let aud = json["aud"]
            .as_array()
            .map(|arr| arr.iter().filter_map(|v| v.as_str().map(String::from)).collect());
        let iss = json["iss"].as_str().map(String::from);

        TokenClaims {
            sub,
            exp,
            scope,
            aud,
            iss,
        }
    }
}
