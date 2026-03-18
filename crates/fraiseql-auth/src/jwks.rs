//! JWKS (JSON Web Key Set) cache for OIDC ID token signature verification.
//!
//! Fetches and caches public keys from an OIDC provider's JWKS endpoint,
//! automatically refreshing when the TTL expires.

/// Request timeout for JWKS endpoint fetches.
const JWKS_FETCH_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(30);

/// Maximum byte size for a JWKS response.
///
/// A real JWKS document contains a handful of RSA/EC public keys, each a few
/// hundred bytes. 1 `MiB` is generous while blocking allocation-bomb responses
/// from a compromised OIDC provider.
const MAX_JWKS_RESPONSE_BYTES: usize = 1024 * 1024; // 1 MiB

use std::{
    collections::HashMap,
    sync::RwLock,
    time::{Duration, Instant},
};

use jsonwebtoken::DecodingKey;
use serde::Deserialize;
use tracing::debug;

/// Errors that can occur when constructing a JWKS cache.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum JwksError {
    /// The provided `jwks_uri` is not a valid URL.
    #[error("Invalid jwks_uri '{uri}': {source}")]
    InvalidUrl {
        /// The URI that failed to parse.
        uri: String,
        /// The underlying parse error.
        source: url::ParseError,
    },
    /// The URL scheme is not HTTPS (or HTTP on localhost for dev).
    #[error("Invalid jwks_uri scheme '{scheme}': must be https (or http on localhost)")]
    InvalidScheme {
        /// The rejected scheme.
        scheme: String,
    },
    /// Failed to build the HTTP client.
    #[error("Failed to build HTTP client: {0}")]
    HttpClient(#[from] reqwest::Error),
}

/// JWKS document returned by the provider.
#[derive(Debug, Deserialize)]
struct JwksDocument {
    keys: Vec<JwkKey>,
}

/// A single JWK (JSON Web Key) entry.
#[derive(Debug, Deserialize)]
struct JwkKey {
    /// Key ID — must be present for key selection.
    kid: Option<String>,
    /// Key type: `RSA` or `EC`.
    kty: String,
    /// RSA modulus (Base64url-encoded).
    n:   Option<String>,
    /// RSA exponent (Base64url-encoded).
    e:   Option<String>,
    /// EC x-coordinate (Base64url-encoded).
    x:   Option<String>,
    /// EC y-coordinate (Base64url-encoded).
    y:   Option<String>,
}

/// Cached JWKS keys with TTL-based refresh.
pub struct JwksCache {
    keys:         RwLock<HashMap<String, DecodingKey>>,
    jwks_uri:     String,
    last_fetched: RwLock<Option<Instant>>,
    ttl:          Duration,
    client:       reqwest::Client,
}

impl JwksCache {
    /// Create a new JWKS cache, validating `jwks_uri` before storing it.
    ///
    /// Keys are lazily fetched on first access.
    ///
    /// # Errors
    ///
    /// Returns [`JwksError`] if `jwks_uri` is not a valid URL, uses a
    /// non-HTTPS scheme (HTTP is allowed only for localhost), or if the
    /// HTTP client cannot be built.
    pub fn new(jwks_uri: &str, ttl: Duration) -> Result<Self, JwksError> {
        // Validate the URI at construction time (SSRF prevention pattern).
        let parsed = reqwest::Url::parse(jwks_uri).map_err(|e| JwksError::InvalidUrl {
            uri: jwks_uri.to_string(),
            source: e,
        })?;

        // OIDC Core 1.0 Section 3 requires HTTPS for jwks_uri. Allow HTTP only
        // for local development (e.g., http://localhost mock OIDC providers).
        let allowed = match parsed.scheme() {
            "https" => true,
            "http" => parsed.host_str().is_some_and(|h| {
                h == "localhost" || h == "127.0.0.1" || h == "[::1]" || h == "::1"
            }),
            _ => false,
        };
        if !allowed {
            return Err(JwksError::InvalidScheme {
                scheme: parsed.scheme().to_string(),
            });
        }

        let client = reqwest::Client::builder()
            .timeout(JWKS_FETCH_TIMEOUT)
            .build()?;

        Ok(Self {
            keys: RwLock::new(HashMap::new()),
            jwks_uri: jwks_uri.to_string(),
            last_fetched: RwLock::new(None),
            ttl,
            client,
        })
    }

