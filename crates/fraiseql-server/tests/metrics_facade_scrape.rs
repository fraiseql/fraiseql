//! Audit H45: the server's Prometheus recorder install + render plumbing must
//! capture `metrics`-facade emissions so they reach the `/metrics` endpoint.
//!
//! The production server does not emit facade metrics itself (it uses the
//! hand-rolled `MetricsCollector`); the facade recorder exists to capture
//! transitive crates' emissions (e.g. fraiseql-wire). Here we emit a facade
//! counter via the test-only `metrics` dev-dependency and assert it renders.
//! The test is a no-op unless the `metrics` feature is built.
#![allow(clippy::unwrap_used, clippy::expect_used)] // Reason: test code, panics acceptable

#[cfg(feature = "metrics")]
#[test]
fn facade_counter_appears_in_recorder_render() {
    use metrics::counter;

    fraiseql_server::metrics_recorder::install();

    counter!("fraiseql_test_facade_total", "entity" => "facade_scrape_test").increment(1);

    let scrape = fraiseql_server::metrics_recorder::render();
    assert!(
        scrape.contains("fraiseql_test_facade_total"),
        "facade counter must appear in the recorder render:\n{scrape}"
    );
    assert!(
        scrape.contains("facade_scrape_test"),
        "the entity label must be present:\n{scrape}"
    );
}
