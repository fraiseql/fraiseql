// ── initialization_tests ──────────────────────────────────────────────────────

mod initialization_tests {
    use super::super::initialization::is_manifest_url_ssrf_blocked;

    #[test]
    fn ssrf_blocks_localhost_by_name() {
        assert!(is_manifest_url_ssrf_blocked("http://localhost/manifest.json"));
    }

    #[test]
    fn ssrf_blocks_localhost_uppercase() {
        assert!(is_manifest_url_ssrf_blocked("http://LOCALHOST/manifest.json"));
    }

    #[test]
    fn ssrf_blocks_ipv4_loopback() {
        assert!(is_manifest_url_ssrf_blocked("http://127.0.0.1/manifest.json"));
    }

    #[test]
    fn ssrf_blocks_ipv4_private_192_168() {
        assert!(is_manifest_url_ssrf_blocked("http://192.168.1.100/manifest.json"));
    }

    #[test]
    fn ssrf_blocks_ipv4_private_10_x() {
        assert!(is_manifest_url_ssrf_blocked("http://10.0.0.1/manifest.json"));
    }

    #[test]
    fn ssrf_blocks_ipv4_private_172_16() {
        assert!(is_manifest_url_ssrf_blocked("http://172.16.0.1/manifest.json"));
    }

    #[test]
    fn ssrf_blocks_ipv4_link_local() {
        assert!(is_manifest_url_ssrf_blocked("http://169.254.1.1/manifest.json"));
    }

    #[test]
    fn ssrf_blocks_ipv6_loopback() {
        assert!(is_manifest_url_ssrf_blocked("http://[::1]/manifest.json"));
    }

    #[test]
    fn ssrf_blocks_ipv6_unspecified() {
        assert!(is_manifest_url_ssrf_blocked("http://[::]/manifest.json"));
    }

    #[test]
    fn ssrf_blocks_ipv6_ula() {
        // fc00::/7 range
        assert!(is_manifest_url_ssrf_blocked("http://[fd00::1]/manifest.json"));
    }

    #[test]
    fn ssrf_blocks_unparseable_url() {
        assert!(is_manifest_url_ssrf_blocked("not a url at all"));
    }

    #[test]
    fn ssrf_allows_public_https() {
        assert!(!is_manifest_url_ssrf_blocked("https://cdn.example.com/manifest.json"));
    }

    #[test]
    fn ssrf_allows_public_ipv4() {
        // 93.184.216.34 is example.com — a real public address
        assert!(!is_manifest_url_ssrf_blocked("http://93.184.216.34/manifest.json"));
    }

    #[test]
    fn ssrf_allows_public_ipv6_global() {
        // 2001:db8:: is documentation range — treated as public by is_manifest_url_ssrf_blocked
        assert!(!is_manifest_url_ssrf_blocked("http://[2001:db8::1]/manifest.json"));
    }
}