    /// Get a decoding key by `kid`, fetching from the remote JWKS endpoint if
    /// the cache is stale or the key is missing.
    ///
    /// # Errors
    ///
    /// Returns a `String` error if the remote JWKS endpoint is unreachable, returns
    /// an oversized response, returns invalid JSON, or if the internal cache lock
    /// is poisoned.
    pub async fn get_key(&self, kid: &str) -> Result<Option<DecodingKey>, String> {
        // Fast path: cache is fresh and key exists
        if let Some(key) = self.get_key_from_cache(kid) {
            if !self.is_stale() {
                return Ok(Some(key));
            }
        }
        // Slow path: fetch and retry
        self.fetch_keys().await?;
        Ok(self.get_key_from_cache(kid))
    }

    /// Look up a key in the local cache without fetching.
    pub fn get_key_from_cache(&self, kid: &str) -> Option<DecodingKey> {
        self.keys.read().ok()?.get(kid).cloned()
    }

    /// Force a refresh of the JWKS keys from the remote endpoint.
    ///
    /// # Errors
    ///
    /// Propagates errors from the remote JWKS fetch (network failure, oversized
    /// response, JSON parse error, or poisoned cache lock).
    pub async fn force_refresh(&self) -> Result<(), String> {
        self.fetch_keys().await
    }

    /// Check whether the cache has exceeded its TTL.
    fn is_stale(&self) -> bool {
        self.last_fetched
            .read()
            .ok()
            .and_then(|guard| *guard)
            .is_none_or(|t| t.elapsed() > self.ttl)
    }

    /// Fetch the JWKS document and populate the cache.
    async fn fetch_keys(&self) -> Result<(), String> {
        debug!(uri = %self.jwks_uri, "Fetching JWKS keys");

        let body = self
            .client
            .get(&self.jwks_uri)
            .send()
            .await
            .map_err(|e| format!("JWKS fetch failed: {e}"))?
            .bytes()
            .await
            .map_err(|e| format!("JWKS read failed: {e}"))?;
        if body.len() > MAX_JWKS_RESPONSE_BYTES {
            return Err(format!(
                "JWKS response too large ({} bytes, max {MAX_JWKS_RESPONSE_BYTES})",
                body.len()
            ));
        }
        let jwks: JwksDocument =
            serde_json::from_slice(&body).map_err(|e| format!("JWKS parse failed: {e}"))?;

        let mut cache = self.keys.write().map_err(|e| format!("JWKS lock poisoned: {e}"))?;
        cache.clear();
        for key in &jwks.keys {
            if let Some(kid) = &key.kid {
                if let Some(decoding_key) = Self::convert_jwk(key) {
                    cache.insert(kid.clone(), decoding_key);
                }
            }
        }
        if let Ok(mut last) = self.last_fetched.write() {
            *last = Some(Instant::now());
        }
        debug!(key_count = cache.len(), "JWKS cache refreshed");
        Ok(())
    }

    /// Convert a JWK entry into a `DecodingKey`.
    fn convert_jwk(jwk: &JwkKey) -> Option<DecodingKey> {
        match jwk.kty.as_str() {
            "RSA" => {
                let n = jwk.n.as_ref()?;
                let e = jwk.e.as_ref()?;
                DecodingKey::from_rsa_components(n, e).ok()
            },
            "EC" => {
                let x = jwk.x.as_ref()?;
                let y = jwk.y.as_ref()?;
                DecodingKey::from_ec_components(x, y).ok()
            },
            _ => None,
        }
    }
}

impl std::fmt::Debug for JwksCache {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let key_count = self.keys.read().map(|k| k.len()).unwrap_or(0);
        f.debug_struct("JwksCache")
            .field("jwks_uri", &self.jwks_uri)
            .field("ttl", &self.ttl)
            .field("cached_keys", &key_count)
            .finish()
    }
}

