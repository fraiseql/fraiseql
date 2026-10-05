//! HTTP request validation with SSRF protection.
//!
//! This module validates outbound HTTP requests to prevent Server-Side Request Forgery (SSRF)
//! attacks by:
//! - Enforcing a domain allowlist
//! - Blocking private IP addresses (RFC 1918, loopback, link-local)
//! - Blocking `IPv6` private ranges

use fraiseql_error::{FraiseQLError, Result};
use fraiseql_guard::net::{BlockedReason, resolve::Refusal};

/// Configuration for HTTP client validation.
#[derive(Debug, Clone)]
pub struct HttpClientConfig {
    /// Allowed domains for outbound requests (glob patterns).
    ///
    /// Deny-by-default: an empty list (the default) permits **no** outbound
    /// hosts — a caller must explicitly opt in to each domain. Glob patterns
    /// such as `"*.example.com"` are supported; `"*"` allows all hosts and is
    /// NOT recommended for production.
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
            // Deny-by-default (fail-closed): no host is allowed until the caller
            // explicitly populates the allowlist.
            allowed_domains:    vec![],
            max_response_bytes: 10 * 1024 * 1024, // 10 MB
            connect_timeout_ms: 5000,
            read_timeout_ms:    30000,
        }
    }
}

/// Validate an outbound URL for SSRF attacks.
///
/// Checks:
/// 1. Domain is in the allowlist (deny-by-default; supports glob patterns)
/// 2. Literal IP address in the host is not private/reserved (RFC 1918, 127.0.0.0/8,
///    169.254.0.0/16, etc.)
/// 3. The host's DNS-resolved addresses are not private/reserved — closes the DNS-rebinding hole
///    where a public name resolves to an internal IP
/// 4. `IPv6` addresses are not private (loopback, link-local, ULA)
///
/// # Arguments
///
/// * `url` - The URL to validate
/// * `config` - HTTP client configuration with allowlist
///
/// # Returns
///
/// - `Ok(())` if the URL is safe to request
/// - `Err` if the URL is blocked by allowlist or resolves to a private IP
///
/// # Errors
///
/// Returns `Err` if the URL is malformed, blocked by the allowlist, fails DNS
/// resolution, or resolves to a private/reserved IP address.
pub async fn validate_outbound_url(url: &str, config: &HttpClientConfig) -> Result<()> {
    // Parse the URL
    let parsed_url = reqwest::Url::parse(url).map_err(|e| FraiseQLError::Validation {
        message: format!("invalid URL: {}", e),
        path:    None,
    })?;

    // Check domain allowlist
    let host = parsed_url.host_str().ok_or_else(|| FraiseQLError::Validation {
        message: "URL has no host".to_string(),
        path:    None,
    })?;

    // Check if host matches allowlist
    if !is_domain_allowed(host, &config.allowed_domains) {
        return Err(FraiseQLError::Authorization {
            message:  format!("domain '{}' not in allowlist", host),
            action:   Some("http_request".to_string()),
            resource: Some(host.to_string()),
        });
    }

    // The one resolve-and-check (#1360): the loopback and metadata aliases and a
    // blocked literal before any lookup, then every resolved address, since a public
    // name that resolves to an internal one is the classic SSRF bypass.
    let port = parsed_url.port_or_known_default().unwrap_or(443);
    let refused = |message: String, resource: String| FraiseQLError::Authorization {
        message,
        action: Some("http_request".to_string()),
        resource: Some(resource),
    };
    match fraiseql_guard::net::resolve::resolve_and_check(host, port).await {
        Ok(_) => Ok(()),
        Err(Refusal::Host(BlockedReason::ReservedAddress)) => Err(refused(
            format!("private/reserved IP address not allowed: {host}"),
            host.to_string(),
        )),
        Err(Refusal::Host(reason)) => {
            Err(refused(format!("host '{host}' not allowed: {reason}"), host.to_string()))
        },
        Err(Refusal::BlockedAddress(ip)) => Err(refused(
            format!("private/reserved IP address not allowed: {ip}"),
            ip.to_string(),
        )),
        Err(other @ (Refusal::ResolutionFailed(_) | Refusal::NoAddresses)) => {
            Err(FraiseQLError::Validation {
                message: format!("host '{host}': {other}"),
                path:    None,
            })
        },
    }
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

#[cfg(test)]
#[allow(clippy::unwrap_used)] // Reason: tests use unwrap for concise assertions
mod tests;
