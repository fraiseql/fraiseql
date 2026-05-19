//! HTTP request validation with SSRF protection.
//!
//! This module validates outbound HTTP requests to prevent Server-Side Request Forgery (SSRF)
//! attacks by:
//! - Enforcing a domain allowlist
//! - Blocking private IP addresses (RFC 1918, loopback, link-local)
//! - Blocking `IPv6` private ranges

use std::net::IpAddr;

use fraiseql_error::{FraiseQLError, Result};

/// Configuration for HTTP client validation.
#[derive(Debug, Clone)]
pub struct HttpClientConfig {
    /// Allowed domains for outbound requests (glob patterns).
    /// Use "*" for unrestricted access (NOT recommended for production).
    pub allowed_domains: Vec<String>,

    /// Maximum response body size in bytes.
    pub max_response_bytes: usize,

    /// Connect timeout in milliseconds.
    pub connect_timeout_ms: u64,

    /// Read timeout in milliseconds.
    pub read_timeout_ms: u64,
}

impl Default for HttpClientConfig {
    fn default() -> Self {
        Self {
            allowed_domains: vec!["*".to_string()],
            max_response_bytes: 10 * 1024 * 1024, // 10 MB
            connect_timeout_ms: 5000,
            read_timeout_ms: 30000,
        }
    }
}

/// Validate an outbound URL for SSRF attacks.
///
/// Checks:
/// 1. Domain is in the allowlist (supports glob patterns)
/// 2. IP address is not private/reserved (RFC 1918, 127.0.0.0/8, 169.254.0.0/16, etc.)
/// 3. `IPv6` addresses are not private (loopback, link-local, ULA)
///
/// # Arguments
///
/// * `url` - The URL to validate
/// * `config` - HTTP client configuration with allowlist
///
/// # Returns
///
/// - `Ok(())` if the URL is safe to request
/// - `Err` if the URL is blocked by allowlist or is a private IP
///
/// # Errors
///
/// Returns `Err` if the URL is malformed, blocked by the allowlist, or resolves to a
/// private/reserved IP address.
pub fn validate_outbound_url(url: &str, config: &HttpClientConfig) -> Result<()> {
    // Parse the URL
    let parsed_url = reqwest::Url::parse(url).map_err(|e| FraiseQLError::Validation {
        message: format!("invalid URL: {}", e),
        path: None,
    })?;

    // Check domain allowlist
    let host = parsed_url.host_str().ok_or_else(|| FraiseQLError::Validation {
        message: "URL has no host".to_string(),
        path: None,
    })?;

    // Check if host matches allowlist
    if !is_domain_allowed(host, &config.allowed_domains) {
        return Err(FraiseQLError::Authorization {
            message: format!("domain '{}' not in allowlist", host),
            action: Some("http_request".to_string()),
            resource: Some(host.to_string()),
        });
    }

    // Check for private/reserved IPs
    if let Ok(ip) = parse_ip_from_host(host) {
        validate_ip(&ip)?;
    }

    Ok(())
}

/// Check if a host (domain or IP) is in the allowlist.
/// Supports glob patterns: "*" matches all, "*.example.com" matches subdomains.
/// Also supports IP addresses (with or without port).
fn is_domain_allowed(host: &str, allowlist: &[String]) -> bool {
    for pattern in allowlist {
        if pattern == "*" {
            return true;
        }

        // Extract IP from host (remove port if present)
        let host_for_comparison = if let Some(colon_pos) = host.rfind(':') {
            // Only strip port if this looks like host:port (not IPv6)
            if !host.starts_with('[') {
                &host[..colon_pos]
            } else {
                host
            }
        } else {
            host
        };

        // Exact match (including IP addresses)
        if host_for_comparison == pattern || host == pattern {
            return true;
        }

        // Simple glob matching: "*.example.com" matches "api.example.com" but NOT "example.com"
        if let Some(domain) = pattern.strip_prefix("*.") {
            // Only match if there's a subdomain (must have a dot before the domain)
            if host_for_comparison.ends_with(&format!(".{}", domain)) {
                return true;
            }
        }
    }

    false
}

/// Parse IP address from a host string, handling `IPv6` brackets.
/// `IPv6` addresses in URLs are bracketed: `[::1]`
fn parse_ip_from_host(host: &str) -> Result<IpAddr> {
    // Strip IPv6 brackets if present
    let clean_host = if host.starts_with('[') && host.ends_with(']') {
        &host[1..host.len() - 1]
    } else {
        host
    };

    clean_host.parse::<IpAddr>().map_err(|e| FraiseQLError::Validation {
        message: format!("failed to parse IP address: {}", e),
        path: None,
    })
}

/// Validate that an IP address is not private/reserved.
/// Blocks:
/// - 127.0.0.0/8 (loopback)
/// - 10.0.0.0/8, 172.16.0.0/12, 192.168.0.0/16 (RFC 1918 private)
/// - 169.254.0.0/16 (link-local)
/// - `::1` (`IPv6` loopback)
/// - `fc00::/7` (`IPv6` unique local addresses)
/// - `fe80::/10` (`IPv6` link-local)
fn validate_ip(ip: &IpAddr) -> Result<()> {
    match ip {
        IpAddr::V4(v4) => {
            if v4.is_loopback()
                || v4.is_private()
                || v4.is_link_local()
                || v4.is_broadcast()
                || is_ipv4_reserved(*v4)
            {
                return Err(FraiseQLError::Authorization {
                    message: format!("private/reserved IP address not allowed: {}", v4),
                    action: Some("http_request".to_string()),
                    resource: Some(v4.to_string()),
                });
            }
            Ok(())
        },
        IpAddr::V6(v6) => {
            if v6.is_loopback() || v6.is_unique_local() || v6.is_unicast_link_local() {
                return Err(FraiseQLError::Authorization {
                    message: format!("private IPv6 address not allowed: {}", v6),
                    action: Some("http_request".to_string()),
                    resource: Some(v6.to_string()),
                });
            }
            Ok(())
        },
    }
}

/// Check if an `IPv4` address is reserved.
/// This is a workaround since `Ipv4Addr::is_reserved()` is unstable.
const fn is_ipv4_reserved(ip: std::net::Ipv4Addr) -> bool {
    let octets = ip.octets();
    matches!(octets[0], 0 | 100..=127 | 240..=255)
}

#[cfg(test)]
#[allow(clippy::unwrap_used)] // Reason: tests use unwrap for concise assertions
mod tests;