#[allow(clippy::unwrap_used)] // Reason: test code, panics are acceptable
#[cfg(test)]
mod tests {
    use std::time::Duration;

    use wiremock::{
        Mock, MockServer, ResponseTemplate,
        matchers::{method, path},
    };

    #[allow(clippy::wildcard_imports)]  // Reason: test module wildcard import; brings all items into test scope
    // Reason: test modules use wildcard imports for conciseness
    use super::*;

    fn jwks_fixture() -> serde_json::Value {
        serde_json::json!({
            "keys": [
                {
                    "kty": "RSA",
                    "kid": "test-key-1",
                    "use": "sig",
                    "n": "0vx7agoebGcQSuuPiLJXZptN9nndrQmbXEps2aiAFbWhM78LhWx4cbbfAAtVT86zwu1RK7aPFFxuhDR1L6tSoc_BJECPebWKRXjBZCiFV4n3oknjhMstn64tZ_2W-5JsGY4Hc5n9yBXArwl93lqt7_RN5w6Cf0h4QyQ5v-65YGjQR0_FDW2QvzqY368QQMicAtaSqzs8KJZgnYb9c7d0zgdAZHzu6qMQvRL5hajrn1n91CbOpbISD08qNLyrdkt-bFTWhAI4vMQFh6WeZu0fM4lFd2NcRwr3XPksINHaQ-G_xBniIqbw0Ls1jF44-csFCur-kEgU8awapJzKnqDKgw",
                    "e": "AQAB"
                }
            ]
        })
    }

    #[tokio::test]
    async fn test_jwks_cache_empty() {
        let cache =
            JwksCache::new("https://example.com/.well-known/jwks.json", Duration::from_secs(3600)).unwrap();
        assert!(cache.get_key_from_cache("nonexistent_kid").is_none());
    }

    #[tokio::test]
    async fn test_jwks_cache_fetch_and_retrieve() {
        let mock_server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/.well-known/jwks.json"))
            .respond_with(ResponseTemplate::new(200).set_body_json(jwks_fixture()))
            .mount(&mock_server)
            .await;

        let cache = JwksCache::new(
            &format!("{}/.well-known/jwks.json", mock_server.uri()),
            Duration::from_secs(3600),
        ).unwrap();

        let key = cache.get_key("test-key-1").await.unwrap();
        assert!(key.is_some());
    }

    #[tokio::test]
    async fn test_jwks_cache_missing_kid_returns_none() {
        let mock_server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/.well-known/jwks.json"))
            .respond_with(ResponseTemplate::new(200).set_body_json(jwks_fixture()))
            .mount(&mock_server)
            .await;

        let cache = JwksCache::new(
            &format!("{}/.well-known/jwks.json", mock_server.uri()),
            Duration::from_secs(3600),
        ).unwrap();

        let key = cache.get_key("nonexistent-kid").await.unwrap();
        assert!(key.is_none());
    }

    #[tokio::test]
    async fn test_jwks_cache_ttl_refresh() {
        let mock_server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/.well-known/jwks.json"))
            .respond_with(ResponseTemplate::new(200).set_body_json(jwks_fixture()))
            .expect(2) // Should be fetched twice (initial + after TTL expires)
            .mount(&mock_server)
            .await;

        // TTL of 0 means always stale
        let cache = JwksCache::new(
            &format!("{}/.well-known/jwks.json", mock_server.uri()),
            Duration::from_secs(0),
        ).unwrap();

        // First fetch
        let _ = cache.get_key("test-key-1").await.unwrap();
        // Second fetch — cache is stale so it refetches
        let _ = cache.get_key("test-key-1").await.unwrap();
    }

