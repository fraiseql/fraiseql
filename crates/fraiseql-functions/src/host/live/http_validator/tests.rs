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
