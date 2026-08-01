//! Proxy and IP address extraction with security validation

use std::net::IpAddr;

/// Validate that a string is a valid IP address format
///
/// # SECURITY
/// Prevents injection attacks where malformed IPs could bypass validation.
/// Returns None for any invalid IP format.
fn validate_ip_format(ip_str: &str) -> Option<IpAddr> {
    ip_str.parse::<IpAddr>().ok()
}

/// Proxy configuration for X-Forwarded-For header validation
#[derive(Debug, Clone)]
pub struct ProxyConfig {
    /// List of trusted proxy IPs (e.g., load balancer, Nginx, HAProxy IPs).
    /// `X-Forwarded-For` / `X-Real-IP` are trusted only when the direct peer is
    /// one of these; an empty list trusts no forwarded headers at all.
    pub trusted_proxies: Vec<IpAddr>,
}

impl ProxyConfig {
    /// Create a new proxy configuration.
    ///
    /// (`#788`: the former `require_trusted_proxy` flag was never read — forwarded
    /// headers are, and always were, trusted only when the direct peer matches
    /// `trusted_proxies`. The dead flag was removed rather than left as a knob that
    /// looks load-bearing and is not.)
    #[must_use]
    pub const fn new(trusted_proxies: Vec<IpAddr>) -> Self {
        Self { trusted_proxies }
    }

    /// Create a proxy config that trusts all local proxies (127.0.0.1 only).
    ///
    /// # Panics
    ///
    /// Cannot panic — the IP literal `"127.0.0.1"` is always valid.
    #[must_use]
    pub fn localhost_only() -> Self {
        Self {
            trusted_proxies: vec!["127.0.0.1".parse().expect("valid IP")], /* Reason: "127.0.0.1" is a compile-time literal and always parses successfully */
        }
    }

    /// Create a proxy config with no trusted proxies
    #[must_use]
    pub const fn none() -> Self {
        Self {
            trusted_proxies: Vec::new(),
        }
    }

    /// Check if an IP address is a trusted proxy
    ///
    /// # SECURITY
    /// Validates IP format before checking against trusted list.
    /// Returns false for any invalid IP format, preventing bypass attempts.
    #[must_use]
    pub fn is_trusted_proxy(&self, ip: &str) -> bool {
        if self.trusted_proxies.is_empty() {
            return false;
        }

        // Validate IP format and check against trusted list
        match validate_ip_format(ip) {
            Some(addr) => self.trusted_proxies.contains(&addr),
            None => false, // Invalid IP format is not trusted
        }
    }

    /// Extract client IP from headers with security validation
    ///
    /// # SECURITY
    /// Only trusts X-Forwarded-For if the request comes from a trusted proxy.
    /// Falls back to direct connection IP if X-Forwarded-For cannot be validated.
    /// Validates all extracted IPs to ensure proper format.
    ///
    /// This prevents IP spoofing attacks where an attacker sends a malicious
    /// X-Forwarded-For header to bypass rate limiting or access controls.
    ///
    /// The client IP is taken by walking `X-Forwarded-For` **right to left**,
    /// skipping entries that are themselves trusted proxies, and returning the
    /// first non-trusted hop (#788). The leftmost entry is attacker-controlled
    /// whenever the trusted proxy *appends* to the header (the common case:
    /// nginx/ALB add the peer they saw to the end), so keying rate limits or
    /// access control on it — as this used to — let any client spoof its IP by
    /// prepending a value.
    #[must_use]
    pub fn extract_client_ip(
        &self,
        headers: &axum::http::HeaderMap,
        socket_addr: Option<std::net::SocketAddr>,
    ) -> Option<String> {
        let direct_ip = socket_addr.map(|addr| addr.ip().to_string());

        // If no direct IP available, return early
        let direct_ip_str = direct_ip.as_deref().unwrap_or("");

        // Check X-Forwarded-For if proxy is trusted
        if let Some(forwarded_for) = headers.get("x-forwarded-for").and_then(|v| v.to_str().ok()) {
            if self.is_trusted_proxy(direct_ip_str) {
                // Walk right→left, skipping trusted proxies; the first untrusted
                // hop is the real client. The trusted proxy appended the peer it
                // saw to the right end, so the rightmost non-proxy entry is the
                // value that peer cannot forge.
                for ip_str in forwarded_for.rsplit(',').map(str::trim) {
                    let Some(addr) = validate_ip_format(ip_str) else {
                        // A malformed entry breaks the chain of trust — stop and
                        // fall back to the direct peer rather than guessing past it.
                        break;
                    };
                    if !self.trusted_proxies.contains(&addr) {
                        return Some(ip_str.to_string());
                    }
                }
            }
            // X-Forwarded-For present but from untrusted proxy (or every hop was a
            // trusted proxy / malformed) — use the direct peer.
            if let Some(ip) = direct_ip {
                return Some(ip);
            }
        }

        // Check X-Real-IP if proxy is trusted
        if let Some(real_ip) = headers.get("x-real-ip").and_then(|v| v.to_str().ok()) {
            if self.is_trusted_proxy(direct_ip_str) {
                // SECURITY: Validate IP format before returning
                if validate_ip_format(real_ip).is_some() {
                    return Some(real_ip.to_string());
                }
                // Invalid IP format - fall through to use direct IP
            }
            // X-Real-IP present but from untrusted proxy - ignore it and use direct IP
            if let Some(ip) = direct_ip {
                return Some(ip);
            }
        }

        // Fall back to direct connection IP (already validated by Axum)
        direct_ip
    }
}