    #[tokio::test]
    async fn test_jwks_cache_force_refresh() {
        let mock_server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/.well-known/jwks.json"))
            .respond_with(ResponseTemplate::new(200).set_body_json(jwks_fixture()))
            .mount(&mock_server)
            .await;

        let cache = JwksCache::new(
            &format!("{}/.well-known/jwks.json", mock_server.uri()),
            Duration::from_secs(3600),
        ).unwrap();

        cache.force_refresh().await.unwrap();
        assert!(cache.get_key_from_cache("test-key-1").is_some());
    }

    #[tokio::test]
    async fn test_jwks_cache_network_error() {
        let cache = JwksCache::new("http://127.0.0.1:1/nonexistent", Duration::from_secs(3600)).unwrap();
        let result = cache.get_key("any-kid").await;
        assert!(result.is_err(), "expected Err for network error (connection refused)");
    }

    // ── S23-H2: JWKS response size cap ────────────────────────────────────────

    #[test]
    fn jwks_response_cap_constant_is_reasonable() {
        const { assert!(MAX_JWKS_RESPONSE_BYTES >= 64 * 1024) }
        const { assert!(MAX_JWKS_RESPONSE_BYTES <= 100 * 1024 * 1024) }
    }

    #[tokio::test]
    async fn jwks_oversized_response_is_rejected() {
        let mock_server = MockServer::start().await;
        let oversized = vec![b'x'; MAX_JWKS_RESPONSE_BYTES + 1];
        Mock::given(method("GET"))
            .and(path("/.well-known/jwks.json"))
            .respond_with(ResponseTemplate::new(200).set_body_bytes(oversized))
            .mount(&mock_server)
            .await;

        let cache = JwksCache::new(
            &format!("{}/.well-known/jwks.json", mock_server.uri()),
            Duration::from_secs(3600),
        ).unwrap();
        let result = cache.get_key("any-kid").await;
        assert!(result.is_err(), "oversized JWKS response must be rejected");
        let msg = result.err().unwrap();
        assert!(msg.contains("too large"), "error must mention size limit: {msg}");
    }

    #[tokio::test]
    async fn jwks_within_size_limit_is_accepted() {
        let mock_server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/.well-known/jwks.json"))
            .respond_with(ResponseTemplate::new(200).set_body_json(jwks_fixture()))
            .mount(&mock_server)
            .await;

        let cache = JwksCache::new(
            &format!("{}/.well-known/jwks.json", mock_server.uri()),
            Duration::from_secs(3600),
        ).unwrap();
        let key = cache
            .get_key("test-key-1")
            .await
            .unwrap_or_else(|e| panic!("normal JWKS response must be accepted, got: {e}"));
        assert!(key.is_some(), "expected key 'test-key-1' to be present in JWKS response");
    }

    // ── URL validation tests ─────────────────────────────────────────────────

    #[test]
    fn test_jwks_cache_rejects_invalid_url() {
        let result = JwksCache::new("not-a-url", Duration::from_secs(3600));
        assert!(result.is_err(), "invalid URL should be rejected at construction");
        assert!(matches!(result.unwrap_err(), JwksError::InvalidUrl { .. }));
    }

    #[test]
    fn test_jwks_cache_rejects_non_http_scheme() {
        let result = JwksCache::new("ftp://example.com/jwks.json", Duration::from_secs(3600));
        assert!(matches!(result.unwrap_err(), JwksError::InvalidScheme { .. }));
    }

    #[test]
    fn test_jwks_cache_rejects_http_non_localhost() {
        let result = JwksCache::new("http://example.com/jwks.json", Duration::from_secs(3600));
        assert!(matches!(result.unwrap_err(), JwksError::InvalidScheme { .. }));
    }

    #[test]
    fn test_jwks_cache_accepts_https() {
        let result = JwksCache::new(
            "https://example.com/.well-known/jwks.json",
            Duration::from_secs(3600),
        );
        assert!(result.is_ok(), "valid https:// URL should be accepted");
    }

    #[test]
    fn test_jwks_cache_accepts_http_localhost() {
        let result = JwksCache::new(
            "http://localhost:8080/.well-known/jwks.json",
            Duration::from_secs(3600),
        );
        assert!(result.is_ok(), "http://localhost should be accepted for dev");
    }
}
