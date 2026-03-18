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

/// Returns `true` for IP addresses that JWKS fetches must not contact.
fn is_ssrf_blocked_ip(ip: &std::net::IpAddr) -> bool {
    match ip {
        std::net::IpAddr::V4(v4) => {
            let o = v4.octets();
            o[0] == 127
                || o[0] == 10
                || (o[0] == 172 && (16..=31).contains(&o[1]))
                || (o[0] == 192 && o[1] == 168)
                || (o[0] == 169 && o[1] == 254)
                || (o[0] == 100 && (o[1] & 0b1100_0000) == 0b0100_0000)
                || o[0] == 0
        },
        std::net::IpAddr::V6(v6) => {
            let s = v6.segments();
            *v6 == std::net::Ipv6Addr::LOCALHOST
                || *v6 == std::net::Ipv6Addr::UNSPECIFIED
                || (s[0] == 0 && s[1] == 0 && s[2] == 0 && s[3] == 0
                    && s[4] == 0 && s[5] == 0xffff)
                || (s[0] & 0xfe00) == 0xfc00
                || (s[0] & 0xffc0) == 0xfe80
        },
    }
}

/// Resolve the host via DNS and reject if any address is private/reserved.
///
/// Prevents DNS rebinding attacks where an attacker-controlled domain initially
/// resolves to a public IP (passing URL validation) but later resolves to a
/// private IP during the actual HTTP request.
///
/// # Errors
///
/// Returns a `String` error if DNS resolution fails, returns no addresses, or
/// any resolved address is in a private/reserved range.
async fn dns_resolve_and_check(host: &str, port: u16) -> Result<(), String> {
    let addrs: Vec<std::net::SocketAddr> = tokio::net::lookup_host((host, port))
        .await
        .map_err(|e| format!("DNS resolution failed for JWKS host '{host}': {e}"))?
        .collect();
    if addrs.is_empty() {
        return Err(format!("DNS resolved to no addresses for JWKS host '{host}'"));
    }
    for addr in &addrs {
        if is_ssrf_blocked_ip(&addr.ip()) {
            return Err(format!(
                "DNS rebinding attack blocked: JWKS host '{host}' resolved to private/reserved IP {}",
                addr.ip()
            ));
        }
    }
    Ok(())
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

        // DNS rebinding prevention: resolve the host and reject private/reserved IPs
        // before making the HTTP request. Skip for localhost URLs (dev/test only).
        if let Ok(parsed) = reqwest::Url::parse(&self.jwks_uri) {
            if let Some(host) = parsed.host_str() {
                let is_localhost = {
                    let h = host.to_ascii_lowercase();
                    h == "localhost" || h == "127.0.0.1" || h == "[::1]" || h == "::1"
                };
                if !is_localhost {
                    let port = parsed.port_or_known_default().unwrap_or(443);
                    dns_resolve_and_check(host, port).await?;
                }
            }
        }

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

    // ── SSRF IP blocking tests ──────────────────────────────────────────────

    #[test]
    fn test_ssrf_blocks_loopback_v4() {
        let ip: std::net::IpAddr = "127.0.0.1".parse().unwrap();
        assert!(is_ssrf_blocked_ip(&ip), "127.0.0.1 must be blocked");
        let ip: std::net::IpAddr = "127.255.255.255".parse().unwrap();
        assert!(is_ssrf_blocked_ip(&ip), "127.x.x.x must be blocked");
    }

    #[test]
    fn test_ssrf_blocks_rfc1918_10() {
        let ip: std::net::IpAddr = "10.0.0.1".parse().unwrap();
        assert!(is_ssrf_blocked_ip(&ip), "10.0.0.1 must be blocked");
        let ip: std::net::IpAddr = "10.255.255.255".parse().unwrap();
        assert!(is_ssrf_blocked_ip(&ip), "10.255.255.255 must be blocked");
    }

    #[test]
    fn test_ssrf_blocks_rfc1918_172() {
        let ip: std::net::IpAddr = "172.16.0.1".parse().unwrap();
        assert!(is_ssrf_blocked_ip(&ip), "172.16.0.1 must be blocked");
        let ip: std::net::IpAddr = "172.31.255.255".parse().unwrap();
        assert!(is_ssrf_blocked_ip(&ip), "172.31.255.255 must be blocked");
        // 172.15.x and 172.32.x are public
        let ip: std::net::IpAddr = "172.15.0.1".parse().unwrap();
        assert!(!is_ssrf_blocked_ip(&ip), "172.15.0.1 must NOT be blocked");
        let ip: std::net::IpAddr = "172.32.0.1".parse().unwrap();
        assert!(!is_ssrf_blocked_ip(&ip), "172.32.0.1 must NOT be blocked");
    }

    #[test]
    fn test_ssrf_blocks_rfc1918_192_168() {
        let ip: std::net::IpAddr = "192.168.0.1".parse().unwrap();
        assert!(is_ssrf_blocked_ip(&ip), "192.168.0.1 must be blocked");
        let ip: std::net::IpAddr = "192.168.255.255".parse().unwrap();
        assert!(is_ssrf_blocked_ip(&ip), "192.168.255.255 must be blocked");
        let ip: std::net::IpAddr = "192.169.0.1".parse().unwrap();
        assert!(!is_ssrf_blocked_ip(&ip), "192.169.0.1 must NOT be blocked");
    }

    #[test]
    fn test_ssrf_blocks_link_local_169_254() {
        let ip: std::net::IpAddr = "169.254.0.1".parse().unwrap();
        assert!(is_ssrf_blocked_ip(&ip), "169.254.x.x must be blocked");
        let ip: std::net::IpAddr = "169.254.169.254".parse().unwrap();
        assert!(is_ssrf_blocked_ip(&ip), "AWS metadata IP must be blocked");
    }

    #[test]
    fn test_ssrf_blocks_cgnat_100_64() {
        let ip: std::net::IpAddr = "100.64.0.1".parse().unwrap();
        assert!(is_ssrf_blocked_ip(&ip), "100.64.0.1 (CGNAT) must be blocked");
        let ip: std::net::IpAddr = "100.127.255.255".parse().unwrap();
        assert!(is_ssrf_blocked_ip(&ip), "100.127.255.255 (CGNAT) must be blocked");
        let ip: std::net::IpAddr = "100.63.255.255".parse().unwrap();
        assert!(!is_ssrf_blocked_ip(&ip), "100.63.x.x is NOT CGNAT");
        let ip: std::net::IpAddr = "100.128.0.1".parse().unwrap();
        assert!(!is_ssrf_blocked_ip(&ip), "100.128.x.x is NOT CGNAT");
    }

    #[test]
    fn test_ssrf_blocks_unspecified_v4() {
        let ip: std::net::IpAddr = "0.0.0.0".parse().unwrap();
        assert!(is_ssrf_blocked_ip(&ip), "0.0.0.0 must be blocked");
    }

    #[test]
    fn test_ssrf_allows_public_ips() {
        for addr in &["8.8.8.8", "1.1.1.1", "93.184.216.34", "203.0.113.1"] {
            let ip: std::net::IpAddr = addr.parse().unwrap();
            assert!(!is_ssrf_blocked_ip(&ip), "{addr} is public and must NOT be blocked");
        }
    }

    #[test]
    fn test_ssrf_blocks_loopback_v6() {
        let ip: std::net::IpAddr = "::1".parse().unwrap();
        assert!(is_ssrf_blocked_ip(&ip), "::1 must be blocked");
    }

    #[test]
    fn test_ssrf_blocks_unspecified_v6() {
        let ip: std::net::IpAddr = "::".parse().unwrap();
        assert!(is_ssrf_blocked_ip(&ip), ":: must be blocked");
    }

    #[test]
    fn test_ssrf_blocks_ipv4_mapped_v6() {
        // ::ffff:127.0.0.1
        let ip: std::net::IpAddr = "::ffff:127.0.0.1".parse().unwrap();
        assert!(is_ssrf_blocked_ip(&ip), "::ffff:127.0.0.1 must be blocked");
        // ::ffff:10.0.0.1
        let ip: std::net::IpAddr = "::ffff:10.0.0.1".parse().unwrap();
        assert!(is_ssrf_blocked_ip(&ip), "::ffff:10.0.0.1 must be blocked");
    }

    #[test]
    fn test_ssrf_blocks_ula_v6() {
        // fc00::/7 — Unique Local Addresses
        let ip: std::net::IpAddr = "fc00::1".parse().unwrap();
        assert!(is_ssrf_blocked_ip(&ip), "fc00::1 (ULA) must be blocked");
        let ip: std::net::IpAddr = "fd00::1".parse().unwrap();
        assert!(is_ssrf_blocked_ip(&ip), "fd00::1 (ULA) must be blocked");
    }

    #[test]
    fn test_ssrf_blocks_link_local_v6() {
        // fe80::/10
        let ip: std::net::IpAddr = "fe80::1".parse().unwrap();
        assert!(is_ssrf_blocked_ip(&ip), "fe80::1 (link-local) must be blocked");
    }

    #[test]
    fn test_ssrf_allows_public_v6() {
        let ip: std::net::IpAddr = "2001:4860:4860::8888".parse().unwrap();
        assert!(!is_ssrf_blocked_ip(&ip), "Google DNS v6 must NOT be blocked");
    }

    // ── Debug impl ──────────────────────────────────────────────────────────

    #[test]
    fn test_jwks_cache_debug_format() {
        let cache = JwksCache::new(
            "https://example.com/.well-known/jwks.json",
            Duration::from_secs(3600),
        ).unwrap();
        let dbg = format!("{cache:?}");
        assert!(dbg.contains("JwksCache"), "Debug output must contain struct name");
        assert!(dbg.contains("example.com"), "Debug output must contain jwks_uri");
    }
}
