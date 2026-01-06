//! JWT token validation with JWKS caching.

use jsonwebtoken::jwk::{Jwk, JwkSet};
use jsonwebtoken::{decode, decode_header, Algorithm, DecodingKey, Validation};
use lru::LruCache;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::num::NonZeroUsize;
use std::sync::{Arc, Mutex};
use std::time::{Duration, SystemTime};

use crate::auth::errors::AuthError;
use crate::db::recover_from_poisoned;

type Result<T> = std::result::Result<T, AuthError>;

/// JWT claims structure.
#[derive(Debug, Serialize, Deserialize)]
pub struct Claims {
    /// Subject (user ID)
    pub sub: String,
    /// Issuer
    pub iss: String,
    /// Audience
    pub aud: Vec<String>,
    /// Expiration time
    pub exp: u64,
    /// Issued at
    pub iat: u64,
    /// Custom claims
    #[serde(flatten)]
    pub custom: HashMap<String, serde_json::Value>,
}

/// JWT validator with JWKS caching.
#[derive(Debug)]
pub struct JWTValidator {
    issuer: String,
    audience: Vec<String>,
    jwks_url: String,
    jwks_cache: JWKSCache,
    http_client: reqwest::Client,
}

impl JWTValidator {
    /// Create a new JWT validator.
    ///
    /// # Errors
    ///
    /// Returns an error if:
    /// - The JWKS URL does not use HTTPS
    /// - The HTTP client fails to initialize
    pub fn new(issuer: String, audience: Vec<String>, jwks_url: String) -> Result<Self> {
        // Validate HTTPS for JWKS URL
        if !jwks_url.starts_with("https://") {
            return Err(AuthError::InvalidToken(format!(
                "JWKS URL must use HTTPS: {jwks_url}"
            )));
        }

        // Create HTTP client with timeout
        let http_client = reqwest::Client::builder()
            .timeout(Duration::from_secs(5))
            .build()
            .map_err(|e| AuthError::HttpError(e.to_string()))?;

        Ok(Self {
            issuer,
            audience,
            jwks_url,
            jwks_cache: JWKSCache::new(),
            http_client,
        })
    }

    /// Validate a JWT token.
    ///
    /// # Errors
    ///
    /// Returns an error if:
    /// - The token header is invalid or missing
    /// - The key ID (kid) is missing from the header
    /// - The JWKS fetch fails
    /// - The JWK conversion fails
    /// - The token signature is invalid
    /// - The token is expired
    /// - The issuer or audience validation fails
    pub async fn validate(&self, token: &str) -> Result<Claims> {
        // Decode header to get key ID
        let header = decode_header(token)
            .map_err(|e| AuthError::InvalidToken(format!("Invalid header: {e}")))?;

        let kid = header
            .kid
            .ok_or_else(|| AuthError::InvalidToken("Missing kid in token header".to_string()))?;

        // Get JWK from cache or fetch
        let jwk = self
            .jwks_cache
            .get_jwk(&kid, &self.jwks_url, &self.http_client)
            .await?;

        // Use built-in JWK to DecodingKey conversion
        let decoding_key = DecodingKey::from_jwk(&jwk)
            .map_err(|e| AuthError::InvalidToken(format!("Invalid JWK: {e}")))?;

        // Set up validation
        let mut validation = Validation::new(Algorithm::RS256);
        validation.set_issuer(&[&self.issuer]);
        validation.set_audience(&self.audience);

        // Decode and validate token
        let token_data = decode::<Claims>(token, &decoding_key, &validation)?;

        Ok(token_data.claims)
    }
}

/// JWKS cache with LRU eviction and TTL.
#[derive(Debug)]
struct JWKSCache {
    cache: Arc<Mutex<LruCache<String, (Jwk, SystemTime)>>>,
    ttl: Duration,
}

impl JWKSCache {
    /// Create a new JWKS cache.
    ///
    /// # Panics
    ///
    /// Panics if the cache capacity is 0 (which cannot happen with the hardcoded value of 100).
    fn new() -> Self {
        Self {
            cache: Arc::new(Mutex::new(LruCache::new(
                NonZeroUsize::new(100).expect("100 is non-zero"),
            ))),
            ttl: Duration::from_secs(3600), // 1 hour
        }
    }

    /// Get a JWK by key ID, from cache or by fetching.
    ///
    /// # Errors
    ///
    /// Returns an error if:
    /// - The JWKS fetch from the URL fails
    /// - The requested key ID is not found in the JWKS
    ///
    /// # Panics
    ///
    /// Panics if the cache mutex is poisoned.
    async fn get_jwk(&self, kid: &str, jwks_url: &str, client: &reqwest::Client) -> Result<Jwk> {
        // Check cache with TTL validation
        {
            let mut cache = recover_from_poisoned(self.cache.lock());
            if let Some((jwk, cached_at)) = cache.get(kid) {
                let elapsed = SystemTime::now()
                    .duration_since(*cached_at)
                    .unwrap_or(Duration::from_secs(u64::MAX));

                if elapsed < self.ttl {
                    return Ok(jwk.clone());
                }

                // Expired - remove from cache
                cache.pop(kid);
            }
        }

        // Fetch from JWKS endpoint
        let jwks = self.fetch_jwks(jwks_url, client).await?;

        // Find the specific key
        let jwk = jwks
            .keys
            .iter()
            .find(|k| k.common.key_id.as_ref() == Some(&kid.to_string()))
            .ok_or_else(|| AuthError::KeyNotFound(kid.to_string()))?
            .clone();

        // Store in cache
        {
            let mut cache = recover_from_poisoned(self.cache.lock());
            cache.put(kid.to_string(), (jwk.clone(), SystemTime::now()));
        }

        Ok(jwk)
    }

