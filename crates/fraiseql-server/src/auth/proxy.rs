//! Proxy and IP address extraction with security validation

use std::net::IpAddr;

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
    pub fn new(trusted_proxies: Vec<IpAddr>, require_trusted_proxy: bool) -> Self {
        Self {
            trusted_proxies,
            require_trusted_proxy,
        }
    }

    /// Create a proxy config that trusts all local proxies (127.0.0.1 only)
    pub fn localhost_only() -> Self {
        Self {
            trusted_proxies: vec!["127.0.0.1".parse().expect("valid IP")],
            require_trusted_proxy: true,
        }
    }

    /// Create a proxy config with no trusted proxies
    pub fn none() -> Self {
        Self {
            trusted_proxies: vec![],
            require_trusted_proxy: false,
        }
    }

    /// Check if an IP address is a trusted proxy
    pub fn is_trusted_proxy(&self, ip: &str) -> bool {
        if self.trusted_proxies.is_empty() {
            return false;
        }

        // Parse the IP address
        match ip.parse::<IpAddr>() {
            Ok(addr) => self.trusted_proxies.contains(&addr),
            Err(_) => false, // Invalid IP is not trusted
        }
    }

    /// Extract client IP from headers with security validation
    ///
    /// # SECURITY
    /// Only trusts X-Forwarded-For if the request comes from a trusted proxy.
    /// Falls back to direct connection IP if X-Forwarded-For cannot be validated.
    ///
    /// This prevents IP spoofing attacks where an attacker sends a malicious
    /// X-Forwarded-For header to bypass rate limiting or access controls.
    pub fn extract_client_ip(
        &self,
        headers: &axum::http::HeaderMap,
        socket_addr: Option<std::net::SocketAddr>,
    ) -> Option<String> {
        let direct_ip = socket_addr.map(|addr| addr.ip().to_string());

        // If no direct IP available, return early
        let direct_ip_str = direct_ip.as_deref().unwrap_or("");

        // Check X-Forwarded-For if proxy is trusted
        if let Some(forwarded_for) = headers.get("x-forwarded-for").and_then(|v| v.to_str().ok())
        {
            if self.is_trusted_proxy(direct_ip_str) {
                // Extract first IP from X-Forwarded-For (client IP in chain)
                return forwarded_for.split(',').next().map(|ip| ip.trim().to_string());
            }
            // X-Forwarded-For present but from untrusted proxy - ignore it and use direct IP
            if let Some(ip) = direct_ip {
                return Some(ip);
            }
        }

        // Check X-Real-IP if proxy is trusted
        if let Some(real_ip) = headers.get("x-real-ip").and_then(|v| v.to_str().ok()) {
            if self.is_trusted_proxy(direct_ip_str) {
                return Some(real_ip.to_string());
            }
            // X-Real-IP present but from untrusted proxy - ignore it and use direct IP
            if let Some(ip) = direct_ip {
                return Some(ip);
            }
        }

        // Fall back to direct connection IP
        direct_ip
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_proxy_config_localhost_only() {
        let config = ProxyConfig::localhost_only();
        assert!(config.is_trusted_proxy("127.0.0.1"));
        assert!(!config.is_trusted_proxy("192.168.1.1"));
    }

    #[test]
    fn test_proxy_config_is_trusted_proxy_valid_ip() {
        let ip: IpAddr = "10.0.0.1".parse().unwrap();
        let config = ProxyConfig::new(vec![ip], true);
        assert!(config.is_trusted_proxy("10.0.0.1"));
    }

    #[test]
    fn test_proxy_config_is_trusted_proxy_untrusted_ip() {
        let ip: IpAddr = "10.0.0.1".parse().unwrap();
        let config = ProxyConfig::new(vec![ip], true);
        assert!(!config.is_trusted_proxy("192.168.1.1"));
    }

    #[test]
    fn test_proxy_config_is_trusted_proxy_invalid_ip() {
        let ip: IpAddr = "10.0.0.1".parse().unwrap();
        let config = ProxyConfig::new(vec![ip], true);
        assert!(!config.is_trusted_proxy("invalid_ip"));
    }

    #[test]
    fn test_extract_client_ip_from_trusted_proxy_x_forwarded_for() {
        let ip: IpAddr = "10.0.0.1".parse().unwrap();
        let config = ProxyConfig::new(vec![ip], true);

        let mut headers = axum::http::HeaderMap::new();
        headers.insert("x-forwarded-for", "192.0.2.1, 10.0.0.1".parse().unwrap());

        let direct_ip = "10.0.0.1".parse::<std::net::IpAddr>().ok();
        let socket = direct_ip.map(|ip| std::net::SocketAddr::new(ip, 8000));

        let result = config.extract_client_ip(&headers, socket);
        assert_eq!(result, Some("192.0.2.1".to_string()));
    }

    #[test]
    fn test_extract_client_ip_from_untrusted_proxy_x_forwarded_for() {
        let ip: IpAddr = "10.0.0.1".parse().unwrap();
        let config = ProxyConfig::new(vec![ip], true);

        let mut headers = axum::http::HeaderMap::new();
        headers.insert("x-forwarded-for", "192.0.2.1, 10.0.0.1".parse().unwrap());

        let direct_ip = "192.168.1.100".parse::<std::net::IpAddr>().ok();
        let socket = direct_ip.map(|ip| std::net::SocketAddr::new(ip, 8000));

        // Should ignore X-Forwarded-For and use direct IP
        let result = config.extract_client_ip(&headers, socket);
        assert_eq!(result, Some("192.168.1.100".to_string()));
    }

    #[test]
    fn test_extract_client_ip_no_headers() {
        let config = ProxyConfig::localhost_only();
        let headers = axum::http::HeaderMap::new();

        let direct_ip = "192.168.1.100".parse::<std::net::IpAddr>().ok();
        let socket = direct_ip.map(|ip| std::net::SocketAddr::new(ip, 8000));

        let result = config.extract_client_ip(&headers, socket);
        assert_eq!(result, Some("192.168.1.100".to_string()));
    }

    #[test]
    fn test_extract_client_ip_empty_headers() {
        let config = ProxyConfig::localhost_only();
        let headers = axum::http::HeaderMap::new();

        let result = config.extract_client_ip(&headers, None);
        assert_eq!(result, None);
    }

    #[test]
    fn test_extract_client_ip_spoofing_attempt() {
        // Attacker tries to spoof IP from untrusted source
        let trusted_ip: IpAddr = "10.0.0.1".parse().unwrap();
        let config = ProxyConfig::new(vec![trusted_ip], true);

        let mut headers = axum::http::HeaderMap::new();
        // Attacker sends malicious X-Forwarded-For header
        headers.insert("x-forwarded-for", "1.2.3.4".parse().unwrap());

        // Request comes from untrusted IP (attacker direct IP)
        let attacker_ip = "192.168.1.100".parse::<std::net::IpAddr>().ok();
        let socket = attacker_ip.map(|ip| std::net::SocketAddr::new(ip, 8000));

        // Should use attacker's direct IP, not the spoofed X-Forwarded-For
        let result = config.extract_client_ip(&headers, socket);
        assert_eq!(result, Some("192.168.1.100".to_string()));
    }
}
