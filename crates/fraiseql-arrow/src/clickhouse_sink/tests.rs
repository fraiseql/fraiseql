#![allow(clippy::unwrap_used)] // Reason: test code extensively uses unwrap for test fixture setup

use super::*;

/// A safe non-loopback URL used by tests that exercise non-URL validation paths.
const TEST_URL: &str = "http://clickhouse.example.com:8123";

#[test]
fn test_config_default() {
    let config = ClickHouseSinkConfig::default();
    assert_eq!(config.batch_size, 10_000);
    assert_eq!(config.batch_timeout_secs, 5);
    assert_eq!(config.max_retries, 3);
}

#[test]
fn test_config_validate_empty_url() {
    let config = ClickHouseSinkConfig {
        url: String::new(),
        ..Default::default()
    };
    assert!(
        matches!(config.validate(), Err(ArrowFlightError::Configuration(_))),
        "expected Configuration error for empty URL, got: {:?}",
        config.validate()
    );
}

#[test]
fn test_config_validate_empty_database() {
    let config = ClickHouseSinkConfig {
        url: TEST_URL.to_string(),
        database: String::new(),
        ..Default::default()
    };
    assert!(
        matches!(config.validate(), Err(ArrowFlightError::Configuration(_))),
        "expected Configuration error for empty database, got: {:?}",
        config.validate()
    );
}

#[test]
fn test_config_validate_empty_table() {
    let config = ClickHouseSinkConfig {
        url: TEST_URL.to_string(),
        table: String::new(),
        ..Default::default()
    };
    assert!(
        matches!(config.validate(), Err(ArrowFlightError::Configuration(_))),
        "expected Configuration error for empty table, got: {:?}",
        config.validate()
    );
}

#[test]
fn test_config_validate_invalid_batch_size() {
    let config_zero = ClickHouseSinkConfig {
        url: TEST_URL.to_string(),
        batch_size: 0,
        ..Default::default()
    };
    assert!(
        matches!(config_zero.validate(), Err(ArrowFlightError::Configuration(_))),
        "expected Configuration error for batch_size=0, got: {:?}",
        config_zero.validate()
    );

    let config_large = ClickHouseSinkConfig {
        url: TEST_URL.to_string(),
        batch_size: 200_000,
        ..Default::default()
    };
    assert!(
        matches!(config_large.validate(), Err(ArrowFlightError::Configuration(_))),
        "expected Configuration error for batch_size=200_000, got: {:?}",
        config_large.validate()
    );
}

#[test]
fn test_config_validate_invalid_timeout() {
    let config = ClickHouseSinkConfig {
        url: TEST_URL.to_string(),
        batch_timeout_secs: 0,
        ..Default::default()
    };
    assert!(
        matches!(config.validate(), Err(ArrowFlightError::Configuration(_))),
        "expected Configuration error for batch_timeout_secs=0, got: {:?}",
        config.validate()
    );
}

#[test]
fn test_config_validate_valid() {
    let config = ClickHouseSinkConfig {
        url: TEST_URL.to_string(),
        ..Default::default()
    };
    config
        .validate()
        .unwrap_or_else(|e| panic!("expected Ok for valid config: {e}"));
}

#[test]
fn test_is_transient_error() {
    let config = ClickHouseSinkConfig {
        url: TEST_URL.to_string(),
        ..Default::default()
    };
    let sink = ClickHouseSink::new(config).unwrap();

    assert!(sink.is_transient_error("Connection refused"));
    assert!(sink.is_transient_error("timeout"));
    assert!(sink.is_transient_error("TEMPORARY_ERROR"));
    assert!(sink.is_transient_error("503 Service Unavailable"));
    assert!(!sink.is_transient_error("Invalid schema"));
}

// --- SSRF protection tests ---

#[test]
fn test_clickhouse_url_scheme_must_be_http() {
    for bad_url in &[
        "file:///etc/passwd",
        "ftp://clickhouse.example.com:8123",
        "clickhouse.example.com:8123",
    ] {
        assert!(
            validate_clickhouse_url(bad_url).is_err(),
            "Expected SSRF rejection for: {bad_url}"
        );
    }
}

#[test]
fn test_clickhouse_url_blocks_loopback() {
    for url in &[
        "http://localhost:8123",
        "http://127.0.0.1:8123",
        "http://127.1.2.3:8123",
        "http://[::1]:8123",
    ] {
        assert!(validate_clickhouse_url(url).is_err(), "Expected SSRF rejection for: {url}");
    }
}

#[test]
fn test_clickhouse_url_blocks_private_ranges() {
    for url in &[
        "http://10.0.0.1:8123",
        "http://172.16.0.1:8123",
        "http://172.31.255.255:8123",
        "http://192.168.1.100:8123",
        "http://169.254.1.1:8123", // link-local
        "http://100.64.0.1:8123",  // CGNAT
    ] {
        assert!(validate_clickhouse_url(url).is_err(), "Expected SSRF rejection for: {url}");
    }
}

#[test]
fn test_clickhouse_url_allows_public_addresses() {
    for url in &[
        "http://clickhouse.example.com:8123",
        "https://analytics.production.example.com:8443",
        "http://203.0.113.10:8123", // TEST-NET-3 (documentation range)
    ] {
        assert!(validate_clickhouse_url(url).is_ok(), "Expected SSRF pass for: {url}");
    }
}

#[test]
fn test_clickhouse_url_blocks_credential_bypass() {
    // H1: credentials in URL must not let attacker bypass host extraction
    for url in &[
        "http://user:password@127.0.0.1:8123",
        "http://attacker@localhost:8123",
        "http://x:y@192.168.1.1:8123",
        "http://evil@10.0.0.1:8123",
    ] {
        assert!(
            validate_clickhouse_url(url).is_err(),
            "Expected SSRF rejection for credential-in-URL: {url}"
        );
    }
}

#[test]
fn test_clickhouse_url_blocks_ipv6_link_local() {
    // H2: fe80::/10 link-local must be blocked
    for url in &[
        "http://[fe80::1]:8123",
        "http://[fe80::dead:beef]:8123",
        "http://[febf::1]:8123", // fe80::/10 covers fe80..febf
    ] {
        assert!(
            validate_clickhouse_url(url).is_err(),
            "Expected SSRF rejection for fe80::/10 link-local: {url}"
        );
    }
}