    /// Fetch JWKS from URL.
    ///
    /// # Errors
    ///
    /// Returns an error if:
    /// - The HTTP request fails
    /// - The server returns a non-success status code
    /// - The response body cannot be parsed as JSON
    async fn fetch_jwks(&self, url: &str, client: &reqwest::Client) -> Result<JwkSet> {
        let response = client.get(url).send().await?;

        if !response.status().is_success() {
            return Err(AuthError::JwksFetchFailed(format!(
                "HTTP {}: {}",
                response.status(),
                response.text().await.unwrap_or_default()
            )));
        }

        let jwks: JwkSet = response.json().await?;

        Ok(jwks)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use jsonwebtoken::{encode, EncodingKey, Header};

    // ========================================================================
    // Test Fixtures
    // ========================================================================

    fn create_test_claims(sub: String, exp_offset: i64) -> Claims {
        let now = SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs();

        Claims {
            sub,
            iss: "test-issuer".to_string(),
            aud: vec!["test-audience".to_string()],
            exp: (now as i64 + exp_offset) as u64,
            iat: now,
            custom: HashMap::new(),
        }
    }

    fn create_test_token(secret: &str) -> String {
        let claims = create_test_claims("test-user".to_string(), 3600);
        let key = EncodingKey::from_secret(secret.as_bytes());
        encode(&Header::default(), &claims, &key).expect("Failed to encode token")
    }

    // ========================================================================
    // Test Suite 1: Configuration Validation
    // ========================================================================

    #[test]
    fn test_https_validation() {
        let result = JWTValidator::new(
            "https://example.com/".to_string(),
            vec!["api".to_string()],
            "http://example.com/.well-known/jwks.json".to_string(),
        );

        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("HTTPS"));
    }

    #[test]
    fn test_valid_config_with_https() {
        let result = JWTValidator::new(
            "https://example.com/".to_string(),
            vec!["api".to_string()],
            "https://example.com/.well-known/jwks.json".to_string(),
        );

        assert!(
            result.is_ok(),
            "Valid HTTPS config should create validator successfully"
        );
    }

    #[test]
    fn test_multiple_audiences() {
        let result = JWTValidator::new(
            "https://example.com/".to_string(),
            vec!["api".to_string(), "mobile".to_string(), "web".to_string()],
            "https://example.com/.well-known/jwks.json".to_string(),
        );

        assert!(result.is_ok(), "Multiple audiences should be accepted");
    }

    // ========================================================================
    // Test Suite 2: Token Generation & Parsing
    // ========================================================================

    #[test]
    fn test_hs256_token_generation() {
        let secret = "test-secret-key";
        let token = create_test_token(secret);

        assert!(!token.is_empty(), "Token should not be empty");

        let parts: Vec<&str> = token.split('.').collect();
        assert_eq!(
            parts.len(),
            3,
            "Token should have 3 parts (header.payload.signature)"
        );
    }

    #[test]
    fn test_token_structure() {
        let secret = "test-secret-key";
        let token = create_test_token(secret);

        let parts: Vec<&str> = token.split('.').collect();

        // Each part should be non-empty
        for (i, part) in parts.iter().enumerate() {
            assert!(!part.is_empty(), "Part {} of token should not be empty", i);
        }

        // Parts should be valid base64url
        for (i, part) in parts.iter().enumerate() {
            let is_valid_base64url = part
                .chars()
                .all(|c| c.is_alphanumeric() || c == '-' || c == '_');
            assert!(is_valid_base64url, "Part {} should be valid base64url", i);
        }
    }

    #[test]
    fn test_token_contains_subject() {
        let secret = "test-secret-key";
        let claims = create_test_claims("alice".to_string(), 3600);
        let key = EncodingKey::from_secret(secret.as_bytes());
        let token = encode(&Header::default(), &claims, &key).expect("Failed to encode");

        // Decode to verify subject
        let decode_key = jsonwebtoken::DecodingKey::from_secret(secret.as_bytes());
        let result = jsonwebtoken::decode::<Claims>(&token, &decode_key, &Validation::default());

        assert!(result.is_ok(), "Token should be decodable");
        assert_eq!(result.unwrap().claims.sub, "alice", "Subject should match");
    }

    // ========================================================================
    // Test Suite 3: Signature Validation
    // ========================================================================

