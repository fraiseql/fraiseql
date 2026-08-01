#![allow(clippy::unwrap_used, clippy::panic)] // Reason: test code, panics acceptable
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
        // Not a TEST-NET/documentation range: those are not globally routable and the
        // shared guard refuses them. This is a real public address.
        "http://93.184.216.34:8123",
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

// ── The shared outbound corpus, at this crate's entry point ───────────────────

#[test]
fn clickhouse_url_refuses_every_blocked_corpus_entry() {
    use fraiseql_guard::net::vectors::{MUST_BLOCK, MUST_BLOCK_HOSTS, url_host};
    for (addr, why) in MUST_BLOCK {
        let url = format!("http://{}:8123", url_host(addr));
        assert!(validate_clickhouse_url(&url).is_err(), "must refuse {addr} ({why})");
    }
    for (host, why) in MUST_BLOCK_HOSTS {
        let url = format!("http://{host}");
        assert!(validate_clickhouse_url(&url).is_err(), "must refuse {host} ({why})");
    }
}

#[test]
fn clickhouse_url_permits_every_allowed_corpus_entry() {
    use fraiseql_guard::net::vectors::{MUST_ALLOW, url_host};
    for addr in MUST_ALLOW {
        let url = format!("http://{}:8123", url_host(addr));
        assert!(validate_clickhouse_url(&url).is_ok(), "must permit {addr}");
    }
}

// ── #718: the flush deadline must anchor at the first buffered row ────────────

/// Under a steady stream arriving faster than `batch_timeout` but slower than
/// `batch_size`, buffered rows must still flush once the timeout lapses after
/// the FIRST buffered row. A timer that resets on every received message never
/// fires, leaving latency unbounded until the size threshold trips.
#[tokio::test(start_paused = true)]
async fn steady_stream_still_flushes_on_the_batch_timeout() {
    use std::sync::{Arc, Mutex};

    let (tx, rx) = mpsc::channel::<u32>(64);
    let flushes: Arc<Mutex<Vec<(usize, Duration)>>> = Arc::new(Mutex::new(Vec::new()));
    let flushes_in_driver = Arc::clone(&flushes);
    let started = tokio::time::Instant::now();

    let driver = tokio::spawn(drive_batches(
        rx,
        1_000, // never reached: the size path must not be what flushes
        Duration::from_secs(1),
        |n: &u32| Ok(vec![*n]),
        move |rows: Vec<u32>| {
            let flushes = Arc::clone(&flushes_in_driver);
            async move {
                flushes.lock().unwrap().push((rows.len(), started.elapsed()));
                Ok(())
            }
        },
    ));

    // A steady stream: one message every 100 ms for 3 virtual seconds.
    for n in 0..30u32 {
        tx.send(n).await.unwrap();
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    drop(tx);
    driver.await.unwrap().unwrap();

    let flushes = flushes.lock().unwrap();
    let (first_count, first_at) = *flushes.first().expect("at least the final flush runs");
    assert!(
        first_at <= Duration::from_millis(1_150),
        "first flush happened at {first_at:?} — the batch timeout (1s) never fired \
         under steady traffic, so latency is unbounded until batch_size trips (#718)"
    );
    assert!(
        first_count <= 12,
        "first flush carried {first_count} rows — it waited for the stream to end \
         instead of firing on the timeout"
    );
}
