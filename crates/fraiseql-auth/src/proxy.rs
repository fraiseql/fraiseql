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
    /// List of trusted proxy IPs (e.g., load balancer, Nginx, HAProxy IPs)
    /// Only X-Forwarded-For headers from these IPs are trusted
    pub trusted_proxies: Vec<IpAddr>,
    /// If true, require request to come from a trusted proxy to use X-Forwarded-For
    pub require_trusted_proxy: bool,
}

impl ProxyConfig {
    /// Create a new proxy configuration
    #[must_use]
    pub const fn new(trusted_proxies: Vec<IpAddr>, require_trusted_proxy: bool) -> Self {
        Self {
            trusted_proxies,
            require_trusted_proxy,
        }
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
            require_trusted_proxy: true,
        }
    }

    /// Create a proxy config with no trusted proxies
    #[must_use]
    pub const fn none() -> Self {
        Self {
            trusted_proxies: Vec::new(),
            require_trusted_proxy: false,
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
                // Extract first IP from X-Forwarded-For (client IP in chain)
                if let Some(ip_str) = forwarded_for.split(',').next().map(|ip| ip.trim()) {
                    // SECURITY: Validate IP format before returning
                    if validate_ip_format(ip_str).is_some() {
                        return Some(ip_str.to_string());
                    }
                    // Invalid IP format - fall through to use direct IP
                }
            }
            // X-Forwarded-For present but from untrusted proxy - ignore it and use direct IP
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
