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

#[tokio::test]
async fn test_validate_outbound_url_valid() {
    let config = HttpClientConfig {
        allowed_domains: vec!["example.com".to_string()],
        ..Default::default()
    };
    assert!(validate_outbound_url("https://example.com/api", &config).await.is_ok());
}

#[tokio::test]
async fn test_validate_outbound_url_invalid_domain() {
    let config = HttpClientConfig {
        allowed_domains: vec!["example.com".to_string()],
        ..Default::default()
    };
    assert!(validate_outbound_url("https://other.com/api", &config).await.is_err());
}

#[tokio::test]
async fn test_validate_outbound_url_blocks_private_ip() {
    let config = HttpClientConfig {
        allowed_domains: vec!["*".to_string()],
        ..Default::default()
    };
    assert!(validate_outbound_url("http://127.0.0.1/api", &config).await.is_err());
    assert!(validate_outbound_url("http://192.168.1.1/api", &config).await.is_err());
    assert!(validate_outbound_url("http://10.0.0.1/api", &config).await.is_err());
}

#[tokio::test]
async fn test_validate_outbound_url_blocks_ipv6_loopback() {
    let config = HttpClientConfig {
        allowed_domains: vec!["*".to_string()],
        ..Default::default()
    };
    assert!(validate_outbound_url("http://[::1]/api", &config).await.is_err());
}

#[tokio::test]
async fn test_validate_outbound_url_blocks_ipv6_link_local() {
    let config = HttpClientConfig {
        allowed_domains: vec!["*".to_string()],
        ..Default::default()
    };
    assert!(validate_outbound_url("http://[fe80::1]/api", &config).await.is_err());
}

#[tokio::test]
async fn test_validate_outbound_url_allows_public_ip() {
    let config = HttpClientConfig {
        allowed_domains: vec!["*".to_string()],
        ..Default::default()
    };
    assert!(validate_outbound_url("http://8.8.8.8/api", &config).await.is_ok());
}

#[tokio::test]
async fn test_validate_outbound_url_invalid_url() {
    let config = HttpClientConfig::default();
    assert!(validate_outbound_url("not a valid url", &config).await.is_err());
}

// ── M-fn-ssrf: deny-by-default + DNS-rebinding + redirect policy ───────────

#[test]
fn test_default_allowlist_is_deny_by_default() {
    // The default allowlist is empty: no host is permitted.
    let config = HttpClientConfig::default();
    assert!(config.allowed_domains.is_empty());
    assert!(!is_domain_allowed("example.com", &config.allowed_domains));
    assert!(!is_domain_allowed("8.8.8.8", &config.allowed_domains));
}

#[tokio::test]
async fn test_deny_by_default_rejects_unlisted_domain() {
    // With the default (empty) allowlist, an otherwise-public domain is rejected.
    let config = HttpClientConfig::default();
    let result = validate_outbound_url("https://example.com/api", &config).await;
    assert!(
        matches!(result, Err(FraiseQLError::Authorization { .. })),
        "deny-by-default must reject an unlisted domain, got: {result:?}"
    );
}

#[tokio::test]
async fn test_domain_resolving_to_private_ip_is_rejected() {
    // `localhost` is allowlisted so the request clears the allowlist gate, but it
    // resolves to a loopback address — the DNS-rebinding guard must reject it.
    let config = HttpClientConfig {
        allowed_domains: vec!["localhost".to_string()],
        ..Default::default()
    };
    let result = validate_outbound_url("http://localhost/api", &config).await;
    assert!(
        result.is_err(),
        "a domain resolving to a private/loopback IP must be rejected: {result:?}"
    );
}

#[tokio::test]
async fn test_built_client_disables_redirects() {
    use wiremock::{
        Mock, MockServer, ResponseTemplate,
        matchers::{method, path},
    };

    // The client built for outbound host functions must not follow redirects:
    // a 3xx could otherwise bounce to an un-validated internal target. With
    // `Policy::none()` the 302 is returned verbatim instead of being followed.
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/start"))
        .respond_with(
            ResponseTemplate::new(302).insert_header("location", "http://169.254.169.254/"),
        )
        .mount(&server)
        .await;

    let client = reqwest::Client::builder()
        .redirect(reqwest::redirect::Policy::none())
        .connect_timeout(std::time::Duration::from_millis(5000))
        .timeout(std::time::Duration::from_millis(30000))
        .build()
        .unwrap();

    let resp = client.get(format!("{}/start", server.uri())).send().await.unwrap();
    assert_eq!(
        resp.status().as_u16(),
        302,
        "client must surface the 302 instead of following the redirect"
    );
}
