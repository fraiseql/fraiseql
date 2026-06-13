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
pub(crate) const MAX_JWKS_RESPONSE_BYTES: usize = 1024 * 1024; // 1 MiB

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
        uri:    String,
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
pub(crate) fn is_ssrf_blocked_ip(ip: &std::net::IpAddr) -> bool {
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
                || (s[0] == 0 && s[1] == 0 && s[2] == 0 && s[3] == 0 && s[4] == 0 && s[5] == 0xffff)
                || (s[0] & 0xfe00) == 0xfc00
                || (s[0] & 0xffc0) == 0xfe80
        },
    }
}

/// Resolve the host via DNS, reject any private/reserved address, and return the
/// validated socket addresses.
///
/// The returned addresses must be pinned into the HTTP client (see
/// [`build_pinned_client`]) so the connection targets exactly these validated IPs.
/// Validating here and then letting reqwest re-resolve independently would leave a
/// DNS-rebinding TOCTOU window (the attacker's domain resolves public for the check,
/// then private for the connect) — which is the bug this pairing closes.
///
/// # Errors
///
/// Returns a `String` error if DNS resolution fails, returns no addresses, or
/// any resolved address is in a private/reserved range.
pub(crate) async fn dns_resolve_and_check(
    host: &str,
    port: u16,
) -> Result<Vec<std::net::SocketAddr>, String> {
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
    Ok(addrs)
}

/// Build a reqwest client pinned to pre-validated socket addresses.
///
/// `addrs` MUST already have passed [`is_ssrf_blocked_ip`] (via
/// [`dns_resolve_and_check`]). Pinning reqwest's resolution to these exact addresses
/// closes the DNS-rebinding TOCTOU: reqwest cannot independently re-resolve `host` to a
/// different (possibly private) IP between our check and the connect. Redirects are
/// disabled so a `3xx` cannot bounce the request to an un-pinned internal target.
///
/// Phase 06 Cycle 3: kept self-contained and copy-ready. The federation/functions SSRF
/// consumers land in Phase 07 Cycle 4, which decides whether to extract this into a
/// shared crate or keep lockstep copies — with all consumers concrete.
pub(crate) fn build_pinned_client(
    host: &str,
    addrs: &[std::net::SocketAddr],
    timeout: Duration,
) -> Result<reqwest::Client, reqwest::Error> {
    reqwest::Client::builder()
        .timeout(timeout)
        .redirect(reqwest::redirect::Policy::none())
        .resolve_to_addrs(host, addrs)
        .build()
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
            uri:    jwks_uri.to_string(),
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

        let client = reqwest::Client::builder().timeout(JWKS_FETCH_TIMEOUT).build()?;

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

    /// Return the HTTP client to use for the next JWKS fetch.
    ///
    /// For non-localhost hosts, resolves and validates the host (rejecting
    /// private/reserved IPs) and pins reqwest to the validated addresses so it cannot
    /// independently re-resolve to a private IP between the check and the connect
    /// (DNS-rebinding TOCTOU). Localhost URLs (dev/test) use the default client.
    async fn request_client(&self) -> Result<reqwest::Client, String> {
        let Ok(parsed) = reqwest::Url::parse(&self.jwks_uri) else {
            return Ok(self.client.clone());
        };
        let Some(host) = parsed.host_str() else {
            return Ok(self.client.clone());
        };
        let is_localhost = {
            let h = host.to_ascii_lowercase();
            h == "localhost" || h == "127.0.0.1" || h == "[::1]" || h == "::1"
        };
        if is_localhost {
            return Ok(self.client.clone());
        }
        // Reason: only https (→443) and http (→80) pass `new()` validation,
        // both have known default ports; fallback 443 is unreachable in practice.
        let port = parsed.port_or_known_default().unwrap_or(443);
        let validated = dns_resolve_and_check(host, port).await?;
        build_pinned_client(host, &validated, JWKS_FETCH_TIMEOUT)
            .map_err(|e| format!("Failed to build pinned JWKS client: {e}"))
    }

    /// Fetch the JWKS document and populate the cache.
    async fn fetch_keys(&self) -> Result<(), String> {
        debug!(uri = %self.jwks_uri, "Fetching JWKS keys");

        // DNS-rebinding prevention: resolve+validate the host and PIN reqwest to the
        // validated IPs, so the connection cannot be redirected to a private address
        // after the check (TOCTOU). Localhost is exempt (dev/test).
        let client = self.request_client().await?;

        let body = client
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

#[allow(clippy::missing_fields_in_debug)] // Reason: last_fetched and client omitted — no diagnostic value, reqwest::Client is noisy
impl std::fmt::Debug for JwksCache {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        // Reason: poisoned lock during Debug formatting → degrade gracefully to 0.
        let key_count = self.keys.read().map(|k| k.len()).unwrap_or(0);
        f.debug_struct("JwksCache")
            .field("jwks_uri", &self.jwks_uri)
            .field("ttl", &self.ttl)
            .field("cached_keys", &key_count)
            .finish_non_exhaustive()
    }
}
