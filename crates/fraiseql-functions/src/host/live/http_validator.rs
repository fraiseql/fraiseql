//! HTTP request validation with SSRF protection.
//!
//! This module validates outbound HTTP requests to prevent Server-Side Request Forgery (SSRF)
//! attacks by:
//! - Enforcing a domain allowlist
//! - Blocking private IP addresses (RFC 1918, loopback, link-local)
//! - Blocking IPv6 private ranges

use fraiseql_error::{FraiseQLError, Result};
use std::net::IpAddr;

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
/// 3. IPv6 addresses are not private (loopback, link-local, ULA)
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
pub fn validate_outbound_url(url: &str, config: &HttpClientConfig) -> Result<()> {
    // Parse the URL
    let parsed_url = reqwest::Url::parse(url).map_err(|e| {
        FraiseQLError::Validation {
            message: format!("invalid URL: {}", e),
            path: None,
        }
    })?;

    // Check domain allowlist
    let host = parsed_url
        .host_str()
        .ok_or_else(|| FraiseQLError::Validation {
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
        if pattern.starts_with("*.") {
            let domain = &pattern[2..];
            // Only match if there's a subdomain (must have a dot before the domain)
            if host_for_comparison.ends_with(&format!(".{}", domain)) {
                return true;
            }
        }
    }

    false
}

/// Parse IP address from a host string, handling IPv6 brackets.
/// IPv6 addresses in URLs are bracketed: `[::1]`
fn parse_ip_from_host(host: &str) -> Result<IpAddr> {
    // Strip IPv6 brackets if present
    let clean_host = if host.starts_with('[') && host.ends_with(']') {
        &host[1..host.len() - 1]
    } else {
        host
    };

    clean_host.parse::<IpAddr>().map_err(|e| {
        FraiseQLError::Validation {
            message: format!("failed to parse IP address: {}", e),
            path: None,
        }
    })
}

/// Validate that an IP address is not private/reserved.
/// Blocks:
/// - 127.0.0.0/8 (loopback)
/// - 10.0.0.0/8, 172.16.0.0/12, 192.168.0.0/16 (RFC 1918 private)
/// - 169.254.0.0/16 (link-local)
/// - ::1 (IPv6 loopback)
/// - fc00::/7 (IPv6 unique local addresses)
/// - fe80::/10 (IPv6 link-local)
fn validate_ip(ip: &IpAddr) -> Result<()> {
    match ip {
        IpAddr::V4(v4) => {
            if v4.is_loopback()
                || v4.is_private()
                || v4.is_link_local()
                || v4.is_broadcast()
                || is_ipv4_reserved(v4)
            {
                return Err(FraiseQLError::Authorization {
                    message: format!("private/reserved IP address not allowed: {}", v4),
                    action: Some("http_request".to_string()),
                    resource: Some(v4.to_string()),
                });
            }
            Ok(())
        }
        IpAddr::V6(v6) => {
            if v6.is_loopback() || v6.is_unique_local() || v6.is_unicast_link_local() {
                return Err(FraiseQLError::Authorization {
                    message: format!("private IPv6 address not allowed: {}", v6),
                    action: Some("http_request".to_string()),
                    resource: Some(v6.to_string()),
                });
            }
            Ok(())
        }
    }
}

/// Check if an IPv4 address is reserved.
/// This is a workaround since Ipv4Addr::is_reserved() is unstable.
fn is_ipv4_reserved(ip: &std::net::Ipv4Addr) -> bool {
    let octets = ip.octets();
    match octets[0] {
        0 | 100..=127 | 240..=255 => true,
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_is_domain_allowed_with_wildcard() {
        let config = HttpClientConfig {
            allowed_domains: vec!["*".to_string()],
            ..Default::default()
        };
        assert!(is_domain_allowed("example.com", &config.allowed_domains));
        assert!(is_domain_allowed("any.domain.anywhere", &config.allowed_domains));
    }

    #[test]
    fn test_is_domain_allowed_exact_match() {
        let config = HttpClientConfig {
            allowed_domains: vec!["example.com".to_string(), "safe.io".to_string()],
            ..Default::default()
        };
        assert!(is_domain_allowed("example.com", &config.allowed_domains));
        assert!(is_domain_allowed("safe.io", &config.allowed_domains));
        assert!(!is_domain_allowed("other.com", &config.allowed_domains));
    }

    #[test]
    fn test_is_domain_allowed_glob_pattern() {
        let config = HttpClientConfig {
            allowed_domains: vec!["*.example.com".to_string()],
            ..Default::default()
        };
        assert!(is_domain_allowed("api.example.com", &config.allowed_domains));
        assert!(is_domain_allowed("sub.api.example.com", &config.allowed_domains));
        assert!(!is_domain_allowed("example.com", &config.allowed_domains));
        assert!(!is_domain_allowed("other.com", &config.allowed_domains));
    }

    #[test]
    fn test_parse_ip_ipv4() {
        let ip = parse_ip_from_host("192.168.1.1").unwrap();
        assert_eq!(ip.to_string(), "192.168.1.1");
    }

    #[test]
    fn test_parse_ip_ipv6_with_brackets() {
        let ip = parse_ip_from_host("[::1]").unwrap();
        assert_eq!(ip.to_string(), "::1");
    }

    #[test]
    fn test_parse_ip_ipv6_without_brackets() {
        let ip = parse_ip_from_host("::1").unwrap();
        assert_eq!(ip.to_string(), "::1");
    }

    #[test]
    fn test_validate_ip_blocks_loopback_v4() {
        let ip = "127.0.0.1".parse::<IpAddr>().unwrap();
        assert!(validate_ip(&ip).is_err());
    }

    #[test]
    fn test_validate_ip_blocks_private_v4() {
        let ips = vec!["10.0.0.1", "172.16.0.1", "192.168.1.1"];
        for ip_str in ips {
            let ip = ip_str.parse::<IpAddr>().unwrap();
            assert!(validate_ip(&ip).is_err(), "should block {}", ip_str);
        }
    }

    #[test]
    fn test_validate_ip_blocks_link_local_v4() {
        let ip = "169.254.1.1".parse::<IpAddr>().unwrap();
        assert!(validate_ip(&ip).is_err());
    }

    #[test]
    fn test_validate_ip_allows_public_v4() {
        let ips = vec!["8.8.8.8", "1.1.1.1", "208.67.222.222"];
        for ip_str in ips {
            let ip = ip_str.parse::<IpAddr>().unwrap();
            assert!(validate_ip(&ip).is_ok(), "should allow {}", ip_str);
        }
    }

    #[test]
    fn test_validate_ip_blocks_loopback_v6() {
        let ip = "::1".parse::<IpAddr>().unwrap();
        assert!(validate_ip(&ip).is_err());
    }

    #[test]
    fn test_validate_ip_blocks_link_local_v6() {
        let ip = "fe80::1".parse::<IpAddr>().unwrap();
        assert!(validate_ip(&ip).is_err());
    }

    #[test]
    fn test_validate_ip_blocks_unique_local_v6() {
        let ips = vec!["fd00::1", "fc00::1"];
        for ip_str in ips {
            let ip = ip_str.parse::<IpAddr>().unwrap();
            assert!(validate_ip(&ip).is_err(), "should block {}", ip_str);
        }
    }

    #[test]
    fn test_validate_ip_allows_public_v6() {
        let ips = vec!["2001:4860:4860::8888", "2606:4700:4700::1111"];
        for ip_str in ips {
            let ip = ip_str.parse::<IpAddr>().unwrap();
            assert!(validate_ip(&ip).is_ok(), "should allow {}", ip_str);
        }
    }

    #[test]
    fn test_validate_outbound_url_valid() {
        let config = HttpClientConfig {
            allowed_domains: vec!["example.com".to_string()],
            ..Default::default()
        };
        assert!(validate_outbound_url("https://example.com/api", &config).is_ok());
    }

    #[test]
    fn test_validate_outbound_url_invalid_domain() {
        let config = HttpClientConfig {
            allowed_domains: vec!["example.com".to_string()],
            ..Default::default()
        };
        assert!(validate_outbound_url("https://other.com/api", &config).is_err());
    }

    #[test]
    fn test_validate_outbound_url_blocks_private_ip() {
        let config = HttpClientConfig {
            allowed_domains: vec!["*".to_string()],
            ..Default::default()
        };
        assert!(validate_outbound_url("http://127.0.0.1/api", &config).is_err());
        assert!(validate_outbound_url("http://192.168.1.1/api", &config).is_err());
        assert!(validate_outbound_url("http://10.0.0.1/api", &config).is_err());
    }

    #[test]
    fn test_validate_outbound_url_blocks_ipv6_loopback() {
        let config = HttpClientConfig {
            allowed_domains: vec!["*".to_string()],
            ..Default::default()
        };
        assert!(validate_outbound_url("http://[::1]/api", &config).is_err());
    }

    #[test]
    fn test_validate_outbound_url_blocks_ipv6_link_local() {
        let config = HttpClientConfig {
            allowed_domains: vec!["*".to_string()],
            ..Default::default()
        };
        assert!(validate_outbound_url("http://[fe80::1]/api", &config).is_err());
    }

    #[test]
    fn test_validate_outbound_url_allows_public_ip() {
        let config = HttpClientConfig {
            allowed_domains: vec!["*".to_string()],
            ..Default::default()
        };
        assert!(validate_outbound_url("http://8.8.8.8/api", &config).is_ok());
    }

    #[test]
    fn test_validate_outbound_url_invalid_url() {
        let config = HttpClientConfig::default();
        assert!(validate_outbound_url("not a valid url", &config).is_err());
    }
}