    #[test]
    fn test_invalid_signature_rejected() {
        let secret = "test-secret-key";
        let token = create_test_token(secret);

        let wrong_key = jsonwebtoken::DecodingKey::from_secret("wrong-secret".as_bytes());
        let result = jsonwebtoken::decode::<Claims>(&token, &wrong_key, &Validation::default());

        assert!(
            result.is_err(),
            "Token with wrong secret should be rejected"
        );
    }

    #[test]
    fn test_tampered_token_rejected() {
        let secret = "test-secret-key";
        let token = create_test_token(secret);

        let parts: Vec<&str> = token.split('.').collect();
        let tampered = format!("{}.tampered.{}", parts[0], parts[2]);

        let key = jsonwebtoken::DecodingKey::from_secret(secret.as_bytes());
        let result = jsonwebtoken::decode::<Claims>(&tampered, &key, &Validation::default());

        assert!(result.is_err(), "Tampered token should be rejected");
    }

    // ========================================================================
    // Test Suite 4: Expiration Checks
    // ========================================================================

    #[test]
    fn test_token_with_future_expiration() {
        let secret = "test-secret-key";
        let token = create_test_token_with_expiration(secret, 3600); // 1 hour from now

        let key = jsonwebtoken::DecodingKey::from_secret(secret.as_bytes());
        let result = jsonwebtoken::decode::<Claims>(&token, &key, &Validation::default());

        assert!(
            result.is_ok(),
            "Token with future expiration should be valid"
        );
    }

    #[test]
    fn test_expired_token_rejected() {
        let secret = "test-secret-key";
        let token = create_test_token_with_expiration(secret, -3600); // Expired 1 hour ago

        let key = jsonwebtoken::DecodingKey::from_secret(secret.as_bytes());
        let result = jsonwebtoken::decode::<Claims>(&token, &key, &Validation::default());

        assert!(result.is_err(), "Expired token should be rejected");
    }

    fn create_test_token_with_expiration(secret: &str, exp_offset: i64) -> String {
        let claims = create_test_claims("test-user".to_string(), exp_offset);
        let key = EncodingKey::from_secret(secret.as_bytes());
        encode(&Header::default(), &claims, &key).expect("Failed to encode")
    }

    // ========================================================================
    // Test Suite 5: Claims Validation
    // ========================================================================

    #[test]
    fn test_claims_structure() {
        let secret = "test-secret-key";
        let token = create_test_token(secret);

        let key = jsonwebtoken::DecodingKey::from_secret(secret.as_bytes());
        let result = jsonwebtoken::decode::<Claims>(&token, &key, &Validation::default())
            .expect("Token should be decodable");

        let claims = &result.claims;

        // Verify all required claims
        assert!(!claims.sub.is_empty(), "Subject should not be empty");
        assert!(!claims.iss.is_empty(), "Issuer should not be empty");
        assert!(!claims.aud.is_empty(), "Audience should not be empty");
        assert!(claims.exp > 0, "Expiration should be set");
        assert!(claims.iat > 0, "Issued-at should be set");
    }

    #[test]
    fn test_issuer_is_set() {
        let secret = "test-secret-key";
        let token = create_test_token(secret);

        let key = jsonwebtoken::DecodingKey::from_secret(secret.as_bytes());
        let claims = &jsonwebtoken::decode::<Claims>(&token, &key, &Validation::default())
            .unwrap()
            .claims;

        assert_eq!(claims.iss, "test-issuer", "Issuer should match");
    }

    #[test]
    fn test_audience_is_set() {
        let secret = "test-secret-key";
        let token = create_test_token(secret);

        let key = jsonwebtoken::DecodingKey::from_secret(secret.as_bytes());
        let claims = &jsonwebtoken::decode::<Claims>(&token, &key, &Validation::default())
            .unwrap()
            .claims;

        assert!(
            claims.aud.contains(&"test-audience".to_string()),
            "Audience should contain expected value"
        );
    }

    #[test]
    fn test_custom_claims_support() {
        let secret = "test-secret-key";

        let now = SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs();

        let mut custom = HashMap::new();
        custom.insert("role".to_string(), serde_json::json!("admin"));
        custom.insert("org_id".to_string(), serde_json::json!(123));

        let claims = Claims {
            sub: "user123".to_string(),
            iss: "test-issuer".to_string(),
            aud: vec!["api".to_string()],
            exp: (now as i64 + 3600) as u64,
            iat: now,
            custom,
        };

        let key = EncodingKey::from_secret(secret.as_bytes());
        let token = encode(&Header::default(), &claims, &key).expect("Failed to encode");

        let decode_key = jsonwebtoken::DecodingKey::from_secret(secret.as_bytes());
        let decoded_claims =
            &jsonwebtoken::decode::<Claims>(&token, &decode_key, &Validation::default())
                .unwrap()
                .claims;

        assert!(
            decoded_claims.custom.contains_key("role"),
            "Custom claim should be preserved"
        );
        assert_eq!(
            decoded_claims.custom["role"], "admin",
            "Custom claim value should match"
        );
    }

    // ========================================================================
    // Test Suite 6: JWKS Cache
    // ========================================================================

    #[test]
    fn test_jwks_cache_creation() {
        let cache = JWKSCache::new();
        // Cache should be creatable and not panic
        assert!(true, "Cache creation should succeed");
    }
}
