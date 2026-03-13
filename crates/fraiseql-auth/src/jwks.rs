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
    /// Create a new JWKS cache.
    ///
    /// Keys are lazily fetched on first access.
    pub fn new(jwks_uri: &str, ttl: Duration) -> Self {
        let client = reqwest::Client::builder()
            .timeout(JWKS_FETCH_TIMEOUT)
            .build()
            .unwrap_or_default();
        Self {
            keys: RwLock::new(HashMap::new()),
            jwks_uri: jwks_uri.to_string(),
            last_fetched: RwLock::new(None),
            ttl,
            client,
        }
    }

    /// Get a decoding key by `kid`, fetching from the remote JWKS endpoint if
    /// the cache is stale or the key is missing.
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

#[allow(clippy::unwrap_used)]  // Reason: test code, panics are acceptable
#[cfg(test)]
mod tests {
    use std::time::Duration;

    use wiremock::{
        Mock, MockServer, ResponseTemplate,
        matchers::{method, path},
    };

    #[allow(clippy::wildcard_imports)] // Reason: test modules use wildcard imports for conciseness
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
            JwksCache::new("https://example.com/.well-known/jwks.json", Duration::from_secs(3600));
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
        );

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
        );

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
        );

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
        );

        cache.force_refresh().await.unwrap();
        assert!(cache.get_key_from_cache("test-key-1").is_some());
    }

    #[tokio::test]
    async fn test_jwks_cache_network_error() {
        let cache = JwksCache::new("http://127.0.0.1:1/nonexistent", Duration::from_secs(3600));
        let result = cache.get_key("any-kid").await;
        assert!(result.is_err());
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
        );
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
        );
        let result = cache.get_key("test-key-1").await;
        assert!(result.is_ok(), "normal JWKS response must be accepted");
        assert!(result.unwrap().is_some());
    }
}
